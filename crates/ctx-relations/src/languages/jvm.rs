// crates/ctx-relations/src/languages/jvm.rs
//
// Port of internal/relations/jvm.go.

use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};

use crate::patterns;

/// Mirror of `jvmIndex`.
#[derive(Debug, Default)]
pub struct JvmIndex {
    /// `<package>.<basename>` → file. Basename without extension.
    pub fqn: HashMap<String, String>,
    /// package name → every file declared in that package.
    pub pkg: HashMap<String, Vec<String>>,
}

/// Mirror of `buildJVMIndex(files)`.
pub fn build_jvm_index(files: &[super::common::FileEntry]) -> JvmIndex {
    let mut idx = JvmIndex::default();
    for fi in files {
        if fi.is_dir {
            continue;
        }
        let ext = super::common::lowercase_ext(&fi.rel);
        if ext != ".java" && ext != ".kt" && ext != ".kts" {
            continue;
        }
        let pkg = read_jvm_package(&fi.abs);
        let base = file_base_no_ext(&fi.rel);
        idx.pkg.entry(pkg.clone()).or_default().push(fi.rel.clone());
        let key = if pkg.is_empty() {
            base
        } else {
            format!("{pkg}.{base}")
        };
        idx.fqn.entry(key).or_insert_with(|| fi.rel.clone());
    }
    idx
}

fn file_base_no_ext(rel: &str) -> String {
    let last = rel.rsplit('/').next().unwrap_or(rel);
    match last.rfind('.') {
        Some(i) => last[..i].to_string(),
        None => last.to_string(),
    }
}

/// Mirror of `readJVMPackage(absPath)`.
pub fn read_jvm_package(abs_path: &std::path::Path) -> String {
    let f = match fs::File::open(abs_path) {
        Ok(f) => f,
        Err(_) => return String::new(),
    };
    let reader = BufReader::new(f);
    let mut in_block_comment = false;
    for line_result in reader.lines() {
        let raw = match line_result {
            Ok(l) => l,
            Err(_) => return String::new(),
        };
        let mut line = raw.trim().to_string();
        if line.is_empty() {
            continue;
        }
        if in_block_comment {
            if let Some(i) = line.find("*/") {
                in_block_comment = false;
                line = line[i + 2..].trim().to_string();
                if line.is_empty() {
                    continue;
                }
            } else {
                continue;
            }
        }
        if line.starts_with("//") {
            continue;
        }
        if line.starts_with("/*") {
            if !line.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }
        if line.starts_with('@') {
            // file-level annotation (e.g. Kotlin `@file:JvmName`).
            continue;
        }
        if line.starts_with("package ") || line.starts_with("package\t") {
            let mut rest = line["package".len()..].trim().to_string();
            if let Some(stripped) = rest.strip_suffix(';') {
                rest = stripped.to_string();
            }
            return rest.trim().to_string();
        }
        // First non-package real line — stop scanning.
        return String::new();
    }
    String::new()
}

/// Mirror of `resolveJVMImports(absPath, rel, idx)`.
pub fn resolve_jvm_imports(abs_path: &std::path::Path, rel: &str, idx: &JvmIndex) -> Vec<String> {
    let src = match fs::read_to_string(abs_path) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };
    let ext = super::common::lowercase_ext(rel);
    let is_java = ext == ".java";

    let mut out = Vec::new();

    if is_java {
        let re = patterns::java_import_re();
        for caps in re.captures_iter(&src) {
            let is_static = caps
                .get(1)
                .map(|m| m.as_str().trim() == "static")
                .unwrap_or(false);
            let spec = caps.get(2).map(|m| m.as_str()).unwrap_or("").to_string();
            if spec.is_empty() {
                continue;
            }
            resolve_one_jvm(&spec, is_static, false, idx, &mut out);
        }
    } else {
        let re = patterns::kotlin_import_re();
        for caps in re.captures_iter(&src) {
            let spec = caps.get(1).map(|m| m.as_str()).unwrap_or("").to_string();
            if spec.is_empty() {
                continue;
            }
            resolve_one_jvm(&spec, false, true, idx, &mut out);
        }
    }
    out
}

fn resolve_one_jvm(
    spec: &str,
    is_static: bool,
    is_kotlin: bool,
    idx: &JvmIndex,
    out: &mut Vec<String>,
) {
    let mut spec = spec.to_string();
    if spec.ends_with(".*") {
        let pkg = spec.trim_end_matches(".*");
        if let Some(files) = idx.pkg.get(pkg) {
            out.extend(files.iter().cloned());
        }
        return;
    }
    if is_static {
        if let Some(i) = spec.rfind('.') {
            if i > 0 {
                spec.truncate(i);
            }
        }
    }
    if let Some(file) = idx.fqn.get(&spec) {
        out.push(file.clone());
        return;
    }
    if is_kotlin {
        if let Some(i) = spec.rfind('.') {
            if i > 0 {
                let pkg = &spec[..i];
                let name = &spec[i + 1..];
                if !name.is_empty()
                    && name
                        .chars()
                        .next()
                        .map(|c| c.is_ascii_lowercase())
                        .unwrap_or(false)
                {
                    if let Some(files) = idx.pkg.get(pkg) {
                        out.extend(files.iter().cloned());
                    }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::languages::common::FileEntry;

    fn write_temp(dir_name: &str, files: &[(&str, &str)]) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "rel-jvm-{}-{}-{}",
            dir_name,
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        for (rel, content) in files {
            let p = dir.join(rel);
            if let Some(parent) = p.parent() {
                std::fs::create_dir_all(parent).unwrap();
            }
            std::fs::write(&p, content).unwrap();
        }
        dir
    }

    #[test]
    fn java_import_resolves_to_class_file() {
        let dir = write_temp(
            "java-resolve",
            &[
                ("a/A.java", "package a;\nclass A {}\n"),
                ("b/B.java", "package b;\nimport a.A;\nclass B {}\n"),
            ],
        );
        let files = vec![
            FileEntry {
                rel: "a/A.java".to_string(),
                abs: dir.join("a/A.java"),
                is_dir: false,
            },
            FileEntry {
                rel: "b/B.java".to_string(),
                abs: dir.join("b/B.java"),
                is_dir: false,
            },
        ];
        let idx = build_jvm_index(&files);
        assert_eq!(idx.fqn.get("a.A"), Some(&"a/A.java".to_string()));
        let r = resolve_jvm_imports(&dir.join("b/B.java"), "b/B.java", &idx);
        assert_eq!(r, vec!["a/A.java".to_string()]);
    }
}
