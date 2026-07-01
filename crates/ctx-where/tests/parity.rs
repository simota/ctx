// crates/ctx-where/tests/parity.rs
//
// Phase 3 parity integration tests for ctx-where.
//
// For each fixture under tests/where-fixtures/<name>/ we:
//   1. Load files.json (the pre-walked file list with symbols + lines).
//   2. Read query.txt.
//   3. Run search_with_options (and suggest_similar for the suggest
//      fixtures).
//   4. Load the Go-side golden from tests/parity/where-goldens/<name>/.
//   5. Assert byte-exact (parsed-JSON) match.
//
// Run with:
//   cargo test --manifest-path crates/ctx-where/Cargo.toml \
//              --test parity --features testing

#![cfg(feature = "testing")]

use pretty_assertions::assert_eq;
use serde_json::Value;

use ctx_where::search::{search_with_options, suggest_similar, FileInput, Options};
use ctx_where::testing::parity_fixture_builder::{fixtures_dir, goldens_dir};

fn load_files(fixture: &str) -> Vec<FileInput> {
    let path = fixtures_dir().join(fixture).join("files.json");
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_query(fixture: &str) -> String {
    let path = fixtures_dir().join(fixture).join("query.txt");
    std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
        .trim()
        .to_string()
}

fn load_opts(fixture: &str) -> Options {
    let p = fixtures_dir().join(fixture).join("opts.json");
    if let Ok(raw) = std::fs::read(&p) {
        if let Ok(v) = serde_json::from_slice::<Options>(&raw) {
            return v;
        }
    }
    Options::default()
}

fn load_golden(fixture: &str, name: &str) -> Value {
    let path = goldens_dir().join(fixture).join(format!("{name}.json"));
    let raw = std::fs::read_to_string(&path)
        .unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()));
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden {}: {e}", path.display()))
}

fn run_parity(fixture: &str) {
    let files = load_files(fixture);
    let query = load_query(fixture);
    let opts = load_opts(fixture);

    let results = search_with_options(&files, &query, &opts);
    let actual = serde_json::to_value(&results).unwrap();
    let expected = load_golden(fixture, "search");
    assert_eq!(actual, expected, "search parity mismatch for {fixture}");

    // Suggest fixture path exists only for fixtures that ship a
    // suggest.json golden.
    let suggest_golden = goldens_dir().join(fixture).join("suggest.json");
    if suggest_golden.exists() {
        let suggestions = suggest_similar(&files, &query, 5);
        let actual_s = serde_json::to_value(&suggestions).unwrap();
        let expected_s = load_golden(fixture, "suggest");
        assert_eq!(
            actual_s, expected_s,
            "suggest parity mismatch for {fixture}"
        );
    }
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
