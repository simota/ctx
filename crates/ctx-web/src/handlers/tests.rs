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
                return response::stat_not_found(&target);
            }
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "stat", &e.to_string());
        }
    };
    if meta.is_dir() {
        return response::error(StatusCode::BAD_REQUEST, "not_a_file", "path is a directory");
    }
    // Opening a FIFO blocks until a writer shows up, pinning a blocking-pool
    // thread for good.
    if !meta.is_file() {
        return response::error(
            StatusCode::BAD_REQUEST,
            "not_a_file",
            "path is not a regular file",
        );
    }

    let rel = relative_to_root(&state.root, &target);
    let insight = match ctx_testinsights::analyze(&state.root, &rel, &params.profile) {
        Ok(i) => i,
        Err(e) => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "tests", &e.to_string())
        }
    };
    let limit = crate::handlers::limit::parse_limit(&params.limit, 8, 1, 50) as usize;

    // Outside Go (which keeps its symbol-aware analysis and coverage), pair
    // the file with its tests — or a test with its sources — by naming
    // convention and imports, for every language.
    if insight.kind.is_empty() && is_code_path(&rel) {
        return response::json(StatusCode::OK, &paired_insight(&state.root, &rel, limit));
    }

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

/// Files a test can exercise or be: code the relations walker understands,
/// plus the other languages test pairing knows.
fn is_code_path(rel: &str) -> bool {
    ctx_relations::supported(rel)
        || matches!(
            rel.rsplit('.')
                .next()
                .map(str::to_ascii_lowercase)
                .as_deref(),
            Some("rb" | "cs" | "scala" | "dart" | "ex" | "exs" | "c" | "cc" | "cpp" | "h" | "hpp")
        )
}

/// Test insight from `ctx_relations` test pairing: the tests of a source
/// file (kind `paired`), or the sources a test file exercises (kind
/// `paired-test`). Scores rank the link kinds
/// (name 3, imports 2, name prefix / elsewhere 1).
fn paired_insight(root: &str, rel: &str, limit: usize) -> TestInsightResponse {
    let files: Vec<String> = ctx_relations::walk::walk(root)
        .map(|walked| {
            walked
                .into_iter()
                .filter(|file| !file.is_dir && file.path != ".")
                .map(|file| file.path)
                .collect()
        })
        .unwrap_or_default();
    let index = ctx_relations::build_cached(root).ok();
    let score = |kind: ctx_relations::TestLinkKind| match kind {
        ctx_relations::TestLinkKind::Name => 3,
        ctx_relations::TestLinkKind::Imports => 2,
        _ => 1,
    };
    let reason = |link: &ctx_relations::TestLink| match link.kind {
        ctx_relations::TestLinkKind::Name => "named after it",
        ctx_relations::TestLinkKind::Imports => "imports it",
        ctx_relations::TestLinkKind::NamePrefix => "named like it",
        ctx_relations::TestLinkKind::NameElsewhere => "same name elsewhere",
    };
    let source_reason = |link: &ctx_relations::TestLink| match link.kind {
        ctx_relations::TestLinkKind::Name => "test is named after it",
        ctx_relations::TestLinkKind::Imports => "test imports it",
        ctx_relations::TestLinkKind::NamePrefix => "test is named like it",
        ctx_relations::TestLinkKind::NameElsewhere => "same name elsewhere",
    };
    let mut resp = TestInsightResponse {
        path: rel.to_string(),
        kind: "paired".to_string(),
        tests: Vec::new(),
        sources: Vec::new(),
        total_tests: 0,
        total_sources: 0,
        coverage: None,
    };
    if ctx_relations::is_test_path(rel) {
        resp.kind = "paired-test".to_string();
        let found = ctx_relations::find_subjects(&files, index.as_ref(), rel);
        resp.total_sources = found.len() as i32;
        resp.sources = found
            .iter()
            .take(limit)
            .map(|link| TestInsightSource {
                path: link.path.clone(),
                score: score(link.kind),
                reasons: vec![source_reason(link).to_string()],
                matched_symbols: Vec::new(),
            })
            .collect();
    } else {
        let found = ctx_relations::find_tests(&files, index.as_ref(), &[rel.to_string()]);
        resp.total_tests = found.len() as i32;
        resp.tests = found
            .iter()
            .take(limit)
            .map(|link| TestInsightFile {
                path: link.path.clone(),
                score: score(link.kind),
                reasons: vec![reason(link).to_string()],
                test_count: 0,
                matched_symbols: Vec::new(),
            })
            .collect();
    }
    resp
}

fn is_zero(v: &i32) -> bool {
    *v == 0
}

#[cfg(all(test, unix))]
mod fifo_tests {
    use super::*;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime};

    #[test]
    fn fifo_path_is_rejected_without_blocking() {
        let dir = std::env::temp_dir().join(format!(
            "ctx-web-tests-fifo-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let status = std::process::Command::new("mkfifo")
            .arg(dir.join("pipe.go"))
            .status()
            .expect("run mkfifo");
        assert!(status.success(), "mkfifo failed");

        let state = AppState {
            root: dir.to_string_lossy().into_owned(),
            bind: String::new(),
            audit: false,
            file_cache: Default::default(),
            diff_cache: Default::default(),
            co_change_cache: Default::default(),
            loopback_only: true,
        };
        let params = TestsParams {
            path: "pipe.go".to_string(),
            profile: String::new(),
            limit: String::new(),
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(handle_sync(state, params).status());
        });
        let status = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("/api/tests did not answer within 10s");
        let _ = std::fs::remove_dir_all(&dir);
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }
}
