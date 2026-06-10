// crates/ctx-symbols/tests/parity.rs — parity vs reference goldens.
//
// We load JSON goldens shipped under `tests/parity/symbols-goldens/`.
// Each fixture has:
//   - apionly_input.json: APIRenderRequest
//   - apionly_output.json: {"rendered": "..."}
//   - corpus.json: Vec<FileSymbols>
//   - lookup_queries.json: Vec<{name, from, kind}>
//   - lookup_resolve_output.json: Vec<Vec<Hit>>  (one per query)

#![cfg(feature = "testing")]

use ctx_symbols::{
    extract, render_api, resolve, APIRenderRequest, FileSymbols, Hit, LookupArgs, Symbol,
};
use std::path::{Path, PathBuf};

fn goldens_root() -> PathBuf {
    // crates/ctx-symbols/tests/parity.rs -> repo/tests/parity/symbols-goldens/
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // repo/
    p.push("tests/parity/symbols-goldens");
    p
}

fn read_json<T: serde::de::DeserializeOwned>(p: &std::path::Path) -> T {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("read {}: {e}", p.display()));
    serde_json::from_slice(&bytes)
        .unwrap_or_else(|e| panic!("parse {}: {e}", p.display()))
}

fn for_each_fixture<F: FnMut(&str, PathBuf)>(mut f: F) {
    let root = goldens_root();
    if !root.exists() {
        return;
    }
    let mut entries: Vec<_> = std::fs::read_dir(&root)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.path());
    for e in entries {
        let name = e.file_name().to_string_lossy().to_string();
        f(&name, e.path());
    }
}

#[test]
fn parity_apionly_render() {
    for_each_fixture(|name, dir| {
        let input_path = dir.join("apionly_input.json");
        let output_path = dir.join("apionly_output.json");
        if !input_path.exists() || !output_path.exists() {
            return;
        }
        let req: APIRenderRequest = read_json(&input_path);
        let expected: serde_json::Value = read_json(&output_path);
        let rendered = render_api(&req);
        let want = expected["rendered"].as_str().unwrap_or("");
        assert_eq!(
            rendered, want,
            "fixture {name}: apionly render mismatch"
        );
    });
}

/// Map a golden fixture dir name to its on-disk source corpus under
/// `tests/symbols-fixtures/<name>_corpus`.
fn corpus_source_dir(name: &str) -> PathBuf {
    let mut p = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    p.pop(); // crates/
    p.pop(); // repo/
    p.push("tests/symbols-fixtures");
    p.push(format!("{name}_corpus"));
    p
}

/// Walk `dir` and produce the same `Vec<FileSymbols>` corpus shape the Go
/// oracle's `buildCorpus` produces: visit every file, extract natively,
/// skip files with zero symbols, forward-slash repo-relative paths,
/// ordered by directory walk. We sort the final corpus by path so the
/// comparison is deterministic regardless of readdir order on either side.
fn build_native_corpus(dir: &Path) -> Vec<FileSymbols> {
    fn walk(dir: &Path, root: &Path, out: &mut Vec<FileSymbols>) {
        let mut entries: Vec<_> = std::fs::read_dir(dir)
            .unwrap_or_else(|e| panic!("read_dir {}: {e}", dir.display()))
            .filter_map(|e| e.ok())
            .collect();
        entries.sort_by_key(|e| e.path());
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, out);
                continue;
            }
            let syms = extract(&path).unwrap_or_else(|e| panic!("extract {}: {e}", path.display()));
            if syms.is_empty() {
                continue;
            }
            let rel = path
                .strip_prefix(root)
                .unwrap()
                .to_string_lossy()
                .replace('\\', "/");
            out.push(FileSymbols { path: rel, symbols: syms });
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    out
}

/// Per-language empirical byte-parity for the NATIVE tree-sitter extractor
/// vs the Go oracle. The Go oracle wrote `corpus.json` from
/// `internal/symbols.Extractor.Extract`; we re-extract the same on-disk
/// corpus natively and assert identical {Name, Kind, Line} tuples in
/// identical order (DFS pre-order, dedup by kind\x00name).
#[test]
fn parity_native_extract_corpus() {
    let mut checked = 0usize;
    for_each_fixture(|name, dir| {
        let corpus_path = dir.join("corpus.json");
        if !corpus_path.exists() {
            return;
        }
        let src_dir = corpus_source_dir(name);
        if !src_dir.exists() {
            return; // golden without an on-disk source corpus (skip).
        }
        let mut expected: Vec<FileSymbols> = read_json(&corpus_path);
        expected.sort_by(|a, b| a.path.cmp(&b.path));
        let got = build_native_corpus(&src_dir);
        assert_eq!(
            got, expected,
            "fixture {name}: native extraction diverges from Go oracle corpus.json"
        );
        checked += 1;
    });
    assert!(checked > 0, "no extraction fixtures were checked");
}

/// Focused per-language sanity: confirms each supported extension extracts
/// at least the expected representative symbols. Guards against a grammar
/// silently producing zero symbols (which would pass the corpus test only
/// by both sides being empty).
#[test]
fn native_extract_each_language_nonempty() {
    let lang = corpus_source_dir("lang");
    if !lang.exists() {
        return;
    }
    let cases: &[(&str, &str, &str, i32)] = &[
        ("go/generics.go", "Sum", "function", 17),
        ("js/app.js", "Widget", "class", 18),
        ("jsx/component.jsx", "Button", "function", 4),
        ("ts/generics.ts", "Repository", "interface", 5),
        ("tsx/view.tsx", "ViewMode", "type", 9),
        ("py/nested.py", "inner", "function", 6),
    ];
    for (rel, name, kind, line) in cases {
        let syms = extract(lang.join(rel)).unwrap();
        assert!(!syms.is_empty(), "{rel}: extracted no symbols");
        let want = Symbol { name: name.to_string(), kind: kind.to_string(), line: *line };
        assert!(
            syms.contains(&want),
            "{rel}: expected {want:?} in {syms:?}"
        );
    }
}

/// Files over the 500 KiB cap and unsupported extensions yield no symbols,
/// mirroring Go's `(nil, nil)` returns.
#[test]
fn native_extract_unsupported_and_oversize() {
    let tmp = std::env::temp_dir().join(format!("ctx-extract-{}", std::process::id()));
    std::fs::create_dir_all(&tmp).unwrap();
    // Unsupported extension.
    let txt = tmp.join("readme.txt");
    std::fs::write(&txt, "func Foo() {}\n").unwrap();
    assert!(extract(&txt).unwrap().is_empty());
    // Oversize .go file (>500 KiB) → skipped.
    let big = tmp.join("big.go");
    let mut body = String::from("package big\n");
    body.push_str(&"// pad\n".repeat(80_000));
    body.push_str("func Real() {}\n");
    assert!(body.len() as u64 > 500 * 1024);
    std::fs::write(&big, body).unwrap();
    assert!(extract(&big).unwrap().is_empty());
    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn parity_lookup_resolve() {
    for_each_fixture(|name, dir| {
        let corpus_path = dir.join("corpus.json");
        let queries_path = dir.join("lookup_queries.json");
        let output_path = dir.join("lookup_resolve_output.json");
        if !corpus_path.exists() || !queries_path.exists() || !output_path.exists() {
            return;
        }
        let corpus: Vec<FileSymbols> = read_json(&corpus_path);
        let queries: Vec<LookupArgs> = read_json(&queries_path);
        let expected: Vec<Vec<Hit>> = read_json(&output_path);
        assert_eq!(queries.len(), expected.len(), "fixture {name}: query/output length mismatch");
        for (i, q) in queries.iter().enumerate() {
            let got = resolve(&corpus, q);
            assert_eq!(got, expected[i], "fixture {name} query #{i} {q:?}: mismatch");
        }
    });
}
