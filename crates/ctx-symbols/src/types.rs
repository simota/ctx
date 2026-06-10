// crates/ctx-symbols/src/types.rs
//
// Shared types mirroring internal/model.Symbol + internal/symbols.Hit and
// the apionly render input/output shapes.
//
// We do NOT mirror tree-sitter's internal AST — extraction stays Go-side
// (see PHASE4_REPORT.md for the scope-split justification). The Rust
// side receives pre-extracted symbols + pre-computed apionly ranges and
// runs the cheap post-processing.

use serde::{Deserialize, Serialize};

/// SymbolKind mirrors `model.SymbolKind` (string-typed in Go).
///
/// We intentionally keep this as a String — the canonical values are
/// "function" / "method" / "type" / "class" / "interface" / "export"
/// but the Go side accepts arbitrary strings via normalize_kind.
pub type SymbolKindStr = String;

/// Symbol mirrors `model.Symbol`. The `line` is 1-indexed (matching Go).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Symbol {
    #[serde(rename = "Name")]
    pub name: String,
    #[serde(rename = "Kind")]
    pub kind: SymbolKindStr,
    #[serde(rename = "Line")]
    pub line: i32,
}

/// FileSymbols pairs a forward-slash repo-relative path with the
/// symbols extracted from that file. Used as the corpus shape for
/// `LookupSession::open`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FileSymbols {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Symbols")]
    pub symbols: Vec<Symbol>,
}

/// Hit mirrors `symbols.Hit` — one match returned by a lookup query.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Hit {
    #[serde(rename = "Path")]
    pub path: String,
    #[serde(rename = "Line")]
    pub line: i32,
    #[serde(rename = "Kind")]
    pub kind: String,
    #[serde(rename = "SymbolName")]
    pub symbol_name: String,
}

/// LookupArgs mirrors `symbols.LookupOptions` plus the query name.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LookupArgs {
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub from: String,
    #[serde(default)]
    pub kind: String,
}

// ---------- apionly render types ----------

/// APIRange is one (start_line, end_line) range in the source file's
/// line vector (0-indexed half-open by Go convention — but `end` is
/// inclusive in the Go apionly code). We preserve the Go semantics.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct APIRange {
    #[serde(rename = "start")]
    pub start: i32,
    #[serde(rename = "end")]
    pub end: i32,
    /// Optional in-place edit applied to `lines[end]` (replicating the
    /// Go `signatureEndLine` side-effect). When set, this string
    /// replaces the line at `end` before rendering.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub end_replacement: Option<String>,
}

/// APIRenderRequest is the Rust-side input for `render_api`. Go's
/// `ExtractPublicAPIFromSource` produces `(lines, ranges)` after the
/// tree-sitter pass — we accept that shape here and run the cheap
/// merge+render in Rust.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct APIRenderRequest {
    pub lines: Vec<String>,
    pub ranges: Vec<APIRange>,
}
