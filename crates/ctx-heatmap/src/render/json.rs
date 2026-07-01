// crates/ctx-heatmap/src/render/json.rs
//
// Rust port of internal/heatmap/render_json.go.
//
// JSON parity contract:
//   - Field order matches Go struct order via serde + preserve_order
//   - 2-space indent matches encoding/json's SetIndent("", "  ")
//   - Trailing newline matches encoding/json.Encoder.Encode behaviour
//   - budget is `null` when absent, integer when present (mirrors *int)
//   - rect inner object key order: x, y, w, h (Go's map literal order)
//
// Go's json package sorts map keys lexicographically when marshalling
// `map[string]int{"x":..., "y":..., "w":..., "h":...}` — the keys end up
// in alphabetical order: h, w, x, y. We replicate that with a BTreeMap.

use serde::{Serialize, Serializer};
use serde_json::Value;
use std::collections::BTreeMap;

use crate::types::{JsonOptions, Rect};

fn ser_weight<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e18 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}

#[derive(Serialize)]
struct JsonRect {
    path: String,
    tokens: i64,
    files: i64,
    symbols: i64,
    #[serde(serialize_with = "ser_weight")]
    weight: f64,
    rect: BTreeMap<&'static str, i64>,
    in_budget: bool,
}

#[derive(Serialize)]
struct JsonEnvelope {
    root: String,
    by: String,
    #[serde(serialize_with = "ser_weight")]
    total: f64,
    total_tokens: i64,
    budget: Option<i64>,
    buckets: Vec<JsonRect>,
}

/// render_json returns the pretty-printed JSON envelope as a byte
/// vector. Mirrors `encoding/json.Encoder` with SetIndent("", "  ") + a
/// terminating newline.
pub fn render_json(rects: &[Rect], opts: &JsonOptions) -> Result<Vec<u8>, serde_json::Error> {
    let mut envelope = JsonEnvelope {
        root: if opts.root.is_empty() {
            ".".into()
        } else {
            opts.root.clone()
        },
        by: if opts.by.is_empty() {
            "tokens".into()
        } else {
            opts.by.clone()
        },
        total: 0.0,
        total_tokens: 0,
        budget: opts.budget,
        buckets: Vec::with_capacity(rects.len()),
    };

    let budget_val = opts.budget.unwrap_or(0);
    let mut used: i64 = 0;
    for r in rects {
        envelope.total += r.bucket.weight;
        envelope.total_tokens += r.bucket.tokens;
        let in_budget = budget_val > 0 && used + r.bucket.tokens <= budget_val;
        if in_budget {
            used += r.bucket.tokens;
        }
        let mut rect_map: BTreeMap<&'static str, i64> = BTreeMap::new();
        rect_map.insert("x", r.x);
        rect_map.insert("y", r.y);
        rect_map.insert("w", r.w);
        rect_map.insert("h", r.h);
        envelope.buckets.push(JsonRect {
            path: r.bucket.path.clone(),
            tokens: r.bucket.tokens,
            files: r.bucket.files,
            symbols: r.bucket.symbols,
            weight: r.bucket.weight,
            rect: rect_map,
            in_budget,
        });
    }

    // Two-space indent + trailing newline matches encoding/json.Encoder.
    let v: Value = serde_json::to_value(&envelope)?;
    let mut buf = Vec::with_capacity(256 + rects.len() * 128);
    let formatter = serde_json::ser::PrettyFormatter::with_indent(b"  ");
    let mut ser = serde_json::Serializer::with_formatter(&mut buf, formatter);
    v.serialize(&mut ser)?;
    buf.push(b'\n');
    Ok(buf)
}
