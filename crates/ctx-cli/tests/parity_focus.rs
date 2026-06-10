use std::fs;

mod common;
use common::*;

/// Byte-parity for `focus` across all four --format values at default hops.
#[test]
fn focus_parity_all_formats() {
    let root = write_where_fixture();
    for format in &["markdown", "plain", "xml", "json"] {
        assert_delegated_parity_in(
            &root,
            &["focus", "Run", "--format", format, "--budget", "50000"],
        );
    }
}

/// Byte-parity for `focus` --hops {0, 1, 2} × markdown (graph-traversal depth).
#[test]
fn focus_parity_hops_depth() {
    let root = write_where_fixture();
    for hops in &["0", "1", "2"] {
        assert_delegated_parity_in(
            &root,
            &[
                "focus", "Run", "--hops", hops, "--format", "markdown", "--budget", "50000",
            ],
        );
    }
}

/// Byte-parity for `focus` with non-zero budget that excludes files.
#[test]
fn focus_parity_budget_nonzero() {
    let root = write_where_fixture();
    // budget=5 is lower than helper.go (14 tokens) so only anchor file included.
    assert_delegated_parity_in(
        &root,
        &["focus", "Run", "--budget", "5", "--format", "markdown"],
    );
    // budget=50000: all files included.
    assert_delegated_parity_in(
        &root,
        &["focus", "Run", "--budget", "50000", "--format", "json"],
    );
}

/// Byte-parity for `focus` with budget=0 (unlimited).
#[test]
fn focus_parity_budget_zero_unlimited() {
    let root = write_where_fixture();
    assert_delegated_parity_in(
        &root,
        &["focus", "Run", "--budget", "0", "--format", "markdown"],
    );
}

/// Byte-parity for `focus --plain` shortcut (same as --format plain).
#[test]
fn focus_parity_plain_shortcut() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["focus", "Run", "--plain", "--budget", "50000"]);
}

/// Byte-parity for `--json focus` shortcut (global --json flag).
#[test]
fn focus_parity_json_shortcut() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["--json", "focus", "Run", "--budget", "50000"]);
}

/// Byte-parity for `focus --focus-engine go` (no-op selector; must match default).
/// Note: --focus-engine rust is a documented carve-out (Go exits 1
/// "requires -tags rust_contract", Rust runs native exits 0) — NOT tested here.
#[test]
fn focus_parity_engine_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "focus",
            "Run",
            "--focus-engine",
            "go",
            "--format",
            "markdown",
            "--budget",
            "50000",
        ],
    );
}

/// Byte-parity for `focus` when anchor is ambiguous: Go and Rust both exit 1
/// with identical error messages.
#[test]
fn focus_parity_ambiguous_anchor() {
    let root = write_where_fixture();
    fs::write(root.join("src/other.go"), "package src\n\nfunc Run() {}\n").unwrap();
    assert_delegated_parity_in(&root, &["focus", "Run"]);
}

/// Byte-parity full flag matrix: all formats × hops {0,1,2} × budget {5,50000}.
#[test]
fn focus_parity_full_flag_matrix() {
    let root = write_where_fixture();
    for format in &["markdown", "plain", "xml", "json"] {
        for hops in &["0", "1", "2"] {
            for budget in &["5", "50000"] {
                assert_delegated_parity_in(
                    &root,
                    &[
                        "focus", "Run", "--format", format, "--hops", hops, "--budget", budget,
                    ],
                );
            }
        }
    }
}

/// Structural smoke-test (kept for regression coverage): focus plain output
/// contains the anchor meta line and file content.
#[test]
fn native_focus_plain_expands_from_symbol_anchor() {
    let root = write_where_fixture();
    // Now uses byte-parity against Go; also verify structural invariants.
    assert_delegated_parity_in(
        &root,
        &["focus", "Run", "--format", "plain", "--budget", "50000"],
    );
    let output = run_rust_in(
        &root,
        &["focus", "Run", "--format", "plain", "--budget", "50000"],
    );
    assert!(
        output.status.success(),
        "focus failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("# anchor=Run origin=src/app.go hops=1"));
    assert!(stdout.contains("+ src/app.go"));
    assert!(stdout.contains("tokens"));
}

/// Byte-parity for ambiguous anchor error (exit 1, stderr matches Go exactly).
#[test]
fn native_focus_reports_ambiguous_symbol_anchor() {
    let root = write_where_fixture();
    fs::write(root.join("src/other.go"), "package src\n\nfunc Run() {}\n").unwrap();

    // Verify byte-parity against Go for the ambiguous-anchor error path.
    assert_delegated_parity_in(&root, &["focus", "Run"]);

    let output = run_rust_in(&root, &["focus", "Run"]);
    assert!(
        !output.status.success(),
        "focus should fail for ambiguous anchor"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("matches multiple definitions"));
    assert!(stderr.contains("src/app.go:3 (function)"));
    assert!(stderr.contains("src/other.go:3 (function)"));
}
