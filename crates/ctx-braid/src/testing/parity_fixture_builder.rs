// crates/ctx-braid/src/testing/parity_fixture_builder.rs

#![cfg(any(test, feature = "testing"))]

use std::path::PathBuf;

pub fn fixtures_dir() -> PathBuf {
    repo_root().join("tests").join("braid-fixtures")
}

pub fn goldens_dir() -> PathBuf {
    repo_root()
        .join("tests")
        .join("parity")
        .join("braid-goldens")
}

fn repo_root() -> PathBuf {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| "crates/ctx-braid".to_string());
    PathBuf::from(manifest_dir).join("..").join("..")
}
