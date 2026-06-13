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

use std::collections::HashSet;
use std::path::Path;

use tree_sitter::{Language, Node, Parser};

use crate::types::Symbol;

/// Mirrors Go's `const maxParseBytes = 500 * 1024`.
const MAX_PARSE_BYTES: u64 = 500 * 1024;

/// One supported language: its grammar plus the node-type → SymbolKind map.
/// The kind strings are the canonical `model.SymbolKind` values.
struct LangSpec {
    language: Language,
    /// (node_type, symbol_kind) pairs. A Vec (not a map) because lookups
    /// are O(n) over ≤a dozen entries and we avoid a HashMap allocation per
    /// file.
    kinds: &'static [(&'static str, &'static str)],
    /// When true, a matched node that lacks a `name` field falls back to its
    /// first identifier-typed named child. Needed for grammars that don't
    /// label the declaration name as a field (tree-sitter-kotlin). Kept
    /// `false` for go/ts/js/py/rust/swift — their matched nodes always carry
    /// a `name` field, so the fallback never fires and extraction stays
    /// byte-identical to the field-only path.
    name_fallback: bool,
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
/// extensions (Go: `ok == false`).
fn lang_spec_for_ext(ext: &str) -> Option<LangSpec> {
    let (language, kinds, name_fallback): (Language, &'static [(&str, &str)], bool) = match ext {
        "go" => (tree_sitter_go::language(), GO_KINDS, false),
        "ts" => (
            tree_sitter_typescript::language_typescript(),
            TS_KINDS,
            false,
        ),
        "tsx" => (tree_sitter_typescript::language_tsx(), TS_KINDS, false),
        "js" | "jsx" | "mjs" => (tree_sitter_javascript::language(), JS_KINDS, false),
        "py" => (tree_sitter_python::language(), PY_KINDS, false),
        "rs" => (tree_sitter_rust::language(), RUST_KINDS, false),
        "swift" => (tree_sitter_swift::language(), SWIFT_KINDS, false),
        "kt" | "kts" => (tree_sitter_kotlin::language(), KOTLIN_KINDS, true),
        "java" => (tree_sitter_java::language(), JAVA_KINDS, false),
        _ => return None,
    };
    Some(LangSpec {
        language,
        kinds,
        name_fallback,
    })
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

    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.to_ascii_lowercase())
        .unwrap_or_default();
    let spec = match lang_spec_for_ext(&ext) {
        Some(spec) => spec,
        None => return Ok(Vec::new()),
    };

    let source = std::fs::read(path)?;

    let mut parser = Parser::new();
    // set_language only fails on ABI mismatch — impossible for grammars we
    // compiled against this tree-sitter version. Treat as "no symbols",
    // mirroring Go's parse-failure → (nil, nil).
    if parser.set_language(&spec.language).is_err() {
        return Ok(Vec::new());
    }
    let tree = match parser.parse(&source, None) {
        Some(tree) => tree,
        None => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    walk_node(
        tree.root_node(),
        &source,
        spec.kinds,
        spec.name_fallback,
        &mut out,
        &mut seen,
    );
    Ok(out)
}

/// Direct port of Go's `walkNode`: DFS pre-order over NamedChild. On a kind
/// match, reads the symbol name (via the `name` field, or — when
/// `name_fallback` is set — the first identifier-typed named child), dedups by
/// `kind\x00name`, and emits a Symbol with 1-indexed line.
fn walk_node(
    node: Node,
    source: &[u8],
    kinds: &[(&str, &str)],
    name_fallback: bool,
    out: &mut Vec<Symbol>,
    seen: &mut HashSet<String>,
) {
    if let Some(kind) = kind_for(kinds, node.kind()) {
        if let Some(name) = symbol_name(node, source, name_fallback) {
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
    // NamedChild iteration in original order (DFS pre-order), matching Go's
    // `for i := 0; i < NamedChildCount; i++ { walkNode(NamedChild(i)) }`.
    let count = node.named_child_count();
    for i in 0..count {
        if let Some(child) = node.named_child(i) {
            walk_node(child, source, kinds, name_fallback, out, seen);
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
fn kind_for<'a>(kinds: &'a [(&'a str, &'a str)], node_type: &str) -> Option<&'a str> {
    kinds
        .iter()
        .find(|(ty, _)| *ty == node_type)
        .map(|(_, kind)| *kind)
}
