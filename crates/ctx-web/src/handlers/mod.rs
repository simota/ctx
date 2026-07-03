//! Per-route handler modules. Each `/api/*` (or `/raw/*`) route is one module
//! exposing an axum handler `fn`, wired in `router.rs`. Adding a route is:
//!   1. a new module here,
//!   2. one `.route(...)` line in `router::build`.

pub mod budget;
pub mod dir;
pub mod evidence;
pub mod file;
pub mod git;
pub mod limit;
pub mod mix;
pub mod raw;
pub mod relations;
pub mod replay;
pub mod role;
pub mod roots;
pub mod symbols;
pub mod tests;
pub mod tree;
pub mod where_;
