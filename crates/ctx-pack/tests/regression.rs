// crates/ctx-pack/tests/regression.rs
//
// Regression tests that mirror internal/pack/ Go test cases. These
// run on the default `cargo test` invocation (no required features)
// so they catch byte-parity drift on every push.

use ctx_pack::diff::render as diff_render;
use ctx_pack::from_where::parse as from_where_parse;
use ctx_pack::preset::apply_preset;
use ctx_pack::redact::redact_lines;
use ctx_pack::relevance::{extract_goal_keywords, score_relevance};
use ctx_pack::types::{
    DiffEntry, DiffOptions, FileInput, MetadataInput, SymbolInput, WarningInput,
};

fn make_file(path: &str, role: &str, syms: &[(&str, &str)]) -> FileInput {
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
fn t_extract_goal_keywords_japanese_and_aliases() {
    let got = extract_goal_keywords("ログイン処理のバグを調べたい");
    for want in [
        "ログイン",
        "login",
        "auth",
        "session",
        "ログイン処理",
        "バグ",
    ] {
        assert!(got.iter().any(|w| w == want), "missing {want:?} in {got:?}");
    }
    for unwanted in ["の", "を", "調べたい"] {
        assert!(
            !got.iter().any(|w| w == unwanted),
            "should not contain {unwanted:?}: {got:?}"
        );
    }
}

#[test]
fn t_score_relevance_uses_basename_path_symbol_and_role() {
    let fi = make_file(
        "src/auth/login.ts",
        "core",
        &[("validateLoginSession", "function")],
    );
    let got = score_relevance(&fi, "ログイン認証", 100, 30000);
    assert_eq!(got.tier, "High");
    assert!(got.score >= 20, "score {} too low: {:?}", got.score, got);
    for signal in [
        r#"basename "login""#,
        r#"path "auth""#,
        r#"symbol "validateLoginSession""#,
        "role core +2",
    ] {
        assert!(
            got.reason.contains(signal),
            "reason missing {signal:?}: {}",
            got.reason
        );
    }
}

#[test]
fn t_score_relevance_medium_from_role() {
    let fi = make_file("cmd/ctx/main.go", "entry", &[]);
    // Force tokens_est==20 so the test budget signal triggers exactly like Go.
    let mut fi = fi;
    fi.metadata.tokens_est = 20;
    fi.tokens = 20;
    let got = score_relevance(&fi, "billing", 20, 1000);
    assert_eq!(got.tier, "Medium");
    assert_eq!(got.score, 3);
}

#[test]
fn t_score_relevance_excluded_reasons() {
    let cases: [(&str, FileInput, &str); 3] = [
        (
            "outside scope",
            make_file("internal/render/tree.go", "unknown", &[]),
            "outside goal scope",
        ),
        (
            "low relevance",
            make_file("internal/pack/pack_test.go", "test", &[]),
            "low relevance",
        ),
        (
            "generated",
            make_file("dist/app.js", "core", &[]),
            "generated",
        ),
    ];
    for (name, fi, want) in cases {
        let got = score_relevance(&fi, "認証", 10, 1000);
        assert_eq!(got.tier, "", "case {name}: tier should be empty: {:?}", got);
        assert_eq!(got.reason, want, "case {name}");
    }
}

#[test]
fn t_doc_role_boost_when_goal_mentions_docs() {
    let fi = make_file("README.md", "doc", &[]);
    let got = score_relevance(&fi, "認証 docs", 10, 1000);
    assert!(
        got.score > 0,
        "doc goal should avoid doc penalty: {:?}",
        got
    );
}

#[test]
fn t_diff_unified_layout() {
    let d = DiffEntry {
        path: "a.go".into(),
        before_content: "old".into(),
        after_content: "new".into(),
        before_commit: "abc".into(),
        after_commit: "def".into(),
        patch: "--- a/a.go\n+++ b/a.go\n@@ @@\n-old\n+new\n".into(),
        added: false,
        deleted: false,
        binary: false,
    };
    let out = diff_render(
        &[d],
        &DiffOptions {
            layout: "unified".into(),
            preset: String::new(),
        },
    );
    assert!(out.starts_with("```diff"));
    assert!(out.contains("@@ @@"));
}

#[test]
fn t_diff_default_sequential() {
    let d = DiffEntry {
        path: "a.go".into(),
        before_content: "old\n".into(),
        after_content: "new\n".into(),
        before_commit: "abc".into(),
        after_commit: "def".into(),
        patch: String::new(),
        added: false,
        deleted: false,
        binary: false,
    };
    let out = diff_render(&[d], &DiffOptions::default());
    assert!(out.contains("### a.go"));
    assert!(out.contains("**Before** (commit abc):"));
    assert!(out.contains("**After** (commit def):"));
    assert!(out.contains("```go"));
}

#[test]
fn t_redact_replaces_marked_line() {
    let data = b"line one\nSECRET=abc\nline three";
    let warnings = [WarningInput {
        path: String::new(),
        line: 2,
        kind: "aws_key".into(),
    }];
    let out = redact_lines(data, &warnings);
    let s = String::from_utf8(out).unwrap();
    assert_eq!(s, "line one\n[REDACTED — kind=aws_key]\nline three");
}

#[test]
fn t_from_where_json_sorted() {
    let body = br#"[{"path":"b.go","score":0.5},{"path":"a.go","score":0.9}]"#;
    let r = from_where_parse(body).unwrap();
    assert_eq!(r, vec!["a.go", "b.go"]);
}

#[test]
fn t_from_where_lines_dedup() {
    let body = b"a.go\n# comment\nb.go\na.go\n";
    let r = from_where_parse(body).unwrap();
    assert_eq!(r, vec!["a.go", "b.go"]);
}

#[test]
fn t_preset_blog() {
    let p = apply_preset("blog").unwrap();
    assert_eq!(p.format.as_deref(), Some("markdown"));
    assert_eq!(p.no_warnings, Some(true));
    assert_eq!(p.no_paths, Some(true));
    assert_eq!(p.frontmatter.as_deref(), Some("mdx"));
}

#[test]
fn t_preset_unknown_errors() {
    assert!(apply_preset("bogus").is_err());
}
