//! `POST /api/pack` — build an LLM context bundle from an explicit, ordered
//! file selection (the pinned-files board) or a saved mix.
//!
//! Request (JSON):
//! ```json
//! {"files": ["src/a.rs", "README.md"], "goal": "…", "budget": 30000,
//!  "format": "markdown", "contract": true}
//! ```
//! or `{"mix": "<id or name>"}` to pack a saved recipe (its goal and budget
//! apply unless the request overrides them).
//!
//! Response:
//! ```json
//! {"content": "# Context Pack …", "format": "markdown", "used": 812,
//!  "budget": 30000, "included": [{"path": "src/a.rs", "tokens": 700}],
//!  "skipped": [{"path": "big.json", "reason": "budget exceeded (90000 tokens)"}]}
//! ```
//!
//! Files are read only inside the served root (see
//! `ctx_pack::selection::pack_selection`) and secret-redacted.

use std::path::Path;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::response;
use crate::AppState;

/// Upper bounds that keep one request from asking for unbounded work.
const MAX_FILES: usize = 2000;
const MAX_BUDGET: i64 = 2_000_000;
const MAX_GOAL_LEN: usize = 2000;

/// Reject a body that is not declared as JSON. Besides catching client
/// mistakes, this keeps cross-site HTML forms (which can only send
/// `text/plain`, form or multipart bodies without a CORS preflight) from
/// reaching the mutation routes.
/// Returns the error response to send, or `None` when the body is JSON.
pub fn require_json(headers: &HeaderMap) -> Option<Response> {
    let is_json = headers
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.split(';').next())
        .is_some_and(|mime| mime.trim().eq_ignore_ascii_case("application/json"));
    (!is_json).then(|| {
        response::error(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            "unsupported_media_type",
            "send the request body as application/json",
        )
    })
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PackRequest {
    #[serde(default)]
    files: Vec<String>,
    #[serde(default)]
    mix: String,
    #[serde(default)]
    goal: Option<String>,
    #[serde(default)]
    budget: Option<i64>,
    #[serde(default)]
    format: String,
    #[serde(default)]
    contract: Option<bool>,
}

#[derive(Serialize)]
struct PackItem {
    path: String,
    tokens: i64,
}

#[derive(Serialize)]
struct SkipItem {
    path: String,
    reason: String,
}

#[derive(Serialize)]
struct PackResponse {
    content: String,
    format: String,
    goal: String,
    used: i64,
    budget: i64,
    included: Vec<PackItem>,
    skipped: Vec<SkipItem>,
}

pub async fn handle(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(resp) = require_json(&headers) {
        return resp;
    }
    crate::blocking::run(move || handle_sync(state, body)).await
}

fn bad_request(msg: &str) -> Response {
    response::error(StatusCode::BAD_REQUEST, "bad_request", msg)
}

fn handle_sync(state: AppState, body: Bytes) -> Response {
    let req: PackRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => return bad_request(&format!("invalid pack body: {e}")),
    };
    let (files, mut goal, mut budget) = if req.mix.trim().is_empty() {
        (req.files, String::new(), 0)
    } else {
        if !req.files.is_empty() {
            return bad_request("pass either files or mix, not both");
        }
        let store = match ctx_mix::Store::open_default() {
            Ok(store) => store,
            Err(e) => {
                return response::error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "mix_store",
                    &e.to_string(),
                )
            }
        };
        match store.find(&req.mix) {
            Ok(mix) => (mix.files, mix.goal, mix.budget.limit),
            Err(e @ ctx_mix::MixError::NotFound(_)) => {
                return response::error(StatusCode::NOT_FOUND, "not_found", &e.to_string())
            }
            Err(e) => return response::error(StatusCode::BAD_REQUEST, e.code(), &e.to_string()),
        }
    };
    if let Some(g) = req.goal {
        goal = g;
    }
    if let Some(b) = req.budget {
        budget = b;
    }
    if files.is_empty() {
        return bad_request("files must list at least one path");
    }
    if files.len() > MAX_FILES {
        return bad_request(&format!("at most {MAX_FILES} files per pack"));
    }
    if !(0..=MAX_BUDGET).contains(&budget) {
        return bad_request(&format!("budget must be between 0 and {MAX_BUDGET}"));
    }
    if goal.len() > MAX_GOAL_LEN {
        return bad_request(&format!("goal longer than {MAX_GOAL_LEN} bytes"));
    }
    let format = if req.format.is_empty() {
        "markdown".to_string()
    } else {
        req.format
    };
    let opts = ctx_pack::selection::SelectionOptions {
        goal: goal.trim().to_string(),
        budget,
        format: format.clone(),
        contract: req.contract.unwrap_or(true),
    };
    let pack = match ctx_pack::selection::pack_selection(Path::new(&state.root), &files, &opts) {
        Ok(pack) => pack,
        Err(e) => return bad_request(&e),
    };
    response::json(
        StatusCode::OK,
        &PackResponse {
            content: pack.content,
            format,
            goal: opts.goal,
            used: pack.used,
            budget: pack.budget,
            included: pack
                .included
                .into_iter()
                .map(|f| PackItem {
                    path: f.path,
                    tokens: f.tokens,
                })
                .collect(),
            skipped: pack
                .skipped
                .into_iter()
                .map(|s| SkipItem {
                    path: s.path,
                    reason: s.reason,
                })
                .collect(),
        },
    )
}
