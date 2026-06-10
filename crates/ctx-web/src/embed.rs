//! SPA static serving — port of `internal/web/embed.go` `DistHandler`.
//!
//! The Svelte build under `internal/web/dist/` is embedded at compile time via
//! `rust-embed`. Behavior mirrors the Go handler:
//!   * a request whose cleaned path exists is served with its content-type;
//!   * anything else (including `/`) falls back to `index.html` so the SPA
//!     router can take over;
//!   * `index.html` (the fallback body) is served with
//!     `Content-Type: text/html; charset=utf-8` and `Cache-Control: no-store`,
//!     matching Go's `serveIndex`.

use axum::http::{header, StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use rust_embed::RustEmbed;

#[derive(RustEmbed)]
// Wave 4 (ADR-0005): the SPA assets live IN this crate (crates/ctx-web/dist),
// not under internal/web/dist, so the native web server survives Go deletion.
// (Path is relative to CARGO_MANIFEST_DIR = crates/ctx-web.)
#[folder = "dist"]
struct Dist;

/// Report whether the SPA shell (`index.html`) is embedded in the binary.
/// Used by `ctx doctor` to honestly verify browse-readiness without standing
/// up the server. Reuses the same `Dist` embed as the live handler, so the
/// answer reflects exactly what `ctx browse` would serve.
pub fn index_html_embedded() -> bool {
    Dist::get("index.html").is_some()
}

/// Serve an embedded asset, falling back to the SPA shell. `path` is the URL
/// path (leading slash optional); query/fragment already stripped by axum.
pub fn serve(uri: &Uri) -> Response {
    let clean = clean_path(uri.path());
    if clean.is_empty() {
        return serve_index();
    }
    match Dist::get(&clean) {
        Some(file) => {
            let ct = content_type_for(&clean);
            (
                StatusCode::OK,
                [(header::CONTENT_TYPE, ct)],
                file.data.into_owned(),
            )
                .into_response()
        }
        None => serve_index(),
    }
}

/// `serveIndex` — the SPA shell with no-store caching.
fn serve_index() -> Response {
    match Dist::get("index.html") {
        Some(file) => (
            StatusCode::OK,
            [
                (header::CONTENT_TYPE, "text/html; charset=utf-8"),
                (header::CACHE_CONTROL, "no-store"),
            ],
            file.data.into_owned(),
        )
            .into_response(),
        None => (StatusCode::INTERNAL_SERVER_ERROR, "index.html missing").into_response(),
    }
}

/// `strings.TrimPrefix(path.Clean("/"+p), "/")` — lexical clean, leading slash
/// stripped. Returns "" for the root.
fn clean_path(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                out.pop();
            }
            s => out.push(s),
        }
    }
    out.join("/")
}

/// Content-Type matching Go's `mime.TypeByExtension` for the extensions the
/// embedded build uses (`http.FileServer` derives the header this way).
fn content_type_for(path: &str) -> String {
    let ext = path.rsplit('.').next().unwrap_or("");
    match ext {
        "html" | "htm" => "text/html; charset=utf-8".into(),
        "css" => "text/css; charset=utf-8".into(),
        "js" | "mjs" => "text/javascript; charset=utf-8".into(),
        "json" => "application/json".into(),
        "svg" => "image/svg+xml".into(),
        "wasm" => "application/wasm".into(),
        "map" => "application/json".into(),
        _ => mime_guess::from_path(path)
            .first_raw()
            .map(|m| m.to_string())
            .unwrap_or_else(|| "application/octet-stream".into()),
    }
}
