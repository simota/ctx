use std::fs;

mod common;
use common::*;

#[test]
fn native_audit_verify_matches_go() {
    let dir = test_dir("audit-verify");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));

    let valid = dir.join("valid.log");
    write_valid_audit_log(&valid);
    let valid_arg = valid.to_string_lossy().to_string();
    assert_delegated_parity(&["audit", "verify", &valid_arg]);

    let broken = dir.join("broken.log");
    write_broken_audit_log(&broken);
    let broken_arg = broken.to_string_lossy().to_string();
    assert_delegated_parity(&["audit", "verify", &broken_arg]);

    let missing = dir.join("missing.log");
    let missing_arg = missing.to_string_lossy().to_string();
    assert_delegated_parity(&["audit", "verify", &missing_arg]);
}

/// `audit verify` — valid log → "OK", exit 0.
#[test]
fn audit_parity_valid_log() {
    let dir = test_dir("audit-parity-valid");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join("valid.log");
    write_valid_audit_log(&path);
    assert_delegated_parity(&["audit", "verify", &path.to_string_lossy()]);
}

/// `audit verify` — broken log (single broken line) → "broken at line: N", exit 1.
#[test]
fn audit_parity_broken_single_line() {
    let dir = test_dir("audit-parity-broken");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join("broken.log");
    write_broken_audit_log(&path);
    assert_delegated_parity(&["audit", "verify", &path.to_string_lossy()]);
}

/// `audit verify` — broken log spanning a range of lines → "broken range: N-M", exit 1.
#[test]
fn audit_parity_broken_range() {
    let dir = test_dir("audit-parity-broken-range");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join("broken-range.log");
    write_broken_range_audit_log(&path);
    assert_delegated_parity(&["audit", "verify", &path.to_string_lossy()]);
}

/// `audit verify <missing-path>` → "file not found: <path>" on stderr, exit 2.
#[test]
fn audit_parity_missing_file() {
    let dir = test_dir("audit-parity-missing");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let path = dir.join("no-such-audit.log");
    assert_delegated_parity(&["audit", "verify", &path.to_string_lossy()]);
}

/// `audit verify` with CTX_AUDIT_DISABLE=1 and no PATH → "no audit log path configured", exit 2.
#[test]
fn audit_parity_no_path_configured() {
    let dir = test_dir("audit-parity-no-path");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go = run_go_in_with_env(&dir, &["audit", "verify"], &[("CTX_AUDIT_DISABLE", "1")]);
    let rust = run_rust_in_with_env(&dir, &["audit", "verify"], &[("CTX_AUDIT_DISABLE", "1")]);
    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch\nGo stderr: {}\nRust stderr: {}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr)
    );
    assert_eq!(
        rust.stdout,
        go.stdout,
        "stdout mismatch\nGo: {}\nRust: {}",
        String::from_utf8_lossy(&go.stdout),
        String::from_utf8_lossy(&rust.stdout)
    );
    assert_eq!(
        rust.stderr,
        go.stderr,
        "stderr mismatch\nGo: {}\nRust: {}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr)
    );
}

/// Full matrix: audit verify × all valid/invalid input combinations.
#[test]
fn audit_parity_full_matrix() {
    let dir = test_dir("audit-parity-matrix");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));

    let valid = dir.join("valid.log");
    write_valid_audit_log(&valid);
    assert_delegated_parity(&["audit", "verify", &valid.to_string_lossy()]);

    let broken = dir.join("broken.log");
    write_broken_audit_log(&broken);
    assert_delegated_parity(&["audit", "verify", &broken.to_string_lossy()]);

    let broken_range = dir.join("broken-range.log");
    write_broken_range_audit_log(&broken_range);
    assert_delegated_parity(&["audit", "verify", &broken_range.to_string_lossy()]);

    let missing = dir.join("no-such.log");
    assert_delegated_parity(&["audit", "verify", &missing.to_string_lossy()]);
}

// ── Wave-1 byte-parity suite for `contract` ───────────────────────────────────
//
// Goal: prove `contract verify` is provably byte-identical to Go across its ENTIRE
// flag/value surface so the command is ready for Wave-3 zero-delegation cutover.
//
// Go surface (internal/cli/contract.go):
//   - `contract verify <pack-file>`   (ExactArgs(1))
//   - --format markdown|json|plain    (default: markdown)
//   - --response <file>               (read LLM response from file)
//   - --strict                        (promote warnings to violations)
//   - --no-symbols                    (skip symbol verification)
//   - --check-worktree               (compare cited files with current worktree)
//   - --root <dir>                    (worktree root for --check-worktree)
//   - --engine go|rust                (CARVE-OUT: rust → Go exits 1, Rust runs native)
//
// Error cases covered:
//   - missing pack file       → "read pack: open <path>: no such file or directory"
//   - pack with no contract   → "pack X does not contain a ctx:contract block ..."
//   - missing response file   → "read response: open <path>: no such file or directory"
//   All errors print twice on stderr: once as "Error: <msg>" and once as "<msg>"
//   (replicating cobra's default SilenceErrors=false behavior in Go's main.go).
//
// `run_contract_verify` has NO reachable `return None` for any valid invocation:
// - `parse_contract_verify_args` returns None only on unknown flags or ≠1 positional
// - All valid invocations (exactly one pack-file positional, known flags) return Some.
//
// --engine rust is a documented carve-out: Go exits 1 ("requires -tags rust_contract"),
// Rust runs native (exit 0). NOT tested here per the project convention.
