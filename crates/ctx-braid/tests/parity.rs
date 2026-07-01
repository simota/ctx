// crates/ctx-braid/tests/parity.rs
//
// Phase 4 Tier 2 #1 parity integration tests for ctx-braid.
//
// For each fixture under tests/braid-fixtures/<name>.toml we:
//   1. Load + Validate the config and compare against
//      tests/parity/braid-goldens/<name>/load_config.json
//   2. Run Allocate(cfg, 32000) and compare against allocate.json
//   3. Run MergePaths(<name>_selections.json) and compare against
//      merge_paths.json
//   4. Run ShellSplit on a fixed source and compare against shell_quote.json
//
// All comparisons use parsed serde_json::Value structural equality, so
// JSON whitespace differences between Go and Rust serialisers do not
// trip parity.
//
// Run:
//   cargo test --manifest-path crates/ctx-braid/Cargo.toml \
//              --test parity --features testing

#![cfg(feature = "testing")]

use pretty_assertions::assert_eq;
use serde_json::Value;

use ctx_braid::{
    allocate, load, merge_paths, shell_split,
    testing::parity_fixture_builder::{fixtures_dir, goldens_dir},
    StrandSelection,
};

const SAMPLE_SHELL_SOURCE: &str = "where 'handler' --regex 'router|Handler' --limit 50";

fn load_toml(fixture: &str) -> Vec<u8> {
    let path = fixtures_dir().join(format!("{fixture}.toml"));
    std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn load_selections(fixture: &str) -> Vec<StrandSelection> {
    let path = fixtures_dir().join(format!("{fixture}_selections.json"));
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_golden(fixture: &str, name: &str) -> Value {
    let path = goldens_dir().join(fixture).join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden {fixture}/{name}: {e}"))
}

fn run_parity(fixture: &str) {
    let toml_bytes = load_toml(fixture);

    // Step 1: load_config (Load + Validate).
    let cfg = load(&toml_bytes).unwrap_or_else(|e| panic!("load {fixture}: {e}"));
    let cfg_value = serde_json::to_value(&cfg).unwrap();
    let cfg_golden = load_golden(fixture, "load_config");
    assert_eq!(
        cfg_value, cfg_golden,
        "load_config parity mismatch for {fixture}"
    );

    // Step 2: allocate(cfg, 32000).
    let alloc_out = allocate(&cfg, 32000);
    let alloc_value = serde_json::json!({
        "allocations": alloc_out.allocations,
        "warning": alloc_out.warning,
    });
    let alloc_golden = load_golden(fixture, "allocate");
    assert_eq!(
        alloc_value, alloc_golden,
        "allocate parity mismatch for {fixture}"
    );

    // Step 3: merge_paths(<fixture>_selections.json).
    let sels = load_selections(fixture);
    let merged = merge_paths(&sels);
    let merged_value = serde_json::to_value(&merged).unwrap();
    let merged_golden = load_golden(fixture, "merge_paths");
    assert_eq!(
        merged_value, merged_golden,
        "merge_paths parity mismatch for {fixture}"
    );

    // Step 4: shell_split on a fixed source string.
    let tokens = shell_split(SAMPLE_SHELL_SOURCE).unwrap();
    let tokens_value = serde_json::to_value(&tokens).unwrap();
    let tokens_golden = load_golden(fixture, "shell_quote");
    assert_eq!(
        tokens_value, tokens_golden,
        "shell_quote parity mismatch for {fixture}"
    );
}

#[test]
fn parity_simple() {
    run_parity("simple");
}

#[test]
fn parity_multi_strand() {
    run_parity("multi_strand");
}

#[test]
fn parity_complex() {
    run_parity("complex");
}
