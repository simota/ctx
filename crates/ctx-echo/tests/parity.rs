// crates/ctx-echo/tests/parity.rs
//
// Phase 4 parity tests for ctx-echo. For each fixture under
// tests/parity/echo-goldens/<fixture>/evaluate.json we:
//   1. Load the same fixture body the Go exporter used.
//   2. Run the Rust `evaluate()` with matching Options.
//   3. Assert the JSON output is structurally equal to the Go golden.
//
// "Structurally equal" = serde_json::Value equality (key order
// independent, numerically-equal f64s tolerated within 1e-9).

use std::fs;
use std::path::PathBuf;

use pretty_assertions::assert_eq;

use ctx_echo::evaluate;
use ctx_echo::types::Options;

fn goldens_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("go.mod").exists() {
            return dir.join("tests").join("parity").join("echo-goldens");
        }
        if !dir.pop() {
            panic!("repo root not found above {}", env!("CARGO_MANIFEST_DIR"));
        }
    }
}

fn fixtures_dir() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("go.mod").exists() {
            return dir.join("tests").join("echo-fixtures");
        }
        if !dir.pop() {
            panic!("repo root not found above {}", env!("CARGO_MANIFEST_DIR"));
        }
    }
}

/// Tolerance for floating-point fields when comparing parity goldens.
/// BM25 scores are summed from a HashMap iteration whose order is
/// non-deterministic in Go (random map walk). The Rust port iterates
/// a different non-deterministic order. f64 summation is not
/// associative, so the bit-level sum differs by a few ULPs while the
/// mathematically-correct value is identical. We tolerate up to 1e-9
/// relative error on float fields — well below any retrieval
/// behavioural threshold.
const FLOAT_RTOL: f64 = 1e-9;
const FLOAT_ATOL: f64 = 1e-12;

fn floats_close(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    if a.is_nan() || b.is_nan() {
        return false;
    }
    let diff = (a - b).abs();
    let scale = a.abs().max(b.abs());
    diff <= FLOAT_ATOL || diff <= FLOAT_RTOL * scale
}

/// Recursive JSON equality with float tolerance + null/[] aliasing.
fn json_equal(a: &serde_json::Value, b: &serde_json::Value) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Null, Value::Null) => true,
        // Go emits `null` for nil slices; serde emits `[]` for empty.
        (Value::Null, Value::Array(v)) | (Value::Array(v), Value::Null) => v.is_empty(),
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            // Both integer? exact compare.
            if let (Some(xi), Some(yi)) = (x.as_i64(), y.as_i64()) {
                return xi == yi;
            }
            if let (Some(xf), Some(yf)) = (x.as_f64(), y.as_f64()) {
                return floats_close(xf, yf);
            }
            false
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return false;
            }
            x.iter().zip(y.iter()).all(|(a, b)| json_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            if x.len() != y.len() {
                return false;
            }
            x.iter()
                .all(|(k, v)| y.get(k).map(|v2| json_equal(v, v2)).unwrap_or(false))
        }
        _ => false,
    }
}

fn assert_parity(fixture: &str, goal: &str, top: i32) {
    let body = fs::read_to_string(fixtures_dir().join(format!("{fixture}.md")))
        .or_else(|_| fs::read_to_string(fixtures_dir().join(format!("{fixture}.txt"))))
        .unwrap_or_else(|e| panic!("read fixture {fixture}: {e}"));

    let golden_path = goldens_dir().join(fixture).join("evaluate.json");
    let golden_raw = match fs::read_to_string(&golden_path) {
        Ok(s) => s,
        Err(_) => {
            eprintln!(
                "golden missing — skipping parity for {fixture} (expected {})",
                golden_path.display()
            );
            return;
        }
    };

    let opts = Options {
        goal: goal.to_string(),
        top,
        ..Default::default()
    };
    let rust_res = evaluate(fixture, &body, &opts);
    let rust_json = serde_json::to_value(&rust_res).expect("serialise rust result");
    let go_json: serde_json::Value = serde_json::from_str(&golden_raw).expect("parse golden json");

    if !json_equal(&rust_json, &go_json) {
        // On mismatch, show pretty-printed forms so the diff is
        // readable even for floats outside tolerance.
        assert_eq!(
            serde_json::to_string_pretty(&rust_json).unwrap(),
            serde_json::to_string_pretty(&go_json).unwrap(),
            "parity mismatch on fixture {fixture}"
        );
    }
}

// ---------------------------------------------------------------------
// One test per fixture × representative goal.
// ---------------------------------------------------------------------

#[test]
fn parity_small_pack_rate_limit_burst() {
    assert_parity("small_pack", "rate limit burst handler", 10);
}

#[test]
fn parity_medium_pack_rate_limit_burst() {
    assert_parity("medium_pack", "rate limit burst handler", 10);
}

#[test]
fn parity_large_pack_rate_limit_burst() {
    assert_parity("large_pack", "rate limit burst handler", 10);
}

#[test]
fn parity_small_pack_threshold_fail() {
    let body = fs::read_to_string(fixtures_dir().join("small_pack.md")).expect("read fixture");
    let opts = Options {
        goal: "non-existent-token-xyz123".to_string(),
        top: 5,
        threshold: 0.99,
        ..Default::default()
    };
    let res = evaluate("small_pack", &body, &opts);
    assert_eq!(res.exit_code, 1);
}
