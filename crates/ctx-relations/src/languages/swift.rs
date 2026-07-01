// crates/ctx-relations/src/languages/swift.rs
//
// Port of internal/relations/swift.go.

use std::collections::HashMap;
use std::fs;

use crate::patterns;

/// Mirror of `swiftModuleIndex` — SPM module name → repo-relative .swift
/// files under `Sources/<Module>/`. Returns None when there are no .swift
/// files in the tree (matches the Go contract — the resolver returns a
/// nil index in that case).
pub fn build_swift_modules(
    files: &[super::common::FileEntry],
) -> Option<HashMap<String, Vec<String>>> {
    let mut has_swift = false;
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for fi in files {
        if fi.is_dir {
            continue;
        }
        if super::common::lowercase_ext(&fi.rel) != ".swift" {
            continue;
        }
        has_swift = true;
        let parts: Vec<&str> = fi.rel.split('/').collect();
        if parts.len() < 3 || parts[0] != "Sources" {
            continue;
        }
        out.entry(parts[1].to_string())
            .or_default()
            .push(fi.rel.clone());
    }
    if !has_swift {
        return None;
    }
    Some(out)
}

/// Mirror of `resolveSwiftImports`.
pub fn resolve_swift_imports(
    abs_path: &std::path::Path,
    rel: &str,
    modules: Option<&HashMap<String, Vec<String>>>,
) -> Vec<String> {
    let modules = match modules {
        Some(m) if !m.is_empty() => m,
        _ => return Vec::new(),
    };
    let data = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let re = patterns::swift_import_re();
    let mut self_mod = String::new();
    let parts: Vec<&str> = rel.split('/').collect();
    if parts.len() >= 3 && parts[0] == "Sources" {
        self_mod = parts[1].to_string();
    }
    let mut out = Vec::new();
    for caps in re.captures_iter(&data) {
        let mod_name = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        if mod_name.is_empty() || mod_name == self_mod {
            continue;
        }
        if let Some(files) = modules.get(mod_name) {
            out.extend(files.iter().cloned());
        }
    }
    out
}
