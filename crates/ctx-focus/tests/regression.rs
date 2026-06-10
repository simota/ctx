// crates/ctx-focus/tests/regression.rs
//
// Regression tests pinning edge cases discovered during the Phase 4 focus port.

use ctx_focus::{
    expand, resolve_anchor,
    types::{AnchorKind, ExpandOptions, SymbolInfo},
    FileInput,
};

fn mkfile(path: &str, lines: Vec<&str>, syms: Vec<(&str, i64)>) -> FileInput {
    FileInput {
        path: path.into(),
        is_dir: false,
        symbols: syms
            .into_iter()
            .map(|(n, l)| SymbolInfo {
                name: n.into(),
                kind: "function".into(),
                line: l,
            })
            .collect(),
        lines: lines.into_iter().map(String::from).collect(),
    }
}

#[test]
fn identifier_pattern_respects_camel_case_boundaries() {
    // The name "Foo" should NOT match inside "FooBar" — the
    // boundary regex is (?:^|[^A-Za-z0-9_])Foo(?:[^A-Za-z0-9_]|$),
    // so leading or trailing alnum kills the match.
    let files = vec![
        mkfile(
            "a/anchor.go",
            vec!["package x", "func Foo() {}"],
            vec![("Foo", 2)],
        ),
        mkfile(
            "b/unrelated.go",
            vec!["package y", "func FooBar() {}", "func FooBaz() {}"],
            vec![],
        ),
    ];
    let anchor = resolve_anchor(&files, "Foo").unwrap();
    let out = expand(&files, &anchor, &ExpandOptions { hops: 1 });
    let paths: Vec<&str> = out.iter().map(|f| f.path.as_str()).collect();
    // anchor.go appears as anchor-origin; unrelated.go (different dir,
    // different stem) must NOT appear because its only occurrences are
    // inside CamelCase compounds.
    assert!(paths.contains(&"a/anchor.go"));
    assert!(
        !paths.contains(&"b/unrelated.go"),
        "compound identifiers should not match: {paths:?}"
    );
}

#[test]
fn identifier_pattern_matches_at_word_boundary() {
    // Inside "Pack()" the `(` is a non-alnum boundary, so the pattern matches.
    let files = vec![
        mkfile("pack.go", vec!["package pack", "func Pack() {}"], vec![("Pack", 2)]),
        mkfile(
            "caller.go",
            vec!["package caller", "// invokes Pack(x) here"],
            vec![],
        ),
    ];
    let a = resolve_anchor(&files, "Pack").unwrap();
    let out = expand(&files, &a, &ExpandOptions { hops: 1 });
    let paths: Vec<&str> = out.iter().map(|f| f.path.as_str()).collect();
    assert!(paths.contains(&"caller.go"), "{paths:?}");
}

#[test]
fn ambiguous_resolution_returns_all_candidates() {
    let files = vec![
        mkfile("a.go", vec!["package a"], vec![("X", 1)]),
        mkfile("b.go", vec!["package b"], vec![("X", 1)]),
        mkfile("c.go", vec!["package c"], vec![("X", 1)]),
    ];
    let err = resolve_anchor(&files, "X").unwrap_err();
    assert_eq!(err.candidates.len(), 3);
}

#[test]
fn dir_entries_skipped() {
    let files = vec![FileInput {
        path: ".".into(),
        is_dir: true,
        symbols: vec![],
        lines: vec![],
    }];
    let err = resolve_anchor(&files, "anything").unwrap_err();
    assert!(err.candidates.is_empty());
}

#[test]
fn empty_corpus_yields_not_found() {
    let err = resolve_anchor(&[], "Foo").unwrap_err();
    assert!(err.candidates.is_empty());
    assert_eq!(err.anchor, "Foo");
}

#[test]
fn anchor_kind_serialises_as_lowercase() {
    let a = ctx_focus::types::Anchor {
        kind: AnchorKind::Symbol,
        raw: "Pack".into(),
        name: "Pack".into(),
        origin_path: "a.go".into(),
    };
    let j = serde_json::to_string(&a).unwrap();
    assert!(j.contains("\"Kind\":\"symbol\""), "{j}");
    assert!(j.contains("\"Raw\":\"Pack\""), "{j}");
    assert!(j.contains("\"OriginPath\":\"a.go\""), "{j}");
}
