//! `inferRole` — shared port of Go's `internal/walk/walk.go` `inferRole`.
//!
//! Used by `/api/tree`, `/api/dir`-adjacent `/api/budget`, and `/api/symbols`'s
//! `/api/definition` file-meta index, so a file gets the same `role`
//! classification everywhere instead of three independently-drifting copies.

/// Infer a file's role from its root-relative, slash-separated path.
pub fn infer_role(rel_slash: &str) -> String {
    let base = rel_slash.rsplit('/').next().unwrap_or(rel_slash);
    let lower_path = rel_slash.to_ascii_lowercase();
    let lower_base = base.to_ascii_lowercase();
    let ext = if lower_base.contains('.') {
        lower_base.rsplit('.').next().unwrap_or("")
    } else {
        ""
    };

    if lower_path.starts_with("tests/")
        || lower_path.contains("/tests/")
        || lower_base.ends_with("_test.go")
        || is_dotted_test_name(&lower_base)
    {
        return "test".to_string();
    }
    if ext == "md" || lower_base.starts_with("license") || lower_base.starts_with("readme") {
        return "doc".to_string();
    }
    if is_config_file(&lower_base, ext) {
        return "config".to_string();
    }
    if base == "main.ts"
        || base == "main.go"
        || base == "main.py"
        || base == "index.ts"
        || base == "index.tsx"
        || base == "index.js"
        || (rel_slash.starts_with("cmd/") && rel_slash.ends_with("/main.go"))
    {
        return "entry".to_string();
    }
    if base.contains("router") || base.contains("route") || base.contains("Router") {
        return "route".to_string();
    }
    if is_core_extension(ext) {
        return "core".to_string();
    }
    String::new()
}

fn is_dotted_test_name(base: &str) -> bool {
    for suffix in &[".test.ts", ".test.tsx", ".test.js", ".test.go", ".test.py"] {
        if base.ends_with(suffix) {
            return true;
        }
    }
    false
}

fn is_config_file(base: &str, ext: &str) -> bool {
    matches!(
        base,
        "package.json" | "go.mod" | "cargo.toml" | "pyproject.toml" | "dockerfile" | "makefile"
    ) || matches!(ext, "toml" | "yaml" | "yml")
}

/// Unifies the tree/budget list with the broader list deliberately added for
/// `/api/symbols` in 78498d3 ("extract Rust/Swift/Kotlin/Java symbols in file
/// detail") — Swift/Kotlin/Java are first-class languages with tree-sitter
/// symbol-extraction support (see `ctx-symbols`), so they should classify as
/// "core" consistently across `/api/tree`, `/api/budget`, and `/api/definition`
/// instead of only in the file-detail view.
fn is_core_extension(ext: &str) -> bool {
    matches!(
        ext,
        "ts" | "tsx" | "js" | "go" | "py" | "rs" | "swift" | "kt" | "kts" | "java"
    )
}
