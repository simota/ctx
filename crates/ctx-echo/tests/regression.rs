// crates/ctx-echo/tests/regression.rs
//
// Regression tests for ctx-echo. Each test exercises one of the 5
// echo_test.go cases via the Rust evaluate() entry point.

use ctx_echo::chunk::chunk_pack;
use ctx_echo::evaluate;
use ctx_echo::tokenize::tokenize;
use ctx_echo::types::{ChunkStrategy, Options};

const SAMPLE_PACK: &str = include_str!("../../../internal/echo/testdata/sample_pack.md");

#[test]
fn r01_tokenize_splits_camel_and_snake() {
    // From TestTokenizeSplitsCamelAndSnake.
    let got = tokenize("TestBurst burst_limit rate-limit the API");
    let allowed: &[&str] = &["test", "burst", "limit", "rate", "api"];
    assert!(!got.is_empty(), "expected non-empty tokens");
    for tok in got.iter() {
        assert!(
            allowed.contains(&tok.as_str()),
            "unexpected token {tok:?} (got={got:?})"
        );
        assert_ne!(tok, "the", "stop word leaked");
    }
}

#[test]
fn r02_evaluate_empty_pack() {
    // From TestEvaluateEmptyPack.
    let opts = Options {
        goal: "anything".into(),
        ..Default::default()
    };
    let res = evaluate("empty.md", "", &opts);
    assert_eq!(res.chunks_total, 0);
    assert_eq!(res.exit_code, 0);
}

#[test]
fn r03_evaluate_single_chunk() {
    // From TestEvaluateSingleChunk.
    let body =
        "## File contents\n\n### foo/bar.go\n\n```go\npackage bar\n\nfunc BurstHandler() {}\n```\n";
    let opts = Options {
        goal: "burst handler".into(),
        top: 5,
        ..Default::default()
    };
    let res = evaluate("inline", body, &opts);
    assert!(!res.top.is_empty(), "expected at least one scored chunk");
    assert_eq!(res.top[0].path, "foo/bar.go");
    assert!(
        res.top[0].matches.get("burst").unwrap_or(&0) > &0
            || res.top[0].matches.get("handler").unwrap_or(&0) > &0,
        "expected burst or handler match, got {:?}",
        res.top[0].matches
    );
}

#[test]
fn r04_evaluate_multi_chunk_ranking() {
    // From TestEvaluateMultiChunkRanking.
    let opts = Options {
        goal: "rate limit burst".into(),
        top: 5,
        ..Default::default()
    };
    let res = evaluate("sample_pack.md", SAMPLE_PACK, &opts);
    assert!(
        res.top.len() >= 2,
        "expected ≥2 top entries, got {} (chunks={})",
        res.top.len(),
        res.chunks_total
    );
    assert!(
        res.top[0].path.contains("limit"),
        "expected top path to mention limit, got {:?}",
        res.top[0].path
    );
    assert!(res.coverage_score > 0.0);
}

#[test]
fn r05_evaluate_threshold_fail() {
    // From TestEvaluateThresholdFail.
    let opts = Options {
        goal: "non-existent-token-xyz123".into(),
        top: 5,
        threshold: 0.99,
        ..Default::default()
    };
    let res = evaluate("sample_pack.md", SAMPLE_PACK, &opts);
    assert_eq!(res.exit_code, 1);
}

#[test]
fn r06_chunk_symbol_strategy() {
    // From TestChunkSymbolStrategy.
    let chunks = chunk_pack(SAMPLE_PACK, ChunkStrategy::Symbol, 0);
    assert!(
        chunks.len() >= 2,
        "expected ≥2 symbol chunks, got {}",
        chunks.len()
    );
}

#[test]
fn r07_chunk_fixed_strategy() {
    // From TestChunkFixedStrategy.
    let chunks = chunk_pack(SAMPLE_PACK, ChunkStrategy::Fixed, 3);
    assert!(
        chunks.len() >= 3,
        "expected ≥3 fixed chunks for size=3, got {}",
        chunks.len()
    );
}

#[test]
fn r08_render_json_contains_required_keys() {
    // From TestRenderJSON.
    let opts = Options {
        goal: "rate limit burst".into(),
        top: 3,
        ..Default::default()
    };
    let res = evaluate("sample_pack.md", SAMPLE_PACK, &opts);
    let json = serde_json::to_string(&res).expect("serialise result");
    for key in &[
        "pack_file",
        "goal",
        "chunks_total",
        "coverage_score",
        "top",
        "exit_code",
    ] {
        assert!(json.contains(&format!("\"{key}\"")), "missing key {key}");
    }
}
