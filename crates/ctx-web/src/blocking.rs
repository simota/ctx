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

/// [`run`] for handlers that read and parse many files on the rayon pool.
/// Once `f` has built its response (and dropped its per-file data), the
/// memory freed by the pool's worker threads is handed back to the OS.
/// glibc keeps freed memory in each thread's arena, so without this every
/// such request left the server holding — and the next request growing
/// past — the high-water mark of the parallel build.
pub async fn run_parallel<F>(f: F) -> Response
where
    F: FnOnce() -> Response + Send + 'static,
{
    run(move || {
        let resp = f();
        release_freed_memory();
        resp
    })
    .await
}

/// Return already-freed heap memory to the OS (glibc `malloc_trim`, which
/// trims every arena). A no-op on other allocators. Call it after work that
/// allocated heavily on the rayon pool, not on cheap paths: it costs a few
/// milliseconds on a large heap.
pub fn release_freed_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only returns already-freed memory to the OS and is
    // thread-safe (it locks each arena it trims).
    unsafe {
        libc::malloc_trim(0);
    }
}
