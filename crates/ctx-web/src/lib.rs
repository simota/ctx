//! crates/ctx-web — Rust port of `internal/web/`.
//!
//! Wave 2 web-server foundation: an axum HTTP server that mirrors the Go mux
//! (`/api/*` + `/raw/*` handlers, then an embedded SPA catch-all). The module
//! layout mirrors the Go package so byte-parity verification stays auditable
//! side-by-side:
//!
//!   safepath.rs   — `Resolve` (root-jail path resolution)
//!   response.rs   — `writeJSON` / `writeError` / `writeBadPath`
//!   embed.rs      — `DistHandler` (rust-embed of internal/web/dist + SPA fallback)
//!   router.rs     — `NewMuxWithBind` (route table)
//!   handlers/     — one module per `/api/*` (and `/raw/*`) route
//!   lib.rs        — `Server` (`New`/`Listen`/`Addr`/`Start`)
//!
//! PORTED routes: `/api/file`, `/raw/<path>`, SPA static. The other 21 routes,
//! the audit sink, and git/symbols-dependent fields are DEFERRED to later
//! Wave 2 slices (see crate README / ADR-0005).

use std::io;
use std::net::SocketAddr;

use tokio::net::TcpListener;

pub mod embed;
pub mod handlers;
pub mod response;
pub mod router;
pub mod safepath;

/// Shared per-request state (the Rust analogue of `web.API`). `audit` is
/// recorded for parity with the Go flag but the audit sink is DEFERRED.
///
/// `file_cache` memoizes `/api/file` response bodies across requests, keyed by
/// resolved path and validated by (mtime, size). It is a pure performance layer:
/// a hit returns the exact bytes a fresh computation would produce, so byte
/// parity with the Go server is preserved. Cleared implicitly when the process
/// exits; bounded to avoid unbounded growth during long browse sessions.
#[derive(Clone)]
pub struct AppState {
    pub root: String,
    pub bind: String,
    pub audit: bool,
    pub file_cache: handlers::file::FileCache,
}

/// Hosts the embedded browser UI + API — analogue of `web.Server`.
pub struct Server {
    root: String,
    bind: String,
    audit: bool,
    listener: Option<TcpListener>,
}

impl Server {
    /// `New(root, bind, audit)`. An empty bind defaults to an ephemeral
    /// loopback port (`127.0.0.1:0`), matching Go.
    pub fn new(root: impl Into<String>, bind: impl Into<String>, audit: bool) -> Self {
        let mut bind = bind.into();
        if bind.is_empty() {
            bind = "127.0.0.1:0".to_string();
        }
        Server {
            root: root.into(),
            bind,
            audit,
            listener: None,
        }
    }

    /// `Listen()` — bind eagerly so callers can read [`Server::addr`] before
    /// [`Server::serve`] blocks. Idempotent.
    pub async fn listen(&mut self) -> io::Result<()> {
        if self.listener.is_some() {
            return Ok(());
        }
        self.listener = Some(TcpListener::bind(&self.bind).await?);
        Ok(())
    }

    /// `Addr()` — the resolved bind address, or `None` before `listen`.
    pub fn addr(&self) -> Option<SocketAddr> {
        self.listener.as_ref().and_then(|l| l.local_addr().ok())
    }

    /// `Start(ctx)` — serve until `shutdown` resolves, then drain. Mirrors the
    /// Go graceful-shutdown contract.
    pub async fn serve<F>(mut self, shutdown: F) -> io::Result<()>
    where
        F: std::future::Future<Output = ()> + Send + 'static,
    {
        self.listen().await?;
        let listener = self.listener.take().expect("listener bound");
        let state = AppState {
            root: self.root,
            bind: self.bind,
            audit: self.audit,
            file_cache: handlers::file::FileCache::default(),
        };
        let app = router::build(state);
        axum::serve(listener, app)
            .with_graceful_shutdown(shutdown)
            .await
    }
}
