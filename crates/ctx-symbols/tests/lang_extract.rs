// crates/ctx-symbols/tests/lang_extract.rs — locks symbol extraction for the
// Rust/Swift/Kotlin grammars added beyond the Go-oracle set (go/ts/js/py).
// These have no byte-parity golden (no Go counterpart), so we assert the
// extracted {name, kind, line} tuples directly against fixed source snippets.

use ctx_symbols::{extract, Symbol};
use std::path::PathBuf;

/// Write `src` to a uniquely-named temp file with `ext` and extract symbols.
fn extract_src(ext: &str, tag: &str, src: &str) -> Vec<Symbol> {
    let mut dir = std::env::temp_dir();
    dir.push(format!("ctx-symbols-lang-{}-{}", std::process::id(), tag));
    std::fs::create_dir_all(&dir).unwrap();
    let path: PathBuf = dir.join(format!("fixture.{ext}"));
    std::fs::write(&path, src).unwrap();
    let out = extract(&path).expect("extract should not error");
    let _ = std::fs::remove_dir_all(&dir);
    out
}

fn tuples(syms: &[Symbol]) -> Vec<(&str, &str, i32)> {
    syms.iter()
        .map(|s| (s.name.as_str(), s.kind.as_str(), s.line))
        .collect()
}

#[test]
fn extract_rust_symbols() {
    let src = "fn free_fn() {}\n\
               struct S { x: i32 }\n\
               enum E { A, B }\n\
               trait T { fn req(&self); }\n\
               mod m { fn inner() {} }\n\
               type Alias = i32;\n\
               const C: i32 = 1;\n\
               static ST: i32 = 2;\n\
               impl S { fn method(&self) {} }\n\
               macro_rules! mac { () => {}; }\n";
    let got = extract_src("rs", "rust", src);
    assert_eq!(
        tuples(&got),
        vec![
            ("free_fn", "function", 1),
            ("S", "struct", 2),
            ("E", "enum", 3),
            ("T", "trait", 4),
            ("req", "function", 4),
            ("m", "module", 5),
            ("inner", "function", 5),
            ("Alias", "type", 6),
            ("C", "const", 7),
            ("ST", "var", 8),
            // `impl S` carries no name field; its `method` is reached as a
            // nested function_item.
            ("method", "function", 9),
            ("mac", "macro", 10),
        ]
    );
}

#[test]
fn extract_swift_symbols() {
    let src = "func freeFn() {}\n\
               class C { func method() {} }\n\
               struct St { var x: Int }\n\
               enum En { case a }\n\
               protocol P { func req() }\n";
    let got = extract_src("swift", "swift", src);
    // class/struct/enum all collapse to `class_declaration` → "class".
    assert_eq!(
        tuples(&got),
        vec![
            ("freeFn", "function", 1),
            ("C", "class", 2),
            ("method", "function", 2),
            ("St", "class", 3),
            ("En", "class", 4),
            ("P", "protocol", 5),
            ("req", "function", 5),
        ]
    );
}

#[test]
fn extract_kotlin_symbols() {
    // tree-sitter-kotlin does not label names as a `name` field — exercises
    // the identifier fallback. `interface` parses as `class_declaration`.
    let src = "fun freeFn() {}\n\
               class C { fun method() {} }\n\
               interface I { fun req() }\n\
               object O {}\n";
    let got = extract_src("kt", "kotlin", src);
    assert_eq!(
        tuples(&got),
        vec![
            ("freeFn", "function", 1),
            ("C", "class", 2),
            ("method", "function", 2),
            ("I", "class", 3),
            ("req", "function", 3),
            ("O", "object", 4),
        ]
    );
}

#[test]
fn extract_kts_uses_kotlin_grammar() {
    let got = extract_src("kts", "kotlin-script", "fun task() {}\n");
    assert_eq!(tuples(&got), vec![("task", "function", 1)]);
}

#[test]
fn extract_java_symbols() {
    let src = "package p;\n\
               class C {\n\
               int f;\n\
               void method() {}\n\
               C() {}\n\
               }\n\
               interface I { void req(); }\n\
               enum E { A, B }\n\
               record R(int x) {}\n\
               @interface Ann {}\n";
    let got = extract_src("java", "java", src);
    // Fields are skipped (no `name` field); constructors surface as "method".
    assert_eq!(
        tuples(&got),
        vec![
            ("C", "class", 2),
            ("method", "method", 4),
            ("C", "method", 5),
            ("I", "interface", 7),
            ("req", "method", 7),
            ("E", "enum", 8),
            ("R", "record", 9),
            ("Ann", "annotation", 10),
        ]
    );
}
