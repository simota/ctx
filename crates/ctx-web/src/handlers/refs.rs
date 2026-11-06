//! `POST /api/refs` — find the repository files (and lines) a piece of text
//! refers to: a model reply, a stack trace, a review comment, a permalink.
//! The pinned-files board uses it to pin what a reply mentions and to pack
//! exactly the cited lines (the `path:START-END` specs feed `POST /api/pack`).
//!
//! Request (JSON): `{"text": "…", "around": 10}` (`around` defaults to 10
//! lines of context either side of a cited line, as `ctx pack --from-refs`).
//!
//! Response:
//! ```json
//! {"targets": [{"path": "src/a.rs", "specs": ["src/a.rs:30-50"],
//!               "lines": "30-50", "mentions": 2,
//!               "reason": "referenced 2 times (first as \"src/a.rs:40\", input line 3)"}],
//!  "unresolved": [{"mention": "lib.rs", "line": 4,
//!                  "reason": "ambiguous reference: matches 3 files (quote more of the path)"}]}
//! ```
//!
//! Only files the search walk lists can resolve: hidden paths (`.env`),
//! secret-bearing names (`*.pem`, `credentials.json`) and anything outside
//! the served root never do.

use std::path::Path;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::response;
use crate::AppState;

/// Bounds that keep one request from asking for unbounded work.
const MAX_TEXT_LEN: usize = 1024 * 1024;
const MAX_AROUND: usize = 1000;
const DEFAULT_AROUND: usize = 10;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RefsRequest {
    text: String,
    #[serde(default)]
    around: Option<usize>,
}

#[derive(Serialize)]
struct Target {
    path: String,
    specs: Vec<String>,
    lines: String,
    mentions: usize,
    reason: String,
}

#[derive(Serialize)]
struct Unresolved {
    mention: String,
    line: usize,
    reason: String,
}

#[derive(Serialize)]
struct RefsResponse {
    targets: Vec<Target>,
    unresolved: Vec<Unresolved>,
}

pub async fn handle(State(state): State<AppState>, headers: HeaderMap, body: Bytes) -> Response {
    if let Some(resp) = super::pack::require_json(&headers) {
        return resp;
    }
    crate::blocking::run(move || handle_sync(state, body)).await
}

fn bad_request(msg: &str) -> Response {
    response::error(StatusCode::BAD_REQUEST, "bad_request", msg)
}

fn handle_sync(state: AppState, body: Bytes) -> Response {
    let req: RefsRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => return bad_request(&format!("invalid refs body: {e}")),
    };
    if req.text.len() > MAX_TEXT_LEN {
        return bad_request(&format!("text longer than {MAX_TEXT_LEN} bytes"));
    }
    let around = req.around.unwrap_or(DEFAULT_AROUND);
    if around > MAX_AROUND {
        return bad_request(&format!("around must be at most {MAX_AROUND}"));
    }
    let root = match crate::safepath::resolve(&state.root, "") {
        Ok(root) => root,
        Err(e) => return response::bad_path(e),
    };
    let files = match super::where_::list_files(Path::new(&root)) {
        Ok(files) => files,
        Err(e) => return response::error(StatusCode::INTERNAL_SERVER_ERROR, "refs", &e),
    };
    let files: Vec<String> = files
        .into_iter()
        .filter(|rel| !super::raw::secret_deny(rel))
        .collect();
    let scan = ctx_pack::refs::scan(&req.text, &files, around);
    response::json(
        StatusCode::OK,
        &RefsResponse {
            targets: scan
                .targets
                .iter()
                .map(|t| Target {
                    path: t.path.clone(),
                    specs: t.specs(),
                    lines: ctx_pack::excerpt::label(&t.ranges),
                    mentions: t.mentions.len(),
                    reason: t.reason(),
                })
                .collect(),
            unresolved: scan
                .unresolved
                .into_iter()
                .map(|u| Unresolved {
                    mention: u.mention,
                    line: u.input_line,
                    reason: u.reason,
                })
                .collect(),
        },
    )
}
