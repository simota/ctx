//! `GET|HEAD /raw/<path>` — port of `internal/web/handlers.go` `handleRaw`.
//!
//! Serves raw file bytes with a locked-down security header set, refusing
//! secret-bearing paths and method != GET/HEAD. Content-Type matches Go's
//! `http.ServeFile` (extension table, then content sniff fallback).

use std::path::Path;

use axum::body::Body;
use axum::extract::State;
use axum::http::{header, Method, StatusCode, Uri};
use axum::response::{IntoResponse, Response};

use crate::response;
use crate::safepath;
use crate::AppState;

/// Filename / path globs that must never be served, mirroring
/// `walk.SecretDenyPatterns`. Implemented as a basename/suffix matcher; the
/// full gitignore-glob engine is DEFERRED (the patterns here are all
/// basename-anchored, so suffix + exact matching is faithful for them).
const SECRET_BASENAMES: &[&str] = &[
    ".env", ".envrc", "id_rsa", "id_rsa.pub", "id_dsa", "id_dsa.pub", "id_ecdsa",
    "id_ecdsa.pub", "id_ed25519", "id_ed25519.pub", "credentials.json", ".netrc",
    ".npmrc", ".pypirc",
];
const SECRET_SUFFIXES: &[&str] = &[
    ".env", ".pem", ".key", ".crt", ".p12", ".pfx", ".jks", ".keystore",
];
const SECRET_DIRS: &[&str] = &[".aws", ".gnupg", ".ssh"];

fn secret_deny(rel: &str) -> bool {
    let rel = rel.replace('\\', "/");
    let base = rel.rsplit('/').next().unwrap_or(&rel);
    if SECRET_BASENAMES.contains(&base) {
        return true;
    }
    if SECRET_SUFFIXES.iter().any(|s| base.ends_with(s)) {
        return true;
    }
    rel.split('/').any(|seg| SECRET_DIRS.contains(&seg))
}

pub async fn handle(State(state): State<AppState>, method: Method, uri: Uri) -> Response {
    if method != Method::GET && method != Method::HEAD {
        return (
            StatusCode::METHOD_NOT_ALLOWED,
            [(header::ALLOW, "GET, HEAD")],
            method_not_allowed_body(),
        )
            .into_response();
    }

    let rel = uri.path().strip_prefix("/raw/").unwrap_or("");
    if rel.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }
    let decoded = match percent_decode(rel) {
        Some(d) => d,
        None => return response::error(StatusCode::BAD_REQUEST, "bad_request", "invalid url encoding"),
    };
    if secret_deny(&decoded) {
        return response::error(
            StatusCode::FORBIDDEN,
            "secret_deny",
            "path matches secret deny list",
        );
    }
    let mut target = match safepath::resolve(&state.root, &decoded) {
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
        // Static-server convention: fall through to index.html if present.
        // Re-resolve through safepath so a symlinked index.html cannot escape
        // the root jail (the join below would otherwise skip that check).
        let idx = match safepath::resolve(&state.root, &format!("{}/index.html", decoded)) {
            Ok(t) => t,
            Err(e) => return response::bad_path(e),
        };
        match std::fs::metadata(&idx) {
            Ok(m) if !m.is_dir() => target = idx,
            _ => {
                return response::error(
                    StatusCode::BAD_REQUEST,
                    "not_a_file",
                    "path is a directory",
                )
            }
        }
    }

    let data = match std::fs::read(&target) {
        Ok(d) => d,
        Err(e) => return response::error(StatusCode::INTERNAL_SERVER_ERROR, "read_file", &e.to_string()),
    };
    let ct = serve_content_type(&target, &data);
    let len = data.len();
    let body = if method == Method::HEAD {
        Body::empty()
    } else {
        Body::from(data)
    };
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, ct),
            (
                header::CONTENT_SECURITY_POLICY,
                "sandbox allow-scripts".to_string(),
            ),
            (header::X_CONTENT_TYPE_OPTIONS, "nosniff".to_string()),
            (header::X_FRAME_OPTIONS, "SAMEORIGIN".to_string()),
            (header::REFERRER_POLICY, "no-referrer".to_string()),
            (header::CONTENT_LENGTH, len.to_string()),
        ],
        body,
    )
        .into_response()
}

fn method_not_allowed_body() -> Vec<u8> {
    let mut v = serde_json::to_vec(&serde_json::json!({
        "error": {"code": "method_not_allowed", "message": "GET or HEAD only"}
    }))
    .unwrap();
    v.push(b'\n');
    v
}

/// Content-Type matching `http.ServeFile`: extension table first, then a
/// content sniff for unknown extensions.
fn serve_content_type(path: &Path, data: &[u8]) -> String {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "txt" | "text" => "text/plain; charset=utf-8".into(),
        "html" | "htm" => "text/html; charset=utf-8".into(),
        "css" => "text/css; charset=utf-8".into(),
        "js" | "mjs" => "text/javascript; charset=utf-8".into(),
        "json" => "application/json".into(),
        "svg" => "image/svg+xml".into(),
        "md" | "markdown" => "text/markdown; charset=utf-8".into(),
        "" => sniff(data),
        _ => mime_guess::from_path(path)
            .first_raw()
            .map(|m| m.to_string())
            .unwrap_or_else(|| sniff(data)),
    }
}

/// Minimal port of Go's `http.DetectContentType` for the cases we exercise:
/// valid UTF-8 text → `text/plain; charset=utf-8`, else octet-stream.
fn sniff(data: &[u8]) -> String {
    // Go sniffs the first 512 bytes; for our binary fixture (ascii text with
    // no extension) it returns text/plain. Files WITH a known extension never
    // reach here. A no-extension file of printable bytes is treated as text.
    let head = &data[..data.len().min(512)];
    let looks_text = std::str::from_utf8(head)
        .map(|s| s.chars().all(|c| !c.is_control() || c == '\n' || c == '\r' || c == '\t'))
        .unwrap_or(false);
    if looks_text {
        "text/plain; charset=utf-8".into()
    } else {
        "application/octet-stream".into()
    }
}

/// Percent-decode a URL path segment string. Returns None on malformed escapes.
fn percent_decode(s: &str) -> Option<String> {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return None;
                }
                let hi = (bytes[i + 1] as char).to_digit(16)?;
                let lo = (bytes[i + 2] as char).to_digit(16)?;
                out.push((hi * 16 + lo) as u8);
                i += 3;
            }
            b => {
                out.push(b);
                i += 1;
            }
        }
    }
    String::from_utf8(out).ok()
}
