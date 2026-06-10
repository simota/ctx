# Architecture Decision Records

This directory contains architecture decision records for the current
Rust implementation of `ctx`.

Historical migration records that described the former implementation
have been removed from the published docs so the documentation presents
one clear premise: `ctx` is a Rust CLI / TUI / MCP server.

## Current Posture

- The main binary is implemented in Rust under `crates/ctx-cli`.
- The web backend is implemented in Rust under `crates/ctx-web`.
- The MCP server is implemented in Rust under `crates/ctx-mcp`.
- The terminal UI is implemented in Rust under `crates/ctx-tui`.
- The Svelte/Vite frontend lives in `web/` and is embedded into the Rust
  web crate via `crates/ctx-web/dist/`.

Add future ADRs here when a decision changes architecture, supported
platforms, persistence format, public CLI behavior, or release process.
