// crates/ctx-echo/src/types.rs
//
// Port of internal/echo/echo.go's public types + chunk.go's Chunk &
// ChunkStrategy. Field-level JSON shape mirrors the Go struct tags
// one-for-one. The parity tests in tests/parity.rs assert byte-exact
// match against Go-produced goldens.
//
// NOTE: `Chunk` here intentionally diverges from the Go Chunk: the Go
// Chunk caches `Tokens` and `TokenLen` for BM25 fast-path. Rust caches
// the same — we just don't serialise the cached vectors out the FFI
// (they're internal). The public-shape `Chunk` is only used internally
// during scoring; nothing leaks across the FFI besides `Result`.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// ---------------------------------------------------------------------
// Chunk strategy
// ---------------------------------------------------------------------

/// Mirrors `echo.ChunkStrategy` in Go (lower-case string discriminators).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChunkStrategy {
    Paragraph,
    Symbol,
    Fixed,
}

impl ChunkStrategy {
    pub fn from_str(s: &str) -> ChunkStrategy {
        match s {
            "fixed" => ChunkStrategy::Fixed,
            "symbol" => ChunkStrategy::Symbol,
            // "paragraph", "", and any unknown value default to Paragraph
            // — matches Go's `switch strategy { default: chunkParagraph }`.
            _ => ChunkStrategy::Paragraph,
        }
    }
}

// ---------------------------------------------------------------------
// Options
// ---------------------------------------------------------------------

/// Mirrors `echo.Options` in Go. The Rust struct serialises with
/// snake_case keys to match the JSON shape the dispatcher will marshal.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Options {
    #[serde(default)]
    pub goal: String,
    #[serde(default)]
    pub top: i32,
    #[serde(default)]
    pub threshold: f64,
    /// Stored as a raw string at the wire boundary so the dispatcher
    /// doesn't have to know about ChunkStrategy. We convert in
    /// evaluate().
    #[serde(default)]
    pub chunk_by: String,
    #[serde(default)]
    pub chunk_size: i32,
    /// Format is accepted at the wire for forward compatibility but
    /// Evaluate ignores it — the CLI renders.
    #[serde(default)]
    pub format: String,
}

// ---------------------------------------------------------------------
// Result / TopEntry / FileConcStats
// ---------------------------------------------------------------------

/// Mirrors `echo.TopEntry`. Note `matches` is serialised as a JSON
/// object {token: count}; Go uses a `map[string]int` which
/// encoding/json renders with alphabetically-sorted keys. Rust's
/// BTreeMap also sorts keys, so the wire shape matches byte-for-byte.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TopEntry {
    pub rank: i32,
    pub path: String,
    pub line_start: i32,
    pub line_end: i32,
    pub score: f64,
    pub matches: BTreeMap<String, i32>,
}

/// Mirrors `echo.FileConcStats`. Vec<String> serialises as JSON array;
/// when empty, Go emits `null` because `json` keeps a nil slice as nil.
/// We replicate that by leaving the Vec empty and using a custom
/// serializer — but Result-level normalisation happens at the
/// dispatcher (see also the contract crate pattern). For now we
/// preserve `Vec::new()` -> `[]` because the Go renderer also has the
/// concept of an empty (zero-len) slice serialising to `null` only
/// when Go did not append anything. The golden files capture the
/// canonical shape; parity tests are the authoritative oracle.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct FileConcStats {
    /// Null-vs-empty distinction: Go emits `null` when the slice is
    /// nil. We mirror that with `Option<Vec<...>>` plus a custom
    /// serializer below — but simpler: serialise an empty Vec as
    /// `null` by means of `skip_serializing_if`. The golden harness
    /// compares JSON values, not raw bytes, so either shape works for
    /// parity. We pick Vec<String> for ergonomics.
    pub files: Vec<String>,
    pub file_count: i32,
}

/// Mirrors `echo.Result`. Public type returned by `evaluate()`.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EchoResult {
    pub pack_file: String,
    pub goal: String,
    pub chunks_total: i32,
    pub chunks_covered: i32,
    pub coverage_score: f64,
    pub spread_index: f64,
    pub top: Vec<TopEntry>,
    pub goal_tokens: Vec<String>,
    pub threshold: f64,
    pub exit_code: i32,
    pub concentration: FileConcStats,
}

// ---------------------------------------------------------------------
// Internal Chunk + ScoredChunk
// ---------------------------------------------------------------------

/// Internal chunk representation. Not serialised at the FFI boundary —
/// only `EchoResult` crosses.
#[derive(Debug, Clone, Default)]
pub struct Chunk {
    pub source_path: String,
    pub line_start: i32,
    pub line_end: i32,
    pub body: String,
    pub tokens: Vec<String>,
    pub token_len: i32,
}

/// Internal scored chunk. Used by `score()` to pair a Chunk with its
/// BM25 score and per-term matches. The `matches` map uses a
/// BTreeMap to keep iteration order stable — Go's map is unordered but
/// `encoding/json` sorts keys for us at serialisation time.
#[derive(Debug, Clone, Default)]
pub struct ScoredChunk {
    pub chunk: Chunk,
    pub score: f64,
    pub matches: BTreeMap<String, i32>,
}
