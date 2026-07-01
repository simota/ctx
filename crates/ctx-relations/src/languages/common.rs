// crates/ctx-relations/src/languages/common.rs
//
// Shared helpers for the per-language extractors.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::walk::WalkedFile;

/// A single walked file the language extractors need to see — repo-relative
/// slash path + absolute on-disk path. Mirrors the (Path, AbsPath, IsDir)
/// subset of model.FileInfo the Go relations package consumes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    pub rel: String,
    pub abs: PathBuf,
    pub is_dir: bool,
}

impl FileEntry {
    pub fn from(walked: &WalkedFile) -> Self {
        Self {
            rel: walked.path.clone(),
            abs: walked.abs_path.clone(),
            is_dir: walked.is_dir,
        }
    }
}

/// Mirror of `supportedExt` in relations.go — reports whether the path's
/// extension is one of the languages we extract imports from.
pub fn supported_ext(p: &str) -> bool {
    let ext = lowercase_ext(p);
    matches!(
        ext.as_str(),
        ".go"
            | ".ts"
            | ".tsx"
            | ".js"
            | ".jsx"
            | ".mjs"
            | ".cjs"
            | ".svelte"
            | ".vue"
            | ".py"
            | ".java"
            | ".kt"
            | ".kts"
            | ".php"
            | ".swift"
    )
}

/// `filepath.Ext(p)` (Go) lower-cased. Returns an empty string when the
/// path has no extension.
pub fn lowercase_ext(p: &str) -> String {
    if let Some(idx) = p.rfind('.') {
        let slash_idx = p.rfind('/').map(|i| i + 1).unwrap_or(0);
        if idx >= slash_idx {
            return p[idx..].to_ascii_lowercase();
        }
    }
    String::new()
}

/// Mirror of `buildFileSet` — set of repo-relative slash-paths in the walk.
pub fn file_set(files: &[FileEntry]) -> HashSet<String> {
    let mut out = HashSet::with_capacity(files.len());
    for fi in files {
        if fi.is_dir {
            continue;
        }
        out.insert(fi.rel.clone());
    }
    out
}

/// Mirror of `dedupSorted` in relations.go — sorts in place and removes
/// adjacent duplicates. Returns the deduped vector.
pub fn dedup_sorted(mut ss: Vec<String>) -> Vec<String> {
    if ss.is_empty() {
        return ss;
    }
    ss.sort();
    ss.dedup();
    ss
}

/// Mirror of Go's `path.Dir(p)` for forward-slash paths. Returns "" when
/// the path has no slash (the Go `path.Dir` returns "."; we normalise
/// to "" because the relations callers compare against "" everywhere).
pub fn parent_slash(p: &str) -> String {
    match p.rfind('/') {
        Some(i) => p[..i].to_string(),
        None => String::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn supported_ext_matches_all_languages() {
        for ext in [
            ".go", ".ts", ".tsx", ".js", ".jsx", ".mjs", ".cjs", ".svelte", ".vue", ".py", ".java",
            ".kt", ".kts", ".php", ".swift",
        ] {
            assert!(supported_ext(&format!("foo{ext}")), "{ext}");
        }
        for ext in [".rs", ".md", ".json", ".css"] {
            assert!(!supported_ext(&format!("foo{ext}")), "{ext}");
        }
    }

    #[test]
    fn parent_slash_strips_basename() {
        assert_eq!(parent_slash("a/b/c.go"), "a/b");
        assert_eq!(parent_slash("foo.go"), "");
        assert_eq!(parent_slash(""), "");
    }
}
