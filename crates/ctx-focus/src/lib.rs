// crates/ctx-focus/src/lib.rs
//
// Rust port of internal/focus/. Phase 4 first LOOKUP_HEAVY shipper —
// validates the sticky-handle session pattern (proven on ctx-where:
// 11.27-18.72× net end-to-end) against a BFS-over-symbol-graph workload.
//
//   types.rs      — AnchorKind / Anchor / Candidate / FileInfo /
//                   ExpandOptions / FileInput / SymbolInfo. JSON tags
//                   match the Go side byte-for-byte.
//   resolve.rs    — ResolveAnchor: exact symbol → basename → path.
//   expand.rs     — Expand: anchor-origin → same-dir → basename-prefix
//                   → name-match, with hops=2 second-round BFS.
//   pack.rs       — Pack orchestrator: ResolveAnchor + Expand returning
//                   a single result envelope (one-shot stateless API).
//   ffi.rs        — extern "C" surface used by internal/focus/rustbridge.
//
// The walk + symbol extraction live on the Go side (same architecture as
// ctx-where) so the Rust hot path focuses on what sticky-handle wins on:
// the per-query resolution + BFS expansion against an already-loaded
// corpus.

pub mod expand;
pub mod ffi;
pub mod pack;
pub mod resolve;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::expand::expand;
pub use crate::pack::pack;
pub use crate::resolve::resolve_anchor;
pub use crate::types::{
    Anchor, AnchorKind, Candidate, ErrAmbiguous, ExpandOptions, FileInfo, FileInput, PackResult,
    SymbolInfo,
};
