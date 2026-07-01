// crates/ctx-relations/src/lib.rs
//
// Rust port of internal/relations/. The module layout mirrors the Go
// package's file split so byte-parity verification stays trivially
// auditable side-by-side.
//
// Phase 2 (REGEX_HEAVY + IO): we mirror Phase 1's (ctx-scan) layout.
//
//   types.rs            — port of the Index/Edge structures used by relations.
//   patterns.rs         — per-language regex patterns (Lazy<Vec<RegexEntry>>).
//   walk.rs             — minimal repo walker matching internal/walk.DefaultOptions
//                         semantics (gitignore + default extra ignores).
//   languages/*         — per-language extractors (go, jsts, py, jvm, php, swift,
//                         scripted for vue/svelte). Mirrors internal/relations/*.go.
//   build.rs (Rust impl)— orchestrates the full graph build. See build.rs.
//   cache.rs            — BuildCached + InvalidateCache. Replicates Go cache
//                         invalidation semantics (size, mtime fingerprint).
//   ffi.rs              — extern "C" surface used by internal/relations/rustbridge.

pub mod build;
pub mod cache;
pub mod ffi;
pub mod languages;
pub mod patterns;
pub mod session;
pub mod types;
pub mod walk;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::build::{build, build_cached, invalidate_cache, supported};
pub use crate::types::{Edges, Index};
