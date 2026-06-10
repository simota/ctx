// crates/ctx-braid/src/lib.rs
//
// Rust port of internal/braid's PURE-COMPUTE layer (Tier 2 #1).
//
// Modules ported:
//   types.rs      — Strand, Config, PolicyKind, Allocation, StrandSelection,
//                   MergedFile, StrandReport, Result, Options
//   policy.rs     — PolicyKind enum + helpers (strandSubcommand, isSupportedSource)
//   shellquote.rs — POSIX-subset shell tokenisation (shellSplit / stripCtxAndSub)
//   config.rs     — TOML schema load + validate + SortedStrandNames
//   allocate.rs   — share-weighted budget distribution
//   merge.rs      — MergePaths (per-strand dedup + cross-strand policy)
//   format.rs     — formatting helpers (renderers for allocation report)
//   ffi.rs        — extern "C" stateless surface
//
// What is NOT ported (out of Tier 2 #1 scope — see brief):
//   exec.go    — orchestrator dispatching into focus/where/digest internal deps
//   Run()      — top-level orchestrator that needs exec.go
//
// The Go side keeps exec.go + Run() and calls into the Rust crate for the
// pure-compute helpers (Allocate, Load/Validate, MergePaths, ShellQuote).
//
// API SHAPE CHOICE (BATCH stateless — same as heatmap):
//
// `ctx braid` invokes each helper once per command (Load → Validate → Allocate
// → MergePaths once across strand selections). There is no session corpus to
// amortise across — the work is sub-µs per call on the Go side. Per the
// screening criterion in HEATMAP_BENCH_REPORT, this is expected to produce
// **evidence-only** results: parity ✅, perf likely regress, take the memory
// win if any.
//
// Justification of stateless over sessioned: identical reasoning to heatmap.
// 1 caller × 1 invocation × no persistent corpus → sticky-handle's open/close
// overhead is added on top of, not amortised against, per-call work.

pub mod allocate;
pub mod config;
pub mod ffi;
pub mod format;
pub mod merge;
pub mod policy;
pub mod shellquote;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::allocate::allocate;
pub use crate::config::{load, load_from_file, sorted_strand_names, validate, SCHEMA_VERSION};
pub use crate::format::{render_json, render_markdown, render_plain};
pub use crate::merge::merge_paths;
pub use crate::policy::{is_supported_source, strand_subcommand, SUPPORTED_SOURCES};
pub use crate::shellquote::{shell_split, strip_ctx_and_sub, ShellSplitError};
pub use crate::types::{
    Allocation, Config, MergedFile, PolicyKind, Result as BraidResult, Strand, StrandReport,
    StrandSelection,
};
