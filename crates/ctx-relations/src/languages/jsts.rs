// crates/ctx-relations/src/languages/jsts.rs
//
// Port of the JS/TS/Svelte/Vue extractors in internal/relations/relations.go.

use std::collections::HashSet;
use std::fs;

use crate::patterns;

/// Suffix probe order used when resolving a relative import without
/// an explicit extension. Mirrors `jsTryExts`.
const JS_TRY_EXTS: &[&str] = &[
    ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".svelte", ".vue", ".d.ts",
];

/// Mirror of `resolveJSImports(absPath, fromRel, all)`.
pub fn resolve_js_imports(
    abs_path: &std::path::Path,
    from_rel: &str,
    all: &HashSet<String>,
) -> Vec<String> {
    let data = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    resolve_js_imports_from_src(&data, from_rel, all)
}

/// Mirror of `resolveSvelteImports` / `resolveVueImports` — both read
/// the file, concatenate every `<script>` body, and run the JS resolver.
pub fn resolve_scripted_file(
    abs_path: &std::path::Path,
    from_rel: &str,
    all: &HashSet<String>,
) -> Vec<String> {
    let data = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let body = concat_script_blocks(&data);
    if body.is_empty() {
        return Vec::new();
    }
    resolve_js_imports_from_src(&body, from_rel, all)
}

/// Mirror of `concatScriptBlocks(src)`.
fn concat_script_blocks(src: &str) -> String {
    let re = patterns::script_re();
    let mut out = String::new();
    for caps in re.captures_iter(src) {
        if let Some(m) = caps.get(1) {
            out.push_str(m.as_str());
            out.push('\n');
        }
    }
    out
}

/// Mirror of `resolveJSImportsFromSrc(src, fromRel, all)`.
pub fn resolve_js_imports_from_src(
    src: &str,
    from_rel: &str,
    all: &HashSet<String>,
) -> Vec<String> {
    let stripped = strip_js_comments(src);
    let from_dir = super::common::parent_slash(from_rel);
    let re = patterns::js_import_re();
    let mut out = Vec::new();
    for caps in re.captures_iter(&stripped) {
        let spec = caps
            .get(1)
            .or_else(|| caps.get(2))
            .map(|m| m.as_str())
            .unwrap_or("");
        if spec.is_empty() || !is_relative_spec(spec) {
            continue;
        }
        if let Some(resolved) = resolve_relative_js(&from_dir, spec, all) {
            out.push(resolved);
        }
    }
    out
}

/// Mirror of `stripJSComments`.
fn strip_js_comments(src: &str) -> String {
    let s = patterns::js_block_comment_re().replace_all(src, "");
    let s = patterns::js_line_comment_re().replace_all(&s, "");
    s.into_owned()
}

/// Mirror of `isRelativeSpec`.
fn is_relative_spec(spec: &str) -> bool {
    spec == "." || spec == ".." || spec.starts_with("./") || spec.starts_with("../")
}

/// Mirror of `resolveRelativeJS(fromDir, spec, all)`.
fn resolve_relative_js(from_dir: &str, spec: &str, all: &HashSet<String>) -> Option<String> {
    let joined = join_slash(from_dir, spec);
    if joined.is_empty() {
        return None;
    }
    let joined = joined.trim_end_matches('/').to_string();

    if all.contains(&joined) {
        return Some(joined);
    }
    for ext in JS_TRY_EXTS {
        let cand = format!("{joined}{ext}");
        if all.contains(&cand) {
            return Some(cand);
        }
    }
    for ext in JS_TRY_EXTS {
        let cand = format!("{joined}/index{ext}");
        if all.contains(&cand) {
            return Some(cand);
        }
    }
    None
}

/// Mirror of Go's `path.Join(a, b)` for forward-slash paths. Handles
/// the dot-pop semantics needed for `../` traversal so `from_dir` +
/// `./y` ≡ `from_dir/y` and `from_dir/sub` + `../other` ≡ `from_dir/other`.
fn join_slash(a: &str, b: &str) -> String {
    let combined = if a.is_empty() {
        b.to_string()
    } else if b.is_empty() {
        a.to_string()
    } else {
        format!("{a}/{b}")
    };
    clean_slash(&combined)
}

/// Mirror of Go's `path.Clean`. Resolves `.` / `..` segments.
fn clean_slash(p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    let absolute = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                if let Some(last) = out.last() {
                    if *last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !absolute {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let mut s = if absolute { "/".to_string() } else { String::new() };
    s.push_str(&out.join("/"));
    if s.is_empty() {
        // Go path.Clean returns "." for an empty result. We use "" so
        // the caller can compare against "" — matches the relations
        // convention which strips "." → "".
        return String::new();
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn resolves_extension_probes() {
        let all = set(&["a/b.ts", "a/c/index.tsx"]);
        let r = resolve_relative_js("a", "./b", &all).unwrap();
        assert_eq!(r, "a/b.ts");
        let r = resolve_relative_js("a", "./c", &all).unwrap();
        assert_eq!(r, "a/c/index.tsx");
    }

    #[test]
    fn parses_import_and_require() {
        let all = set(&["a/foo.ts", "a/bar.js"]);
        let src = r#"import x from "./foo";
const y = require("./bar");"#;
        let r = resolve_js_imports_from_src(src, "a/main.ts", &all);
        assert!(r.contains(&"a/foo.ts".to_string()));
        assert!(r.contains(&"a/bar.js".to_string()));
    }

    #[test]
    fn ignores_bare_specs() {
        let all = set(&[]);
        let src = r#"import x from "react";"#;
        let r = resolve_js_imports_from_src(src, "a/main.ts", &all);
        assert!(r.is_empty(), "{r:?}");
    }

    #[test]
    fn join_slash_handles_parent_traversal() {
        assert_eq!(join_slash("a/b", "../c"), "a/c");
        assert_eq!(join_slash("", "./x"), "x");
    }

    #[test]
    fn script_concat_combines_blocks() {
        let src = r#"<script>import x from "./a";</script>
        <p>html</p>
        <script lang="ts">import y from "./b";</script>"#;
        let body = concat_script_blocks(src);
        assert!(body.contains("./a"));
        assert!(body.contains("./b"));
    }
}
