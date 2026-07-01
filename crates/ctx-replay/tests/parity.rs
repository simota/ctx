// crates/ctx-replay/tests/parity.rs
//
// Phase 3 parity integration tests for ctx-replay.
//
// For each fixture under tests/replay-fixtures/<name>/ we:
//   1. Load base.json + current.json (and optional opts.json) as Manifests.
//   2. Drive compute / compute_selection_diff in Rust.
//   3. Load the Go-side golden from tests/parity/replay-goldens/<name>/{diff,selection}.json.
//   4. Assert byte-exact JSON match.
//
// Run with:
//   cargo test --manifest-path crates/ctx-replay/Cargo.toml \
//              --test parity --features testing

#![cfg(feature = "testing")]

use pretty_assertions::assert_eq;
use serde_json::Value;

use ctx_replay::diff::{compute, compute_selection_diff, sort_selection_diff, DiffOptions};
use ctx_replay::testing::parity_fixture_builder::{fixtures_dir, goldens_dir};
use ctx_replay::types::Manifest;

fn load_manifest(fixture: &str, name: &str) -> Manifest {
    let path = fixtures_dir().join(fixture).join(format!("{name}.json"));
    let raw =
        std::fs::read(&path).unwrap_or_else(|e| panic!("read fixture {}: {e}", path.display()));
    serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse fixture {}: {e}", path.display()))
}

fn load_golden(fixture: &str, name: &str) -> Value {
    let path = goldens_dir().join(fixture).join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden {}: {e}", path.display()))
}

fn load_opts(fixture: &str) -> DiffOptions {
    let p = fixtures_dir().join(fixture).join("opts.json");
    if let Ok(raw) = std::fs::read(&p) {
        if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&raw) {
            return DiffOptions {
                strict: v.get("strict").and_then(|x| x.as_bool()).unwrap_or(false),
            };
        }
    }
    DiffOptions::default()
}

fn run_parity(fixture: &str) {
    let base = load_manifest(fixture, "base");
    let cur = load_manifest(fixture, "current");
    let opts = load_opts(fixture);

    let summary = compute(&base, &cur, opts);
    let actual = serde_json::to_value(&summary).unwrap();
    let expected = load_golden(fixture, "diff");
    assert_eq!(actual, expected, "diff parity mismatch for {fixture}");

    let mut sel = compute_selection_diff(&base, &cur);
    sort_selection_diff(&mut sel, "tier");
    let sel_actual = serde_json::to_value(&sel).unwrap();
    let sel_expected = load_golden(fixture, "selection");
    assert_eq!(
        sel_actual, sel_expected,
        "selection parity mismatch for {fixture}"
    );
}

#[test]
fn parity_single_snap() {
    run_parity("single_snap");
}

#[test]
fn parity_multi_snap_drift() {
    run_parity("multi_snap_drift");
}

#[test]
fn parity_scoring_change() {
    run_parity("scoring_change");
}
