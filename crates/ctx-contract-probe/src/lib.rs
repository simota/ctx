//! Minimal smoke-probe crate. The single exported symbol below is the
//! ONLY thing the parity pipeline cares about: if `cargo build` succeeds
//! and `nm`/`objdump` can find `ctx_contract_probe` in the staticlib,
//! the cross-compile target is healthy enough to host the real
//! `ctx-contract` crate on it.
//!
//! Returning a constant `42` (rather than `0`) means a future smoke test
//! can call this from a Go cgo harness and assert on the value, catching
//! ABI mismatches that a plain link-check would miss.
//!
//! We intentionally do NOT use `#![no_std]` — the real ctx-contract
//! crate will depend on std (serde / regex / anyhow), so a probe that
//! also links against std is a more honest dry-run.

/// Probe entry point.
///
/// Marked `#[no_mangle]` so the symbol name survives Rust mangling and
/// `extern "C"` so the calling convention is the C ABI the eventual Go
/// caller will use via cgo.
#[no_mangle]
pub extern "C" fn ctx_contract_probe() -> i32 {
    42
}
