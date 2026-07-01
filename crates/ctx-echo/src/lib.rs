// crates/ctx-echo/src/lib.rs
//
// Tier 2 #3 — Rust port of internal/echo/. The module layout mirrors
// the Go package's file split so byte-parity verification stays
// trivially auditable side-by-side:
//
//   types.rs    — port of echo.Options, Result, TopEntry, FileConcStats,
//                 ChunkStrategy, Chunk
//   tokenize.rs — port of tokenize.go
//   chunk.rs    — port of chunk.go
//   score.rs    — port of score.go (BM25)
//   format.rs   — port of format.go (renderers — parity-test only)
//   evaluate.rs — port of echo.go's Evaluate orchestrator
//   ffi.rs      — extern "C" surface consumed by internal/echo/rustbridge
//
// SHAPE: REGEX_HEAVY stateless batch. Single Evaluate(pack_path,
// pack_body, opts) entry point. No corpus reuse — each call is
// independent over its own pack body.

pub mod chunk;
pub mod evaluate;
pub mod ffi;
pub mod format;
pub mod score;
pub mod tokenize;
pub mod types;

pub use crate::evaluate::evaluate;
pub use crate::types::{
    Chunk, ChunkStrategy, EchoResult, FileConcStats, Options, ScoredChunk, TopEntry,
};
