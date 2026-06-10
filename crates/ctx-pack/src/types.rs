// crates/ctx-pack/src/types.rs
//
// JSON-friendly types ported from internal/pack/. Field names match
// the Go side's exported JSON tags so the cgo bridge can shuttle
// payloads without a translation layer.

use serde::{Deserialize, Serialize};

fn null_as_empty_vec<'de, D, T>(d: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::Deserialize<'de>,
{
    let v: Option<Vec<T>> = Option::deserialize(d)?;
    Ok(v.unwrap_or_default())
}

/// Subset of model.Symbol that scoreRelevance reads.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SymbolInput {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub line: i64,
}

/// Subset of model.Metadata the relevance scorer needs. We only carry
/// fields the pure helpers actually inspect — Size for the binary
/// heuristic, Role for the role boost, Symbols for the symbol bonus,
/// TokensEst for budget gating.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MetadataInput {
    #[serde(default)]
    pub size: i64,
    #[serde(default)]
    pub tokens_est: i64,
    /// model.FileRole — empty string falls back to FileInput.role.
    #[serde(default)]
    pub role: String,
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub symbols: Vec<SymbolInput>,
}

/// Subset of model.FileInfo the pack relevance scorer needs. The Go
/// dispatcher fills in Path / AbsPath / Role / Metadata and ships them
/// over FFI; the full FileInfo tree (children, gitstatus, mod time)
/// is NOT carried because the scorer never inspects it.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct FileInput {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub abs_path: String,
    #[serde(default)]
    pub is_dir: bool,
    #[serde(default)]
    pub tokens: i64,
    #[serde(default)]
    pub role: String,
    #[serde(default)]
    pub metadata: MetadataInput,
    /// Optional pre-read content. When present (Go side opted in to
    /// the binary-detect override), is_binary is computed against the
    /// first 512 bytes. When absent, binary detection is skipped —
    /// matches the Go behaviour where isBinaryFile returns false on
    /// read-error.
    #[serde(default, deserialize_with = "null_as_empty_vec")]
    pub content_head: Vec<u8>,
}

/// Mirrors pack.ScoreBreakdown — same field names, JSON-friendly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ScoreBreakdown {
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub basename: i64,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub path: i64,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub symbol: i64,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub content: i64,
    #[serde(default, skip_serializing_if = "is_zero_i64")]
    pub role: i64,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// Mirrors the unexported pack.relevanceResult. The exported public
/// fields are uppercased so they survive JSON marshalling; the Go
/// side uses the same shape to receive results.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RelevanceResult {
    #[serde(default)]
    pub score: i64,
    #[serde(default)]
    pub tier: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub signals: Vec<String>,
    #[serde(default)]
    pub breakdown: ScoreBreakdown,
}

/// Pre-decoded option payload for ScoreFile session calls.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct PackOptions {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub budget: i64,
}

/// Diff input — mirrors ctxgit.FileDiff that PackDiff iterates over.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffEntry {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub before_content: String,
    #[serde(default)]
    pub after_content: String,
    #[serde(default)]
    pub before_commit: String,
    #[serde(default)]
    pub after_commit: String,
    #[serde(default)]
    pub patch: String,
    #[serde(default)]
    pub added: bool,
    #[serde(default)]
    pub deleted: bool,
    #[serde(default)]
    pub binary: bool,
}

/// Options driving render_diff. Mirrors the subset of pack.Options
/// PackDiff actually inspects (Layout + APIOnly are honoured here;
/// APIOnly content rewriting stays on the Go side because it depends
/// on symbols.ExtractPublicAPIFromSource, which is not ported).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DiffOptions {
    #[serde(default)]
    pub layout: String,
    #[serde(default)]
    pub preset: String,
}

/// Subset of model.Warning the redactor reads. Both flag-on conditions
/// (SecretScan + Redact) gate on the Go side; what comes across is
/// the already-filtered warning list.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WarningInput {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub line: i64,
    #[serde(default)]
    pub kind: String,
}

/// Mirrors pack.WhereResult — see internal/pack/from_where.go.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WhereResult {
    #[serde(default)]
    pub path: String,
    #[serde(default)]
    pub score: f64,
}

/// Named preset identifier — matches ApplyPreset's switch arms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PresetName {
    None,
    Blog,
    Review,
    Debug,
    Llm,
}

impl PresetName {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "" => Some(Self::None),
            "blog" => Some(Self::Blog),
            "review" => Some(Self::Review),
            "debug" => Some(Self::Debug),
            "llm" => Some(Self::Llm),
            _ => None,
        }
    }
}

/// Patch payload emitted by `apply_preset`. Mirrors the fields ApplyPreset
/// would overwrite on pack.Options; the Go caller merges each field
/// onto its in-memory Options struct.
///
/// Optional fields use `Option<T>` semantics on the wire: when None
/// the corresponding Go field is left untouched (mirrors the Go
/// switch arms that don't assign every field on every preset).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PresetPatch {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub format: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_warnings: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_paths: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub no_metadata: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub frontmatter: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plain_file_contents: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub explain: Option<bool>,
}
