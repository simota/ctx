use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_echo_json_scores_pack() {
    let root = test_dir("echo");
    fs::create_dir_all(&root).unwrap();
    let pack = root.join("pack.md");
    fs::write(
        &pack,
        "# Pack\n\n--- src/app.go\n\nfunc Run() {\n    Helper()\n}\n\n--- src/helper.go\n\nfunc Helper() string {\n    return \"ok\"\n}\n",
    )
    .unwrap();

    let output = run_rust_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "helper run",
            "--format",
            "json",
            "--top",
            "2",
        ],
    );
    assert!(
        output.status.success(),
        "echo failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse echo JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(payload["pack_file"], "pack.md");
    assert_eq!(payload["goal"], "helper run");
    assert!(payload["chunks_total"].as_i64().unwrap_or(0) > 0);
    assert!(payload["top"]
        .as_array()
        .is_some_and(|items| !items.is_empty()));
}

/// Byte-parity for `echo --format markdown` across all three --chunk-by values.
#[test]
fn echo_parity_markdown_all_chunk_by() {
    let root = write_echo_fixture();
    for chunk_by in &["paragraph", "symbol", "fixed"] {
        assert_delegated_parity_in(
            &root,
            &[
                "echo",
                "pack.md",
                "--goal",
                "rate limit burst handler",
                "--format",
                "markdown",
                "--chunk-by",
                chunk_by,
            ],
        );
    }
}

/// Byte-parity for `echo --format plain` across all three --chunk-by values.
#[test]
fn echo_parity_plain_all_chunk_by() {
    let root = write_echo_fixture();
    for chunk_by in &["paragraph", "symbol", "fixed"] {
        assert_delegated_parity_in(
            &root,
            &[
                "echo",
                "pack.md",
                "--goal",
                "rate limit burst handler",
                "--format",
                "plain",
                "--chunk-by",
                chunk_by,
            ],
        );
    }
}

/// Byte-parity for `echo --format json` across all three --chunk-by values
/// using float-tolerant comparison (BM25 scores have inherent ULP non-determinism
/// from Go's randomised map iteration — even Go's own output varies across runs).
#[test]
fn echo_parity_json_all_chunk_by() {
    let root = write_echo_fixture();
    for chunk_by in &["paragraph", "symbol", "fixed"] {
        assert_echo_json_parity_in(
            &root,
            &[
                "echo",
                "pack.md",
                "--goal",
                "rate limit burst handler",
                "--format",
                "json",
                "--chunk-by",
                chunk_by,
            ],
        );
    }
}

/// Byte-parity for `echo --top N` across representative N values × markdown.
#[test]
fn echo_parity_top_variations() {
    let root = write_echo_fixture();
    for top in &["1", "3", "5"] {
        assert_delegated_parity_in(
            &root,
            &[
                "echo",
                "pack.md",
                "--goal",
                "rate limit burst handler",
                "--top",
                top,
                "--format",
                "markdown",
            ],
        );
    }
}

/// Byte-parity for `echo --chunk-size N` with the fixed strategy.
#[test]
fn echo_parity_chunk_size_fixed() {
    let root = write_echo_fixture();
    for size in &["3", "5", "10"] {
        assert_delegated_parity_in(
            &root,
            &[
                "echo",
                "pack.md",
                "--goal",
                "rate limit burst handler",
                "--chunk-by",
                "fixed",
                "--chunk-size",
                size,
                "--format",
                "markdown",
            ],
        );
    }
}

/// Byte-parity for `echo --threshold` when coverage passes (exit 0) and
/// fails (exit 1 + "Error: \n" on stderr). Verifies exit code AND stderr match.
#[test]
fn echo_parity_threshold_pass_and_fail() {
    let root = write_echo_fixture();
    // Threshold well below expected coverage: exit 0, no Error: on stderr.
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "rate limit burst handler",
            "--threshold",
            "0.0",
            "--format",
            "markdown",
        ],
    );
    // Threshold above 1.0: always fails (coverage is ≤ 1.0), exit 1 + Error:.
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "rate limit burst handler",
            "--threshold",
            "1.1",
            "--format",
            "markdown",
        ],
    );
    // Plain format on threshold failure: stdout + stderr must match.
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "nonexistentword_xyz123",
            "--threshold",
            "0.5",
            "--format",
            "plain",
        ],
    );
}

/// Byte-parity for `echo --unit tokens` (reserved flag, accepted but ignored).
/// The output must be identical to the default (no --unit flag).
#[test]
fn echo_parity_unit_tokens() {
    let root = write_echo_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "rate limit burst handler",
            "--unit",
            "tokens",
            "--format",
            "markdown",
        ],
    );
}

/// Byte-parity for `echo --unit chars` (reserved flag, accepted but ignored).
/// Go also silently ignores --unit (see echo.go: `_ = unit`).
#[test]
fn echo_parity_unit_chars() {
    let root = write_echo_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "rate limit burst handler",
            "--unit",
            "chars",
            "--format",
            "markdown",
        ],
    );
}

/// Byte-parity for `echo --echo-engine go` (documented no-op in both Go and Rust).
/// Note: --echo-engine rust is a documented carve-out (Go exits 1
/// "requires -tags rust_contract", Rust runs native exits 0) — NOT tested here.
#[test]
fn echo_parity_echo_engine_go() {
    let root = write_echo_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "rate limit burst handler",
            "--echo-engine",
            "go",
            "--format",
            "markdown",
        ],
    );
}

/// Byte-parity for `echo` with a goal that produces zero results (no matching
/// chunks). Verifies that both Go and Rust emit empty top / null concentration.
#[test]
fn echo_parity_zero_results() {
    let root = write_echo_fixture();
    // markdown
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "nonexistentword_xyz123",
            "--format",
            "markdown",
        ],
    );
    // plain
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "nonexistentword_xyz123",
            "--format",
            "plain",
        ],
    );
    // json: BYTE-EXACT here. With zero matching chunks there are no `score`
    // f64 fields at all (top is null), and coverage_score/spread_index/threshold
    // are all 0 → integer-encoded by go_float. So this JSON output has no ULP
    // surface and is genuinely byte-identical to Go, every run. Assert it
    // byte-for-byte (not ULP-tolerant) to lock that in.
    assert_delegated_parity_in(
        &root,
        &[
            "echo",
            "pack.md",
            "--goal",
            "nonexistentword_xyz123",
            "--format",
            "json",
        ],
    );
}

/// Byte-EXACT JSON parity for an input whose BM25 scores happen to land on
/// identical f64 bits in Go and Rust despite the math.Log≠f64::ln 1-ULP idf
/// difference (the rounding of the final score absorbs the idf ULP for this
/// particular fixture). Verified byte-identical across 20 Go runs AND Go==Rust.
///
/// This complements the ULP-tolerant score tests: it proves the Rust scorer
/// can and does reproduce Go's exact f64 output when the arithmetic aligns,
/// so the tolerance in assert_echo_json_parity_in is genuinely only absorbing
/// last-bit noise, not masking a systematic Rust-side error.
///
/// NOTE: this is the large_pack fixture — its richer/longer chunks yield scores
/// whose final f64 rounding is insensitive to the 1-ULP idf wobble. It is NOT a
/// general guarantee (most inputs differ by 1 ULP — see the suite header), so
/// only this specific, empirically-stable invocation is byte-tested.
#[test]
fn echo_parity_json_byte_exact_stable_input() {
    assert_delegated_parity_in(
        &repo_root(),
        &[
            "echo",
            "tests/echo-fixtures/large_pack.md",
            "--goal",
            "pack relevance budget tokens",
            "--format",
            "json",
            "--top",
            "20",
            "--chunk-by",
            "fixed",
            "--chunk-size",
            "20",
        ],
    );
}

/// Full echo flag-matrix: all format values × all chunk-by values × goal × top.
/// markdown/plain are byte-exact; json is float-tolerant.
#[test]
fn echo_parity_full_flag_matrix() {
    let root = write_echo_fixture();
    for chunk_by in &["paragraph", "symbol", "fixed"] {
        for top in &["1", "5", "10"] {
            // markdown: byte-exact
            assert_delegated_parity_in(
                &root,
                &[
                    "echo",
                    "pack.md",
                    "--goal",
                    "rate limit burst handler",
                    "--format",
                    "markdown",
                    "--chunk-by",
                    chunk_by,
                    "--top",
                    top,
                ],
            );
            // plain: byte-exact
            assert_delegated_parity_in(
                &root,
                &[
                    "echo",
                    "pack.md",
                    "--goal",
                    "rate limit burst handler",
                    "--format",
                    "plain",
                    "--chunk-by",
                    chunk_by,
                    "--top",
                    top,
                ],
            );
            // json: float-tolerant
            assert_echo_json_parity_in(
                &root,
                &[
                    "echo",
                    "pack.md",
                    "--goal",
                    "rate limit burst handler",
                    "--format",
                    "json",
                    "--chunk-by",
                    chunk_by,
                    "--top",
                    top,
                ],
            );
        }
    }
}

// ── Wave-1 byte-parity suite for `focus` ─────────────────────────────────────
//
// Goal: prove `focus` is byte-identical to Go across its entire flag/value
// surface so it is ready for Wave-3 zero-delegation cutover.
//
// Fixture: `write_where_fixture()` — two Go files (src/app.go declares Run,
// src/helper.go declares Helper).  Run → anchor-origin src/app.go, 1-hop
// expansion pulls in src/helper.go.
//
// Not covered: `--focus-engine rust` — the Go binary (without the
// rust_contract build tag) returns exit-1 with "requires -tags rust_contract".
// The Rust CLI accepts the flag and runs native (exit 0), so outputs can never
// match. Same documented carve-out as map's --heatmap-engine and where's
// --where-engine.
