mod common;
use common::*;

#[test]
fn native_browse_rejects_non_loopback_before_go_delegate() {
    let output = run_rust_in_with_env(
        &repo_root(),
        &["browse", "--bind", "0.0.0.0", "--no-open"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("refusing to bind to non-loopback"));
}

// ── Wave-1 byte-parity suite for `echo` ──────────────────────────────────────
//
// Goal: prove `echo` is provably byte-identical to Go across its full flag/value
// surface so the command is ready for Wave-3 zero-delegation cutover.
//
// Flag surface (from internal/cli/echo.go):
//   Flags: --goal <str>, --top N, --threshold F, --chunk-size N
//   --chunk-by <value>: paragraph (default) | symbol | fixed
//   --unit <value>: tokens (default, reserved only) | chars (reserved only)
//   --format <value>: markdown (default) | json | plain
//   --echo-engine {go|rust}: go is the default/no-op; rust is a documented
//     carve-out (Go exits 1 "requires -tags rust_contract", Rust runs native
//     exit 0) — NOT tested here.
//
// Fixture: write_echo_fixture() — multi-paragraph, multi-file pack with three
// distinct sections (middleware/limit.go, middleware/auth.go, docs/intro.md)
// so all chunk strategies produce non-trivial results with a goal that has
// clear and weak relevance.
//
// Format strategy:
//   - markdown and plain: byte-exact via assert_delegated_parity_in — these
//     formats round scores to %.2f / %.4f, which absorbs the 1-ULP BM25 score
//     differences (see below), so they are genuinely byte-identical to Go.
//   - json: ULP-tolerant via assert_echo_json_parity_in — the raw-f64 `score`
//     field cannot be byte-equal to Go. TWO empirically-verified causes, both
//     bounded to ~1.3e-16 relative (one f64 ULP):
//       (1) Go `math.Log` ≠ Rust `f64::ln` in the last bit; the BM25 idf term
//           is a natural log, so every score inherits a 1-ULP difference. This
//           is DETERMINISTIC on each side but Go≠Rust (e.g. medium_pack
//           --goal middleware: Go 3.3358682934662056 vs Rust 3.335868293466205
//           every run).
//       (2) For chunks matching 2+ goal tokens, Go sums idf*weight while
//           iterating a map in randomised order; f64 add is non-associative, so
//           Go's OWN output varies run-to-run in the last ULP (small_pack
//           --goal "rate limit burst handler" --chunk-by paragraph: 7 distinct
//           Go byte-outputs over 20 runs).
//     assert_echo_json_parity_in compares score with a 1e-12 tolerance (4
//     orders above the ULP noise, 9+ below any real regression) and compares
//     EVERY OTHER field (integers, strings, null/[]) exactly. The earlier
//     claim that the divergence was "purely Go map non-determinism" was
//     INCOMPLETE — the dominant, always-present cause is math.Log vs f64::ln.
//
// No `return None` in run_echo_command is reachable by a valid echo invocation:
//   parse_echo_args returns None only for unknown flags or wrong positional
//   count — both are error/usage paths. The goal==empty check at parse time
//   also maps to None (--goal is required). The happy path has no unconditional
//   None.
//
// --echo-engine carve-out: same as map/where/focus/deps. Go exits 1 with
//   "requires -tags rust_contract"; Rust runs native (exit 0). Not tested.
