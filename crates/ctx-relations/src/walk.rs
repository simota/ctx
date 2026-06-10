// crates/ctx-relations/src/walk.rs
//
// Minimal repo walker covering the slice of internal/walk.DefaultOptions
// the relations module actually consumes:
//
//   - Recursive directory traversal from `root`.
//   - ExtraIgnore defaults: `.git/`, `node_modules/`, `dist/`, `coverage/`.
//   - .gitignore / .ctxignore support is INTENTIONALLY OMITTED in the
//     Rust port — the parity fixtures the goldens are generated against
//     never contain a .gitignore file (they are synthetic trees we
//     control). For real-world repos at runtime the dispatcher calls
//     the Go walker before handing paths to relations; Rust only sees
//     the post-filter file list. (See dispatch_rust.go.)
//
// We deliberately keep this walker dumb-and-fast: a single std::fs
// recursion with byte-cheap path normalisation. The relations regex
// hot path dominates total runtime even on big trees so the walker
// itself is not a perf concern.

use std::fs;
use std::path::{Path, PathBuf};

const EXTRA_IGNORE: &[&str] = &[".git", "node_modules", "dist", "coverage"];

/// A walked file. `path` is repo-relative slash-separated; `abs_path`
/// is the absolute on-disk path the language extractors will open.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WalkedFile {
    pub path: String,
    pub abs_path: PathBuf,
    pub is_dir: bool,
}

/// Walk `root` and return every file/directory encountered, with
/// repo-relative slash-path identifiers. Order matches std::fs::read_dir,
/// which on most filesystems is dirent order — the relations build
/// re-sorts wherever it matters (Index BTreeMaps, dedup_sorted).
pub fn walk(root: &str) -> std::io::Result<Vec<WalkedFile>> {
    let root_path = PathBuf::from(root);
    let abs_root = match fs::canonicalize(&root_path) {
        Ok(p) => p,
        Err(_) => root_path.clone(),
    };
    let mut out = Vec::new();
    visit(&abs_root, &abs_root, &mut out)?;
    Ok(out)
}

fn visit(root: &Path, dir: &Path, out: &mut Vec<WalkedFile>) -> std::io::Result<()> {
    let rel = relativise(root, dir);
    out.push(WalkedFile {
        path: rel,
        abs_path: dir.to_path_buf(),
        is_dir: true,
    });
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };
    let mut children: Vec<_> = entries.filter_map(Result::ok).collect();
    // Deterministic order — std::fs::read_dir is OS-dependent. Sort by
    // file name so the parity goldens are reproducible.
    children.sort_by_key(|e| e.file_name());
    for entry in children {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if EXTRA_IGNORE.iter().any(|ig| name_str == *ig) {
            continue;
        }
        let path = entry.path();
        let ty = match entry.file_type() {
            Ok(t) => t,
            Err(_) => continue,
        };
        if ty.is_symlink() {
            continue;
        }
        if ty.is_dir() {
            visit(root, &path, out)?;
        } else {
            out.push(WalkedFile {
                path: relativise(root, &path),
                abs_path: path,
                is_dir: false,
            });
        }
    }
    Ok(())
}

fn relativise(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    if rel.as_os_str().is_empty() {
        return ".".to_string();
    }
    let mut s = String::with_capacity(rel.as_os_str().len());
    for (i, comp) in rel.components().enumerate() {
        if i > 0 {
            s.push('/');
        }
        s.push_str(&comp.as_os_str().to_string_lossy());
    }
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}
