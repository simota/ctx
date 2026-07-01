// crates/ctx-scan/src/lib.rs
//
// Rust port of internal/scan/. The module layout mirrors the Go
// package's file split so byte-parity verification stays trivially
// auditable side-by-side.
//
// Phase 1 (REGEX_HEAVY): we mirror the pioneer's (ctx-contract) layout.
// `types.rs`     — port of model.Warning + scan.Options (the Rust slice
//                  of internal/model used by scan).
// `entropy.rs`   — port of entropy.go (Shannon entropy).
// `patterns.rs`  — port of the secretPatterns table (secret.go) PLUS
//                  the env_assignment pattern (env_patterns.go).
// `scan.rs`      — port of ScanFile / ScanFileWithOptions / ScanFiles /
//                  ScanFilesWithOptions and the small helpers
//                  (allowlist, preview, entropy candidates).
// `ffi.rs`       — extern "C" surface used by internal/scan/rustbridge.

pub mod entropy;
pub mod ffi;
pub mod patterns;
pub mod scan;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::scan::{scan_file, scan_file_with_options, scan_files, scan_files_with_options};
pub use crate::types::{Options, Warning};
