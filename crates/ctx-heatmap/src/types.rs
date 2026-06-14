// crates/ctx-heatmap/src/types.rs
//
// Wire types mirroring internal/heatmap's exported surface plus the
// FileMetric shape used to ship pre-walked metrics across FFI. JSON tags
// match the Go side byte-for-byte so parity goldens compare cleanly.

use serde::{Deserialize, Serialize, Serializer};

/// FileMetric is the per-file payload shipped across FFI. The Go side
/// pre-walks + counts tokens + extracts symbols; the Rust hot path then
/// runs aggregation + squarify + render against this digest.
///
/// JSON tags are intentionally snake_case to match the FFI contract the
/// dispatcher emits.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileMetric {
    pub path: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub tokens: i64,
    /// Number of symbols extracted from this file (Go's len(Metadata.Symbols)).
    #[serde(default)]
    pub symbols: i64,
    /// Number of commits that touched this file (git-log churn). Absent from
    /// the Go FFI contract, so it defaults to 0 — only the Rust `map --by
    /// churn` path populates it.
    #[serde(default)]
    pub churn: i64,
}

/// AggregateOptions controls the per-call aggregation. `by` selects the
/// weighting axis: "tokens" | "files" | "symbols". Any other value is
/// treated as "tokens" — matches Go heatmap.weightFor.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateOptions {
    #[serde(default = "default_by")]
    pub by: String,
    #[serde(default)]
    pub depth: i64,
    /// Optional TopN truncation applied after sort. 0 means "all".
    #[serde(default)]
    pub top: i64,
}

fn default_by() -> String {
    "tokens".to_string()
}

impl Default for AggregateOptions {
    fn default() -> Self {
        AggregateOptions {
            by: default_by(),
            depth: 0,
            top: 0,
        }
    }
}

/// Bucket is a single aggregated directory cell — mirrors Go's
/// heatmap.Bucket field-by-field. JSON tags use the **CamelCase** Go
/// field names so byte-exact parity with `encoding/json.Marshal` on
/// the Go-side `heatmap.Bucket` is automatic (Go's struct has no
/// per-field JSON tags, so the field names ship as-is).
///
/// In Rust we keep the snake_case identifiers (Rust convention) and
/// rename only the JSON form. This matters for the aggregate_*.json
/// and squarify.json goldens, which compare on parsed serde_json::Value
/// — the key set must match Go exactly.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Bucket {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Tokens")]
    pub tokens: i64,
    #[serde(rename = "Files")]
    pub files: i64,
    #[serde(rename = "Symbols")]
    pub symbols: i64,
    /// git-log churn (commit count) summed over the bucket. Skipped when
    /// zero so the tokens/files/symbols axes ship byte-identical JSON to the
    /// Go parity goldens (Go's Bucket has no Churn field); only the
    /// churn axis — which has no Go golden — emits the key.
    #[serde(rename = "Churn", default, skip_serializing_if = "is_zero_i64")]
    pub churn: i64,
    #[serde(rename = "Weight", serialize_with = "ser_weight")]
    pub weight: f64,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// ser_weight matches Go's `encoding/json` float64 output: integer-
/// valued floats serialise without the trailing ".0". Without this the
/// goldens diverge — Go emits `"Weight":4180`, naive Rust would emit
/// `"Weight":4180.0`. JSON has no integer/float distinction in the
/// language, but `serde_json::Value` does, and our parity comparison
/// uses Value equality.
fn ser_weight<S: Serializer>(v: &f64, s: S) -> Result<S::Ok, S::Error> {
    if v.is_finite() && v.fract() == 0.0 && v.abs() < 1e18 {
        s.serialize_i64(*v as i64)
    } else {
        s.serialize_f64(*v)
    }
}

/// Rect is the integer-pixel rectangle assigned to a single bucket by
/// the Squarify algorithm. Mirrors Go's heatmap.Rect (Bucket / X / Y /
/// W / H — no JSON tags Go-side, so CamelCase ships as-is).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Rect {
    #[serde(rename = "Bucket")]
    pub bucket: Bucket,
    #[serde(rename = "X")]
    pub x: i64,
    #[serde(rename = "Y")]
    pub y: i64,
    #[serde(rename = "W")]
    pub w: i64,
    #[serde(rename = "H")]
    pub h: i64,
}

/// ASCIIOptions controls the per-cell decoration in render_ascii.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AsciiOptions {
    #[serde(default = "default_width")]
    pub width: i64,
    #[serde(default = "default_height")]
    pub height: i64,
    #[serde(default = "default_by")]
    pub by: String,
    #[serde(default = "default_root")]
    pub root: String,
    /// 0 disables the budget highlight.
    #[serde(default)]
    pub budget: i64,
}

fn default_width() -> i64 {
    80
}
fn default_height() -> i64 {
    20
}
fn default_root() -> String {
    ".".to_string()
}

impl Default for AsciiOptions {
    fn default() -> Self {
        AsciiOptions {
            width: default_width(),
            height: default_height(),
            by: default_by(),
            root: default_root(),
            budget: 0,
        }
    }
}

/// JSONOptions carries the metadata that ends up at the top level of
/// the JSON envelope. Budget is `Option<i64>` so a missing budget
/// renders as JSON null rather than the misleading 0 (matches Go).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct JsonOptions {
    #[serde(default = "default_root")]
    pub root: String,
    #[serde(default = "default_by")]
    pub by: String,
    #[serde(default)]
    pub budget: Option<i64>,
}

/// PlainOptions controls the screen-reader-friendly linearised output.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PlainOptions {
    #[serde(default = "default_root")]
    pub root: String,
    #[serde(default = "default_by")]
    pub by: String,
    #[serde(default)]
    pub budget: i64,
}
