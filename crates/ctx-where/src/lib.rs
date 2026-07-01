// crates/ctx-where/src/lib.rs
//
// Rust port of internal/where/. The module layout mirrors the Go
// package's functional decomposition so byte-parity verification stays
// trivially auditable side-by-side.
//
// Phase 3 (LOOKUP_HEAVY): the FRAGILITY TEST. Predicted 1.85-2.5×
// intrinsic speedup; net end-to-end (with cgo) must clear ≥1.3× for the
// LOOKUP_HEAVY thesis to survive into Phase 4.
//
//   types.rs        — Suggestion/Match/ScoreBreakdown/Result/Options/KeywordSet
//   levenshtein.rs  — pure DP edit distance (first parity gate)
//   score.rs        — scoreFile + scoreFileWithSets, identifier splitting,
//                     keyword extraction, match scoring
//   search.rs       — top-level Search/SearchWithOptions/SuggestSimilar
//                     orchestrator that takes a pre-walked file list.
//                     The walk + symbol extraction live on the Go side so
//                     the Rust port focuses on the LOOKUP hot path (the
//                     part we expect to win on).
//   ffi.rs          — extern "C" surface used by internal/where/rustbridge.

pub mod ffi;
pub mod levenshtein;
pub mod score;
pub mod search;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::levenshtein::levenshtein;
pub use crate::score::{
    extract_keywords, score_file, score_file_literal, score_file_with_sets, split_identifier,
};
pub use crate::search::{search_with_options, suggest_similar, FileInput, Options, SymbolInput};
pub use crate::types::{KeywordSet, Match, Result as SearchResult, ScoreBreakdown, Suggestion};
