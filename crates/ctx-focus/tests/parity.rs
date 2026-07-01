// crates/ctx-focus/tests/parity.rs
//
// Phase 4 parity integration tests for ctx-focus.
//
// For each fixture under tests/focus-fixtures/<name>/ we:
//   1. Load files.json (the pre-walked file list with symbols + lines).
//   2. Read anchor.txt.
//   3. Run resolve_anchor + expand (hops=1, hops=2) + pack.
//   4. Load the Go-side golden from tests/parity/focus-goldens/<name>/.
//   5. Assert parsed-JSON equality.
//
// Run:
//   cargo test --manifest-path crates/ctx-focus/Cargo.toml \
//              --test parity --features testing

#![cfg(feature = "testing")]

use pretty_assertions::assert_eq;
use serde_json::Value;

use ctx_focus::{
    expand, pack, resolve_anchor,
    testing::parity_fixture_builder::{fixtures_dir, goldens_dir},
    types::{ExpandOptions, FileInput},
};

fn load_files(fixture: &str) -> Vec<FileInput> {
    let path = fixtures_dir().join(fixture).join("files.json");
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_anchor(fixture: &str) -> String {
    let path = fixtures_dir().join(fixture).join("anchor.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .trim()
        .to_string()
}

fn load_golden(fixture: &str, name: &str) -> Value {
    let path = goldens_dir().join(fixture).join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden {}: {e}", path.display()))
}

fn run_parity(fixture: &str) {
    let files = load_files(fixture);
    let anchor_str = load_anchor(fixture);

    // resolve
    let anchor =
        resolve_anchor(&files, &anchor_str).unwrap_or_else(|e| panic!("resolve {fixture}: {e:?}"));
    let actual_resolve = serde_json::to_value(&anchor).unwrap();
    let expected_resolve = load_golden(fixture, "resolve");
    assert_eq!(
        actual_resolve, expected_resolve,
        "resolve parity mismatch for {fixture}"
    );

    // expand hops=1
    let e1 = expand(&files, &anchor, &ExpandOptions { hops: 1 });
    let actual_e1 = serde_json::to_value(&e1).unwrap();
    let expected_e1 = load_golden(fixture, "expand_hops1");
    assert_eq!(
        actual_e1, expected_e1,
        "expand_hops1 parity mismatch for {fixture}"
    );

    // expand hops=2
    let e2 = expand(&files, &anchor, &ExpandOptions { hops: 2 });
    let actual_e2 = serde_json::to_value(&e2).unwrap();
    let expected_e2 = load_golden(fixture, "expand_hops2");
    assert_eq!(
        actual_e2, expected_e2,
        "expand_hops2 parity mismatch for {fixture}"
    );

    // pack (envelope) — only check anchor + files (which are individually verified).
    let p = pack(&files, &anchor_str, &ExpandOptions { hops: 1 }).unwrap();
    let actual_p = serde_json::to_value(&p).unwrap();
    let expected_p = load_golden(fixture, "pack");
    assert_eq!(actual_p, expected_p, "pack parity mismatch for {fixture}");
}

#[test]
fn parity_small_repo() {
    run_parity("small_repo");
}

#[test]
fn parity_medium_repo() {
    run_parity("medium_repo");
}

#[test]
fn parity_large_repo() {
    run_parity("large_repo");
}
