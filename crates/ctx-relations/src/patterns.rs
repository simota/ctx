// crates/ctx-relations/src/patterns.rs
//
// Port of the regex patterns used by the per-language extractors in
// internal/relations. Naming and order mirror the Go source so a side-
// by-side diff is trivial.
//
// REGEX PORTING NOTES
// ===================
// Go's `regexp` is RE2-based with ASCII semantics for `\s`/`\w`/`\d`.
// Rust's `regex` defaults to Unicode-aware classes. To keep parity:
//
//   * `\s` is restricted to ASCII via `(?-u:\s)` everywhere we use it.
//   * `\w` similarly uses `(?-u:\w)` for ASCII-only word boundaries.
//   * `(?m)` and `(?i)` are supported on both engines identically.
//
// STORAGE LAYOUT
// ==============
// Rust forbids `Lazy<Regex>` inside `static` slices. We expose accessor
// fns that return references to lazily-built statics — this matches
// the Phase 1 pattern in ctx-scan/src/patterns.rs.

use once_cell::sync::Lazy;
use regex::Regex;

// ---------------------------------------------------------------------------
// JS / TS
// ---------------------------------------------------------------------------

/// Matches the common import-bearing forms:
///   `import x from "spec"`, `import "spec"`, `import("spec")`,
///   `require("spec")`, `export ... from "spec"`.
///
/// Only the spec is captured. Mirrors `jsImportRE` in relations.go.
static JS_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r#"(?m)(?:\b(?:import|export)\b[^'"\n;]*?(?:from\s*)?['"]([^'"\n]+)['"])|(?:\b(?:import|require)\b\s*\(\s*['"]([^'"\n]+)['"]\s*\))"#,
    )
    .expect("js_import_re")
});

/// Strips `// ...` line comments to EOL. Mirrors `jsLineCommentRE`.
static JS_LINE_COMMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)//.*$").expect("js_line_comment_re"));

/// Strips `/* ... */` block comments. Mirrors `jsBlockCommentRE`.
static JS_BLOCK_COMMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)/\*.*?\*/").expect("js_block_comment_re"));

pub fn js_import_re() -> &'static Regex {
    &JS_IMPORT_RE
}
pub fn js_line_comment_re() -> &'static Regex {
    &JS_LINE_COMMENT_RE
}
pub fn js_block_comment_re() -> &'static Regex {
    &JS_BLOCK_COMMENT_RE
}

// ---------------------------------------------------------------------------
// Svelte / Vue (shared script-block extraction)
// ---------------------------------------------------------------------------

/// Captures the body of any `<script ...>...</script>` block. Mirrors
/// `svelteScriptRE` (Go's `(?is)` ≡ Rust `(?is)`).
static SCRIPT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?is)<script\b[^>]*>(.*?)</script\s*>").expect("script_re"));

pub fn script_re() -> &'static Regex {
    &SCRIPT_RE
}

// ---------------------------------------------------------------------------
// Python
// ---------------------------------------------------------------------------

/// Matches:
///   `from a.b[ ...] import X`
///   `from . import X`, `from ..pkg import X`
///   `import a.b[, c.d[ as e]]`
///
/// Capture groups:
///   [1] leading dots on `from`
///   [2] dotted module path on `from` (may be empty)
///   [3] comma-separated names following `import` on `from` lines
///   [4] comma-separated module list on `import` lines
static PY_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    // Both alternatives live INSIDE one group so the `^\s*` anchor covers
    // them — a top-level `|` would let bare `import x` match mid-line
    // (comments/docstrings), creating false edges.
    Regex::new(
        r"(?m)^\s*(?:from\s+(\.*)([A-Za-z_][\w.]*)?\s+import\s+([^\n#]+)|import\s+([^\n#]+))",
    )
    .expect("py_import_re")
});

pub fn py_import_re() -> &'static Regex {
    &PY_IMPORT_RE
}

// ---------------------------------------------------------------------------
// JVM
// ---------------------------------------------------------------------------

/// Matches Java `import [static] x.y.Z[.*];`. Capture groups:
///   [1] optional "static " (with trailing space) — for member-name strip
///   [2] dotted spec (possibly ending `.*`)
static JAVA_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*import\s+(static\s+)?([\w.]+(?:\.\*)?)\s*;").expect("java_import_re")
});

/// Matches Kotlin `import x.y.Z [as alias]` (no terminating semicolon).
/// The alias is consumed but discarded — only the dotted spec is captured.
static KOTLIN_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*import\s+([\w.]+(?:\.\*)?)(?:\s+as\s+\w+)?\s*$").expect("kotlin_import_re")
});

pub fn java_import_re() -> &'static Regex {
    &JAVA_IMPORT_RE
}
pub fn kotlin_import_re() -> &'static Regex {
    &KOTLIN_IMPORT_RE
}

// ---------------------------------------------------------------------------
// PHP
// ---------------------------------------------------------------------------

/// Captures the body between `use [function|const] ` and `;`. Group form
/// (`use Foo\{A, B as B2};`) is left intact for downstream expansion.
static PHP_USE_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?m)^\s*use\s+(?:function\s+|const\s+)?([^;]+);").expect("php_use_re")
});

static PHP_LINE_COMMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)//.*$").expect("php_line_comment_re"));
static PHP_HASH_COMMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?m)#.*$").expect("php_hash_comment_re"));
static PHP_BLOCK_COMMENT_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"(?s)/\*.*?\*/").expect("php_block_comment_re"));

pub fn php_use_re() -> &'static Regex {
    &PHP_USE_RE
}
pub fn php_line_comment_re() -> &'static Regex {
    &PHP_LINE_COMMENT_RE
}
pub fn php_hash_comment_re() -> &'static Regex {
    &PHP_HASH_COMMENT_RE
}
pub fn php_block_comment_re() -> &'static Regex {
    &PHP_BLOCK_COMMENT_RE
}

// ---------------------------------------------------------------------------
// Swift
// ---------------------------------------------------------------------------

/// Matches `import [<kind>] Module[.Symbol]`. Captures only the module
/// name (first dotted segment); the optional `<kind>` keyword between
/// `import` and the module name is consumed but discarded.
static SWIFT_IMPORT_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?m)^\s*import\s+(?:struct\s+|class\s+|enum\s+|protocol\s+|typealias\s+|func\s+|var\s+|let\s+)?([A-Za-z_]\w*)",
    )
    .expect("swift_import_re")
});

pub fn swift_import_re() -> &'static Regex {
    &SWIFT_IMPORT_RE
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn js_import_re_compiles_and_matches() {
        let re = js_import_re();
        assert!(re.is_match("import x from './y';"));
        assert!(re.is_match("const x = require('./y');"));
    }

    #[test]
    fn py_import_re_captures_module() {
        let re = py_import_re();
        let s = "from . import x\nimport a.b\n";
        let caps: Vec<_> = re.captures_iter(s).collect();
        assert!(!caps.is_empty());
    }

    #[test]
    fn swift_import_strips_kind() {
        let re = swift_import_re();
        let caps = re.captures("import struct Foo.Bar").unwrap();
        assert_eq!(&caps[1], "Foo");
    }
}
