// crates/ctx-echo/src/format.rs
//
// Port of internal/echo/format.go's renderers. The FFI only emits
// JSON (the dispatcher decodes EchoResult on the Go side). These
// renderers exist for parity tests so the Rust port can be diffed
// against Go's `Render` output byte-for-byte.
//
// Format names (case-insensitive): "json", "plain", "markdown"
// (default).

use crate::types::{EchoResult, TopEntry};
use std::collections::BTreeMap;
use std::fmt::Write;

/// Render an EchoResult to the chosen format. Mirrors `echo.Render`.
pub fn render(res: &EchoResult, format: &str) -> String {
    match format.to_lowercase().as_str() {
        "json" => render_json(res),
        "plain" => render_plain(res),
        _ => render_markdown(res),
    }
}

/// Convert a whole-number f64 to a JSON integer, keeping fractional floats
/// as-is. This matches Go's encoding/json behaviour: `0.0` serialises as `0`,
/// `1.0` as `1`, but `0.5` stays `0.5`.
fn go_float(v: f64) -> serde_json::Value {
    if v.is_finite() && v.fract() == 0.0 && v >= i64::MIN as f64 && v <= i64::MAX as f64 {
        serde_json::Value::Number(serde_json::Number::from(v as i64))
    } else {
        serde_json::json!(v)
    }
}

fn render_json(res: &EchoResult) -> String {
    // Build the JSON Value tree manually so we can apply Go's integer-like
    // float encoding: whole-number f64s (0.0, 1.0) are emitted as integers,
    // not as "0.0" / "1.0". Go's encoding/json does this automatically via
    // strconv.AppendFloat with format 'f' which strips trailing zeros.
    //
    // BM25 scores (TopEntry.score) are non-integer and stay as f64 literals.
    // Note: there is an inherent last-digit (ULP) difference between Go and
    // Rust BM25 scores due to HashMap iteration order in Go being randomised —
    // Go itself is non-deterministic across runs. The markdown and plain
    // renderers round to 2/4 decimal places and are byte-identical; JSON
    // exposes the raw f64, so JSON parity tests use float-tolerance comparison
    // rather than byte-exact matching.

    // top: Go emits `null` when res.Top is nil (no matching chunks), not `[]`.
    // res.Top is only appended to when there are non-zero scored chunks.
    let top_value: serde_json::Value = if res.top.is_empty() {
        serde_json::Value::Null
    } else {
        let top: Vec<serde_json::Value> = res
            .top
            .iter()
            .map(|t| {
                let matches: serde_json::Map<String, serde_json::Value> = t
                    .matches
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::json!(v)))
                    .collect();
                serde_json::json!({
                    "rank": t.rank,
                    "path": t.path,
                    "line_start": t.line_start,
                    "line_end": t.line_end,
                    "score": t.score,
                    "matches": matches,
                })
            })
            .collect();
        serde_json::json!(top)
    };

    // concentration.files: Go emits `null` when the slice is nil (no appends).
    // Rust Vec::new() → serde_json would emit `[]`. We match Go by emitting
    // null when files is empty.
    let conc_files: serde_json::Value = if res.concentration.files.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!(res.concentration.files)
    };

    // goal_tokens: same nil-vs-empty treatment — Go emits `null` for a nil
    // slice (no tokens found), `[]string{...}` otherwise. uniqueTokenList
    // always returns at least one element when goal is non-empty, so `null`
    // only appears when goal_tokens is truly empty.
    let goal_tokens: serde_json::Value = if res.goal_tokens.is_empty() {
        serde_json::Value::Null
    } else {
        serde_json::json!(res.goal_tokens)
    };

    let obj = serde_json::json!({
        "pack_file":       res.pack_file,
        "goal":            res.goal,
        "chunks_total":    res.chunks_total,
        "chunks_covered":  res.chunks_covered,
        "coverage_score":  go_float(res.coverage_score),
        "spread_index":    go_float(res.spread_index),
        "top":             top_value,
        "goal_tokens":     goal_tokens,
        "threshold":       go_float(res.threshold),
        "exit_code":       res.exit_code,
        "concentration": {
            "files":      conc_files,
            "file_count": res.concentration.file_count,
        },
    });

    let mut s = serde_json::to_string_pretty(&obj).unwrap_or_default();
    s.push('\n');
    s
}

fn render_markdown(res: &EchoResult) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "# CTX-ECHO: pack={} goal={:?} chunks={} covered={}\n",
        display_path(&res.pack_file),
        res.goal,
        res.chunks_total,
        res.chunks_covered
    );

    let _ = writeln!(s, "## Top {} chunks by retrieval score\n", res.top.len());
    if res.top.is_empty() {
        let _ = writeln!(
            s,
            "(no matching chunks — goal tokens did not appear in pack)"
        );
        let _ = writeln!(s);
    } else {
        for t in res.top.iter() {
            let _ = writeln!(
                s,
                "{}. {}\tscore={:.2}  matches: {}",
                t.rank,
                format_range(&t.path, t.line_start, t.line_end),
                t.score,
                format_matches(&t.matches)
            );
        }
        let _ = writeln!(s);
    }

    let _ = writeln!(s, "## Coverage");
    let _ = writeln!(s);
    let _ = writeln!(
        s,
        "- Goal tokens: {} ({})",
        res.goal_tokens.len(),
        res.goal_tokens.join(", ")
    );
    if res.concentration.file_count > 0 {
        let _ = writeln!(
            s,
            "- Concentrated in: {} files ({})",
            res.concentration.file_count,
            res.concentration.files.join(", ")
        );
    } else {
        let _ = writeln!(s, "- Concentrated in: 0 files");
    }
    let _ = writeln!(
        s,
        "- Spread index: {:.2} {}",
        res.spread_index,
        spread_hint(res.spread_index)
    );
    let _ = writeln!(
        s,
        "- Coverage score: {:.2}  (threshold: {:.2})",
        res.coverage_score, res.threshold
    );
    let _ = writeln!(s);

    let _ = writeln!(s, "## Exit");
    if res.exit_code == 0 {
        let _ = writeln!(
            s,
            "0 — coverage {:.2} >= {:.2}",
            res.coverage_score, res.threshold
        );
    } else {
        let _ = writeln!(
            s,
            "1 — coverage {:.2} < {:.2}",
            res.coverage_score, res.threshold
        );
    }
    s
}

fn render_plain(res: &EchoResult) -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "pack={} goal={:?} chunks={} covered={} coverage={:.2} spread={:.2} exit={}",
        display_path(&res.pack_file),
        res.goal,
        res.chunks_total,
        res.chunks_covered,
        res.coverage_score,
        res.spread_index,
        res.exit_code
    );
    for t in res.top.iter() {
        let _ = writeln!(
            s,
            "{}\t{}\t{:.4}\t{}",
            t.rank,
            format_range(&t.path, t.line_start, t.line_end),
            t.score,
            format_matches(&t.matches)
        );
    }
    s
}

fn format_range(path: &str, start: i32, end: i32) -> String {
    if path.is_empty() {
        format!("(anon):{}-{}", start, end)
    } else {
        format!("{}:{}-{}", path, start, end)
    }
}

/// Mirrors `format.go::formatMatches`. Sort by count desc, then key
/// asc.
fn format_matches(m: &BTreeMap<String, i32>) -> String {
    if m.is_empty() {
        return "(none)".to_string();
    }
    let mut keys: Vec<&String> = m.keys().collect();
    keys.sort_by(|a, b| {
        let va = *m.get(*a).unwrap_or(&0);
        let vb = *m.get(*b).unwrap_or(&0);
        if va != vb {
            return vb.cmp(&va);
        }
        a.cmp(b)
    });
    let parts: Vec<String> = keys
        .iter()
        .map(|k| format!("{}({})", k, m.get(*k).copied().unwrap_or(0)))
        .collect();
    parts.join(", ")
}

fn spread_hint(score: f64) -> &'static str {
    if score == 0.0 {
        ""
    } else if score < 0.4 {
        "(low — consider --focus or --braid)"
    } else if score < 0.7 {
        "(moderate)"
    } else {
        "(high)"
    }
}

fn display_path(p: &str) -> String {
    if p.is_empty() || p == "-" {
        "(stdin)".to_string()
    } else {
        p.to_string()
    }
}

// allow unused for unit-test helper
#[allow(dead_code)]
fn touch_top(_t: &TopEntry) {}
