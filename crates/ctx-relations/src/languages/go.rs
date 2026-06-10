// crates/ctx-relations/src/languages/go.rs
//
// Port of the Go-language portion of internal/relations/relations.go.
//
// resolve_go_imports + parse_go_import_specs reproduce the import
// extraction behaviour of Go's `go/parser.ParseFile(.., ImportsOnly)`.
// Since we don't have a Go parser in Rust we implement a minimal
// tokeniser scoped to the file header (up to the first non-import
// declaration), which is exactly what the Go ImportsOnly mode covers.
//
// The tokeniser is intentionally permissive: it scans the head of the
// file until it sees a `func`/`type`/`var`/`const` declaration outside
// any import block, then stops. This mirrors the Go parser's early exit
// and is robust against malformed code below the import block.

use std::collections::HashMap;
use std::fs;

/// Mirror of `readModulePath(root)`. Reads `<root>/go.mod` and returns
/// the module path declared on the `module ...` line, or "" when the
/// file is missing or has no module declaration.
pub fn read_module_path(root: &str) -> String {
    let path = std::path::Path::new(root).join("go.mod");
    let data = match fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    for line in data.lines() {
        let trimmed = line.trim();
        if let Some(rest) = trimmed.strip_prefix("module ") {
            return rest.trim().to_string();
        }
        if let Some(rest) = trimmed.strip_prefix("module\t") {
            return rest.trim().to_string();
        }
    }
    String::new()
}

/// Mirror of `buildGoPackageMap`. Group every .go file by its
/// containing directory (repo-relative, slash-separated). Test files
/// are intentionally included.
pub fn build_go_package_map(files: &[super::common::FileEntry]) -> HashMap<String, Vec<String>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for fi in files {
        if fi.is_dir {
            continue;
        }
        if super::common::lowercase_ext(&fi.rel) != ".go" {
            continue;
        }
        let dir = super::common::parent_slash(&fi.rel);
        out.entry(dir).or_default().push(fi.rel.clone());
    }
    out
}

/// Mirror of `resolveGoImports(absPath, modulePath, pkgFiles)`.
pub fn resolve_go_imports(
    abs_path: &std::path::Path,
    module_path: &str,
    pkg_files: &HashMap<String, Vec<String>>,
) -> Vec<String> {
    if module_path.is_empty() {
        return Vec::new();
    }
    let data = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let specs = parse_go_import_specs(&data);
    if specs.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for spec in specs {
        if !is_in_module(&spec, module_path) {
            continue;
        }
        let mut pkg_dir = spec[module_path.len()..].to_string();
        if let Some(stripped) = pkg_dir.strip_prefix('/') {
            pkg_dir = stripped.to_string();
        }
        if let Some(files) = pkg_files.get(&pkg_dir) {
            out.extend(files.iter().cloned());
        }
    }
    out
}

/// Mirror of `isInModule(spec, modulePath)` — full path-segment match.
fn is_in_module(spec: &str, module_path: &str) -> bool {
    if module_path.is_empty() {
        return false;
    }
    if spec == module_path {
        return true;
    }
    spec.starts_with(&format!("{module_path}/"))
}

/// Mirror of `parseGoImportSpecs(absPath)`. Returns the import-path
/// literals declared at the top of the file. Behaves like Go's
/// ImportsOnly mode: stops at the first non-import declaration.
///
/// Algorithm:
///   1. Strip block and line comments.
///   2. Walk through tokens. The package clause `package X` is consumed
///      and we then look for `import` blocks until we hit a non-import
///      keyword (`func`/`type`/`var`/`const`).
///   3. An `import` clause may be:
///        - `import "spec"`
///        - `import alias "spec"`
///        - `import . "spec"` / `import _ "spec"`
///        - `import ( ... )` — block form with one spec per line.
///   4. Each captured spec is a double-quoted Go string literal we
///      Unquote with `unquote_go_string`.
pub fn parse_go_import_specs(src: &str) -> Vec<String> {
    let cleaned = strip_go_comments(src);
    let bytes = cleaned.as_bytes();
    let mut i = 0;
    let mut specs = Vec::new();
    let n = bytes.len();

    // Skip leading whitespace + package clause.
    skip_ws(bytes, &mut i);
    if matches_keyword(bytes, i, "package") {
        i += "package".len();
        skip_until_newline(bytes, &mut i);
    }

    loop {
        skip_ws(bytes, &mut i);
        if i >= n {
            break;
        }
        if matches_keyword(bytes, i, "import") {
            i += "import".len();
            skip_ws_inline(bytes, &mut i);
            if i < n && bytes[i] == b'(' {
                i += 1;
                // Block form. Scan lines until ')'.
                loop {
                    skip_ws(bytes, &mut i);
                    if i >= n {
                        break;
                    }
                    if bytes[i] == b')' {
                        i += 1;
                        break;
                    }
                    // Optional alias / . / _.
                    skip_import_prefix(bytes, &mut i);
                    skip_ws_inline(bytes, &mut i);
                    if let Some(spec) = read_quoted(bytes, &mut i) {
                        specs.push(spec);
                    } else {
                        // No string literal here — skip to newline to
                        // avoid an infinite loop.
                        skip_until_newline(bytes, &mut i);
                    }
                    skip_until_newline(bytes, &mut i);
                }
            } else {
                // Single-import form. Optional alias / . / _.
                skip_import_prefix(bytes, &mut i);
                skip_ws_inline(bytes, &mut i);
                if let Some(spec) = read_quoted(bytes, &mut i) {
                    specs.push(spec);
                }
                skip_until_newline(bytes, &mut i);
            }
            continue;
        }
        // Any other top-level keyword stops the scan, matching Go's
        // ImportsOnly early-exit.
        if matches_keyword(bytes, i, "func")
            || matches_keyword(bytes, i, "type")
            || matches_keyword(bytes, i, "var")
            || matches_keyword(bytes, i, "const")
        {
            break;
        }
        // Unknown token — advance one byte to make progress.
        i += 1;
    }

    specs
}

// ----- low-level scanner helpers --------------------------------------

fn strip_go_comments(src: &str) -> String {
    let bytes = src.as_bytes();
    let mut out = String::with_capacity(src.len());
    let mut i = 0;
    let n = bytes.len();
    while i < n {
        let b = bytes[i];
        // Preserve string literals so a `//` inside a string isn't
        // mistaken for a comment.
        if b == b'"' {
            out.push('"');
            i += 1;
            while i < n {
                let c = bytes[i];
                out.push(c as char);
                if c == b'\\' && i + 1 < n {
                    out.push(bytes[i + 1] as char);
                    i += 2;
                    continue;
                }
                i += 1;
                if c == b'"' {
                    break;
                }
            }
            continue;
        }
        if b == b'`' {
            out.push('`');
            i += 1;
            while i < n {
                let c = bytes[i];
                out.push(c as char);
                i += 1;
                if c == b'`' {
                    break;
                }
            }
            continue;
        }
        if b == b'/' && i + 1 < n {
            if bytes[i + 1] == b'/' {
                while i < n && bytes[i] != b'\n' {
                    i += 1;
                }
                continue;
            }
            if bytes[i + 1] == b'*' {
                i += 2;
                while i + 1 < n && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                    if bytes[i] == b'\n' {
                        out.push('\n');
                    }
                    i += 1;
                }
                if i + 1 < n {
                    i += 2;
                }
                continue;
            }
        }
        out.push(b as char);
        i += 1;
    }
    out
}

fn matches_keyword(bytes: &[u8], i: usize, kw: &str) -> bool {
    let kw_b = kw.as_bytes();
    if i + kw_b.len() > bytes.len() {
        return false;
    }
    if &bytes[i..i + kw_b.len()] != kw_b {
        return false;
    }
    // Boundary check — next byte must be non-identifier.
    if i + kw_b.len() == bytes.len() {
        return true;
    }
    let nb = bytes[i + kw_b.len()];
    !(nb.is_ascii_alphanumeric() || nb == b'_')
}

fn skip_ws(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() {
        let b = bytes[*i];
        if b == b' ' || b == b'\t' || b == b'\n' || b == b'\r' {
            *i += 1;
        } else {
            break;
        }
    }
}

fn skip_ws_inline(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() {
        let b = bytes[*i];
        if b == b' ' || b == b'\t' {
            *i += 1;
        } else {
            break;
        }
    }
}

fn skip_until_newline(bytes: &[u8], i: &mut usize) {
    while *i < bytes.len() && bytes[*i] != b'\n' {
        *i += 1;
    }
    if *i < bytes.len() {
        *i += 1;
    }
}

fn skip_import_prefix(bytes: &[u8], i: &mut usize) {
    // Optional alias / `.` / `_` before the string literal.
    if *i < bytes.len() {
        let b = bytes[*i];
        if b == b'.' || b == b'_' {
            *i += 1;
            return;
        }
        if b.is_ascii_alphabetic() || b == b'_' {
            let start = *i;
            while *i < bytes.len() {
                let c = bytes[*i];
                if c.is_ascii_alphanumeric() || c == b'_' {
                    *i += 1;
                } else {
                    break;
                }
            }
            // If we didn't actually consume any identifier or the next
            // char is a quote, treat what we read as an alias.
            if *i == start {
                return;
            }
        }
    }
}

fn read_quoted(bytes: &[u8], i: &mut usize) -> Option<String> {
    if *i >= bytes.len() {
        return None;
    }
    let q = bytes[*i];
    if q != b'"' && q != b'`' {
        return None;
    }
    *i += 1;
    let mut s = String::new();
    while *i < bytes.len() {
        let b = bytes[*i];
        if b == q {
            *i += 1;
            return Some(s);
        }
        if q == b'"' && b == b'\\' && *i + 1 < bytes.len() {
            // Minimal escape handling — relations only cares about the
            // import path so we just pass through the next byte.
            let next = bytes[*i + 1];
            match next {
                b'n' => s.push('\n'),
                b't' => s.push('\t'),
                b'r' => s.push('\r'),
                b'"' => s.push('"'),
                b'\\' => s.push('\\'),
                b => s.push(b as char),
            }
            *i += 2;
            continue;
        }
        s.push(b as char);
        *i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_single_import() {
        let src = r#"package m
import "fmt"
"#;
        assert_eq!(parse_go_import_specs(src), vec!["fmt".to_string()]);
    }

    #[test]
    fn parses_block_imports_with_alias_and_blank() {
        let src = r#"package m

import (
    "fmt"
    foo "example.com/lib"
    _ "example.com/init"
    . "example.com/dot"
)

func main() {}
"#;
        let got = parse_go_import_specs(src);
        assert_eq!(
            got,
            vec![
                "fmt".to_string(),
                "example.com/lib".to_string(),
                "example.com/init".to_string(),
                "example.com/dot".to_string(),
            ]
        );
    }

    #[test]
    fn stops_at_first_non_import_decl() {
        let src = r#"package m

import "fmt"

func x() {}

import "bad" // should NOT be picked up
"#;
        assert_eq!(parse_go_import_specs(src), vec!["fmt".to_string()]);
    }

    #[test]
    fn read_module_path_extracts_module_line() {
        let dir = std::env::temp_dir().join(format!("rel-go-mod-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(
            dir.join("go.mod"),
            "module example.com/m\n\ngo 1.22\n",
        )
        .unwrap();
        assert_eq!(read_module_path(&dir.to_string_lossy()), "example.com/m");
    }
}
