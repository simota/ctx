//! `GET /api/tests` — native port via `ctx-testinsights`.
//!
//! Mirrors Go's `handleTests` in `internal/web/handlers.go` exactly:
//! envelope shape, limit clamping (default 8, clamp [1,50]), JSON omitempty
//! behaviour, error codes.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::handlers::file::relative_to_root;
use crate::response;
use crate::safepath;
use crate::AppState;

#[derive(Deserialize)]
pub struct TestsParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    profile: String,
    #[serde(default)]
    limit: String,
}

/// Mirror of Go's `TestInsightResponse`.
/// `tests` is always emitted (no omitempty in Go).
/// `sources` has omitempty — Go emits it as `[]` when initialised to empty
/// non-nil, but omitempty on a zero-len slice drops it. We match that by
/// only emitting sources when non-empty.
#[derive(Serialize)]
struct TestInsightResponse {
    path: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    kind: String,
    tests: Vec<TestInsightFile>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    sources: Vec<TestInsightSource>,
    #[serde(skip_serializing_if = "is_zero")]
    total_tests: i32,
    #[serde(skip_serializing_if = "is_zero")]
    total_sources: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    coverage: Option<TestCoverageSummary>,
}

#[derive(Serialize)]
struct TestInsightFile {
    path: String,
    score: i32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "is_zero")]
    test_count: i32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    matched_symbols: Vec<String>,
}

#[derive(Serialize)]
struct TestInsightSource {
    path: String,
    score: i32,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    reasons: Vec<String>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    matched_symbols: Vec<String>,
}

#[derive(Serialize)]
struct TestCoverageSummary {
    profile: String,
    total_stmts: i32,
    covered_stmts: i32,
    percent: f64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    uncovered_lines: Vec<TestCoverageRange>,
}

#[derive(Serialize)]
struct TestCoverageRange {
    start: i32,
    end: i32,
}

pub async fn handle(State(state): State<AppState>, Query(params): Query<TestsParams>) -> Response {
    // analyze() walks the repo + parses tests; keep it off the tokio workers.
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: TestsParams) -> Response {
    if params.path.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }
    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };
    let meta = match std::fs::metadata(&target) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return response::error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!("stat {}: no such file or directory", target.display()),
                );
            }
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "stat", &e.to_string());
        }
    };
    if meta.is_dir() {
        return response::error(StatusCode::BAD_REQUEST, "not_a_file", "path is a directory");
    }

    let rel = relative_to_root(&state.root, &target);
    let insight = match ctx_testinsights::analyze(&state.root, &rel, &params.profile) {
        Ok(i) => i,
        Err(e) => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "tests", &e.to_string())
        }
    };
    let limit = clamp_limit(&params.limit);

    let resp = TestInsightResponse {
        path: insight.path,
        kind: insight.kind,
        tests: insight
            .tests
            .into_iter()
            .take(limit)
            .map(|t| TestInsightFile {
                path: t.path,
                score: t.score,
                reasons: t.reasons,
                test_count: t.test_count,
                matched_symbols: t.matched_symbols,
            })
            .collect(),
        sources: insight
            .sources
            .into_iter()
            .take(limit)
            .map(|s| TestInsightSource {
                path: s.path,
                score: s.score,
                reasons: s.reasons,
                matched_symbols: s.matched_symbols,
            })
            .collect(),
        total_tests: insight.total_tests,
        total_sources: insight.total_sources,
        coverage: insight.coverage.map(|c| TestCoverageSummary {
            profile: c.profile,
            total_stmts: c.total_stmts,
            covered_stmts: c.covered_stmts,
            percent: c.percent,
            uncovered_lines: c
                .uncovered_lines
                .into_iter()
                .map(|r| TestCoverageRange {
                    start: r.start,
                    end: r.end,
                })
                .collect(),
        }),
    };
    response::json(StatusCode::OK, &resp)
}

fn clamp_limit(s: &str) -> usize {
    let mut limit = s.parse::<i32>().unwrap_or(8);
    if limit < 1 {
        limit = 1;
    }
    if limit > 50 {
        limit = 50;
    }
    limit as usize
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}
