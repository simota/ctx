// crates/ctx-replay/src/lib.rs
//
// Rust port of internal/replay/. The module layout mirrors the Go
// package's file split so byte-parity verification stays trivially
// auditable side-by-side.
//
// Phase 3 (JSON_HEAVY): we mirror Phase 2's (ctx-relations) layout.
//
//   types.rs    — port of Manifest/Entry/Skipped/BuildInput/DiffSummary types.
//   manifest.rs — port of BuildManifest, hashFile, ctxVersion.
//   store.rs    — port of Store directory backend (Save/Load/List/Delete).
//   diff.rs     — port of Compute, ComputeSelectionDiff, Sort/Write helpers.
//   prune.rs    — port of ParseDuration, Prune.
//   ffi.rs      — extern "C" surface used by internal/replay/rustbridge.

pub mod types;
pub mod manifest;
pub mod store;
pub mod diff;
pub mod prune;
pub mod session;
pub mod ffi;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::diff::{compute, compute_selection_diff, sort_selection_diff, DiffOptions};
pub use crate::manifest::{build_manifest, BuildInput, EntryInput, SkippedInput};
pub use crate::prune::{parse_duration, prune};
pub use crate::store::{open_store, resolve, ResolveOptions, Store, StoreError};
pub use crate::types::{
    ChangeKind, DiffSummary, Entry, FileChange, Manifest, SelectionCategory, SelectionChange,
    SelectionCounts, SelectionGroups, SelectionSummary, Skipped,
};
