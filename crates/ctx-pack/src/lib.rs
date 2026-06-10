// crates/ctx-pack/src/lib.rs
//
// Rust port of internal/pack/'s pure-compute layer. Phase 4 Tier 2 #2.
// The internal/pack Go package is the LARGEST single module in the
// codebase (~3.1 kLOC src+test). The orchestrator (Pack /
// PackWithResult) calls into walk + scan + contract + where and stays
// on the Go side; what we port here are the SELF-CONTAINED pure-
// compute helpers the planner uses:
//
//   relevance.rs (SESSIONED) — scoreRelevance + its supporting
//     helpers. The pack planner walks the file list and scores every
//     file against the same goal; that means hundreds-to-thousands of
//     calls per `ctx pack` invocation sharing identical state
//     (extracted goal keywords + role boost table). The sticky-handle
//     pattern from ctx-where applies cleanly.
//
//   diff.rs (STATELESS) — pack-vs-pack diff rendering. Fires once per
//     CLI invocation; no amortisation surface.
//
//   redact.rs (STATELESS) — redact_lines marker replacement. Per-file
//     call that takes the warning list as input; the secret scan
//     itself lives in ctx-scan, so the only thing the port covers is
//     the bytewise line replacement.
//
//   from_where.rs (STATELESS) — newline / JSON parse from `ctx where`
//     output. Fires once per invocation.
//
//   preset.rs (STATELESS) — preset definitions and Options patching
//     for "blog" / "review" / "debug" / "llm". Tiny pure data.
//
// The Go orchestrator (pack.go's PackWithResult) is intentionally
// NOT ported — it depends on walk / scan / contract / hooks. Routing
// pure helpers through this crate exercises the scope-split pattern
// from braid.
//
// Module layout
// =============
//   types.rs               — shared serde shapes (FileInput, Symbol,
//                            ScoreBreakdown, RelevanceResult, ...)
//   relevance/mod.rs       — score_file + extract_goal_keywords +
//                            role_boost helpers
//   relevance/session.rs   — sessioned scoring API (open/score/rank/
//                            close) that amortises corpus state and
//                            re-uses the keyword cache.
//   diff.rs                — pack-vs-pack diff rendering
//   redact.rs              — redact_lines + apply_redaction
//   from_where.rs          — `ctx where` output parser
//   preset.rs              — Options patcher for named presets
//   ffi.rs                 — mixed FFI: sessioned relevance handles
//                            plus stateless batch functions

pub mod assemble;
pub mod diff;
pub mod ffi;
pub mod from_where;
pub mod preset;
pub mod redact;
pub mod relevance;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::relevance::session::RelevanceSession;
pub use crate::relevance::{extract_goal_keywords, role_boost, score_relevance, RelevanceContext};
pub use crate::types::{
    DiffEntry, DiffOptions, FileInput, MetadataInput, PackOptions, PresetName, RelevanceResult,
    ScoreBreakdown, SymbolInput, WarningInput, WhereResult,
};
