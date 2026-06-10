// crates/ctx-replay/src/testing/parity_fixture_builder.rs
//
// Tiny helper that resolves repo-relative paths used by the Phase 3
// parity tests. Mirrors crates/ctx-relations/src/testing/parity_fixture_builder.rs.

#![cfg(any(test, feature = "testing"))]

use std::path::PathBuf;

pub fn fixtures_dir() -> PathBuf {
    repo_root().join("tests").join("replay-fixtures")
}

pub fn goldens_dir() -> PathBuf {
    repo_root().join("tests").join("parity").join("replay-goldens")
}

fn repo_root() -> PathBuf {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .unwrap_or_else(|_| "crates/ctx-replay".to_string());
    PathBuf::from(manifest_dir).join("..").join("..")
}
