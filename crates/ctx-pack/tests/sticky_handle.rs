// crates/ctx-pack/tests/sticky_handle.rs
//
// Soak test for the sessioned relevance API. Mirrors the contract
// patterns from ctx-where/tests/sticky_handle.rs.

use ctx_pack::relevance::session::RelevanceSession;
use ctx_pack::types::{FileInput, MetadataInput, SymbolInput};

fn make_corpus() -> Vec<FileInput> {
    vec![
        FileInput {
            path: "src/auth/login.ts".into(),
            abs_path: String::new(),
            is_dir: false,
            tokens: 100,
            role: "core".into(),
            metadata: MetadataInput {
                size: 100,
                tokens_est: 100,
                role: "core".into(),
                symbols: vec![SymbolInput {
                    name: "validateLoginSession".into(),
                    kind: "function".into(),
                    line: 1,
                }],
            },
            content_head: Vec::new(),
        },
        FileInput {
            path: "internal/render/tree.go".into(),
            abs_path: String::new(),
            is_dir: false,
            tokens: 80,
            role: "unknown".into(),
            metadata: MetadataInput {
                size: 80,
                tokens_est: 80,
                role: "unknown".into(),
                symbols: vec![],
            },
            content_head: Vec::new(),
        },
        FileInput {
            path: "cmd/ctx/main.go".into(),
            abs_path: String::new(),
            is_dir: false,
            tokens: 200,
            role: "entry".into(),
            metadata: MetadataInput {
                size: 200,
                tokens_est: 200,
                role: "entry".into(),
                symbols: vec![],
            },
            content_head: Vec::new(),
        },
    ]
}

#[test]
fn session_open_close_5000_cycles() {
    for _ in 0..5000 {
        let s = RelevanceSession::new("ログイン認証", 30000);
        assert!(!s.goal_keywords().is_empty());
    }
}

#[test]
fn session_score_same_corpus_many_times() {
    let corpus = make_corpus();
    let s = RelevanceSession::new("ログイン認証", 30000);
    for _ in 0..5000 {
        for fi in &corpus {
            let r = s.score_file(fi, fi.tokens);
            let _ = r.score;
        }
    }
}

#[test]
fn session_score_corpus_results_match_individual_calls() {
    let corpus = make_corpus();
    let s = RelevanceSession::with_corpus("ログイン認証", 30000, corpus.clone(), vec![100, 80, 200]);
    let batch = s.score_corpus();
    assert_eq!(batch.len(), corpus.len());
    for (i, fi) in corpus.iter().enumerate() {
        let single = s.score_file(fi, fi.tokens);
        assert_eq!(batch[i].score, single.score, "score mismatch at {}", fi.path);
        assert_eq!(batch[i].tier, single.tier, "tier mismatch at {}", fi.path);
        assert_eq!(batch[i].reason, single.reason, "reason mismatch at {}", fi.path);
    }
}

#[test]
fn session_rank_top_n_orders_correctly() {
    let corpus = make_corpus();
    let s = RelevanceSession::with_corpus("ログイン認証", 30000, corpus, vec![100, 80, 200]);
    let top = s.rank_top_n(2);
    // src/auth/login.ts should rank first; cmd/ctx/main.go second.
    assert_eq!(top.len(), 2);
    assert_eq!(top[0].0, 0);
}
