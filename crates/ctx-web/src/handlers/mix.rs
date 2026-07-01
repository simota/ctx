//! `/api/mix` and `/api/mix/<id>` handlers — port of `internal/web/mix.go`
//! `handleMixCollection` (GET list) and `handleMixRoute` (GET by id).
//!
//! ## Ported (READ side only)
//! - GET `/api/mix`       → `handle_collection` (list)
//! - GET `/api/mix/{id}`  → `handle_route` (get by id)
//!
//! ## Mutation routes — DEFERRED
//! POST `/api/mix` (create) and DELETE `/api/mix/{id}` (delete) are not
//! implemented in Rust. They return the same 405 Method Not Allowed that Go
//! would return for unsupported methods on each path, so no wrong/stub data
//! is emitted. See `crates/ctx-web/DEFERRED_ROUTES.md` for the rationale.
//!
//! ## On-disk format
//! Each mix is stored as `<id>.mix.json` in the mixes directory:
//! ```json
//! {
//!   "schema_version": 1,
//!   "id": "aabbccdd11223344",
//!   "name": "Alpha Mix",
//!   "goal": "parity test alpha mix",    // omitempty
//!   "created": "2026-01-01T10:00:00Z",
//!   "files": ["hello.txt", "notes.md"],
//!   "budget": {}                         // always present, even when empty
//! }
//! ```
//! List is sorted newest-first by `created` (lexicographic on RFC3339 works
//! correctly for same-timezone timestamps).

use std::io;
use std::path::{Path, PathBuf};

use axum::extract::State;
use axum::http::{Method, StatusCode};
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::response;
use crate::AppState;

// ---------------------------------------------------------------------------
// Mix types (mirrors internal/mix/mix.go)
// ---------------------------------------------------------------------------

/// Budget is optional plan + token limit (mirrors `mix.Budget`).
/// Always serialized (even when empty), matching Go's `"budget":{}` output.
#[derive(Debug, Default, Deserialize, Serialize)]
pub struct Budget {
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub plan: String,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub limit: i64,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// Mix is one saved recipe artifact (mirrors `mix.Mix`).
/// Field order matches the Go struct for byte-identical JSON output.
#[derive(Debug, Deserialize, Serialize)]
pub struct Mix {
    pub schema_version: i64,
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "str::is_empty")]
    pub goal: String,
    /// RFC3339 timestamp string — kept as opaque String to round-trip faithfully.
    pub created: String,
    pub files: Vec<String>,
    /// Always serialized (even when empty) — matches Go's "budget":{} output.
    pub budget: Budget,
}

// ---------------------------------------------------------------------------
// Store errors
// ---------------------------------------------------------------------------

#[derive(Debug)]
enum StoreError {
    InvalidId(String),
    NotFound(String),
    Io(std::io::Error),
    Json(serde_json::Error),
}

impl std::fmt::Display for StoreError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StoreError::InvalidId(s) => write!(f, "mix: invalid recipe id: {s}"),
            StoreError::NotFound(id) => write!(f, "mix: recipe not found: {id}"),
            StoreError::Io(e) => write!(f, "mix: io error: {e}"),
            StoreError::Json(e) => write!(f, "mix: json: {e}"),
        }
    }
}

// ---------------------------------------------------------------------------
// ID validation (mirrors internal/mix/store.go validateID)
// ---------------------------------------------------------------------------

fn validate_id(id: &str) -> Result<(), StoreError> {
    if id.is_empty() {
        return Err(StoreError::InvalidId("empty id".to_string()));
    }
    for c in id.chars() {
        match c {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' => {}
            _ => {
                return Err(StoreError::InvalidId(format!(
                    "{id:?} contains disallowed character {c:?}"
                )));
            }
        }
    }
    if id == "." || id == ".." || id.starts_with('.') {
        return Err(StoreError::InvalidId(format!("{id:?}")));
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Store resolve (mirrors internal/mix/store.go Resolve)
// ---------------------------------------------------------------------------

/// Resolve picks the mixes store directory using Go's documented precedence:
///  1. $XDG_STATE_HOME/ctx/mixes/
///  2. $HOME/.local/state/ctx/mixes/  (if .local/state exists)
///  3. $HOME/.ctx/mixes/
///
/// Shared mode (project-local) is not used by the web handler (`Shared: false`).
fn resolve_store() -> io::Result<PathBuf> {
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        let xdg = xdg.trim();
        if !xdg.is_empty() {
            return Ok(Path::new(xdg).join("ctx").join("mixes"));
        }
    }
    let home = home_dir().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "mix: cannot resolve store directory",
        )
    })?;
    let state_dir = Path::new(&home).join(".local").join("state");
    if state_dir.is_dir() {
        return Ok(state_dir.join("ctx").join("mixes"));
    }
    Ok(Path::new(&home).join(".ctx").join("mixes"))
}

fn home_dir() -> Option<String> {
    std::env::var("HOME").ok().filter(|s| !s.is_empty())
}

// ---------------------------------------------------------------------------
// Store load / list (mirrors internal/mix/store.go Load + List)
// ---------------------------------------------------------------------------

fn store_load(dir: &Path, id: &str) -> Result<Mix, StoreError> {
    validate_id(id)?;
    let path = dir.join(format!("{id}.mix.json"));
    let data = std::fs::read(&path).map_err(|e| {
        if e.kind() == io::ErrorKind::NotFound {
            StoreError::NotFound(id.to_string())
        } else {
            StoreError::Io(e)
        }
    })?;
    serde_json::from_slice(&data).map_err(StoreError::Json)
}

fn store_list(dir: &Path) -> Result<Vec<Mix>, StoreError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(vec![]),
        Err(e) => return Err(StoreError::Io(e)),
    };
    let mut out = Vec::new();
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if entry.path().is_dir() {
            continue;
        }
        // strip_suffix removes exactly one ".mix.json" (trim_end_matches would
        // also eat repeated suffixes, e.g. "a.mix.json.mix.json" → "a").
        let Some(id) = name_str.strip_suffix(".mix.json") else {
            continue;
        };
        if let Ok(m) = store_load(dir, id) {
            out.push(m);
        }
    }
    // Sort newest-first by created (RFC3339 lexicographic = chronological).
    out.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(out)
}

// ---------------------------------------------------------------------------
// Response shapes (mirrors internal/web/mix.go MixSummary + MixListResponse)
// ---------------------------------------------------------------------------

/// MixSummary is one entry in GET /api/mix list responses.
/// Field order matches the Go struct declaration order for byte-identical output.
#[derive(Serialize)]
struct MixSummary {
    id: String,
    name: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    goal: String,
    created: String,
    file_count: usize,
}

/// MixListResponse is the envelope for GET /api/mix.
#[derive(Serialize)]
struct MixListResponse {
    mixes: Vec<MixSummary>,
}

// ---------------------------------------------------------------------------
// GET /api/mix (list) — mirrored from handleMixList
// ---------------------------------------------------------------------------

pub async fn handle_list(State(_state): State<AppState>) -> Response {
    let dir = match resolve_store() {
        Ok(d) => d,
        Err(_) => {
            return response::json(StatusCode::OK, &MixListResponse { mixes: vec![] });
        }
    };

    match std::fs::metadata(&dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return response::json(StatusCode::OK, &MixListResponse { mixes: vec![] });
        }
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "mix_store",
                &e.to_string(),
            );
        }
        Ok(m) if !m.is_dir() => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "mix_store",
                "store path is not a directory",
            );
        }
        Ok(_) => {}
    }

    let mixes = match store_list(&dir) {
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
// GET /api/mix/{id} — mirrored from handleMixGet
// ---------------------------------------------------------------------------

pub async fn handle_get(State(_state): State<AppState>, uri: axum::http::Uri) -> Response {
    let path = uri.path();
    // Strip the "/api/mix/" prefix to get the id.
    let id = path.trim_start_matches("/api/mix/");
    if id.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "id is required");
    }

    let dir = match resolve_store() {
        Ok(d) => d,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "mix_store",
                &e.to_string(),
            );
        }
    };

    match std::fs::metadata(&dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return response::error(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("mix not found: {id}"),
            );
        }
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "mix_store",
                &e.to_string(),
            );
        }
        Ok(m) if !m.is_dir() => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "mix_store",
                "store path is not a directory",
            );
        }
        Ok(_) => {}
    }

    match store_load(&dir, id) {
        Ok(m) => response::json(StatusCode::OK, &m),
        Err(e) => store_error_response(e),
    }
}

fn store_error_response(e: StoreError) -> Response {
    let msg = e.to_string();
    match e {
        StoreError::InvalidId(_) => response::error(StatusCode::BAD_REQUEST, "invalid_id", &msg),
        StoreError::NotFound(_) => response::error(StatusCode::NOT_FOUND, "not_found", &msg),
        _ => response::error(StatusCode::INTERNAL_SERVER_ERROR, "mix_get", &msg),
    }
}

// ---------------------------------------------------------------------------
// Collection dispatcher: GET /api/mix — mutations DEFERRED
// ---------------------------------------------------------------------------

/// Handles GET /api/mix (list).
///
/// `POST` (create) is DEFERRED, NOT method-not-allowed in Go. Go's
/// `handleMixCollection` routes POST → `handleMixCreate` which CREATES a mix
/// and returns 201. Rust cannot byte-match that response (the created mix
/// carries a `crypto/rand` `GenerateID` + a wall-clock `created` timestamp,
/// both non-deterministic) and the operation has write side-effects, so it is
/// genuinely deferred. We return a deliberate 405 sentinel for POST
/// ("rust engine: mix mutations not yet supported"). This is a KNOWN
/// DIVERGENCE from Go (Go=201-create, Rust=405) that BLOCKS cutover until mix
/// mutations are ported with a deterministic-ID strategy. See DEFERRED_ROUTES.md.
///
/// Other methods (PUT/PATCH/...) genuinely 405 in Go with the same
/// "GET or POST only" envelope + `Allow: GET, POST` — for THOSE the Rust 405
/// is true byte-parity (covered by the `mix_collection_put_rejected` case).
pub async fn handle_collection(method: Method, State(state): State<AppState>) -> Response {
    match method {
        Method::GET => handle_list(State(state)).await,
        _ => {
            let mut resp = response::error(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "GET or POST only",
            );
            resp.headers_mut().insert(
                axum::http::header::ALLOW,
                axum::http::HeaderValue::from_static("GET, POST"),
            );
            resp
        }
    }
}

// ---------------------------------------------------------------------------
// Route dispatcher: GET /api/mix/{id} — mutations DEFERRED
// ---------------------------------------------------------------------------

/// Handles GET /api/mix/{id} (get by id).
///
/// `DELETE` is DEFERRED, NOT method-not-allowed in Go. Go's `handleMixRoute`
/// routes DELETE → `handleMixDelete` which DELETES the mix and returns 204.
/// Rust returns a deliberate 405 sentinel for DELETE because the operation has
/// write side-effects (the parity harness shares a single pinned fixture store
/// between both servers, so a real delete would pollute the read cases). This
/// is a KNOWN DIVERGENCE from Go (Go=204-delete, Rust=405) that BLOCKS cutover
/// until mix mutations are ported with isolated write fixtures. See
/// DEFERRED_ROUTES.md.
///
/// Other methods (PUT/PATCH/...) genuinely 405 in Go with the same
/// "GET or DELETE only" envelope + `Allow: GET, DELETE` — for THOSE the Rust
/// 405 is true byte-parity (covered by the `mix_item_put_rejected` case).
pub async fn handle_route(
    method: Method,
    State(state): State<AppState>,
    uri: axum::http::Uri,
) -> Response {
    match method {
        Method::GET => handle_get(State(state), uri).await,
        _ => {
            let mut resp = response::error(
                StatusCode::METHOD_NOT_ALLOWED,
                "method_not_allowed",
                "GET or DELETE only",
            );
            resp.headers_mut().insert(
                axum::http::header::ALLOW,
                axum::http::HeaderValue::from_static("GET, DELETE"),
            );
            resp
        }
    }
}
