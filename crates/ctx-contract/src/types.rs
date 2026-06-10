// crates/ctx-contract/src/types.rs
//
// Port of internal/contract/contract.go and the FileInput type from
// build.go. Field-level JSON shape mirrors Go's struct tags one-for-one;
// see the matching Go source for documentation and `tests/parity/goldens/`
// for the canonical wire format.

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------
// Contract / File
// ---------------------------------------------------------------------

/// File records one packed file in the contract manifest. Mirrors
/// `contract.File` in Go.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct File {
    pub path: String,
    pub line_start: i32,
    pub line_end: i32,
    pub sha256: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub line_hashes: Vec<LineHash>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub symbols: Vec<String>,
}

/// LineHash records the SHA-256 of one packed logical line. Mirrors
/// `contract.LineHash` in Go.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct LineHash {
    pub line: i32,
    pub sha256: String,
}

/// Contract is the full manifest embedded into a pack output. Mirrors
/// `contract.Contract` in Go.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Contract {
    pub schema_version: i32,
    pub created: String,
    #[serde(default)]
    pub files: Vec<File>,
}

// ---------------------------------------------------------------------
// Reference
// ---------------------------------------------------------------------

/// Discriminator for `Reference.kind`. The string values mirror the Go
/// constants used in `parse_refs.go`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReferenceKind {
    File,
    LineRange,
    Symbol,
    DiffHeader,
}

impl ReferenceKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ReferenceKind::File => "file",
            ReferenceKind::LineRange => "line-range",
            ReferenceKind::Symbol => "symbol",
            ReferenceKind::DiffHeader => "diff-header",
        }
    }
}

/// Reference is one citation found inside an LLM response. The Go side
/// stores `Kind` as a free-form string ("file"/"line-range"/"symbol"/
/// "diff-header"); we mirror that here rather than expose the enum at
/// the JSON boundary so the parity goldens line up byte-for-byte.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Reference {
    pub kind: String,
    pub path: String,
    pub line_start: i32,
    pub line_end: i32,
    pub symbol: String,
    pub source_line: i32,
}

// ---------------------------------------------------------------------
// Violation / OK / StaleFile
// ---------------------------------------------------------------------

/// ViolationKind enumerates the failure modes Verify can flag. Wire
/// representation matches the Go constants verbatim.
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationKind {
    #[serde(rename = "out-of-context")]
    OutOfContext,
    #[serde(rename = "stale-content")]
    StaleContent,
    #[serde(rename = "phantom-symbol")]
    PhantomSymbol,
}

impl ViolationKind {
    pub fn as_str(&self) -> &'static str {
        match self {
            ViolationKind::OutOfContext => "out-of-context",
            ViolationKind::StaleContent => "stale-content",
            ViolationKind::PhantomSymbol => "phantom-symbol",
        }
    }
}

/// Violation is one failed reference check. Field omission rules match
/// Go's `,omitempty` semantics so the JSON wire shape is identical.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct Violation {
    pub kind: ViolationKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub line_start: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub line_end: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub symbol: String,
    #[serde(
        default,
        rename = "expected_sha256",
        skip_serializing_if = "String::is_empty"
    )]
    pub expected_sha: String,
    #[serde(
        default,
        rename = "got_sha256",
        skip_serializing_if = "String::is_empty"
    )]
    pub got_sha: String,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub source_line: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

impl Default for Violation {
    fn default() -> Self {
        Self {
            kind: ViolationKind::OutOfContext,
            path: String::new(),
            line_start: 0,
            line_end: 0,
            symbol: String::new(),
            expected_sha: String::new(),
            got_sha: String::new(),
            source_line: 0,
            message: String::new(),
        }
    }
}

/// OK is one reference that successfully matched the contract.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct OK {
    pub kind: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub path: String,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub line_start: i32,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub line_end: i32,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub symbol: String,
    #[serde(default, skip_serializing_if = "is_zero_i32")]
    pub source_line: i32,
}

/// StaleFile records a pack-resident file whose current worktree bytes
/// no longer match the bytes embedded in the pack contract.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct StaleFile {
    pub path: String,
    #[serde(rename = "expected_sha256")]
    pub expected_sha: String,
    #[serde(
        default,
        rename = "got_sha256",
        skip_serializing_if = "String::is_empty"
    )]
    pub got_sha: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub message: String,
}

// ---------------------------------------------------------------------
// VerifyOptions / Result
// ---------------------------------------------------------------------

/// VerifyOptions tune Verify behaviour. Zero value is "lenient defaults".
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VerifyOptions {
    pub strict: bool,
    pub no_symbols: bool,
    pub worktree_root: String,
}

/// Result is the structured outcome of a verify run.
///
/// CRITICAL: collections deliberately do NOT use `skip_serializing_if`
/// — they must emit `[]` (matching Go's renderJSON nil→[] normalisation)
/// for parity with the golden snapshots.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq, Default)]
pub struct Result {
    pub pack_file: String,
    pub schema_version: i32,
    pub total_files_in_contract: i32,
    pub references_found: i32,
    #[serde(default)]
    pub violations: Vec<Violation>,
    #[serde(default)]
    pub ok: Vec<OK>,
    #[serde(default)]
    pub stale_files: Vec<StaleFile>,
    #[serde(default)]
    pub repack_suggestions: Vec<String>,
    pub exit_code: i32,
}

// ---------------------------------------------------------------------
// FileInput (build-time)
// ---------------------------------------------------------------------

/// FileInput is the minimal data the pack writer hands to `builder::build`.
/// Mirrors `contract.FileInput` in build.go.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FileInput {
    pub path: String,
    pub content: Vec<u8>,
    pub symbols: Vec<String>,
}

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}
