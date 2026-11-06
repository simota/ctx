//! `/api/mix` and `/api/mix/<id>` — saved file selections ("mixes").
//!
//! - GET    `/api/mix`       → list (summaries, newest first)
//! - POST   `/api/mix`       → create from `{name, goal?, files, budget?,
//!   skip_missing?}` (201)
//! - GET    `/api/mix/{id}`  → full recipe
//! - DELETE `/api/mix/{id}`  → delete (204)
//!
//! The store is the one `ctx mix` and `ctx pack --from-mix` use (see the
//! `ctx-mix` crate for its location and on-disk format). GET responses keep
//! the byte shape of the Go server. Other methods get the Go server's 405
//! envelopes.
//!
//! Creating a recipe only accepts files that exist as regular files inside
//! the served root (no `..`, no links leaving it), so the browser cannot use
//! the store to record paths it could not read.

use std::io;

use axum::body::Bytes;
use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};

use crate::response;
use crate::AppState;

fn open_store() -> Result<ctx_mix::Store, Box<Response>> {
    ctx_mix::Store::open_default().map_err(|e| {
        Box::new(response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "mix_store",
            &e.to_string(),
        ))
    })
}

/// `Ok(true)` when the store directory exists, `Ok(false)` when it does not
/// yet, an error response when the path is unusable.
fn store_dir_state(store: &ctx_mix::Store) -> Result<bool, Box<Response>> {
    match std::fs::metadata(store.dir()) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(Box::new(response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "mix_store",
            &e.to_string(),
        ))),
        Ok(m) if !m.is_dir() => Err(Box::new(response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "mix_store",
            "store path is not a directory",
        ))),
        Ok(_) => Ok(true),
    }
}

fn error_response(e: ctx_mix::MixError, fallback_code: &str) -> Response {
    let msg = e.to_string();
    let status = match e {
        ctx_mix::MixError::InvalidId(_) | ctx_mix::MixError::Invalid(_) => StatusCode::BAD_REQUEST,
        ctx_mix::MixError::NotFound(_) => StatusCode::NOT_FOUND,
        ctx_mix::MixError::Exists(_) | ctx_mix::MixError::Ambiguous(..) => StatusCode::CONFLICT,
        _ => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, fallback_code, &msg);
        }
    };
    response::error(status, e.code(), &msg)
}

// ---------------------------------------------------------------------------
// Response shapes (mirror internal/web/mix.go MixSummary + MixListResponse)
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct MixSummary {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    goal: String,
    created: String,
    file_count: usize,
}

#[derive(Serialize)]
struct MixListResponse {
    mixes: Vec<MixSummary>,
}

// ---------------------------------------------------------------------------
// GET /api/mix (list)
// ---------------------------------------------------------------------------

pub async fn handle_list(State(state): State<AppState>) -> Response {
    crate::blocking::run(move || handle_list_sync(state)).await
}

fn handle_list_sync(_state: AppState) -> Response {
    let empty = || response::json(StatusCode::OK, &MixListResponse { mixes: vec![] });
    let Ok(store) = ctx_mix::Store::open_default() else {
        return empty();
    };
    match store_dir_state(&store) {
        Ok(true) => {}
        Ok(false) => return empty(),
        Err(resp) => return *resp,
    }
    let mixes = match store.list() {
        Ok(m) => m,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "mix_list",
                &e.to_string(),
            );
        }
    };
    let items: Vec<MixSummary> = mixes
        .into_iter()
        .map(|m| MixSummary {
            id: m.id,
            name: m.name,
            goal: m.goal,
            created: m.created,
            file_count: m.files.len(),
        })
        .collect();
    response::json(StatusCode::OK, &MixListResponse { mixes: items })
}

// ---------------------------------------------------------------------------
// GET /api/mix/{id}
// ---------------------------------------------------------------------------

fn id_from_uri(uri: &axum::http::Uri) -> String {
    uri.path().trim_start_matches("/api/mix/").to_string()
}

pub async fn handle_get(State(state): State<AppState>, uri: axum::http::Uri) -> Response {
    let id = id_from_uri(&uri);
    crate::blocking::run(move || handle_get_sync(state, id)).await
}

fn handle_get_sync(_state: AppState, id: String) -> Response {
    if id.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "id is required");
    }
    let store = match open_store() {
        Ok(store) => store,
        Err(resp) => return *resp,
    };
    match store_dir_state(&store) {
        Ok(true) => {}
        Ok(false) => {
            return response::error(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("mix not found: {id}"),
            );
        }
        Err(resp) => return *resp,
    }
    match store.load(&id) {
        Ok(m) => response::json(StatusCode::OK, &m),
        Err(e) => error_response(e, "mix_get"),
    }
}

// ---------------------------------------------------------------------------
// POST /api/mix (create)
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CreateRequest {
    name: String,
    #[serde(default)]
    goal: String,
    files: Vec<String>,
    /// A token count, or the stored `{"limit": N}` shape.
    #[serde(default)]
    budget: Option<serde_json::Value>,
    /// Drop files that are not in the served root (and list them in the
    /// response's `skipped`) instead of rejecting the request.
    #[serde(default)]
    skip_missing: bool,
}

/// 201 body: the saved recipe, plus the files `skip_missing` dropped.
#[derive(Serialize)]
struct CreateResponse {
    #[serde(flatten)]
    mix: ctx_mix::Mix,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    skipped: Vec<String>,
}

fn budget_limit(value: Option<&serde_json::Value>) -> Result<i64, String> {
    let limit = match value {
        None | Some(serde_json::Value::Null) => return Ok(0),
        Some(serde_json::Value::Number(n)) => n.as_i64(),
        Some(serde_json::Value::Object(map)) => match map.get("limit") {
            None => Some(0),
            Some(v) => v.as_i64(),
        },
        _ => None,
    };
    limit.ok_or_else(|| "budget must be a token count".to_string())
}

pub async fn handle_create(
    state: AppState,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    if let Some(resp) = crate::handlers::pack::require_json(&headers) {
        return resp;
    }
    crate::blocking::run(move || handle_create_sync(state, body)).await
}

fn handle_create_sync(state: AppState, body: Bytes) -> Response {
    let req: CreateRequest = match serde_json::from_slice(&body) {
        Ok(req) => req,
        Err(e) => {
            return response::error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                &format!("invalid mix body: {e}"),
            )
        }
    };
    let limit = match budget_limit(req.budget.as_ref()) {
        Ok(limit) => limit,
        Err(msg) => return response::error(StatusCode::BAD_REQUEST, "bad_request", &msg),
    };
    let files = match ctx_mix::normalize_files(&req.files) {
        Ok(files) => files,
        Err(e) => return error_response(e, "mix_create"),
    };
    let root = match std::fs::canonicalize(&state.root) {
        Ok(root) => root,
        Err(e) => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "root", &e.to_string())
        }
    };
    let mut kept = Vec::with_capacity(files.len());
    let mut skipped = Vec::new();
    for file in files {
        let inside = std::fs::canonicalize(root.join(&file)).is_ok_and(|target| {
            target.starts_with(&root) && std::fs::metadata(&target).is_ok_and(|m| m.is_file())
        });
        if inside {
            kept.push(file);
        } else if req.skip_missing {
            skipped.push(file);
        } else {
            return response::error(
                StatusCode::BAD_REQUEST,
                "bad_path",
                &format!("not a file in the served root: {file}"),
            );
        }
    }
    let files = kept;
    let store = match open_store() {
        Ok(store) => store,
        Err(resp) => return *resp,
    };
    match store.create(ctx_mix::Draft {
        id: String::new(),
        name: req.name,
        goal: req.goal,
        files,
        budget: limit,
    }) {
        Ok(mix) => response::json(StatusCode::CREATED, &CreateResponse { mix, skipped }),
        Err(e) => error_response(e, "mix_create"),
    }
}

// ---------------------------------------------------------------------------
// DELETE /api/mix/{id}
// ---------------------------------------------------------------------------

fn handle_delete_sync(id: String) -> Response {
    let store = match open_store() {
        Ok(store) => store,
        Err(resp) => return *resp,
    };
    match store.delete(&id) {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => error_response(e, "mix_delete"),
    }
}

// ---------------------------------------------------------------------------
// Dispatchers
// ---------------------------------------------------------------------------

fn method_not_allowed(message: &'static str, allow: &'static str) -> Response {
    let mut resp = response::error(
        StatusCode::METHOD_NOT_ALLOWED,
        "method_not_allowed",
        message,
    );
    resp.headers_mut().insert(
        axum::http::header::ALLOW,
        axum::http::HeaderValue::from_static(allow),
    );
    resp
}

/// `/api/mix`: GET → list, POST → create, anything else → the Go 405.
pub async fn handle_collection(
    method: Method,
    State(state): State<AppState>,
    headers: axum::http::HeaderMap,
    body: Bytes,
) -> Response {
    match method {
        Method::GET => handle_list(State(state)).await,
        Method::POST => handle_create(state, headers, body).await,
        _ => method_not_allowed("GET or POST only", "GET, POST"),
    }
}

/// `/api/mix/{id}`: GET → get, DELETE → delete, anything else → the Go 405.
pub async fn handle_route(
    method: Method,
    State(state): State<AppState>,
    uri: axum::http::Uri,
) -> Response {
    match method {
        Method::GET => handle_get(State(state), uri).await,
        Method::DELETE => {
            let id = id_from_uri(&uri);
            crate::blocking::run(move || handle_delete_sync(id)).await
        }
        _ => method_not_allowed("GET or DELETE only", "GET, DELETE"),
    }
}
