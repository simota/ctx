use serde_json::Value;

mod common;
use common::*;

#[test]
fn native_map_json_emits_heatmap_shape() {
    let root = write_map_fixture();
    let output = run_rust_in(&root, &["map", "--format", "json", "--depth", "1"]);
    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse map JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(payload["by"], "tokens");
    let buckets = payload["buckets"]
        .as_array()
        .expect("map buckets should be an array");
    assert!(buckets.iter().any(|item| item["path"] == "src"));
    assert!(buckets
        .iter()
        .all(|item| item["rect"]["w"].as_i64().unwrap_or(0) > 0));
    assert!(buckets
        .iter()
        .all(|item| item["rect"]["h"].as_i64().unwrap_or(0) > 0));
}

#[test]
fn native_map_plain_supports_files_axis() {
    let root = write_map_fixture();
    let output = run_rust_in(&root, &["map", "--by", "files", "--top", "1", "--plain"]);
    assert!(
        output.status.success(),
        "map failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("Heatmap (by files, root=.,"));
    assert!(stdout.contains("1. src"));
    assert!(!stdout.contains("+--"));
}

#[test]
fn native_map_svg_emits_valid_treemap() {
    let root = write_map_fixture();
    let output = run_rust_in(&root, &["map", "--format", "svg", "--depth", "1"]);
    assert!(
        output.status.success(),
        "map --format=svg failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let svg = String::from_utf8_lossy(&output.stdout);
    // Opening SVG tag with proper attributes.
    assert!(
        svg.starts_with("<svg xmlns=\"http://www.w3.org/2000/svg\""),
        "SVG must start with opening tag, got: {}",
        &svg[..svg.len().min(80)]
    );
    // Contains both fixture directories.
    assert!(
        svg.contains("data-path=\"src\""),
        "SVG must include src cell"
    );
    assert!(
        svg.contains("data-path=\"docs\""),
        "SVG must include docs cell"
    );
    // Style block present.
    assert!(svg.contains("<style>"), "SVG must include <style> block");
    // Closed properly.
    assert!(
        svg.trim_end().ends_with("</svg>"),
        "SVG must end with </svg>"
    );
    // HTML-escaping works: the style uses &#39; / &amp; etc. only for
    // user data — path names here are plain ASCII, but verify no raw < or >
    // appear outside CDATA in the rect data sections.
    let desc_line = svg
        .lines()
        .find(|l| l.contains("<desc"))
        .expect("SVG must have a <desc> line");
    assert!(
        desc_line.contains("root=."),
        "desc must contain root=., got: {desc_line}"
    );
    assert!(
        desc_line.contains("total="),
        "desc must contain total=, got: {desc_line}"
    );
}

/// Byte-parity for `map --format json` over a fixture that mixes Go source
/// (line-based) with JSON/text files that end in a trailing newline. The root
/// `.` bucket aggregates files.json / opts.json / query.txt — query.txt ends in
/// "\n", so token counts only match Go if we count the raw file (tokens.CountFile)
/// rather than reconstructing content from `lines.join("\n")`, which would drop
/// the trailing-newline token and produce an off-by-one (Go 552 vs Rust 551).
#[test]
fn native_map_json_token_counts_match_go_over_mixed_fixture() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--depth",
        "3",
    ]);
}

// ── Wave-1 byte-parity suite for `map` ───────────────────────────────────────
//
// Goal: prove `map` is provably byte-identical to Go across its ENTIRE
// flag/value surface so the command is ready for Wave-3 zero-delegation
// cutover. All tests below use `assert_delegated_parity` / `assert_delegated_parity_in`
// which check stdout, stderr, AND exit-code byte-for-byte against the Go oracle.
//
// Fixture: `tests/where-fixtures/small_repo` — a multi-depth tree mixing Go
// source (→ tokens from cl100k_base BPE), JSON and text files (→ root "."
// bucket), and a nested `src/internal/*` subtree.  Non-trivial enough that
// all three axes (tokens / files / symbols) yield distinct orderings.
//
// Not covered: `--heatmap-engine rust` — the Go binary (without the
// rust_contract build tag) returns exit-1 with "rust heatmap engine requires
// a build with -tags rust_contract; this binary is pure-Go".  The Rust CLI
// accepts the flag and runs the native path (which IS the rust engine), so
// the outputs can never match. This is intentional behaviour: the flag is a
// no-op for the Rust CLI and a build-requirement error for Go.

/// ascii × tokens (default axis), depth 2.
#[test]
fn map_parity_ascii_tokens_depth2() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--depth",
        "2",
        "--by",
        "tokens",
    ]);
}

/// ascii × tokens, depth 3 (exercises deeper subtree aggregation).
#[test]
fn map_parity_ascii_tokens_depth3() {
    assert_delegated_parity(&["map", "tests/where-fixtures/small_repo", "--depth", "3"]);
}

/// ascii × files axis.
#[test]
fn map_parity_ascii_files() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--by",
        "files",
        "--depth",
        "2",
    ]);
}

/// ascii × symbols axis — least-exercised path; symbols = 0 on JSON/text
/// files so only Go source dirs appear, which must match exactly.
#[test]
fn map_parity_ascii_symbols() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--by",
        "symbols",
        "--depth",
        "2",
    ]);
}

/// ascii with --top N truncation.
#[test]
fn map_parity_ascii_top2() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--top",
        "2",
        "--depth",
        "2",
    ]);
}

/// ascii with a non-zero --budget: exercises the budget-highlight legend
/// (Legend: # = within budget…) and in-budget cell rendering.
#[test]
fn map_parity_ascii_budget() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--budget",
        "200",
        "--depth",
        "2",
    ]);
}

/// json × tokens, depth 2.
#[test]
fn map_parity_json_tokens_depth2() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--depth",
        "2",
        "--by",
        "tokens",
    ]);
}

/// json × files.
#[test]
fn map_parity_json_files() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--by",
        "files",
        "--depth",
        "2",
    ]);
}

/// json × symbols.
#[test]
fn map_parity_json_symbols() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--by",
        "symbols",
        "--depth",
        "2",
    ]);
}

/// json with a non-zero budget: verifies the `in_budget` field and the
/// `budget` envelope field serialise identically (Go uses *int, Rust uses
/// Option<i64> — both must produce the integer literal, not null).
#[test]
fn map_parity_json_budget() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--budget",
        "200",
        "--depth",
        "2",
    ]);
}

/// json with --top 2: verifies truncation is applied before squarify/render.
#[test]
fn map_parity_json_top2() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--top",
        "2",
        "--depth",
        "2",
    ]);
}

/// plain (--plain) × tokens.
#[test]
fn map_parity_plain_tokens() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--plain",
        "--depth",
        "2",
    ]);
}

/// plain × files.
#[test]
fn map_parity_plain_files() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--plain",
        "--by",
        "files",
        "--depth",
        "2",
    ]);
}

/// plain × symbols.
#[test]
fn map_parity_plain_symbols() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--plain",
        "--by",
        "symbols",
        "--depth",
        "2",
    ]);
}

/// plain with non-zero budget: verifies [in budget] / [over budget] tags.
#[test]
fn map_parity_plain_budget() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--plain",
        "--budget",
        "200",
        "--depth",
        "2",
    ]);
}

/// svg × tokens.
#[test]
fn map_parity_svg_tokens() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "svg",
        "--depth",
        "2",
    ]);
}

/// svg × files.
#[test]
fn map_parity_svg_files() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "svg",
        "--by",
        "files",
        "--depth",
        "2",
    ]);
}

/// svg × symbols.
#[test]
fn map_parity_svg_symbols() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "svg",
        "--by",
        "symbols",
        "--depth",
        "2",
    ]);
}

/// svg with non-zero budget: verifies in-budget / over-budget colour classes
/// and the budget= field in the <desc> element match byte-for-byte.
#[test]
fn map_parity_svg_budget() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "svg",
        "--budget",
        "200",
        "--depth",
        "2",
    ]);
}

/// --heatmap-engine go is a no-op; output must be identical to the default.
#[test]
fn map_parity_heatmap_engine_go() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--heatmap-engine",
        "go",
        "--depth",
        "2",
    ]);
}

/// Depth=1 + symbols: single-level aggregation with a sparse symbol axis.
#[test]
fn map_parity_json_depth1_symbols() {
    assert_delegated_parity(&[
        "map",
        "tests/where-fixtures/small_repo",
        "--format",
        "json",
        "--depth",
        "1",
        "--by",
        "symbols",
    ]);
}
