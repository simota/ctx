// crates/ctx-relations/tests/regression.rs
//
// Regression tests pinning edge cases discovered during the port.

use std::path::PathBuf;

use ctx_relations::build::build;

fn write_tree(dir: &std::path::Path, files: &[(&str, &str)]) {
    for (rel, content) in files {
        let p = dir.join(rel);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        std::fs::write(&p, content).unwrap();
    }
}

fn tmp_dir(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "rel-regress-{}-{}-{}",
        tag,
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn empty_repo_returns_empty_index() {
    let dir = tmp_dir("empty");
    let idx = build(&dir.to_string_lossy()).unwrap();
    assert_eq!(idx.module_path, "");
    assert!(idx.imports.is_empty());
    assert!(idx.importers.is_empty());
}

#[test]
fn unsupported_files_are_ignored() {
    let dir = tmp_dir("unsupported");
    write_tree(
        &dir,
        &[
            ("README.md", "# hello"),
            ("data.json", "{}"),
            ("hidden.txt", "x"),
        ],
    );
    let idx = build(&dir.to_string_lossy()).unwrap();
    assert!(idx.imports.is_empty());
}

#[test]
fn comment_only_go_file_produces_no_edges() {
    let dir = tmp_dir("comment");
    write_tree(
        &dir,
        &[
            ("go.mod", "module example.com/m\n"),
            ("a.go", "// just a comment\npackage m\n"),
        ],
    );
    let idx = build(&dir.to_string_lossy()).unwrap();
    assert!(idx.imports.is_empty());
}

#[test]
fn js_bare_specifier_is_dropped() {
    let dir = tmp_dir("js-bare");
    write_tree(
        &dir,
        &[
            ("a.ts", "import x from \"react\";\n"),
        ],
    );
    let idx = build(&dir.to_string_lossy()).unwrap();
    assert!(idx.imports.is_empty(), "{:?}", idx);
}

#[test]
fn php_without_composer_returns_no_edges() {
    let dir = tmp_dir("php-no-composer");
    write_tree(
        &dir,
        &[
            (
                "src/Foo.php",
                "<?php\nnamespace App;\nuse App\\Bar;\nclass Foo {}\n",
            ),
            ("src/Bar.php", "<?php\nnamespace App;\nclass Bar {}\n"),
        ],
    );
    let idx = build(&dir.to_string_lossy()).unwrap();
    // No composer.json → resolvePHPImports returns nil for every file.
    assert!(idx.imports.is_empty(), "{:?}", idx);
}

#[test]
fn swift_outside_sources_produces_no_edges() {
    let dir = tmp_dir("swift-outside");
    write_tree(
        &dir,
        &[
            ("main.swift", "import Foundation\n"),
        ],
    );
    let idx = build(&dir.to_string_lossy()).unwrap();
    // No `Sources/<Module>/` layout → no module index → no edges.
    assert!(idx.imports.is_empty());
}

#[test]
fn build_is_deterministic() {
    let dir = tmp_dir("determinism");
    write_tree(
        &dir,
        &[
            ("go.mod", "module example.com/m\n"),
            (
                "a.go",
                "package m\nimport \"example.com/m/lib\"\n",
            ),
            ("lib/x.go", "package lib\n"),
            ("lib/y.go", "package lib\n"),
        ],
    );
    let root = dir.to_string_lossy();
    let first = build(&root).unwrap();
    let second = build(&root).unwrap();
    assert_eq!(first, second);
}
