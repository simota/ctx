//! `GET /api/evidence` and `POST /api/evidence/verify` — port of
//! `internal/web/handlers.go` `handleEvidence` + `handleEvidenceVerify`.
//!
//! `handleEvidence` maps a file to replay snapshots that included it,
//! comparing the packed sha256 against current worktree bytes.
//! Reuses `ctx-replay` crate for store resolution + manifest listing.
//!
//! `handleEvidenceVerify` verifies a pasted LLM response against a pasted
//! ctx contract pack. Reuses `ctx-contract::verify` — same path as the
//! existing `/api/replay/verify` handler, but without loading a manifest.

use std::path::Path;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use ctx_replay::{open_store, resolve, ResolveOptions};

use crate::handlers::file::relative_to_root;
use crate::response;
use crate::safepath;
use crate::AppState;

const MAX_EVIDENCE_VERIFY_REQUEST_BYTES: usize = 3 << 20; // 3 MiB
const MAX_EVIDENCE_VERIFY_PACK_BYTES: usize = 2 << 20; // 2 MiB
const MAX_EVIDENCE_VERIFY_RESPONSE_BYTES: usize = 256 << 10; // 256 KiB

// ---------------------------------------------------------------------------
// /api/evidence
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct EvidenceParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    limit: Option<i32>,
}

/// Mirrors `web.EvidenceSnapshot`. Field order matches Go struct.
#[derive(Serialize)]
struct EvidenceSnapshot {
    id: String,
    /// RFC3339 string — passed through from TOML-stored manifest unchanged.
    created_at: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    goal: String,
    budget: i64,
    used: i64,
    format: String,
    status: String,
    path: String,
    pack_sha256: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    current_sha256: String,
    tokens: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    current_tokens: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    token_delta: i64,
    #[serde(skip_serializing_if = "str::is_empty")]
    relevance: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    score: i64,
    #[serde(skip_serializing_if = "str::is_empty")]
    reason: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    message: String,
}

/// Mirrors `web.EvidenceResponse`. Field order matches Go struct.
#[derive(Serialize)]
struct EvidenceResponse {
    path: String,
    status: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    store_path: String,
    total_snapshots: i32,
    snapshots: Vec<EvidenceSnapshot>,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

pub async fn handle(
    State(state): State<AppState>,
    params: Result<Query<EvidenceParams>, QueryRejection>,
) -> Response {
    let Query(params) = match params {
        Ok(q) => q,
        Err(e) => return response::bad_query(e),
    };
    // Store listing + file hashing/token counting are blocking work.
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: EvidenceParams) -> Response {
    if params.path.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }

    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let info = match std::fs::metadata(&target) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return response::stat_not_found(&target);
            }
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "stat", &e.to_string());
        }
    };
    if info.is_dir() {
        return response::error(StatusCode::BAD_REQUEST, "not_a_file", "path is a directory");
    }

    // Clamp limit: default 6, min 1, max 25.
    let limit = match params.limit {
        None => 6,
        Some(l) if l < 1 => 1,
        Some(l) if l > 25 => 25,
        Some(l) => l,
    };

    let rel_slash = relative_to_root(&state.root, &target);

    // Resolve store directory.
    let store_dir = match resolve(ResolveOptions {
        shared: false,
        root: state.root.clone(),
    }) {
        Ok(d) => d,
        Err(_) => {
            // Cannot resolve store → no-store response (no StorePath).
            return response::json(
                StatusCode::OK,
                &EvidenceResponse {
                    path: rel_slash,
                    status: "no-store".to_string(),
                    store_path: String::new(),
                    total_snapshots: 0,
                    snapshots: vec![],
                },
            );
        }
    };

    // Check store directory exists.
    match std::fs::metadata(&store_dir) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return response::json(
                StatusCode::OK,
                &EvidenceResponse {
                    path: rel_slash,
                    status: "no-store".to_string(),
                    store_path: store_dir,
                    total_snapshots: 0,
                    snapshots: vec![],
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

    let store = match open_store(&store_dir) {
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

    // Sort descending by created_at (RFC3339 lexicographic == chronological).
    manifests.sort_by(|a, b| b.created_at.cmp(&a.created_at));

    // Compute current file SHA256 and token count.
    let (current_sha, sha_err) = file_sha256(&target);
    let current_tokens = {
        let est = ctx_tokens::estimate_by_size(info.len() as i64);
        // Try exact tiktoken count; fall back to size-based estimate.
        match ctx_tokens::count_file(target.to_str().unwrap_or("")) {
            Ok(n) => n,
            Err(_) => est,
        }
    };

    let mut resp = EvidenceResponse {
        path: rel_slash.clone(),
        status: "no-evidence".to_string(),
        store_path: store_dir,
        total_snapshots: 0,
        snapshots: vec![],
    };

    for manifest in &manifests {
        // Find entry for this file in the manifest.
        let entry = match manifest_entry_for_path(manifest, &rel_slash) {
            Some(e) => e,
            None => continue,
        };

        resp.total_snapshots += 1;
        if resp.snapshots.len() as i32 >= limit {
            continue;
        }

        let mut snap = EvidenceSnapshot {
            id: manifest.id.clone(),
            created_at: manifest.created_at.clone(),
            goal: manifest.goal.clone(),
            budget: manifest.budget,
            used: manifest.used,
            format: manifest.format.clone(),
            status: "fresh".to_string(),
            path: rel_slash.clone(),
            pack_sha256: entry.sha256.clone(),
            current_sha256: current_sha.clone().unwrap_or_default(),
            tokens: entry.tokens,
            current_tokens,
            token_delta: current_tokens - entry.tokens,
            relevance: entry.relevance.clone(),
            score: entry.score,
            reason: entry.reason.clone(),
            message: String::new(),
        };

        match &sha_err {
            Some(err) => {
                snap.status = "missing".to_string();
                snap.message = err.clone();
            }
            None if snap.current_sha256 != entry.sha256 => {
                snap.status = "stale".to_string();
                snap.message = "worktree file differs from replay snapshot".to_string();
            }
            None => {}
        }

        resp.snapshots.push(snap);
    }

    if resp.total_snapshots > 0 {
        resp.status = "fresh".to_string();
        for s in &resp.snapshots {
            if s.status == "stale" || s.status == "missing" {
                resp.status = s.status.clone();
                break;
            }
        }
    }

    response::json(StatusCode::OK, &resp)
}

fn file_sha256(path: &Path) -> (Option<String>, Option<String>) {
    match std::fs::read(path) {
        Ok(data) => {
            let mut hasher = Sha256::new();
            hasher.update(&data);
            let sum = hasher.finalize();
            let hex = hex_encode(&sum);
            (Some(hex), None)
        }
        Err(e) => (None, Some(e.to_string())),
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

/// True when the body read failed because the length limit was exceeded
/// (axum wraps `http_body_util::LengthLimitError` somewhere in the chain).
fn is_length_limit_error(err: &axum::Error) -> bool {
    let mut source: Option<&(dyn std::error::Error + 'static)> = Some(err);
    while let Some(e) = source {
        if e.is::<http_body_util::LengthLimitError>() {
            return true;
        }
        source = e.source();
    }
    false
}

fn manifest_entry_for_path<'a>(
    manifest: &'a ctx_replay::types::Manifest,
    rel_slash: &str,
) -> Option<&'a ctx_replay::types::Entry> {
    manifest
        .entries
        .iter()
        .find(|e| e.path.replace('\\', "/") == rel_slash)
}

// ---------------------------------------------------------------------------
// /api/evidence/verify
// ---------------------------------------------------------------------------

/// POST /api/evidence/verify — verifies a pasted LLM response against a pasted
/// ctx contract pack (no manifest needed; pack embeds the contract boundary).
///
/// Mirrors Go `handleEvidenceVerify`:
///   - Method guard (POST only)
///   - Body size limit: 3 MiB total, 2 MiB pack, 256 KiB response
///   - JSON decode with DisallowUnknownFields equivalent (via `#[serde(deny_unknown_fields)]`)
///   - `contract.ParseFromPack` + `contract.Verify`
///   - `res.PackFile = "(pasted)"`
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

    // Read body up to the total request limit. Only an actual length-limit
    // overflow is a 413; any other read failure is a plain bad request.
    let body_bytes =
        match axum::body::to_bytes(req.into_body(), MAX_EVIDENCE_VERIFY_REQUEST_BYTES + 1).await {
            Ok(b) => b,
            Err(e) if is_length_limit_error(&e) => {
                return response::error(
                    StatusCode::PAYLOAD_TOO_LARGE,
                    "request_too_large",
                    "request exceeds 3 MiB",
                );
            }
            Err(e) => {
                return response::error(
                    StatusCode::BAD_REQUEST,
                    "bad_request",
                    &format!("read request body: {e}"),
                );
            }
        };

    // Go uses `dec.DisallowUnknownFields()`. We mimic with a deny_unknown_fields
    // wrapper struct parsed via serde_json.
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StrictReq {
        #[serde(default)]
        pack: String,
        #[serde(default)]
        response: String,
        #[serde(default)]
        check_worktree: bool,
        #[serde(default)]
        no_symbols: bool,
        #[serde(default)]
        strict: bool,
    }

    let req: StrictReq = match serde_json::from_slice(&body_bytes) {
        Ok(v) => v,
        Err(e) => {
            return response::error(
                StatusCode::BAD_REQUEST,
                "bad_request",
                &format!("invalid JSON: {e}"),
            );
        }
    };

    if req.pack.trim().is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "pack is required");
    }
    if req.response.trim().is_empty() {
        return response::error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "response is required",
        );
    }
    if req.pack.len() > MAX_EVIDENCE_VERIFY_PACK_BYTES {
        return response::error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "pack_too_large",
            "pack exceeds 2 MiB",
        );
    }
    if req.response.len() > MAX_EVIDENCE_VERIFY_RESPONSE_BYTES {
        return response::error(
            StatusCode::PAYLOAD_TOO_LARGE,
            "response_too_large",
            "response exceeds 256 KiB",
        );
    }

    let contract = match ctx_contract::embed::parse_from_pack(req.pack.as_bytes()) {
        Some(c) => c,
        None => {
            return response::error(
                StatusCode::BAD_REQUEST,
                "no_contract",
                "pack does not contain a ctx contract",
            );
        }
    };

    let opts = ctx_contract::types::VerifyOptions {
        strict: req.strict,
        no_symbols: req.no_symbols,
        worktree_root: if req.check_worktree {
            state.root.clone()
        } else {
            String::new()
        },
    };

    let mut res = ctx_contract::verify::verify(&contract, req.response.as_bytes(), &opts);
    res.pack_file = "(pasted)".to_string();
    // Rust Result already emits [] (not null) for empty vecs — matches Go's
    // normalizeContractResult nil→[] normalisation.

    response::json(StatusCode::OK, &res)
}
