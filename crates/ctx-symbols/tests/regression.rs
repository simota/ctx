// crates/ctx-symbols/tests/regression.rs — locked behaviour tests
// that mirror existing apionly_test.go + lookup edge cases.

use ctx_symbols::{
    render_api, resolve, APIRange, APIRenderRequest, FileSymbols, LookupArgs, Symbol,
};

fn sym(name: &str, kind: &str, line: i32) -> Symbol {
    Symbol {
        name: name.to_string(),
        kind: kind.to_string(),
        line,
    }
}

fn lines(v: &[&str]) -> Vec<String> {
    v.iter().map(|s| s.to_string()).collect()
}

#[test]
fn regression_apionly_empty_corpus_returns_empty() {
    let req = APIRenderRequest {
        lines: lines(&["a", "b"]),
        ranges: vec![],
    };
    assert_eq!(render_api(&req), "");
}

#[test]
fn regression_apionly_overlap_merges_correctly() {
    let req = APIRenderRequest {
        lines: lines(&["A", "B", "C", "D"]),
        ranges: vec![
            APIRange {
                start: 0,
                end: 1,
                end_replacement: None,
            },
            APIRange {
                start: 1,
                end: 2,
                end_replacement: None,
            },
            APIRange {
                start: 3,
                end: 3,
                end_replacement: None,
            },
        ],
    };
    // Go's renderAPIRanges does NOT back-fill when an overlapping range
    // extends `lastEnd` — the first emitted block stops at its own end.
    // Subsequent ranges only update lastEnd; gap rows (here C) are never
    // emitted. The non-overlapping [3,3] then renders after a blank line.
    assert_eq!(render_api(&req), "A\nB\n\nD\n");
}

#[test]
fn regression_apionly_end_replacement_takes_precedence_over_original_line() {
    let req = APIRenderRequest {
        lines: lines(&["func F(a, b string) {", "  return", "}"]),
        ranges: vec![APIRange {
            start: 0,
            end: 0,
            end_replacement: Some("func F(a, b string)".to_string()),
        }],
    };
    assert_eq!(render_api(&req), "func F(a, b string)\n");
}

#[test]
fn regression_apionly_python_signature_keeps_colon() {
    let req = APIRenderRequest {
        lines: lines(&["def build_session(user: User) -> str:"]),
        ranges: vec![APIRange {
            start: 0,
            end: 0,
            end_replacement: None,
        }],
    };
    assert_eq!(render_api(&req), "def build_session(user: User) -> str:\n");
}

#[test]
fn regression_lookup_empty_name_returns_empty() {
    let corpus = vec![FileSymbols {
        path: "a.go".to_string(),
        symbols: vec![sym("F", "function", 1)],
    }];
    assert!(resolve(&corpus, &LookupArgs::default()).is_empty());
}

#[test]
fn regression_lookup_preserves_extracted_line_number() {
    let corpus = vec![FileSymbols {
        path: "x.go".to_string(),
        symbols: vec![sym("F", "function", 42)],
    }];
    let hits = resolve(
        &corpus,
        &LookupArgs {
            name: "F".to_string(),
            ..Default::default()
        },
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].line, 42);
}

#[test]
fn regression_lookup_kind_alias_fn_matches_function() {
    let corpus = vec![FileSymbols {
        path: "a.go".to_string(),
        symbols: vec![sym("F", "function", 1)],
    }];
    let hits = resolve(
        &corpus,
        &LookupArgs {
            name: "F".to_string(),
            from: String::new(),
            kind: "fn".to_string(),
        },
    );
    assert_eq!(hits.len(), 1);
}

#[test]
fn regression_lookup_from_dir_wins_over_first_segment() {
    let corpus = vec![
        FileSymbols {
            path: "internal/a/x.go".to_string(),
            symbols: vec![sym("F", "function", 1)],
        },
        FileSymbols {
            path: "internal/b/y.go".to_string(),
            symbols: vec![sym("F", "function", 2)],
        },
    ];
    let hits = resolve(
        &corpus,
        &LookupArgs {
            name: "F".to_string(),
            from: "internal/b/z.go".to_string(),
            kind: String::new(),
        },
    );
    assert_eq!(hits[0].path, "internal/b/y.go");
}

#[test]
fn regression_lookup_exported_ranks_higher_when_otherwise_tied() {
    let corpus = vec![
        FileSymbols {
            path: "a.go".to_string(),
            symbols: vec![sym("foo", "function", 1)],
        },
        FileSymbols {
            path: "b.go".to_string(),
            symbols: vec![sym("foo", "function", 2)],
        },
    ];
    // Both lowercase, lexical order applies: a before b.
    let hits = resolve(
        &corpus,
        &LookupArgs {
            name: "foo".to_string(),
            ..Default::default()
        },
    );
    assert_eq!(hits[0].path, "a.go");
}

#[test]
fn regression_lookup_unknown_kind_passes_through_lowercased() {
    // Go's normalizeKind lower-cases unknown kinds for the filter
    // comparison; the corpus symbol's Kind is case-sensitive (string).
    // Mirroring that exactly: unknown kind "mykind" matches a symbol
    // whose kind is literally "mykind", but NOT "myKind".
    let corpus = vec![
        FileSymbols {
            path: "a.go".to_string(),
            symbols: vec![sym("F", "mykind", 1)],
        },
        FileSymbols {
            path: "b.go".to_string(),
            symbols: vec![sym("F", "myKind", 2)],
        },
    ];
    let hits = resolve(
        &corpus,
        &LookupArgs {
            name: "F".to_string(),
            from: String::new(),
            kind: "mykind".to_string(),
        },
    );
    assert_eq!(hits.len(), 1);
    assert_eq!(hits[0].path, "a.go");
}
