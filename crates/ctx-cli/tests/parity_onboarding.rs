use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_onboarding_json_ranks_entrypoint() {
    let root = test_dir("onboarding");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
    fs::write(
        root.join("core.go"),
        "package main\n\n// Core is the domain core.\nfunc Core() {}\nfunc Helper() {}\n",
    )
    .unwrap();
    fs::write(
        root.join("core_test.go"),
        "package main\n\nfunc TestCore() {}\n",
    )
    .unwrap();

    let output = run_rust_in(&root, &["onboarding", "--format", "json", "--limit", "5"]);
    assert!(
        output.status.success(),
        "onboarding failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse onboarding JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let steps = payload["steps"]
        .as_array()
        .expect("onboarding steps should be an array");
    assert!(steps.iter().any(|item| item["path"] == "main.go"));
    assert!(!steps.iter().any(|item| item["path"] == "core_test.go"));
    let main = steps.iter().find(|item| item["path"] == "main.go").unwrap();
    assert_eq!(main["role"], "entry");
    assert_eq!(main["score_breakdown"]["entry_role"], 50.0);
}

// ── Wave-1 byte-parity suite for `skim` ──────────────────────────────────────
//
// Goal: prove `skim` is byte-identical to Go across its full flag/value surface.
//
// Go surface (internal/cli/skim.go):
//   - `skim <path>`             (ExactArgs(1))
//   - --budget N               (default 1000)
//   - --unit tokens|chars      (default "tokens")
//   - --lang <lang>            (default "auto")
//   - --tier full|api+doc|signatures|outline  (default "" = auto-degrade)
//
// Output format (always text, no --format flag):
//   Line 1: "# tier=T tokens=N/B path=P lang=L" (or "N/B (over budget)" when overflow)
//   Line 2: blank
//   Lines 3+: tier body
//   Stderr: degradation/overflow warnings; overflow also emits "Error: " (cobra ExitError).
//   Exit: 0 = OK, 2 = overflow.
//
// Risk areas:
//   - Token counting: uses ctx_tokens (cl100k_base), already proven identical to Go.
//   - "(over budget)" placement: INSIDE the tokens string, not at the end of the line.
//   - Overflow exit also emits cobra's "Error: \n" to stderr.
//
// `run_skim_command` has NO reachable `return None` for any valid invocation:
//   - `parse_skim_args` returns None only on: unknown flags, double `skim`, or
//     ≠1 positionals. All valid invocations (exactly 1 positional, known flags)
//     always return Some(SkimArgs) and thus always return Some(ExitCode).

/// `onboarding` text format — human persona (default): full byte-parity.
#[test]
fn onboarding_parity_text_human() {
    let root = write_onboarding_fixture();
    assert_delegated_parity_in(&root, &["onboarding", &root.to_string_lossy()]);
}

/// `onboarding` text format — ai persona.
#[test]
fn onboarding_parity_text_ai() {
    let root = write_onboarding_fixture();
    assert_delegated_parity_in(
        &root,
        &["onboarding", &root.to_string_lossy(), "--persona", "ai"],
    );
}

/// `onboarding` text format — explicit --format text.
#[test]
fn onboarding_parity_format_text_explicit() {
    let root = write_onboarding_fixture();
    assert_delegated_parity_in(
        &root,
        &["onboarding", &root.to_string_lossy(), "--format", "text"],
    );
}

/// `onboarding` JSON format — full byte-parity EXCEPT score_breakdown.symbol_count
/// (1-ULP math.Log2 vs f64::log2 difference).
/// Uses float-tolerant comparison (same as echo) for score fields only.
#[test]
fn onboarding_parity_json_human() {
    let root = write_onboarding_fixture();
    assert_onboarding_json_parity_in(
        &root,
        &["onboarding", &root.to_string_lossy(), "--format", "json"],
    );
}

/// `onboarding` JSON format — ai persona.
#[test]
fn onboarding_parity_json_ai() {
    let root = write_onboarding_fixture();
    assert_onboarding_json_parity_in(
        &root,
        &[
            "onboarding",
            &root.to_string_lossy(),
            "--format",
            "json",
            "--persona",
            "ai",
        ],
    );
}

/// `onboarding` — --limit flag reduces step count.
#[test]
fn onboarding_parity_limit() {
    let root = write_onboarding_fixture();
    // limit=1: only top file
    assert_delegated_parity_in(
        &root,
        &["onboarding", &root.to_string_lossy(), "--limit", "1"],
    );
    assert_onboarding_json_parity_in(
        &root,
        &[
            "onboarding",
            &root.to_string_lossy(),
            "--limit",
            "1",
            "--format",
            "json",
        ],
    );
}

/// Full onboarding flag matrix: format × persona × limit.
#[test]
fn onboarding_parity_full_matrix() {
    let root = write_onboarding_fixture();
    let root_s = root.to_string_lossy().into_owned();

    // text × persona
    for persona in &["human", "ai"] {
        assert_delegated_parity_in(&root, &["onboarding", &root_s, "--persona", persona]);
        // with limit
        assert_delegated_parity_in(
            &root,
            &["onboarding", &root_s, "--persona", persona, "--limit", "1"],
        );
    }

    // json × persona (float-tolerant)
    for persona in &["human", "ai"] {
        assert_onboarding_json_parity_in(
            &root,
            &[
                "onboarding",
                &root_s,
                "--format",
                "json",
                "--persona",
                persona,
            ],
        );
        assert_onboarding_json_parity_in(
            &root,
            &[
                "onboarding",
                &root_s,
                "--format",
                "json",
                "--persona",
                persona,
                "--limit",
                "1",
            ],
        );
    }
}

/// `onboarding` — default path (no positional arg) uses cwd.
/// Run from the fixture dir so both Go and Rust see the same files.
#[test]
fn onboarding_parity_default_path() {
    let root = write_onboarding_fixture();
    // Run both binaries with cwd = fixture dir, no path arg.
    let go = run_go_in(&root, &["onboarding"]);
    let rust = run_rust_in(&root, &["onboarding"]);
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
        "stdout mismatch\nGo:\n{}\nRust:\n{}",
        String::from_utf8_lossy(&go.stdout),
        String::from_utf8_lossy(&rust.stdout)
    );
    assert_eq!(
        rust.stderr,
        go.stderr,
        "stderr mismatch\nGo:\n{}\nRust:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr)
    );
}
