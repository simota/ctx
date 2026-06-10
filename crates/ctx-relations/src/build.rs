// crates/ctx-relations/src/build.rs
//
// Main orchestrator. Mirrors `relations.Build(root)` from
// internal/relations/relations.go.

use std::collections::BTreeMap;

use crate::languages;
use crate::languages::common::{dedup_sorted, file_set, supported_ext, FileEntry};
use crate::types::Index;
use crate::walk::walk;

/// Mirror of `relations.Supported(p)`.
pub fn supported(p: &str) -> bool {
    supported_ext(p)
}

/// Mirror of `relations.Build(root)`.
pub fn build(root: &str) -> std::io::Result<Index> {
    let walked = walk(root)?;
    let files: Vec<FileEntry> = walked.iter().map(FileEntry::from).collect();

    let module_path = languages::go::read_module_path(root);

    // Pre-compute the lookups Go's Build uses.
    let go_pkg_files = languages::go::build_go_package_map(&files);
    let all_files = file_set(&files);
    let jvm_idx = languages::jvm::build_jvm_index(&files);
    let swift_mods = languages::swift::build_swift_modules(&files);
    let php_map = languages::php::read_composer_psr4(root);

    let mut imports: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for fi in &files {
        if fi.is_dir || fi.rel == "." {
            continue;
        }
        if !supported_ext(&fi.rel) {
            continue;
        }
        let ext = languages::common::lowercase_ext(&fi.rel);
        let edges: Vec<String> = match ext.as_str() {
            ".go" => languages::go::resolve_go_imports(&fi.abs, &module_path, &go_pkg_files),
            ".ts" | ".tsx" | ".js" | ".jsx" | ".mjs" | ".cjs" => {
                languages::jsts::resolve_js_imports(&fi.abs, &fi.rel, &all_files)
            }
            ".svelte" | ".vue" => {
                languages::jsts::resolve_scripted_file(&fi.abs, &fi.rel, &all_files)
            }
            ".py" => languages::py::resolve_py_imports(&fi.abs, &fi.rel, &all_files),
            ".java" | ".kt" | ".kts" => {
                languages::jvm::resolve_jvm_imports(&fi.abs, &fi.rel, &jvm_idx)
            }
            ".php" => languages::php::resolve_php_imports(&fi.abs, php_map.as_ref(), &all_files),
            ".swift" => {
                languages::swift::resolve_swift_imports(&fi.abs, &fi.rel, swift_mods.as_ref())
            }
            _ => Vec::new(),
        };
        if !edges.is_empty() {
            imports.insert(fi.rel.clone(), dedup_sorted(edges));
        }
    }

    let mut importers: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for (from, tos) in &imports {
        for to in tos {
            importers.entry(to.clone()).or_default().push(from.clone());
        }
    }
    for v in importers.values_mut() {
        let taken = std::mem::take(v);
        *v = dedup_sorted(taken);
    }

    Ok(Index {
        module_path,
        imports,
        importers,
    })
}

/// Mirror of `relations.BuildCached(root)`.
pub fn build_cached(root: &str) -> std::io::Result<Index> {
    crate::cache::build_cached(root)
}

/// Mirror of `relations.InvalidateCache(root)`.
pub fn invalidate_cache(root: &str) {
    crate::cache::invalidate_cache(root)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    fn write_tree(dir: &Path, files: &[(&str, &str)]) {
        for (rel, content) in files {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
        }
    }

    #[test]
    fn build_go_imports_smoke() {
        let dir = std::env::temp_dir().join(format!(
            "rel-build-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        write_tree(
            &dir,
            &[
                ("go.mod", "module example.com/m\n\ngo 1.22\n"),
                (
                    "main.go",
                    "package main\nimport \"example.com/m/lib\"\nfunc main() {}\n",
                ),
                ("lib/a.go", "package lib\n"),
            ],
        );

        let idx = build(&dir.to_string_lossy()).unwrap();
        assert_eq!(idx.module_path, "example.com/m");
        let edges = idx.edges("main.go");
        assert_eq!(edges.imports, vec!["lib/a.go".to_string()]);
        let r_edges = idx.edges("lib/a.go");
        assert_eq!(r_edges.importers, vec!["main.go".to_string()]);
    }

    #[test]
    fn supported_smoke() {
        assert!(supported("a.go"));
        assert!(!supported("a.rs"));
    }
}
