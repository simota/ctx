// crates/ctx-relations/tests/parity.rs
//
// Phase 2 parity integration tests for ctx-relations.
//
// For each fixture under tests/relations-fixtures/<name>/ we:
//   1. Drive the Rust implementation (build / build_cached) against
//      the on-disk fixture directory.
//   2. Load the Go-side golden from
//      tests/parity/relations-goldens/<name>/{build,build_cached}.json.
//   3. Assert byte-exact (parsed-JSON) match.
//
// Run with:
//   cargo test --manifest-path crates/ctx-relations/Cargo.toml \
//              --test parity --features testing

#![cfg(feature = "testing")]

use pretty_assertions::assert_eq;
use serde_json::Value;

use ctx_relations::build::{build, build_cached, invalidate_cache};
use ctx_relations::testing::parity_fixture_builder::{fixtures_dir, goldens_dir};

fn load_golden(fixture: &str, name: &str) -> Value {
    let path = goldens_dir().join(fixture).join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden {}: {e}", path.display()))
}

fn run_parity(fixture: &str) {
    let fx_dir = fixtures_dir().join(fixture);
    assert!(fx_dir.exists(), "fixture missing: {}", fx_dir.display());

    let idx = build(&fx_dir.to_string_lossy())
        .unwrap_or_else(|e| panic!("build({}): {e}", fx_dir.display()));
    let actual = serde_json::to_value(&idx).unwrap();
    let expected = load_golden(fixture, "build");
    assert_eq!(actual, expected, "parity mismatch (build) for {fixture}");

    // Invalidate so each parity run starts from a known state.
    invalidate_cache(&fx_dir.to_string_lossy());
    let cidx = build_cached(&fx_dir.to_string_lossy())
        .unwrap_or_else(|e| panic!("build_cached({}): {e}", fx_dir.display()));
    let cactual = serde_json::to_value(&cidx).unwrap();
    let cexpected = load_golden(fixture, "build_cached");
    assert_eq!(
        cactual, cexpected,
        "parity mismatch (build_cached) for {fixture}"
    );
}

#[test]
fn parity_go_project() {
    run_parity("go_project");
}

#[test]
fn parity_jsts_project() {
    run_parity("jsts_project");
}

#[test]
fn parity_jvm_project() {
    run_parity("jvm_project");
}

#[test]
fn parity_php_project() {
    run_parity("php_project");
}

#[test]
fn parity_swift_project() {
    run_parity("swift_project");
}

#[test]
fn parity_py_project() {
    run_parity("py_project");
}

#[test]
fn parity_mixed_project() {
    run_parity("mixed_project");
}
