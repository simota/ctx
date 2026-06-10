// crates/ctx-focus/src/types.rs
//
// Wire types mirroring internal/focus/expand.go's exported surface plus
// the FileInput shape used to ship pre-walked corpus across FFI. JSON
// tags match the Go side byte-for-byte so parity goldens compare cleanly.

use serde::{Deserialize, Serialize};

/// AnchorKind classifies how the anchor was resolved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AnchorKind {
    Symbol,
    Basename,
    Path,
}

impl Default for AnchorKind {
    fn default() -> Self {
        AnchorKind::Symbol
    }
}

/// Anchor is the resolved anchor for a focus operation. Field order /
/// serialization tags must match Go's internal/focus.Anchor.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Anchor {
    #[serde(rename = "Kind")]
    pub kind: AnchorKind,
    #[serde(rename = "Raw")]
    pub raw: String,
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "OriginPath")]
    pub origin_path: String,
}

/// Candidate is an ambiguous anchor resolution candidate.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Candidate {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Line")]
    pub line: i64,
    #[serde(rename = "Kind")]
    pub kind: String,
}

/// ErrAmbiguous mirrors Go's *focus.ErrAmbiguous; serialized when
/// ResolveAnchor matches multiple definitions.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ErrAmbiguous {
    #[serde(rename = "Anchor")]
    pub anchor: String,
    #[serde(rename = "Candidates")]
    pub candidates: Vec<Candidate>,
}

/// FileInfo describes a single resolved file in an expansion result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileInfo {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Reason")]
    pub reason: String,
    #[serde(rename = "Tokens")]
    pub tokens: i64,
}

/// ExpandOptions configures Expand. Hops outside [1, 2] are normalised:
/// hops<1 → 1, hops>2 → 2 (with a warning emitted Go-side; the Rust side
/// silently clamps).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ExpandOptions {
    #[serde(default)]
    pub hops: i64,
}

/// SymbolInfo mirrors internal/model.Symbol's three exported fields, in
/// the same JSON shape the Go dispatcher emits for ctx-where.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SymbolInfo {
    pub name: String,
    pub kind: String,
    pub line: i64,
}

/// FileInput is the per-file payload shipped across FFI: repo-relative
/// path, pre-extracted symbols, and the absolute path used to short-
/// circuit content-scan for name-match (the Go dispatcher pre-walks; for
/// the Rust crate we also pre-read line content so the hot path stays
/// inside Rust).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileInput {
    pub path: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub symbols: Vec<SymbolInfo>,
    /// Pre-read content lines (already binary/UTF-8 filtered Go-side).
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub lines: Vec<String>,
}

fn null_as_empty_vec<'de, D, T>(d: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    use serde::Deserialize;
    let v: Option<Vec<T>> = Option::deserialize(d)?;
    Ok(v.unwrap_or_default())
}

/// PackResult bundles the output of `pack` — the one-shot stateless
/// entry point that runs ResolveAnchor + Expand and returns both pieces
/// for the caller to render.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PackResult {
    pub anchor: Anchor,
    pub files: Vec<FileInfo>,
}
