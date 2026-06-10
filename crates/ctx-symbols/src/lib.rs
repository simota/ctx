// crates/ctx-symbols/src/lib.rs
//
// Rust port of internal/symbols's PURE-COMPUTE post-processing layer
// (Tier 2 #5).
//
// Modules ported:
//   types.rs                — Symbol, FileSymbols, Hit, LookupArgs,
//                             APIRange, APIRenderRequest
//   apionly.rs              — render_api (post-tree-sitter line merge
//                             + trim + concat)
//   lookup/mod.rs           — resolve (stateless) + sort_hits helpers
//   lookup/session.rs       — LookupSession (sticky-handle over
//                             pre-extracted corpus)
//   ffi.rs                  — extern "C" mixed stateless + sessioned
//
// What is NOT ported (out of Tier 2 #5 scope — see brief):
//   extractor.go            — tree-sitter parsing (already CGO into C
//                             tree-sitter). Double-cgo Go → Rust → C
//                             inflates complexity for no measured win.
//   apionly.go AST walk     — collectAPIRanges / isDeclaration /
//                             isPublic / headerRanges all need
//                             *sitter.Node. Go computes the (lines,
//                             ranges) pair and hands them to Rust for
//                             rendering.
//
// API SHAPE CHOICE
// ================
//   apionly  — STATELESS. Per-file render is sub-µs Go work; cgo floor
//              (~10 µs) dominates a single call. Expected
//              EVIDENCE-ONLY (memory delta only).
//   lookup   — STATELESS + SESSIONED. The sole Go caller (web
//              /api/definition) currently does a fresh walk+extract on
//              every request. A pool can cache the corpus per root and
//              answer N name resolutions cheaply; sessioning helps if
//              the corpus is pre-cached on the Go side and reused
//              across requests. Single-request verdict likely
//              evidence-only; multi-request (or batched query) is the
//              ship lane.
//
// L1-L4 SUMMARY (applied honestly per the brief)
// ==============================================
//   L1 size:   apionly per-file render ≤ a few KB of lines; lookup
//              corpus can grow to MBs (full repo of symbols).
//   L2 cgo floor: ~10 µs/call estimated.
//   L3 hot path: apionly = String alloc + Vec push (NOT
//                regex/byte-scan — replicates echo's "HashMap/String"
//                profile, ships memory bucket at best).
//                lookup = HashMap probe + small Vec sort (sub-µs in
//                Rust).
//   L4 per-query: apionly per-call → likely sub-cgo-floor; lookup
//                 per-query likely sub-cgo-floor. Verdict expected
//                 EVIDENCE-ONLY for single-call paths; lookup
//                 sessioned wins by amortising walk+extract Go-side
//                 (NOT by per-query Rust speedup).

pub mod apionly;
pub mod extract;
pub mod ffi;
pub mod lookup;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::apionly::{is_comment_line, leading_comment_start, render_api};
pub use crate::extract::extract;
pub use crate::lookup::{resolve, session::LookupSession, sort_hits};
pub use crate::types::{APIRange, APIRenderRequest, FileSymbols, Hit, LookupArgs, Symbol};
