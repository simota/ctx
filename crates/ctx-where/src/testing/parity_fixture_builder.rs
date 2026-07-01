// crates/ctx-where/src/testing/parity_fixture_builder.rs
//
// Resolves repo-relative paths used by the Phase 3 parity tests.

#![cfg(any(test, feature = "testing"))]

use std::path::PathBuf;

pub fn fixtures_dir() -> PathBuf {
    repo_root().join("tests").join("where-fixtures")
}

pub fn goldens_dir() -> PathBuf {
    repo_root()
        .join("tests")
        .join("parity")
        .join("where-goldens")
}

fn repo_root() -> PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| "crates/ctx-where".to_string());
    PathBuf::from(manifest_dir).join("..").join("..")
}
