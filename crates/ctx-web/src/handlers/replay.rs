//! `/api/replay/*` handlers — port of `internal/web/handlers.go`
//! `handleReplayList`, `handleReplayShow`, `handleReplayDiff`, `handleReplayVerify`.
//!
//! All four routes reuse `ctx-replay` crate:
//!   list    → `ctx_replay::resolve` + `Store::list`
//!   show    → `ctx_replay::resolve` + `Store::load`
//!   diff    → `Store::load` + worktree walk + `ctx_replay::compute`
//!   verify  → `Store::load` + `ctx_contract::verify` (reuses the native
//!             contract-verification logic shipped in crates/ctx-contract)
//!
//! The Rust `Manifest.created_at` is an opaque String (not parsed into a
//! chrono/time value). This round-trips faithfully through Go's RFC3339 encoder:
//! both sides emit the same bytes when the fixture JSON uses RFC3339 with no
//! fractional seconds (e.g. "2026-01-01T10:00:00Z").

use std::io;
use std::path::Path;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use ctx_replay::{open_store, resolve, store::StoreError, types::Manifest, ResolveOptions};

use crate::response;
use crate::AppState;

// ---------------------------------------------------------------------------
// /api/replay/list
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ListParams {}

/// `ReplayListItem` mirrors `web.ReplayListItem`. Field order matches Go struct.
/// `goal` and `preset` are omitted when empty (`omitempty`).
#[derive(Serialize)]
struct ReplayListItem {
    id: String,
    created_at: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    goal: String,
    budget: i64,
    used: i64,
    format: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    preset: String,
    ctx_version: String,
    file_count: usize,
}

/// `ReplayListResponse` mirrors `web.ReplayListResponse`.
#[derive(Serialize)]
struct ReplayListResponse {
    snapshots: Vec<ReplayListItem>,
    store_path: String,
}

pub async fn handle_list(State(state): State<AppState>, Query(_): Query<ListParams>) -> Response {
    let dir = match resolve(ResolveOptions {
        shared: false,
        root: state.root.clone(),
    }) {
        Ok(d) => d,
        Err(_) => {
            return response::json(
                StatusCode::OK,
                &ReplayListResponse {
                    snapshots: vec![],
                    store_path: String::new(),
                },
            );
        }
    };

    // Missing directory → empty list (not an error).
    let stat = std::fs::metadata(&dir);
    match stat {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return response::json(
                StatusCode::OK,
                &ReplayListResponse {
                    snapshots: vec![],
                    store_path: dir,
                },
            );
        }
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            );
        }
        Ok(m) if !m.is_dir() => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                "store path is not a directory",
            );
        }
        Ok(_) => {}
    }

    let store = match open_store(&dir) {
        Ok(s) => s,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_list",
                &e.to_string(),
            );
        }
    };

    let mut manifests = match store.list() {
        Ok(m) => m,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_list",
                &e.to_string(),
            );
        }
    };

    // Sort descending by created_at (string lexicographic is the same as
    // chronological for RFC3339 timestamps at the same timezone offset).
    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    let items: Vec<ReplayListItem> = manifests
        .into_iter()
        .map(|m| ReplayListItem {
            id: m.id,
            created_at: m.created_at,
            goal: m.goal,
            budget: m.budget,
            used: m.used,
            format: m.format,
            preset: m.preset,
            ctx_version: m.ctx_version,
            file_count: m.entries.len(),
        })
        .collect();

    response::json(
        StatusCode::OK,
        &ReplayListResponse {
            snapshots: items,
            store_path: dir,
        },
    )
}

// ---------------------------------------------------------------------------
// /api/replay/show
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct ShowParams {
    #[serde(default)]
    id: String,
}

pub async fn handle_show(
    State(state): State<AppState>,
    Query(params): Query<ShowParams>,
) -> Response {
    if params.id.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "id is required");
    }

    let dir = match resolve(ResolveOptions {
        shared: false,
        root: state.root.clone(),
    }) {
        Ok(d) => d,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            );
        }
    };

    match std::fs::metadata(&dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return response::error(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("snapshot not found: {}", params.id),
            );
        }
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            );
        }
        Ok(m) if !m.is_dir() => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                "store path is not a directory",
            );
        }
        Ok(_) => {}
    }

    let store = match open_store(&dir) {
        Ok(s) => s,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            );
        }
    };

    match store.load(&params.id) {
        Ok(m) => response::json(StatusCode::OK, &m),
        Err(e) => store_load_error_response(e, "replay_show"),
    }
}

/// Map a `StoreError` from a load operation to the HTTP response that
/// mirrors Go's error-code mapping in `handleReplayShow` and siblings.
fn store_load_error_response(e: StoreError, internal_code: &str) -> Response {
    let msg = e.to_string();
    match e {
        StoreError::InvalidId(_) => response::error(StatusCode::BAD_REQUEST, "invalid_id", &msg),
        StoreError::NotFound(_) => response::error(StatusCode::NOT_FOUND, "not_found", &msg),
        _ => response::error(StatusCode::INTERNAL_SERVER_ERROR, internal_code, &msg),
    }
}

// ---------------------------------------------------------------------------
// /api/replay/diff
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DiffParams {
    #[serde(default)]
    id: String,
    #[serde(default)]
    strict: bool,
    #[serde(default)]
    limit: i64,
}

/// `ReplayDiffChange` mirrors `web.ReplayDiffChange`. Field order matches Go struct.
#[derive(Serialize)]
struct ReplayDiffChange {
    path: String,
    kind: String,
    tokens_delta: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    base_tokens: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    current_tokens: i64,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// `ReplayDiffResponse` mirrors `web.ReplayDiffResponse`. Field order matches Go struct.
#[derive(Serialize)]
struct ReplayDiffResponse {
    snapshot_id: String,
    snapshot_time: String,
    changes: Vec<ReplayDiffChange>,
    unchanged_count: i64,
    total_token_delta: i64,
    strict: bool,
    truncated: bool,
}

pub async fn handle_diff(
    State(state): State<AppState>,
    params: Result<Query<DiffParams>, QueryRejection>,
) -> Response {
    let Query(params) = match params {
        Ok(q) => q,
        Err(e) => return response::bad_query(e),
    };
    // The diff walks the whole worktree (read + SHA256 + token count per
    // file); keep it off the tokio workers.
    crate::blocking::run(move || handle_diff_sync(state, params)).await
}

fn handle_diff_sync(state: AppState, params: DiffParams) -> Response {
    if params.id.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "id is required");
    }

    let mut limit = params.limit;
    if limit < 1 {
        limit = 200;
    }
    if limit > 10000 {
        limit = 10000;
    }

    let dir = match resolve(ResolveOptions {
        shared: false,
        root: state.root.clone(),
    }) {
        Ok(d) => d,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            );
        }
    };

    match std::fs::metadata(&dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return response::error(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("snapshot not found: {}", params.id),
            );
        }
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            );
        }
        Ok(m) if !m.is_dir() => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                "store path is not a directory",
            );
        }
        Ok(_) => {}
    }

    let store = match open_store(&dir) {
        Ok(s) => s,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            );
        }
    };

    let base = match store.load(&params.id) {
        Ok(m) => m,
        Err(e) => return store_load_error_response(e, "replay_show"),
    };

    // Build current manifest by walking the root.
    let current_entries = match build_current_entries(&state.root) {
        Ok(e) => e,
        Err(e) => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "manifest_build", &e);
        }
    };
    let current = Manifest {
        entries: current_entries,
        ..Default::default()
    };

    let diff_opts = ctx_replay::DiffOptions {
        strict: params.strict,
    };
    let summary = ctx_replay::compute(&base, &current, diff_opts);

    let mut changes: Vec<ReplayDiffChange> = Vec::new();
    let mut unchanged = 0i64;
    let mut total_delta = 0i64;

    for ch in &summary.changes {
        use ctx_replay::types::ChangeKind;
        if ch.kind == ChangeKind::Unchanged {
            unchanged += 1;
            continue;
        }
        total_delta += ch.token_delta;
        changes.push(ReplayDiffChange {
            path: ch.path.replace('\\', "/"),
            kind: format!("{:?}", ch.kind).to_lowercase(),
            tokens_delta: ch.token_delta,
            base_tokens: ch.base_tokens,
            current_tokens: ch.cur_tokens,
        });
    }

    // Sort: modified < added < removed, then by abs(token_delta) desc, then path asc.
    changes.sort_by(|a, b| {
        let pa = kind_priority(&a.kind);
        let pb = kind_priority(&b.kind);
        if pa != pb {
            return pa.cmp(&pb);
        }
        let da = a.tokens_delta.abs();
        let db = b.tokens_delta.abs();
        if da != db {
            return db.cmp(&da);
        }
        a.path.cmp(&b.path)
    });

    let truncated = changes.len() as i64 > limit;
    if truncated {
        changes.truncate(limit as usize);
    }

    response::json(
        StatusCode::OK,
        &ReplayDiffResponse {
            snapshot_id: base.id,
            snapshot_time: base.created_at,
            changes,
            unchanged_count: unchanged,
            total_token_delta: total_delta,
            strict: params.strict,
            truncated,
        },
    )
}

fn kind_priority(kind: &str) -> i32 {
    match kind {
        "modified" => 0,
        "added" => 1,
        "removed" => 2,
        _ => 3,
    }
}

/// Build current manifest entries by walking `root`, hashing each file,
/// and estimating tokens. Mirrors Go's `buildCurrentEntries`.
fn build_current_entries(root: &str) -> Result<Vec<ctx_replay::types::Entry>, String> {
    let root_path = Path::new(root);
    let mut entries = Vec::new();
    collect_entries(root_path, root_path, &mut entries)?;
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn collect_entries(
    root: &Path,
    dir: &Path,
    out: &mut Vec<ctx_replay::types::Entry>,
) -> Result<(), String> {
    let read = std::fs::read_dir(dir).map_err(|e| e.to_string())?;
    for entry in read.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') || name_str == "node_modules" || name_str == "dist" {
            continue;
        }
        // DirEntry::file_type does not follow symlinks, so symlinked dirs are
        // never recursed into (cyclic links would loop; absolute links would
        // leak outside root).
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            collect_entries(root, &path, out)?;
            continue;
        }
        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        let data = match std::fs::read(&path) {
            Ok(d) => d,
            Err(_) => continue,
        };

        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(&data);
        let sha256 = format!("{:x}", hasher.finalize());

        // Mirror Go buildCurrentEntries: exact tiktoken count via CountFile,
        // falling back to a size-based estimate when the count fails.
        let path_str = path.to_string_lossy();
        let tokens = ctx_tokens::count_file(&path_str)
            .unwrap_or_else(|_| ctx_tokens::estimate_by_size(data.len() as i64));

        out.push(ctx_replay::types::Entry {
            path: rel,
            sha256,
            tokens,
            relevance: String::new(),
            score: 0,
            reason: String::new(),
        });
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// /api/replay/verify (POST)
// ---------------------------------------------------------------------------

const MAX_VERIFY_RESPONSE_BYTES: usize = 256 << 10; // matches Go maxEvidenceVerifyResponseBytes

/// `ReplayVerifyRequest` mirrors `web.ReplayVerifyRequest`. Field names match
/// the Go struct JSON tags. `check_worktree` and `strict` are optional.
#[derive(Deserialize)]
struct ReplayVerifyRequest {
    #[serde(default)]
    id: String,
    #[serde(default)]
    response: String,
    #[serde(default)]
    check_worktree: bool,
    #[serde(default)]
    strict: bool,
}

/// POST /api/replay/verify — verifies an LLM response against a replay
/// snapshot's manifest treated as the evidence boundary.
///
/// Reuses `ctx_contract::verify` (the same native logic the CLI's
/// `contract verify` command uses). The replay manifest is converted to a
/// `ctx_contract::Contract` (every entry becomes a whole-file reference with
/// line range [1, 1<<30]) and the response is cross-checked against it.
/// Mirrors Go `handleReplayVerify` request parsing + JSON response envelope.
pub async fn handle_verify(State(state): State<AppState>, req: axum::extract::Request) -> Response {
    use axum::http::Method;
    if req.method() != Method::POST {
        let mut resp = response::error(
            StatusCode::METHOD_NOT_ALLOWED,
            "method_not_allowed",
            "POST only",
        );
        resp.headers_mut().insert(
            axum::http::header::ALLOW,
            axum::http::HeaderValue::from_static("POST"),
        );
        return resp;
    }

    // Read the full body (with size limit matching Go's MaxBytesReader of
    // maxEvidenceVerifyResponseBytes + 4096).
    let max_bytes = MAX_VERIFY_RESPONSE_BYTES + 4096;
    let body_bytes = match axum::body::to_bytes(req.into_body(), max_bytes + 1).await {
        Ok(b) => b,
        Err(_) => {
            return response::error(
                StatusCode::PAYLOAD_TOO_LARGE,
                "response_too_large",
                "response exceeds 256 KiB",
            );
        }
    };

    let req: ReplayVerifyRequest = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return response::error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                &format!("invalid JSON: {e}"),
            );
        }
    };

    let id = req.id.trim().to_string();
    if id.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "id is required");
    }
    if req.response.trim().is_empty() {
        return response::error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "response is required",
        );
    }
    if req.response.len() > MAX_VERIFY_RESPONSE_BYTES {
        return response::error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "response_too_large",
            "response exceeds 256 KiB",
        );
    }

    // Load the snapshot manifest (maps store errors to the Go status codes).
    let manifest = match load_replay_manifest(&state.root, &id) {
        Ok(m) => m,
        Err(resp) => return resp,
    };

    let contract = contract_from_replay_manifest(&manifest);
    let opts = ctx_contract::VerifyOptions {
        strict: req.strict,
        no_symbols: true,
        worktree_root: if req.check_worktree {
            state.root.clone()
        } else {
            String::new()
        },
    };
    let mut res = ctx_contract::verify::verify(&contract, req.response.as_bytes(), &opts);
    res.pack_file = format!("replay:{}", manifest.id);
    // Rust's Result already emits [] (not null) for empty collections, which
    // matches Go's normalizeContractResult nil→[] normalisation.

    response::json(StatusCode::OK, &res)
}

/// Resolve + open the store and load the manifest with `id`. On any failure
/// returns the byte-parity error Response that Go's `loadReplayManifestForAPI`
/// would produce.
fn load_replay_manifest(root: &str, id: &str) -> std::result::Result<Manifest, Response> {
    let dir = match resolve(ResolveOptions {
        shared: false,
        root: root.to_string(),
    }) {
        Ok(d) => d,
        Err(e) => {
            return Err(response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            ));
        }
    };

    match std::fs::metadata(&dir) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => {
            return Err(response::error(
                StatusCode::NOT_FOUND,
                "not_found",
                &format!("snapshot not found: {id}"),
            ));
        }
        Err(e) => {
            return Err(response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            ));
        }
        Ok(m) if !m.is_dir() => {
            return Err(response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                "store path is not a directory",
            ));
        }
        Ok(_) => {}
    }

    let store = match open_store(&dir) {
        Ok(s) => s,
        Err(e) => {
            return Err(response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "replay_store",
                &e.to_string(),
            ));
        }
    };

    store
        .load(id)
        .map_err(|e| store_load_error_response(e, "replay_show"))
}

/// Mirror of Go `contractFromReplayManifest`: every manifest entry with a
/// non-blank path becomes a whole-file `contract.File` (line range [1, 1<<30]).
fn contract_from_replay_manifest(m: &Manifest) -> ctx_contract::Contract {
    let files: Vec<ctx_contract::File> = m
        .entries
        .iter()
        .filter(|e| !e.path.trim().is_empty())
        .map(|e| ctx_contract::File {
            path: e.path.replace('\\', "/"),
            line_start: 1,
            line_end: 1 << 30,
            sha256: e.sha256.clone(),
            ..Default::default()
        })
        .collect();
    ctx_contract::Contract {
        schema_version: ctx_contract::SCHEMA_VERSION,
        // Go formats m.CreatedAt.UTC() as RFC3339; the manifest's created_at is
        // already an RFC3339 string. `created` does not appear in the verify
        // Result, so its exact value does not affect response parity.
        created: m.created_at.clone(),
        files,
    }
}
