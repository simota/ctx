// crates/ctx-where/tests/regression.rs
//
// Regression tests pinning edge cases discovered during the Phase 3
// where port — Levenshtein bounds, split_identifier corner cases, and
// the score zero-budget result.

use ctx_where::levenshtein::levenshtein;
use ctx_where::score::{extract_keywords, split_identifier};
use ctx_where::search::{search_with_options, FileInput, Options, SymbolInput};

fn chars(s: &str) -> Vec<char> {
    s.chars().collect()
}

fn mkfile(path: &str, lines: Vec<&str>, syms: Vec<(&str, i64)>) -> FileInput {
    FileInput {
        path: path.into(),
        is_dir: false,
        symbols: syms
            .into_iter()
            .map(|(n, l)| SymbolInput {
                name: n.into(),
                kind: "function".into(),
                line: l,
            })
            .collect(),
        lines: lines.into_iter().map(String::from).collect(),
    }
}

#[test]
fn levenshtein_canonical_examples() {
    assert_eq!(levenshtein(&chars("kitten"), &chars("sitting")), 3);
    assert_eq!(levenshtein(&chars("Saturday"), &chars("Sunday")), 3);
    assert_eq!(levenshtein(&chars("abc"), &chars("abc")), 0);
}

#[test]
fn split_identifier_examples() {
    assert_eq!(
        split_identifier("getUserByID"),
        vec!["get", "user", "by", "id"]
    );
    assert_eq!(
        split_identifier("parseHTTPHeader"),
        vec!["parse", "http", "header"]
    );
    assert_eq!(
        split_identifier("user_repository"),
        vec!["user", "repository"]
    );
    assert_eq!(split_identifier("v2User"), vec!["user"]);
}

#[test]
fn extract_keywords_handles_japanese() {
    let kws = extract_keywords("ユーザ session");
    assert!(kws.contains(&"session".to_string()));
}

#[test]
fn search_respects_limit() {
    let files = vec![
        mkfile("a/pack.go", vec!["package a"], vec![]),
        mkfile("b/pack.go", vec!["package b"], vec![]),
        mkfile("c/pack.go", vec!["package c"], vec![]),
    ];
    let opts = Options {
        limit: 2,
        ..Default::default()
    };
    let r = search_with_options(&files, "pack", &opts);
    assert_eq!(r.len(), 2);
}

#[test]
fn search_require_all_filters() {
    let files = vec![
        mkfile("auth/session.go", vec!["package auth"], vec![]),
        mkfile("util/strings.go", vec!["package util"], vec![]),
    ];
    let opts = Options {
        limit: 10,
        require_all: true,
        ..Default::default()
    };
    let r = search_with_options(&files, "session util", &opts);
    // Neither file has BOTH keywords — empty result.
    assert!(r.is_empty());
}

#[test]
fn search_context_n_attaches_lines() {
    let files = vec![mkfile(
        "head.go",
        vec!["// l1", "// l2", "func alpha() {}", "// l4", "// l5"],
        vec![],
    )];
    let opts = Options {
        limit: 10,
        context_n: 1,
        ..Default::default()
    };
    let r = search_with_options(&files, "alpha", &opts);
    assert!(!r.is_empty());
    let m = r[0].matches.iter().find(|m| m.kind == "content").expect("content match");
    assert_eq!(m.before.len(), 1);
    assert_eq!(m.after.len(), 1);
}

#[test]
fn empty_query_matches_everything_for_parity_with_go() {
    let files = vec![mkfile("a.go", vec!["package a"], vec![])];
    let r = search_with_options(&files, "", &Options::default());
    // Go fallback: empty extract_keywords → keywords = [""], and
    // strings.Contains(_, "") is true for every file. Score is non-zero.
    // We intentionally match Go semantics here for parity.
    assert!(!r.is_empty(), "Go semantics: empty query matches all files");
}

#[test]
fn search_dir_entry_skipped() {
    let files = vec![FileInput {
        path: ".".into(),
        is_dir: true,
        symbols: vec![],
        lines: vec![],
    }];
    let r = search_with_options(&files, "anything", &Options::default());
    assert!(r.is_empty());
}
