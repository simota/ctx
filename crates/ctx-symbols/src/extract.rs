// crates/ctx-symbols/src/extract.rs
//
// Native tree-sitter symbol extraction — a 1:1 port of
// internal/symbols/extractor.go's `TreeSitterExtractor`.
//
// The Go oracle uses smacker/go-tree-sitter (which bundles ABI-14 C
// grammars). We pin the matching upstream grammar crate versions
// (tree-sitter 0.22.6 + tree-sitter-go 0.21.2 + js/py/ts 0.21.x) so the
// parse trees — and therefore the extracted {Name, Kind, Line, order}
// tuples — are byte-identical. Grammars are vendored C compiled by the
// `cc` crate at build time; the binary stays cgo-free (no Go linkage).
//
// Rust/Swift/Kotlin/Java (rs / swift / kt|kts / java) are extraction-only
// extensions with no Go-oracle counterpart, so they carry no byte-parity
// obligation — their behaviour is locked by tests/lang_extract.rs instead.
// They resolve onto the same tree-sitter 0.22.6 core (one ABI per binary).
// Kotlin's grammar omits the `name` field, so its LangSpec sets
// `name_fallback` (see `symbol_name`); the field-only path for every other
// language is untouched.
//
// Behavior matched exactly against extractor.go:
//   - per-extension language + node-type→SymbolKind `kinds` map
//   - files larger than maxParseBytes (500 KiB) yield no symbols
//   - unsupported extensions yield no symbols
//   - walk recurses NamedChild in order (DFS pre-order); on a kind match
//     reads ChildByFieldName("name"), dedups by `kind\x00name`, emits
//     Symbol{name, kind, line = start_row + 1}.

use std::cell::RefCell;
use std::collections::HashSet;
use std::path::Path;
use std::sync::OnceLock;

use tree_sitter::{Language, Node, Parser};

use crate::types::Symbol;

/// Mirrors Go's `const maxParseBytes = 500 * 1024`.
const MAX_PARSE_BYTES: u64 = 500 * 1024;

/// One supported language: its grammar plus the node-type → SymbolKind map.
/// The kind strings are the canonical `model.SymbolKind` values.
struct LangSpec {
    language: Language,
    /// When true, a matched node that lacks a `name` field falls back to its
    /// first identifier-typed named child. Needed for grammars that don't
    /// label the declaration name as a field (tree-sitter-kotlin). Kept
    /// `false` for go/ts/js/py/rust/swift — their matched nodes always carry
    /// a `name` field, so the fallback never fires and extraction stays
    /// byte-identical to the field-only path.
    name_fallback: bool,
    /// The language's (node_type, symbol_kind) pairs keyed by node kind id:
    /// `kind_by_id[id]` is the symbol kind of a node whose `kind_id()` is
    /// `id`. Every id whose name is one of the node types is filled (several
    /// ids can share one name), so a lookup by id answers exactly what
    /// comparing `node.kind()` against the pairs does, without turning each
    /// node's C type name into a `&str` first.
    kind_by_id: Vec<Option<&'static str>>,
}

impl LangSpec {
    fn new(
        language: Language,
        kinds: &'static [(&'static str, &'static str)],
        name_fallback: bool,
    ) -> Self {
        let kind_by_id = (0..language.node_kind_count())
            .map(|id| {
                let id = u16::try_from(id).ok()?;
                kind_for(kinds, language.node_kind_for_id(id)?)
            })
            .collect();
        Self {
            language,
            name_fallback,
            kind_by_id,
        }
    }

    /// The symbol kind of `node`, if its node type is one of `kinds`.
    #[inline]
    fn kind_of(&self, node: Node) -> Option<&'static str> {
        // An id outside the table (the ERROR node's 65535) has no `kinds`
        // entry, just as its "ERROR" name matches none.
        self.kind_by_id
            .get(usize::from(node.kind_id()))
            .copied()
            .flatten()
    }
}

// ── Per-language kind maps (mirror langSpecs in extractor.go) ────────────

const GO_KINDS: &[(&str, &str)] = &[
    ("function_declaration", "function"),
    ("method_declaration", "method"),
    ("type_spec", "type"),
];

// TypeScript (.ts) and TSX (.tsx) share the same kind map in Go.
const TS_KINDS: &[(&str, &str)] = &[
    ("function_declaration", "function"),
    ("class_declaration", "class"),
    ("interface_declaration", "interface"),
    ("type_alias_declaration", "type"),
];

// JavaScript (.js/.jsx/.mjs) share the same kind map in Go.
const JS_KINDS: &[(&str, &str)] = &[
    ("function_declaration", "function"),
    ("class_declaration", "class"),
];

const PY_KINDS: &[(&str, &str)] = &[
    ("function_definition", "function"),
    ("class_definition", "class"),
];

// Rust. `function_item` covers both free functions and inherent/trait-impl
// methods (the grammar does not distinguish them by node kind), so they all
// surface as "function". `impl_item` carries no `name` field and is not
// matched — its methods are reached as nested `function_item`s.
const RUST_KINDS: &[(&str, &str)] = &[
    ("function_item", "function"),
    ("function_signature_item", "function"),
    ("struct_item", "struct"),
    ("union_item", "struct"),
    ("enum_item", "enum"),
    ("trait_item", "trait"),
    ("mod_item", "module"),
    ("type_item", "type"),
    ("const_item", "const"),
    ("static_item", "var"),
    ("macro_definition", "macro"),
];

// Swift. The grammar collapses class / struct / enum / extension into a single
// `class_declaration` node, so we cannot tell them apart from the node kind
// alone — they all surface as "class". Free functions and in-type methods are
// `function_declaration`; protocol requirements are
// `protocol_function_declaration`.
const SWIFT_KINDS: &[(&str, &str)] = &[
    ("function_declaration", "function"),
    ("protocol_function_declaration", "function"),
    ("class_declaration", "class"),
    ("protocol_declaration", "protocol"),
];

// Kotlin. tree-sitter-kotlin does not expose declaration names as a `name`
// field, so this spec sets `name_fallback`. `class_declaration` also covers
// `interface` declarations (same node kind), surfaced as "class".
const KOTLIN_KINDS: &[(&str, &str)] = &[
    ("function_declaration", "function"),
    ("class_declaration", "class"),
    ("object_declaration", "object"),
];

// Java. All declarations carry a `name` field (no fallback). Constructors are
// a distinct node from methods but surface as "method"; `@interface` types are
// `annotation_type_declaration` → "annotation".
const JAVA_KINDS: &[(&str, &str)] = &[
    ("class_declaration", "class"),
    ("interface_declaration", "interface"),
    ("enum_declaration", "enum"),
    ("record_declaration", "record"),
    ("annotation_type_declaration", "annotation"),
    ("method_declaration", "method"),
    ("constructor_declaration", "method"),
];

/// Resolve the LangSpec for a file extension, mirroring `langSpecs` keyed
/// by `strings.ToLower(filepath.Ext(path))`. Returns `None` for unsupported
/// extensions (Go: `ok == false`). Each spec (with its kind-id table) is
/// built once per process.
fn lang_spec_for_ext(ext: &str) -> Option<&'static LangSpec> {
    static SPECS: [OnceLock<LangSpec>; 9] = [const { OnceLock::new() }; 9];
    let (slot, build): (usize, fn() -> LangSpec) = match ext {
        "go" => (0, || {
            LangSpec::new(tree_sitter_go::language(), GO_KINDS, false)
        }),
        "ts" => (1, || {
            LangSpec::new(
                tree_sitter_typescript::language_typescript(),
                TS_KINDS,
                false,
            )
        }),
        "tsx" => (2, || {
            LangSpec::new(tree_sitter_typescript::language_tsx(), TS_KINDS, false)
        }),
        "js" | "jsx" | "mjs" => (3, || {
            LangSpec::new(tree_sitter_javascript::language(), JS_KINDS, false)
        }),
        "py" => (4, || {
            LangSpec::new(tree_sitter_python::language(), PY_KINDS, false)
        }),
        "rs" => (5, || {
            LangSpec::new(tree_sitter_rust::language(), RUST_KINDS, false)
        }),
        "swift" => (6, || {
            LangSpec::new(tree_sitter_swift::language(), SWIFT_KINDS, false)
        }),
        "kt" | "kts" => (7, || {
            LangSpec::new(tree_sitter_kotlin::language(), KOTLIN_KINDS, true)
        }),
        "java" => (8, || {
            LangSpec::new(tree_sitter_java::language(), JAVA_KINDS, false)
        }),
        _ => return None,
    };
    Some(SPECS[slot].get_or_init(build))
}

/// Extracts named symbols for a supported source file, mirroring Go's
/// `TreeSitterExtractor.Extract(path)`.
///
/// Returns an empty Vec (never an error) for: unsupported extensions,
/// files over the 500 KiB cap, and parse failures — matching the Go path
/// where those cases return `(nil, nil)`. A read/stat failure surfaces as
/// `Err`, matching Go's `os.Stat` / `os.ReadFile` error returns.
pub fn extract(path: impl AsRef<Path>) -> std::io::Result<Vec<Symbol>> {
    let path = path.as_ref();

    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_PARSE_BYTES {
        return Ok(Vec::new());
    }
    let spec = match lang_spec_for_path(path) {
        Some(spec) => spec,
        None => return Ok(Vec::new()),
    };
    let source = std::fs::read(path)?;
    Ok(parse_symbols(spec, &source))
}

/// [`extract`] for a caller that has already read the file: `source` is the
/// content of `path` (the extension still selects the grammar). Applies the
/// same 500 KiB cap to `source.len()` — the length `extract` stats — so a
/// walker that reads every file anyway avoids a second stat + read.
pub fn extract_source(path: impl AsRef<Path>, source: &[u8]) -> Vec<Symbol> {
    if source.len() as u64 > MAX_PARSE_BYTES {
        return Vec::new();
    }
    match lang_spec_for_path(path.as_ref()) {
        Some(spec) => parse_symbols(spec, source),
        None => Vec::new(),
    }
}

/// Sources larger than this parse alone on a walk worker (see
/// [`extract_on_worker`]) and get their freed parse memory handed back by
/// [`release_memory_after_large_parse`].
const LARGE_PARSE_BYTES: u64 = 64 * 1024;

/// Call after parsing a `len`-byte file on a worker thread of a parallel
/// walk. glibc gives each thread its own malloc arena, and an arena keeps
/// its high-water mark after the (many times source-sized) parse tree is
/// freed, so without this every worker that ever met a large file retained
/// that much memory and peak RSS rose above a sequential walk. Only large
/// files (rare) trigger [`release_freed_memory`].
pub fn release_memory_after_large_parse(len: u64) {
    if len > LARGE_PARSE_BYTES {
        release_freed_memory();
    }
}

/// Hand already-freed heap memory back to the OS (glibc `malloc_trim`,
/// which trims every thread's arena) — for after work that allocated on
/// rayon workers, whose arenas otherwise keep their high-water mark. Costs
/// a few milliseconds on a large heap, so call it after heavy work only.
/// A no-op on other allocators.
pub fn release_freed_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim only returns already-freed memory to the OS and is
    // thread-safe (it locks each arena it trims).
    unsafe {
        libc::malloc_trim(0);
    }
}

/// [`extract`] for a worker thread of a parallel walk, keeping the walk's
/// peak memory at the sequential walk's. A parse tree is many times the
/// size of its source, so large (`> LARGE_PARSE_BYTES`) files parse one at a
/// time — small ones stay parallel — and each large parse's freed memory is
/// handed back with [`release_memory_after_large_parse`] before the next
/// large parse starts.
pub fn extract_on_worker(path: impl AsRef<Path>) -> std::io::Result<Vec<Symbol>> {
    let path = path.as_ref();

    let meta = std::fs::metadata(path)?;
    if meta.len() > MAX_PARSE_BYTES {
        return Ok(Vec::new());
    }
    let spec = match lang_spec_for_path(path) {
        Some(spec) => spec,
        None => return Ok(Vec::new()),
    };
    let source = std::fs::read(path)?;
    Ok(parse_symbols_on_worker(spec, &source))
}

/// [`extract_source`] for a worker thread of a parallel walk; see
/// [`extract_on_worker`].
pub fn extract_source_on_worker(path: impl AsRef<Path>, source: &[u8]) -> Vec<Symbol> {
    if source.len() as u64 > MAX_PARSE_BYTES {
        return Vec::new();
    }
    match lang_spec_for_path(path.as_ref()) {
        Some(spec) => parse_symbols_on_worker(spec, source),
        None => Vec::new(),
    }
}

fn parse_symbols_on_worker(spec: &LangSpec, source: &[u8]) -> Vec<Symbol> {
    static LARGE_PARSE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    let len = source.len() as u64;
    if len <= LARGE_PARSE_BYTES {
        return parse_symbols(spec, source);
    }
    // The lock guards no data, so a parse that panicked leaves nothing to
    // repair: a poisoned lock is still usable.
    let _one_large_parse = LARGE_PARSE
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let symbols = parse_symbols(spec, source);
    release_memory_after_large_parse(len);
    symbols
}

fn parse_symbols(spec: &LangSpec, source: &[u8]) -> Vec<Symbol> {
    thread_local! {
        // One parser per thread, reused across files: building a parser
        // allocates its stack and lexer buffers, a measurable share of a
        // small file's extraction. Its trees are independent of it.
        static PARSER: RefCell<Parser> = RefCell::new(Parser::new());
    }
    PARSER.with(|parser| {
        // A nested call on this thread (none exist) would find the parser
        // borrowed; give it a parser of its own rather than panicking.
        match parser.try_borrow_mut() {
            Ok(mut parser) => parse_with(&mut parser, spec, source),
            Err(_) => parse_with(&mut Parser::new(), spec, source),
        }
    })
}

fn parse_with(parser: &mut Parser, spec: &LangSpec, source: &[u8]) -> Vec<Symbol> {
    // set_language only fails on ABI mismatch — impossible for grammars we
    // compiled against this tree-sitter version. Treat as "no symbols",
    // mirroring Go's parse-failure → (nil, nil).
    if parser.set_language(&spec.language).is_err() {
        return Vec::new();
    }
    // No timeout or cancellation flag is ever set, so every parse runs to
    // completion; reset anyway so a parse can never resume earlier state.
    parser.reset();
    let tree = match parser.parse(source, None) {
        Some(tree) => tree,
        None => return Vec::new(),
    };

    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    walk_node(tree.root_node(), source, spec, &mut out, &mut seen);
    out
}

fn lang_spec_for_path(path: &Path) -> Option<&'static LangSpec> {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    lang_spec_for_ext(&ext)
}

/// Direct port of Go's `walkNode`: DFS pre-order over NamedChild. On a kind
/// match, reads the symbol name (via the `name` field, or — when
/// `name_fallback` is set — the first identifier-typed named child), dedups by
/// `kind\x00name`, and emits a Symbol with 1-indexed line.
fn walk_node(
    root: Node,
    source: &[u8],
    spec: &LangSpec,
    out: &mut Vec<Symbol>,
    seen: &mut HashSet<String>,
) {
    // Explicit stack instead of recursion: a deeply nested (generated or
    // minified) file would otherwise overflow the thread stack and abort the
    // process. Children are pushed in reverse so they pop in original order,
    // keeping Go's NamedChild DFS pre-order
    // (`for i := 0; i < NamedChildCount; i++ { walkNode(NamedChild(i)) }`).
    let mut stack = vec![root];
    while let Some(node) = stack.pop() {
        if let Some(kind) = spec.kind_of(node) {
            if let Some(name) = symbol_name(node, source, spec.name_fallback) {
                let key = format!("{kind}\x00{name}");
                if seen.insert(key) {
                    out.push(Symbol {
                        name,
                        kind: kind.to_string(),
                        line: node.start_position().row as i32 + 1,
                    });
                }
            }
        }
        let count = node.named_child_count();
        for i in (0..count).rev() {
            if let Some(child) = node.named_child(i) {
                stack.push(child);
            }
        }
    }
}

/// Resolve a matched node's symbol name. The primary path is the `name` field
/// child (`Content(source)` in Go == the node's byte slice as UTF-8) — this is
/// the only path taken by go/ts/js/py/rust/swift, preserving exact parity.
/// When `fallback` is set (Kotlin), a node without a `name` field instead uses
/// its first identifier-typed named child (`simple_identifier` /
/// `type_identifier`), since that grammar does not label the name as a field.
fn symbol_name(node: Node, source: &[u8], fallback: bool) -> Option<String> {
    if let Some(name_node) = node.child_by_field_name("name") {
        return Some(name_node.utf8_text(source).unwrap_or_default().to_string());
    }
    if fallback {
        let count = node.named_child_count();
        for i in 0..count {
            if let Some(child) = node.named_child(i) {
                if child.kind().ends_with("identifier") {
                    return Some(child.utf8_text(source).unwrap_or_default().to_string());
                }
            }
        }
    }
    None
}

#[inline]
fn kind_for(kinds: &[(&str, &'static str)], node_type: &str) -> Option<&'static str> {
    kinds
        .iter()
        .find(|(ty, _)| *ty == node_type)
        .map(|(_, kind)| *kind)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every node of a parsed sample (named or not, aliased or not) gets the
    /// same kind from the id table as from comparing its type name.
    #[test]
    fn kind_ids_match_kind_names_for_every_language() {
        type Kinds = &'static [(&'static str, &'static str)];
        let samples: &[(&str, Kinds, &str)] = &[
            ("go", GO_KINDS, "package p\ntype T struct{}\nfunc (t T) M() {}\nfunc F() { var x = 1; _ = x }\n"),
            ("ts", TS_KINDS, "interface I { a: number }\ntype A = string;\nclass C { m() {} }\nfunction f(): void {}\n"),
            ("tsx", TS_KINDS, "function App() { return <div className=\"a\">{1}</div>; }\nclass K {}\n"),
            ("js", JS_KINDS, "class C { m() {} }\nfunction f() { return `x${1}`; }\nconst g = () => 1;\n"),
            ("py", PY_KINDS, "class C:\n    def m(self):\n        return [x for x in range(3)]\ndef f(): pass\n"),
            ("rs", RUST_KINDS, "mod m { pub fn f() {} }\nstruct S; enum E { A }\ntrait T { fn r(&self); }\nmacro_rules! mm { () => {} }\nconst C: u8 = 1; static X: u8 = 2; type Y = u8; union U { a: u8 }\n"),
            ("swift", SWIFT_KINDS, "protocol P { func r() }\nclass C { func m() {} }\nstruct S {}\nfunc f() {}\n"),
            ("kt", KOTLIN_KINDS, "class C { fun m() {} }\nobject O\ninterface I\nfun f() = 1\n"),
            ("java", JAVA_KINDS, "@interface A {}\nrecord R(int a) {}\nenum E { X }\ninterface I {}\nclass C { C() {} void m() {} }\n"),
        ];
        for (ext, kinds, source) in samples {
            let spec = lang_spec_for_ext(ext).expect("supported extension");
            let mut parser = Parser::new();
            parser.set_language(&spec.language).unwrap();
            let tree = parser.parse(source, None).unwrap();
            let mut matched = 0;
            let mut cursor = tree.walk();
            let mut stack = vec![tree.root_node()];
            while let Some(node) = stack.pop() {
                let expected = kind_for(kinds, node.kind());
                assert_eq!(spec.kind_of(node), expected, "{ext}: {}", node.kind());
                matched += usize::from(expected.is_some());
                stack.extend(node.children(&mut cursor));
            }
            assert!(matched >= 2, "{ext}: sample matched only {matched} nodes");
        }
    }

    /// A Rust source of roughly `items` functions (each ~40 bytes).
    fn rust_source(items: usize) -> String {
        (0..items)
            .map(|i| format!("pub fn item_{i}(x: u32) -> u32 {{ x + {i} }}\n"))
            .collect()
    }

    #[test]
    fn worker_extraction_matches_plain_extraction() {
        let dir = std::env::temp_dir().join(format!("ctx-symbols-worker-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        // Small, large (> LARGE_PARSE_BYTES, parsed under the lock) and over
        // the MAX_PARSE_BYTES cap; plus an unsupported extension.
        let cases = [
            ("small.rs", rust_source(10)),
            ("large.rs", rust_source(3_000)),
            ("huge.rs", rust_source(20_000)),
            ("notes.txt", rust_source(10)),
        ];
        assert!(cases[1].1.len() as u64 > LARGE_PARSE_BYTES);
        assert!(cases[2].1.len() as u64 > MAX_PARSE_BYTES);
        for (name, body) in &cases {
            std::fs::write(dir.join(name), body).unwrap();
        }

        // Many threads at once, as on a rayon pool: every call must still
        // return what the plain extractor does.
        std::thread::scope(|scope| {
            for _ in 0..4 {
                scope.spawn(|| {
                    for _ in 0..3 {
                        for (name, body) in &cases {
                            let path = dir.join(name);
                            let expected = extract(&path).unwrap();
                            assert_eq!(extract_on_worker(&path).unwrap(), expected, "{name}");
                            assert_eq!(
                                extract_source_on_worker(&path, body.as_bytes()),
                                expected,
                                "{name}"
                            );
                        }
                    }
                });
            }
        });
        assert_eq!(extract(dir.join("large.rs")).unwrap().len(), 3_000);
        assert!(extract(dir.join("huge.rs")).unwrap().is_empty());
        assert!(extract_on_worker(dir.join("missing.rs")).is_err());

        let _ = std::fs::remove_dir_all(dir);
    }
}
