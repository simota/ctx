mod common;
use common::*;

#[test]
fn native_skim_full_and_outline_tiers() {
    let root = write_where_fixture();
    let full = run_rust_in(
        &root,
        &["skim", "src/app.go", "--budget", "10000", "--unit", "chars"],
    );
    assert!(
        full.status.success(),
        "skim full failed: {}",
        String::from_utf8_lossy(&full.stderr)
    );
    let full_stdout = String::from_utf8_lossy(&full.stdout);
    assert!(full_stdout.starts_with("# tier=full"));
    assert!(full_stdout.contains("func Run()"));

    let outline = run_rust_in(&root, &["skim", "src/app.go", "--tier", "outline"]);
    assert!(
        outline.status.success(),
        "skim outline failed: {}",
        String::from_utf8_lossy(&outline.stderr)
    );
    let outline_stdout = String::from_utf8_lossy(&outline.stdout);
    assert!(outline_stdout.starts_with("# tier=outline"));
    assert!(outline_stdout.contains("src/app.go:3 function Run"));
    assert!(!outline_stdout.contains("Helper()"));
}

/// `skim` — default flags (tier=auto, budget=1000 tokens): full tier, no overflow.
#[test]
fn skim_parity_default_full() {
    let root = write_skim_fixture();
    assert_delegated_parity_in(&root, &["skim", "src/app.go"]);
    assert_delegated_parity_in(&root, &["skim", "src/helper.go"]);
}

/// `skim` — unit=chars: budget in chars, no overflow.
#[test]
fn skim_parity_unit_chars() {
    let root = write_skim_fixture();
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--unit", "chars"]);
    assert_delegated_parity_in(
        &root,
        &["skim", "src/app.go", "--unit", "chars", "--budget", "10000"],
    );
}

/// `skim` — explicit --tier full: forced, no degradation.
#[test]
fn skim_parity_tier_full() {
    let root = write_skim_fixture();
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--tier", "full"]);
}

/// `skim` — explicit --tier outline: forced outline tier.
#[test]
fn skim_parity_tier_outline() {
    let root = write_skim_fixture();
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--tier", "outline"]);
    assert_delegated_parity_in(&root, &["skim", "src/helper.go", "--tier", "outline"]);
}

/// `skim` — overflow: budget so small that even outline exceeds it.
/// Go emits "(over budget)" inside the tokens string, exit 2, plus "Error: " on stderr.
#[test]
fn skim_parity_overflow() {
    let root = write_skim_fixture();
    // budget=1 forces overflow even on outline
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--budget", "1"]);
}

/// `skim` — degradation: budget forces auto-degrade from full to outline.
/// Go emits the degradation warning to stderr.
#[test]
fn skim_parity_degraded() {
    let root = write_skim_fixture();
    // budget=5 tokens: outline (12 tokens) still exceeds → overflow
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--budget", "5"]);
    // budget=20 tokens: full is 11 tokens → fits → no degradation
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--budget", "20"]);
}

/// `skim` — explicit --lang flag overrides auto-detection.
#[test]
fn skim_parity_lang_flag() {
    let root = write_skim_fixture();
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--lang", "go"]);
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--lang", "text"]);
}

/// `skim` — missing file → error exit 1, matching Go error message.
#[test]
fn skim_parity_missing_file() {
    let root = write_skim_fixture();
    assert_delegated_parity_in(&root, &["skim", "src/nonexistent.go"]);
}

/// Full skim flag matrix: tier × unit × representative budget values.
#[test]
fn skim_parity_full_matrix() {
    let root = write_skim_fixture();

    // Auto-degrade path × both units
    for unit in &["tokens", "chars"] {
        assert_delegated_parity_in(
            &root,
            &["skim", "src/app.go", "--unit", unit, "--budget", "10000"],
        );
        assert_delegated_parity_in(
            &root,
            &["skim", "src/app.go", "--unit", unit, "--budget", "1"],
        );
    }

    // Forced tier × both files
    for tier in &["full", "outline"] {
        assert_delegated_parity_in(&root, &["skim", "src/app.go", "--tier", tier]);
        assert_delegated_parity_in(&root, &["skim", "src/helper.go", "--tier", tier]);
    }

    // lang flag
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--lang", "go"]);
    assert_delegated_parity_in(&root, &["skim", "src/app.go", "--lang", "auto"]);
}

// ── Wave-1 byte-parity suite for `onboarding` ────────────────────────────────
//
// Goal: prove `onboarding` is byte-identical to Go across its full flag/value
// surface.
//
// Go surface (internal/cli/onboarding.go):
//   - `onboarding [path]`       (MaximumNArgs(1); defaults to ".")
//   - --limit N                (default 10)
//   - --persona human|ai       (default "human")
//   - --format text|json       (default "text")
//
// Risk areas:
//   - `score_breakdown.symbol_count`: Go math.Log2 vs Rust f64::log2 differ
//     by 1 ULP for non-power-of-2 symbol counts. This is unavoidable — both
//     are correct IEEE 754 implementations with different rounding. Only this
//     field requires tolerance; everything else is byte-exact.
//   - Reason strings: Go buildReason uses ". " join; Rust onboarding_reason
//     now mirrors it exactly.
//   - Arrow char: Go WriteText uses "→" (U+2192); Rust now matches.
//   - JSON omitempty: Go omits tokens/symbols/description/loc/ref_count/hot
//     when zero/false/empty; Rust now manually builds the JSON Value tree.
//   - Float integer encoding: Go encoding/json emits whole-number f64 as
//     integers (73 not 73.0); Rust onboarding_go_float() mirrors this.
//
// `run_onboarding_command` has NO reachable `return None` for any valid invocation:
//   - `parse_onboarding_args` returns None only on: unknown flags, double
//     `onboarding`, or >1 positionals. All valid invocations return Some.
