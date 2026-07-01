// crates/ctx-heatmap/tests/parity.rs
//
// Phase 4 Tier 1 #2 parity integration tests for ctx-heatmap.
//
// For each fixture under tests/heatmap-fixtures/<name>/ we:
//   1. Load metrics.json (the pre-walked file-metric list).
//   2. Run aggregate (3 axes), squarify, and all 3 renderers.
//   3. Compare against the Go-side goldens under
//      tests/parity/heatmap-goldens/<name>/.
//
// The aggregate / squarify goldens compare on serde_json::Value
// (structural). The render_ascii / render_plain goldens compare byte-
// exact (raw string). render_json compares structurally so trailing
// whitespace differences don't trip parity even if a future Go-side
// encoder change rejiggers indentation.
//
// Run:
//   cargo test --manifest-path crates/ctx-heatmap/Cargo.toml \
//              --test parity --features testing

#![cfg(feature = "testing")]

use pretty_assertions::assert_eq;
use serde_json::Value;

use ctx_heatmap::{
    aggregate, render_ascii, render_json, render_plain, render_svg, squarify,
    testing::parity_fixture_builder::{fixtures_dir, goldens_dir},
    AggregateOptions, AsciiOptions, FileMetric, JsonOptions, PlainOptions, SvgOptions,
};

const CANVAS_W: i64 = 80;
const CANVAS_H: i64 = 20;

fn load_metrics(fixture: &str) -> Vec<FileMetric> {
    let path = fixtures_dir().join(fixture).join("metrics.json");
    let raw = std::fs::read(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_slice(&raw).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn load_golden(fixture: &str, name: &str) -> String {
    let path = goldens_dir().join(fixture).join(format!("{name}.json"));
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("read golden {}: {e}", path.display()))
}

fn load_golden_value(fixture: &str, name: &str) -> Value {
    let raw = load_golden(fixture, name);
    serde_json::from_str(&raw).unwrap_or_else(|e| panic!("parse golden {fixture}/{name}: {e}"))
}

fn run_parity(fixture: &str) {
    let metrics = load_metrics(fixture);

    // Aggregate: tokens / files / symbols (depth=2, top=0).
    for axis in ["tokens", "files", "symbols"] {
        let buckets = aggregate(
            &metrics,
            &AggregateOptions {
                by: axis.into(),
                depth: 2,
                top: 0,
            },
        );
        let actual = serde_json::to_value(&buckets).unwrap();
        let expected = load_golden_value(fixture, &format!("aggregate_{axis}"));
        assert_eq!(
            actual, expected,
            "aggregate parity mismatch for {fixture} axis={axis}"
        );
    }

    // Use the tokens-axis buckets for squarify + renderers (matches Go
    // golden export — see cmd/heatmap-golden-export/main.go).
    let buckets = aggregate(
        &metrics,
        &AggregateOptions {
            by: "tokens".into(),
            depth: 2,
            top: 0,
        },
    );
    let rects = squarify(&buckets, CANVAS_W, CANVAS_H);
    let actual_sq = serde_json::to_value(&rects).unwrap();
    let expected_sq = load_golden_value(fixture, "squarify");
    assert_eq!(
        actual_sq, expected_sq,
        "squarify parity mismatch for {fixture}"
    );

    // render_ascii byte-exact.
    let ascii = render_ascii(
        &rects,
        &AsciiOptions {
            width: CANVAS_W,
            height: CANVAS_H,
            by: "tokens".into(),
            root: ".".into(),
            budget: 0,
        },
    );
    let expected_ascii = load_golden(fixture, "render_ascii");
    assert_eq!(
        ascii, expected_ascii,
        "render_ascii parity mismatch for {fixture}"
    );

    // render_json structural compare.
    let bytes = render_json(
        &rects,
        &JsonOptions {
            root: ".".into(),
            by: "tokens".into(),
            budget: None,
        },
    )
    .unwrap();
    let actual_json: Value = serde_json::from_slice(&bytes).unwrap();
    let expected_json = load_golden_value(fixture, "render_json");
    assert_eq!(
        actual_json, expected_json,
        "render_json parity mismatch for {fixture}"
    );

    // render_plain byte-exact.
    let plain = render_plain(
        &buckets,
        &PlainOptions {
            root: ".".into(),
            by: "tokens".into(),
            budget: 0,
        },
    );
    let expected_plain = load_golden(fixture, "render_plain");
    assert_eq!(
        plain, expected_plain,
        "render_plain parity mismatch for {fixture}"
    );

    // render_svg byte-exact against Go's RenderSVG output.
    let svg = render_svg(
        &rects,
        &SvgOptions {
            width: CANVAS_W,
            height: CANVAS_H,
            by: "tokens".into(),
            root: ".".into(),
            budget: 0,
        },
    );
    let expected_svg = load_golden(fixture, "render_svg");
    assert_eq!(
        svg, expected_svg,
        "render_svg parity mismatch for {fixture}"
    );
}

#[test]
fn parity_small_metrics() {
    run_parity("small_metrics");
}

#[test]
fn parity_medium_metrics() {
    run_parity("medium_metrics");
}

#[test]
fn parity_large_metrics() {
    run_parity("large_metrics");
}
