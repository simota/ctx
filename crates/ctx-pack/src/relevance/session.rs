// crates/ctx-pack/src/relevance/session.rs
//
// Sticky-handle session API for the relevance scorer. Goal + budget
// are bound once on open; subsequent `score_file` calls re-use the
// extracted keywords. This is the equivalent of ctx-where's
// WhereSession — load corpus state ONCE, query MANY times.
//
// Memory model
// ============
// A RelevanceSession owns the precomputed RelevanceContext and an
// optional corpus snapshot. The FFI surface (ffi.rs) wraps it in a
// Box and surfaces it across cgo as an opaque pointer. The Go side
// MUST call session_close exactly once per successful open.
//
// Thread-safety
// =============
// RelevanceSession is immutable after construction. score_file takes
// &self and produces a fresh RelevanceResult per call; the Go side
// can call session_score concurrently across goroutines on the same
// handle. session_close MUST NOT race with score_file — the caller
// quiesces queries first.

use crate::relevance::{rank_top_n, score_all, score_relevance_with_ctx, RelevanceContext};
use crate::types::{FileInput, RelevanceResult};

#[derive(Debug, Clone)]
pub struct RelevanceSession {
    ctx: RelevanceContext,
    /// Optional pre-loaded corpus. When present the session owns the
    /// file list and the caller can rank without re-marshaling.
    corpus: Vec<FileInput>,
    corpus_token_counts: Vec<i64>,
}

impl RelevanceSession {
    /// Open a session bound to `goal` + `budget`. The corpus is
    /// optional — leave it empty when the Go caller wants the cheap
    /// keyword-cache-only variant (score every file individually).
    pub fn new(goal: &str, budget: i64) -> Self {
        Self {
            ctx: RelevanceContext::new(goal, budget),
            corpus: Vec::new(),
            corpus_token_counts: Vec::new(),
        }
    }

    /// Build with a pre-loaded corpus. token_counts may be shorter
    /// than files; missing entries fall back to FileInput.tokens.
    pub fn with_corpus(goal: &str, budget: i64, files: Vec<FileInput>, token_counts: Vec<i64>) -> Self {
        Self {
            ctx: RelevanceContext::new(goal, budget),
            corpus: files,
            corpus_token_counts: token_counts,
        }
    }

    pub fn goal_keywords(&self) -> &[String] {
        &self.ctx.goal_keywords
    }

    pub fn budget(&self) -> i64 {
        self.ctx.budget
    }

    pub fn corpus_len(&self) -> usize {
        self.corpus.len()
    }

    /// Score a single file against the session's pre-extracted
    /// keywords. The caller provides token_count because it may
    /// differ from the stored FileInput.tokens (e.g. tiktoken at
    /// pack time vs. cheap size estimate at walk time).
    pub fn score_file(&self, file: &FileInput, token_count: i64) -> RelevanceResult {
        score_relevance_with_ctx(file, &self.ctx, token_count)
    }

    /// Score every file in the loaded corpus. Returns the list of
    /// results in input order. Requires with_corpus() at open time.
    pub fn score_corpus(&self) -> Vec<RelevanceResult> {
        score_all(
            &self.ctx,
            &self.corpus,
            if self.corpus_token_counts.is_empty() {
                None
            } else {
                Some(&self.corpus_token_counts)
            },
        )
    }

    /// Score every file in the loaded corpus and return only the top
    /// N (sorted by score desc, path asc) along with their indices
    /// into the original corpus.
    pub fn rank_top_n(&self, n: usize) -> Vec<(usize, RelevanceResult)> {
        rank_top_n(
            &self.ctx,
            &self.corpus,
            if self.corpus_token_counts.is_empty() {
                None
            } else {
                Some(&self.corpus_token_counts)
            },
            n,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{FileInput, MetadataInput, SymbolInput};

    fn mkfile(path: &str, role: &str, syms: &[(&str, &str)]) -> FileInput {
        FileInput {
            path: path.into(),
            abs_path: String::new(),
            is_dir: false,
            tokens: 100,
            role: role.into(),
            metadata: MetadataInput {
                size: 100,
                tokens_est: 100,
                role: role.into(),
                symbols: syms
                    .iter()
                    .map(|(n, k)| SymbolInput {
                        name: (*n).into(),
                        kind: (*k).into(),
                        line: 1,
                    })
                    .collect(),
            },
            content_head: Vec::new(),
        }
    }

    #[test]
    fn session_score_matches_stateless() {
        let f = mkfile("src/auth/login.ts", "core", &[("validateLoginSession", "function")]);
        let s = RelevanceSession::new("ログイン認証", 30000);
        let sticky = s.score_file(&f, 100);
        let stateless = super::super::score_relevance(&f, "ログイン認証", 100, 30000);
        assert_eq!(sticky.score, stateless.score);
        assert_eq!(sticky.tier, stateless.tier);
        assert_eq!(sticky.breakdown, stateless.breakdown);
        assert_eq!(sticky.reason, stateless.reason);
    }

    #[test]
    fn session_caches_keywords() {
        let s = RelevanceSession::new("ログイン認証", 30000);
        let kws = s.goal_keywords();
        assert!(kws.contains(&"login".to_string()));
        assert!(kws.contains(&"auth".to_string()));
    }

    #[test]
    fn session_with_corpus_ranks_files() {
        let files = vec![
            mkfile("src/auth/login.ts", "core", &[("validateLoginSession", "function")]),
            mkfile("cmd/ctx/main.go", "entry", &[]),
            mkfile("internal/render/tree.go", "unknown", &[]),
        ];
        let s = RelevanceSession::with_corpus("ログイン認証", 30000, files, vec![100, 100, 100]);
        let top = s.rank_top_n(2);
        assert!(!top.is_empty());
        // src/auth/login.ts should win.
        assert_eq!(top[0].0, 0);
    }
}
