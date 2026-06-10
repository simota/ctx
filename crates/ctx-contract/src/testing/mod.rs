// testing/mod.rs
//
// Hand-off note (Phase 3): this module is reachable from the crate
// root only if `src/lib.rs` declares `pub mod testing;`. The Phase 3
// stub at `src/lib.rs` does so; if Codex's branch overwrites lib.rs
// without re-adding the declaration, the parity tests will fail with
// "unresolved module" — that is the intended signal.

pub mod parity_fixture_builder;
