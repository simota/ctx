//! Offload synchronous handler bodies to the blocking thread pool.
//!
//! Handlers that walk the filesystem recursively, parse with tree-sitter, or
//! shell out to `git` would otherwise stall a tokio worker thread and starve
//! concurrent requests (the SPA fires several API calls in parallel).

use axum::http::StatusCode;
use axum::response::Response;

use crate::response;

/// Run `f` on the blocking pool and return its response. A panic inside `f`
/// propagates unchanged so behavior matches running the body inline.
pub async fn run<F>(f: F) -> Response
where
    F: FnOnce() -> Response + Send + 'static,
{
    match tokio::task::spawn_blocking(f).await {
        Ok(resp) => resp,
        Err(e) => match e.try_into_panic() {
            Ok(p) => std::panic::resume_unwind(p),
            // Cancellation only happens at runtime shutdown, where the
            // response is never delivered anyway.
            Err(_) => response::error(StatusCode::INTERNAL_SERVER_ERROR, "internal", "cancelled"),
        },
    }
}
