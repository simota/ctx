// crates/ctx-relations/src/languages/py.rs
//
// Port of the Python extractor in internal/relations/relations.go.

use std::collections::HashSet;
use std::fs;

use crate::patterns;

const PY_TRY_EXTS: &[&str] = &[".py"];

/// Mirror of `resolvePyImports`.
pub fn resolve_py_imports(
    abs_path: &std::path::Path,
    from_rel: &str,
    all: &HashSet<String>,
) -> Vec<String> {
    let src = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let re = patterns::py_import_re();
    let from_dir = super::common::parent_slash(from_rel);
    let mut out = Vec::new();

    for caps in re.captures_iter(&src) {
        let dots = caps.get(1).map(|m| m.as_str()).unwrap_or("");
        let from_mod = caps.get(2).map(|m| m.as_str()).unwrap_or("");
        let from_names = caps.get(3).map(|m| m.as_str()).unwrap_or("");
        let import_mods = caps.get(4).map(|m| m.as_str()).unwrap_or("");

        if !import_mods.is_empty() {
            // `import a.b, c.d as e` — split on comma, drop `as ...`.
            for part in import_mods.split(',') {
                let mod_name = strip_py_as(part).trim().to_string();
                if mod_name.is_empty() {
                    continue;
                }
                if let Some(resolved) = resolve_py_module(&mod_name, "", &from_dir, all) {
                    out.push(resolved);
                }
            }
        } else if !from_names.is_empty() {
            let mut fallback_used = false;
            for part in from_names.split(',') {
                let mut name = strip_py_as(part).trim().to_string();
                name = name.trim_start_matches('(').to_string();
                name = name.trim_end_matches(')').to_string();
                name = name.trim().to_string();
                if name.is_empty() || name == "*" {
                    continue;
                }
                let spec = if !from_mod.is_empty() {
                    format!("{from_mod}.{name}")
                } else {
                    name.clone()
                };
                if let Some(resolved) = resolve_py_module(&spec, dots, &from_dir, all) {
                    out.push(resolved);
                    continue;
                }
                // Fall back to the bare module once per match-row.
                if from_mod.is_empty() || fallback_used {
                    continue;
                }
                if let Some(resolved) = resolve_py_module(from_mod, dots, &from_dir, all) {
                    out.push(resolved);
                    fallback_used = true;
                }
            }
        }
    }
    out
}

/// Mirror of `stripPyAs`.
fn strip_py_as(part: &str) -> &str {
    if let Some(i) = part.find(" as ") {
        &part[..i]
    } else {
        part
    }
}

/// Mirror of `resolvePyModule(spec, dots, fromDir, all)`.
fn resolve_py_module(
    spec: &str,
    dots: &str,
    from_dir: &str,
    all: &HashSet<String>,
) -> Option<String> {
    let mut base = if dots.is_empty() {
        String::new()
    } else {
        let mut b = from_dir.to_string();
        for _ in 1..dots.len() {
            if b.is_empty() {
                return None;
            }
            b = super::common::parent_slash(&b);
        }
        b
    };

    let tail = spec.replace('.', "/");
    let full = if base.is_empty() {
        tail
    } else {
        // Use Go path.Join semantics — concatenate with "/".
        if tail.is_empty() {
            base
        } else {
            base.push('/');
            base.push_str(&tail);
            base
        }
    };
    let full = full.trim_end_matches('/').to_string();
    if full.is_empty() {
        return None;
    }
    for ext in PY_TRY_EXTS {
        let cand = format!("{full}{ext}");
        if all.contains(&cand) {
            return Some(cand);
        }
    }
    let init_cand = format!("{full}/__init__.py");
    if all.contains(&init_cand) {
        return Some(init_cand);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn set(items: &[&str]) -> HashSet<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    fn write_temp(content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rel-py-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join("a.py");
        std::fs::write(&p, content).unwrap();
        p
    }

    #[test]
    fn import_resolves_absolute_module() {
        let all = set(&["pkg/__init__.py", "pkg/mod.py"]);
        let p = write_temp("import pkg.mod\n");
        let r = resolve_py_imports(&p, "a.py", &all);
        assert!(r.contains(&"pkg/mod.py".to_string()), "{r:?}");
    }

    #[test]
    fn from_import_falls_back_to_module_init() {
        let all = set(&["pkg/__init__.py"]);
        let p = write_temp("from pkg import a, b\n");
        let r = resolve_py_imports(&p, "main.py", &all);
        assert!(r.contains(&"pkg/__init__.py".to_string()), "{r:?}");
    }
}
