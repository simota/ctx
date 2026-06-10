// crates/ctx-symbols/src/lookup/session.rs
//
// Phase 4 ADR-002 sticky-handle for symbol lookup: open once with a
// pre-extracted corpus (Vec<FileSymbols>), answer N queries against
// the cached corpus.
//
// Why session-shaped given only one Go caller? The web handler at
// internal/web/handlers.go:713 does a fresh walk+extract on EVERY
// /api/definition request. A pool that opens a session per repo root
// and resolves N name lookups against it amortises the corpus
// preparation cost across all requests for that root.
//
// Honest L1-L4 prediction (recorded in PHASE4_REPORT.md):
//   - Per-query cost in Rust is hash-equality + small vec sort —
//     sub-microsecond intrinsic.
//   - The cgo floor (~5-10 µs per call) dominates a single query.
//   - Multi-query amortisation against the SAME root is where this
//     pays off: N queries × 0.1 µs Rust vs N × (walk + extract) Go.
//   - VERDICT EXPECTATION: session-fit only when caller batches OR
//     reuses across requests. Per-query may still be EVIDENCE-ONLY.

use crate::lookup::resolve;
use crate::types::{FileSymbols, Hit, LookupArgs};

/// A `LookupSession` caches a corpus in memory and answers N lookups
/// without re-paying the Go-side walk + tree-sitter extract cost.
pub struct LookupSession {
    root: String,
    corpus: Vec<FileSymbols>,
}

impl LookupSession {
    /// Open a session over a pre-extracted symbol corpus. `root` is
    /// stored for diagnostics and to give callers an idempotent
    /// per-root key in a pool.
    pub fn open(root: &str, corpus: Vec<FileSymbols>) -> Self {
        Self {
            root: root.to_string(),
            corpus,
        }
    }

    pub fn root(&self) -> &str {
        &self.root
    }

    pub fn corpus_len(&self) -> usize {
        self.corpus.len()
    }

    pub fn total_symbols(&self) -> usize {
        self.corpus.iter().map(|fs| fs.symbols.len()).sum()
    }

    /// Resolve a name against the session corpus, returning hits per
    /// `LookupArgs.{from, kind}`. Stable + lexically sorted per
    /// lookup.go::sortHits.
    pub fn resolve(&self, args: &LookupArgs) -> Vec<Hit> {
        resolve(&self.corpus, args)
    }

    /// find_references is a thin alias for `resolve` — kept distinct
    /// so the FFI surface can grow into per-kind-aware reference
    /// matching without churning the public API.
    pub fn find_references(&self, args: &LookupArgs) -> Vec<Hit> {
        self.resolve(args)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Symbol;

    fn make_corpus() -> Vec<FileSymbols> {
        vec![
            FileSymbols {
                path: "internal/web/handlers.go".to_string(),
                symbols: vec![
                    Symbol {
                        name: "ListFiles".to_string(),
                        kind: "function".to_string(),
                        line: 100,
                    },
                    Symbol {
                        name: "BuildIndex".to_string(),
                        kind: "function".to_string(),
                        line: 200,
                    },
                ],
            },
            FileSymbols {
                path: "internal/pack/pack.go".to_string(),
                symbols: vec![Symbol {
                    name: "BuildIndex".to_string(),
                    kind: "function".to_string(),
                    line: 50,
                }],
            },
        ]
    }

    #[test]
    fn session_open_records_root_and_corpus() {
        let s = LookupSession::open("/repo", make_corpus());
        assert_eq!(s.root(), "/repo");
        assert_eq!(s.corpus_len(), 2);
        assert_eq!(s.total_symbols(), 3);
    }

    #[test]
    fn session_resolve_returns_hits() {
        let s = LookupSession::open("/repo", make_corpus());
        let hits = s.resolve(&LookupArgs {
            name: "BuildIndex".to_string(),
            from: String::new(),
            kind: String::new(),
        });
        assert_eq!(hits.len(), 2);
    }

    #[test]
    fn session_resolve_respects_from_directory() {
        let s = LookupSession::open("/repo", make_corpus());
        let hits = s.resolve(&LookupArgs {
            name: "BuildIndex".to_string(),
            from: "internal/pack/diff.go".to_string(),
            kind: String::new(),
        });
        assert_eq!(hits.len(), 2);
        // pack should rank first because same-directory wins
        assert_eq!(hits[0].path, "internal/pack/pack.go");
    }

    #[test]
    fn session_resolve_respects_kind_filter() {
        let s = LookupSession::open("/repo", make_corpus());
        let hits = s.resolve(&LookupArgs {
            name: "BuildIndex".to_string(),
            from: String::new(),
            kind: "type".to_string(),
        });
        assert_eq!(hits.len(), 0);
    }

    #[test]
    fn session_find_references_aliases_resolve() {
        let s = LookupSession::open("/repo", make_corpus());
        let a = s.resolve(&LookupArgs {
            name: "ListFiles".to_string(),
            ..Default::default()
        });
        let b = s.find_references(&LookupArgs {
            name: "ListFiles".to_string(),
            ..Default::default()
        });
        assert_eq!(a, b);
    }

    #[test]
    fn session_supports_many_queries_no_panic() {
        let s = LookupSession::open("/repo", make_corpus());
        for _ in 0..1000 {
            let _ = s.resolve(&LookupArgs {
                name: "ListFiles".to_string(),
                ..Default::default()
            });
        }
    }
}
