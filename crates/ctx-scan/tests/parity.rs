// crates/ctx-scan/tests/parity.rs
//
// Phase 1 parity integration tests for ctx-scan.
//
// For each fixture under tests/scan-fixtures/ we:
//   1. Load the fixture body from tests/scan-fixtures/<fixture>.<ext>
//   2. Drive the Rust implementation (scan_file_with_options) with the
//      same fixture file used by the Go exporter
//      (cmd/scan-golden-export).
//   3. Load the Go-side golden from tests/parity/scan-goldens/<fixture>/scan.json
//   4. Assert byte-exact match (canonical JSON shape).
//
// Run with:
//   cargo test --manifest-path crates/ctx-scan/Cargo.toml \
//              --test parity --features testing

#![cfg(feature = "testing")]

use std::path::PathBuf;

use pretty_assertions::assert_eq;
use serde_json::Value;

use ctx_scan::scan::scan_file_with_options;
use ctx_scan::testing::parity_fixture_builder::{fixtures_dir, goldens_dir};
use ctx_scan::types::{Options, Warning};

fn load_golden(fixture: &str, name: &str) -> Value {
    let path = goldens_dir().join(fixture).join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    serde_json::from_str(&raw)
        .unwrap_or_else(|e| panic!("parse golden {}: {e}", path.display()))
}

/// Replace the absolute test-fixture path inside every Warning.path
/// with the fixture-relative path the exporter uses ("<stem>.<ext>")
/// so the parity comparison is independent of the developer's
/// home-directory layout.
fn normalise(mut warnings: Vec<Warning>, expected_path: &str) -> Vec<Warning> {
    for w in &mut warnings {
        w.path = expected_path.to_string();
    }
    warnings
}

fn load_fixture(stem: &str) -> (PathBuf, String) {
    for ext in &[".txt", ".env", ".go", ".md"] {
        let p = fixtures_dir().join(format!("{stem}{ext}"));
        if p.exists() {
            return (p, ext.to_string());
        }
    }
    panic!("no fixture for stem '{stem}' under {}", fixtures_dir().display());
}

fn parity_for(fixture: &str) {
    let (path, ext) = load_fixture(fixture);
    let opts = Options {
        enable_entropy: true,
        ..Default::default()
    };
    let warnings = scan_file_with_options(&path.to_string_lossy(), &opts)
        .unwrap_or_else(|e| panic!("scan_file_with_options({}): {e}", path.display()));
    let stem_with_ext = format!("{fixture}{ext}");
    let normalised = normalise(warnings, &stem_with_ext);

    let actual = serde_json::to_value(&normalised).unwrap();
    let expected = load_golden(fixture, "scan");
    assert_eq!(actual, expected, "parity mismatch for fixture {fixture}");
}

#[test]
fn parity_clean_code() {
    parity_for("clean_code");
}

#[test]
fn parity_api_keys() {
    parity_for("api_keys");
}

#[test]
fn parity_high_entropy() {
    parity_for("high_entropy");
}

#[test]
fn parity_env_config() {
    parity_for("env_config");
}
