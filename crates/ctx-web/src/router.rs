//! Router assembly — the Rust analogue of `internal/web/routes.go`
//! `NewMuxWithBind`. Mounts `/api/*` + `/raw/*` handlers, then the embedded
//! SPA as the catch-all fallback.
//!
//! ADDING A ROUTE: add a `pub mod` in `handlers/mod.rs`, then a single
//! `.route("/api/<name>", get(handlers::<name>::handle))` line below. The
//! parity harness picks it up by adding one entry to its route matrix.

use axum::routing::{any, get};
use axum::Router;

use crate::embed;
use crate::handlers;
use crate::AppState;

/// Build the full application router for the given state.
pub fn build(state: AppState) -> Router {
    Router::new()
        // --- /api/* JSON routes (PORTED) ---
        .route("/api/budget", get(handlers::budget::handle))
        .route("/api/tests", get(handlers::tests::handle))
        .route("/api/tree", get(handlers::tree::handle))
        .route("/api/dir", get(handlers::dir::handle))
        .route("/api/file", get(handlers::file::handle))
        .route("/api/where", get(handlers::where_::handle))
        .route("/api/relations", get(handlers::relations::handle))
        .route("/api/roots", get(handlers::roots::handle))
        .route("/api/symbols", get(handlers::symbols::handle_symbols))
        .route("/api/definition", get(handlers::symbols::handle_definition))
        .route("/api/evidence", get(handlers::evidence::handle))
        .route(
            "/api/evidence/verify",
            get(handlers::evidence::handle_verify).post(handlers::evidence::handle_verify),
        )
        .route("/api/git/diff", get(handlers::git::handle_diff))
        .route("/api/git/log", get(handlers::git::handle_repo_log))
        .route("/api/git/file-log", get(handlers::git::handle_file_log))
        .route(
            "/api/git/commit-files",
            get(handlers::git::handle_commit_files),
        )
        .route(
            "/api/git/commit-diff",
            get(handlers::git::handle_commit_diff),
        )
        .route("/api/replay/list", get(handlers::replay::handle_list))
        .route("/api/replay/show", get(handlers::replay::handle_show))
        .route("/api/replay/diff", get(handlers::replay::handle_diff))
        .route(
            "/api/replay/verify",
            get(handlers::replay::handle_verify).post(handlers::replay::handle_verify),
        )
        // --- /api/mix/* (GET list + GET by id; POST/DELETE deferred — see DEFERRED_ROUTES.md) ---
        .route("/api/mix", any(handlers::mix::handle_collection))
        .route("/api/mix/:id", any(handlers::mix::handle_route))
        // --- /raw/* static byte serving ---
        .route(
            "/raw/",
            get(handlers::raw::handle).head(handlers::raw::handle),
        )
        .route(
            "/raw/*path",
            get(handlers::raw::handle).head(handlers::raw::handle),
        )
        // --- SPA catch-all: embedded dist/ with index.html fallback ---
        .fallback(spa_fallback)
        .with_state(state)
}

async fn spa_fallback(uri: axum::http::Uri) -> axum::response::Response {
    embed::serve(&uri)
}
