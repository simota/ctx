// crates/ctx-scan/src/testing/parity_fixture_builder.rs
//
// Tiny helper that resolves repo-relative paths used by the Phase 1
// parity tests. Mirrors the convention in
// crates/ctx-contract/src/testing/parity_fixture_builder.rs but trims
// out the unused clock seam (scan has no time dependency).

#![cfg(any(test, feature = "testing"))]

use std::path::PathBuf;

/// Returns the directory holding the synthetic scan fixtures shared
/// between the Go-side exporter and the Rust parity harness.
pub fn fixtures_dir() -> PathBuf {
    repo_root().join("tests").join("scan-fixtures")
}

/// Returns the directory holding the Go-generated parity goldens.
pub fn goldens_dir() -> PathBuf {
    repo_root().join("tests").join("parity").join("scan-goldens")
}

fn repo_root() -> PathBuf {
    // CARGO_MANIFEST_DIR is `<repo>/crates/ctx-scan` at build time.
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "crates/ctx-scan".to_string());
    PathBuf::from(manifest_dir).join("..").join("..")
}
