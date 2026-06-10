use std::fs;

mod common;
use common::*;

#[test]
fn native_contract_verify_matches_go() {
    let dir = test_dir("contract-verify");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));

    let clean_response = dir.join("clean-response.md");
    fs::write(
        &clean_response,
        "See internal/limit/limit.go and `NewLimiter` for the behavior.\n",
    )
    .unwrap_or_else(|err| panic!("write {}: {err}", clean_response.display()));

    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &clean_response.to_string_lossy(),
        "--format",
        "json",
    ]);

    let bad_response = dir.join("bad-response.md");
    fs::write(
        &bad_response,
        "See imaginary/phantom.go and `MissingSymbol` for the behavior.\n",
    )
    .unwrap_or_else(|err| panic!("write {}: {err}", bad_response.display()));

    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &bad_response.to_string_lossy(),
        "--format=plain",
    ]);
}

// ── Wave-1 byte-parity suite for `audit` ─────────────────────────────────────
//
// Goal: prove `audit verify` is provably byte-identical to Go across its ENTIRE
// flag/value surface so the command is ready for Wave-3 zero-delegation cutover.
//
// Go surface (internal/cli/audit_verify.go):
//   - `audit verify [PATH]`   (MaximumNArgs(1) — zero or one positional)
//   - No flags beyond globals (no --format, no --engine)
//   - Reads CTX_AUDIT_LOG env / default path when no PATH given
//   - CTX_AUDIT_DISABLE=1 → "no audit log path configured" exit 2
//
// Inputs covered:
//   - valid log (OK, exit 0)
//   - broken log — single broken line (broken at line N, exit 1)
//   - broken log — range (broken range: N-M, exit 1)
//   - missing file (file not found: <path>, exit 2)
//   - no path configured (CTX_AUDIT_DISABLE=1, exit 2)
//   - --help (rendered natively by clap, covered by native_subcommand_help_renders)
//
// `run_audit_verify` has NO reachable `return None` for any valid invocation:
// - `args.iter().any(|arg| is_option(arg))` catches unknown flags → None (error path)
// - `args.len() > 1` catches multiple positionals → None (error path)
// - All valid paths (0 or 1 positional, no flags) always return Some(ExitCode).

/// `contract verify` — clean response + json format → violations=[], exit 0.
#[test]
fn contract_parity_json_clean() {
    let dir = test_dir("contract-parity-json-clean");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See internal/limit/limit.go and `NewLimiter` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "json",
    ]);
}

/// `contract verify` — bad response + json format → violations populated, exit 1.
#[test]
fn contract_parity_json_violations() {
    let dir = test_dir("contract-parity-json-violations");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See imaginary/phantom.go and `MissingSymbol` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "json",
    ]);
}

/// `contract verify` — clean response + markdown format → OK section, exit 0.
#[test]
fn contract_parity_markdown_clean() {
    let dir = test_dir("contract-parity-markdown-clean");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See internal/limit/limit.go and `NewLimiter` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "markdown",
    ]);
}

/// `contract verify` — bad response + markdown format → Violations section, exit 1.
#[test]
fn contract_parity_markdown_violations() {
    let dir = test_dir("contract-parity-markdown-violations");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See imaginary/phantom.go and `MissingSymbol` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "markdown",
    ]);
}

/// `contract verify` — bad response + plain format.
#[test]
fn contract_parity_plain_violations() {
    let dir = test_dir("contract-parity-plain-violations");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See imaginary/phantom.go and `MissingSymbol` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "plain",
    ]);
}

/// `contract verify` — clean response + plain format.
#[test]
fn contract_parity_plain_clean() {
    let dir = test_dir("contract-parity-plain-clean");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See internal/limit/limit.go and `NewLimiter` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "plain",
    ]);
}

/// `contract verify` — --strict flag → promotes warnings, all formats.
#[test]
fn contract_parity_strict_flag() {
    let dir = test_dir("contract-parity-strict");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let clean = dir.join("clean.md");
    fs::write(
        &clean,
        "See internal/limit/limit.go and `NewLimiter` for the behavior.\n",
    )
    .unwrap();
    let bad = dir.join("bad.md");
    fs::write(
        &bad,
        "See imaginary/phantom.go and `MissingSymbol` for the behavior.\n",
    )
    .unwrap();

    // strict + clean response → still exit 0 (no violations)
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &clean.to_string_lossy(),
        "--format",
        "json",
        "--strict",
    ]);
    // strict + bad response → exit 1
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &bad.to_string_lossy(),
        "--format",
        "json",
        "--strict",
    ]);
}

/// `contract verify --no-symbols` — skips symbol check; only path violations remain.
#[test]
fn contract_parity_no_symbols_flag() {
    let dir = test_dir("contract-parity-no-symbols");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See imaginary/phantom.go and `MissingSymbol` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "json",
        "--no-symbols",
    ]);
}

/// `contract verify --engine go` — explicit no-op engine flag matches default.
/// NOTE: --engine rust is a documented carve-out (Go exits 1, Rust runs native). NOT tested.
#[test]
fn contract_parity_engine_go() {
    let dir = test_dir("contract-parity-engine-go");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(
        &response,
        "See internal/limit/limit.go and `NewLimiter` for the behavior.\n",
    )
    .unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &response.to_string_lossy(),
        "--format",
        "json",
        "--engine",
        "go",
    ]);
}

/// Error path: missing pack file → "read pack: open <path>: no such file or directory", exit 1.
/// Both Go and Rust print this error TWICE on stderr (cobra default + main's Fprintln).
#[test]
fn contract_parity_error_missing_pack() {
    let dir = test_dir("contract-parity-err-missing-pack");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(&response, "anything\n").unwrap();
    let missing_pack = dir.join("no-such-pack.md");
    assert_delegated_parity(&[
        "contract",
        "verify",
        &missing_pack.to_string_lossy(),
        "--response",
        &response.to_string_lossy(),
    ]);
}

/// Error path: pack with no contract block → "pack X does not contain a ctx:contract block", exit 1.
/// Both Go and Rust print this error TWICE on stderr.
#[test]
fn contract_parity_error_no_contract_block() {
    let dir = test_dir("contract-parity-err-no-block");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let response = dir.join("response.md");
    fs::write(&response, "anything\n").unwrap();
    let plain_pack = dir.join("plain.md");
    fs::write(&plain_pack, "# No contract here\n\nJust text.\n").unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        &plain_pack.to_string_lossy(),
        "--response",
        &response.to_string_lossy(),
    ]);
}

/// Error path: missing response file → "read response: open <path>: no such file or directory", exit 1.
#[test]
fn contract_parity_error_missing_response() {
    let dir = test_dir("contract-parity-err-missing-response");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let missing_response = dir.join("no-such-response.md");
    assert_delegated_parity(&[
        "contract",
        "verify",
        "internal/contract/testdata/sample_pack.md",
        "--response",
        &missing_response.to_string_lossy(),
    ]);
}

/// Full matrix: contract verify × all format × clean/violation inputs.
#[test]
fn contract_parity_full_matrix() {
    let dir = test_dir("contract-parity-matrix");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let clean = dir.join("clean.md");
    fs::write(
        &clean,
        "See internal/limit/limit.go and `NewLimiter` for the behavior.\n",
    )
    .unwrap();
    let bad = dir.join("bad.md");
    fs::write(
        &bad,
        "See imaginary/phantom.go and `MissingSymbol` for the behavior.\n",
    )
    .unwrap();

    let pack = "internal/contract/testdata/sample_pack.md";

    // format × clean/violation cross-product
    for format in &["json", "markdown", "plain"] {
        assert_delegated_parity(&[
            "contract",
            "verify",
            pack,
            "--response",
            &clean.to_string_lossy(),
            "--format",
            format,
        ]);
        assert_delegated_parity(&[
            "contract",
            "verify",
            pack,
            "--response",
            &bad.to_string_lossy(),
            "--format",
            format,
        ]);
    }

    // flags
    assert_delegated_parity(&[
        "contract",
        "verify",
        pack,
        "--response",
        &bad.to_string_lossy(),
        "--format",
        "json",
        "--strict",
    ]);
    assert_delegated_parity(&[
        "contract",
        "verify",
        pack,
        "--response",
        &bad.to_string_lossy(),
        "--format",
        "json",
        "--no-symbols",
    ]);
    assert_delegated_parity(&[
        "contract",
        "verify",
        pack,
        "--response",
        &clean.to_string_lossy(),
        "--format",
        "json",
        "--engine",
        "go",
    ]);

    // error paths
    let missing_pack = dir.join("no-pack.md");
    assert_delegated_parity(&[
        "contract",
        "verify",
        &missing_pack.to_string_lossy(),
        "--response",
        &clean.to_string_lossy(),
    ]);
    let plain_pack = dir.join("plain.md");
    fs::write(&plain_pack, "# No contract block\n\nJust text.\n").unwrap();
    assert_delegated_parity(&[
        "contract",
        "verify",
        &plain_pack.to_string_lossy(),
        "--response",
        &clean.to_string_lossy(),
    ]);
    let missing_response = dir.join("no-response.md");
    assert_delegated_parity(&[
        "contract",
        "verify",
        pack,
        "--response",
        &missing_response.to_string_lossy(),
    ]);
}
