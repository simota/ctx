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
    /// are O(n) over ≤4 entries and we avoid a HashMap allocation per file.
    kinds: &'static [(&'static str, &'static str)],
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

/// Resolve the LangSpec for a file extension, mirroring `langSpecs` keyed
/// by `strings.ToLower(filepath.Ext(path))`. Returns `None` for unsupported
/// extensions (Go: `ok == false`).
fn lang_spec_for_ext(ext: &str) -> Option<LangSpec> {
    let (language, kinds): (Language, &'static [(&str, &str)]) = match ext {
        "go" => (tree_sitter_go::language(), GO_KINDS),
        "ts" => (tree_sitter_typescript::language_typescript(), TS_KINDS),
        "tsx" => (tree_sitter_typescript::language_tsx(), TS_KINDS),
        "js" | "jsx" | "mjs" => (tree_sitter_javascript::language(), JS_KINDS),
        "py" => (tree_sitter_python::language(), PY_KINDS),
        _ => return None,
    };
    Some(LangSpec { language, kinds })
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
    walk_node(tree.root_node(), &source, spec.kinds, &mut out, &mut seen);
    Ok(out)
}

/// Direct port of Go's `walkNode`: DFS pre-order over NamedChild. On a kind
/// match, reads the `name` field child, dedups by `kind\x00name`, and emits
/// a Symbol with 1-indexed line.
fn walk_node(
    node: Node,
    source: &[u8],
    kinds: &[(&str, &str)],
    out: &mut Vec<Symbol>,
    seen: &mut HashSet<String>,
) {
    if let Some(kind) = kind_for(kinds, node.kind()) {
        if let Some(name_node) = node.child_by_field_name("name") {
            // Content(source) in Go == the node's byte slice as UTF-8.
            let name = name_node
                .utf8_text(source)
                .unwrap_or_default()
                .to_string();
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
            walk_node(child, source, kinds, out, seen);
        }
    }
}

#[inline]
fn kind_for<'a>(kinds: &'a [(&'a str, &'a str)], node_type: &str) -> Option<&'a str> {
    kinds
        .iter()
        .find(|(ty, _)| *ty == node_type)
        .map(|(_, kind)| *kind)
}
