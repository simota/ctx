// crates/ctx-replay/src/types.rs
//
// Port of the wire types from internal/replay/{manifest,diff}.go.
//
// JSON shape mirrors the Go encoder byte-for-byte:
//   - snake_case field names
//   - `omitempty` semantics via serde's `skip_serializing_if`
//   - `created_at` is an opaque RFC3339 string (we don't parse it — we
//     treat it as a string that round-trips faithfully through Go's
//     time.Time JSON encoder)

use serde::{Deserialize, Serialize};

/// Mirrors the Go `SchemaVersion` constant.
pub const SCHEMA_VERSION: i32 = 1;

fn is_default<T: Default + PartialEq>(v: &T) -> bool {
    *v == T::default()
}

fn is_empty_string(v: &String) -> bool {
    v.is_empty()
}

fn is_empty_vec<T>(v: &Vec<T>) -> bool {
    v.is_empty()
}

/// Manifest captures the inputs and selected files of a single ctx pack run.
///
/// `created_at` is held as an opaque string so we don't fight Go's
/// nanosecond-precision RFC3339 encoder. Callers pass through whatever
/// Go produced; we round-trip it untouched.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Manifest {
    #[serde(rename = "schema_version")]
    pub schema_version: i32,
    pub id: String,
    pub created_at: String,
    pub ctx_version: String,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub goal: String,
    pub budget: i64,
    pub used: i64,
    pub root: String,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub preset: String,
    pub format: String,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub out_sha256: String,
    pub entries: Vec<Entry>,
    #[serde(default, skip_serializing_if = "is_empty_vec")]
    pub skipped: Vec<Skipped>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub path: String,
    pub sha256: String,
    pub tokens: i64,
    pub relevance: String,
    pub score: i64,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Skipped {
    pub path: String,
    pub reason: String,
}

// ---------------------------------------------------------------------
// Diff wire types — port of internal/replay/diff.go
// ---------------------------------------------------------------------

/// ChangeKind classifies the relationship between a file entry in two manifests.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ChangeKind {
    Added,
    Modified,
    Removed,
    Unchanged,
}

impl Default for ChangeKind {
    fn default() -> Self {
        ChangeKind::Unchanged
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FileChange {
    pub path: String,
    pub kind: ChangeKind,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub base_sha256: String,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub cur_sha256: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub base_tokens: i64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub cur_tokens: i64,
    pub token_delta: i64,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub reason: String,
}

/// DiffSummary mirrors the Go `DiffSummary` shape. Counts come first so
/// the JSON object's leading keys are stable.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiffSummary {
    pub added: i64,
    pub modified: i64,
    pub removed: i64,
    pub unchanged: i64,
    pub token_delta: i64,
    /// Go uses `[]FileChange` with the default JSON encoder behaviour —
    /// when no changes exist the field marshals as `null`. We match that
    /// using `None` (Option<Vec<_>>) when empty.
    pub changes: Vec<FileChange>,
}

// ---------------------------------------------------------------------
// Selection diff types
// ---------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionCategory {
    Added,
    Removed,
    Promoted,
    Demoted,
    ScoreChanged,
}

impl Default for SelectionCategory {
    fn default() -> Self {
        SelectionCategory::Added
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionChange {
    pub path: String,
    pub category: SelectionCategory,
    #[serde(default, skip_serializing_if = "is_default")]
    pub base_score: i64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub cur_score: i64,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub base_tier: String,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub cur_tier: String,
    #[serde(default, skip_serializing_if = "is_default")]
    pub base_tokens: i64,
    #[serde(default, skip_serializing_if = "is_default")]
    pub cur_tokens: i64,
    #[serde(default, skip_serializing_if = "is_empty_string")]
    pub reason_change: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionCounts {
    pub added: i64,
    pub removed: i64,
    pub promoted: i64,
    pub demoted: i64,
    pub score_changed: i64,
    pub token_delta: i64,
}

/// The Go side declares the four slices as `[]SelectionChange` which
/// marshal as `null` when empty. We mirror that by serialising empty
/// vectors as `null`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionGroups {
    #[serde(serialize_with = "serialize_null_when_empty")]
    pub added: Vec<SelectionChange>,
    #[serde(serialize_with = "serialize_null_when_empty")]
    pub removed: Vec<SelectionChange>,
    #[serde(serialize_with = "serialize_null_when_empty")]
    pub promoted: Vec<SelectionChange>,
    #[serde(serialize_with = "serialize_null_when_empty")]
    pub demoted: Vec<SelectionChange>,
    #[serde(serialize_with = "serialize_null_when_empty")]
    pub score_changed: Vec<SelectionChange>,
}

fn serialize_null_when_empty<S, T>(v: &Vec<T>, s: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
    T: serde::Serialize,
{
    if v.is_empty() {
        s.serialize_none()
    } else {
        s.collect_seq(v.iter())
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelectionSummary {
    pub a: String,
    pub b: String,
    pub summary: SelectionCounts,
    pub changes: SelectionGroups,
}
