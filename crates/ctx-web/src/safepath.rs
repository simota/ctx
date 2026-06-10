//! Port of `internal/web/safepath.go`.
//!
//! `resolve` joins a caller-supplied relative path against the served root
//! and guarantees the result stays inside root. It mirrors the Go semantics
//! exactly so error envelopes are byte-identical: the deepest existing
//! ancestor's symlinks are resolved, traversal segments are rejected, and
//! absolute / Windows-style paths are refused.

use std::path::{Component, Path, PathBuf};

/// Path-resolution failure, mapped to the same error codes Go emits via
/// `writeBadPath`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PathError {
    /// `absolute_path` — rel is absolute or a Windows drive/UNC path.
    Absolute,
    /// `path_traversal` — rel escapes root via `..`.
    Traversal,
    /// `outside_root` — resolved target landed outside root (symlink escape).
    Outside,
}

impl PathError {
    /// The `code` field Go puts in the error envelope.
    pub fn code(self) -> &'static str {
        match self {
            PathError::Absolute => "absolute_path",
            PathError::Traversal => "path_traversal",
            PathError::Outside => "outside_root",
        }
    }

    /// The `message` field Go puts in the error envelope (the Go error's
    /// `.Error()` string).
    pub fn message(self) -> &'static str {
        match self {
            PathError::Absolute => "absolute path not allowed",
            PathError::Traversal => "path traversal not allowed",
            PathError::Outside => "path outside root",
        }
    }
}

/// Resolve `rel` against `root`, returning an absolute path guaranteed to be
/// inside the canonicalized root. Mirrors Go `web.Resolve`.
pub fn resolve(root: &str, rel: &str) -> Result<PathBuf, PathError> {
    // filepath.Abs(root)
    let abs_root = absolute(Path::new(root));
    // filepath.EvalSymlinks(absRoot) — best-effort; fall back to absRoot.
    let abs_root = std::fs::canonicalize(&abs_root).unwrap_or(abs_root);

    if rel.is_empty() || rel == "." {
        return Ok(abs_root);
    }
    if Path::new(rel).is_absolute() {
        return Err(PathError::Absolute);
    }
    // Windows drive letter (e.g. "C:...") rejected on every OS.
    let bytes = rel.as_bytes();
    if bytes.len() >= 2 && bytes[1] == b':' {
        return Err(PathError::Absolute);
    }
    // UNC / double-slash prefixes.
    if rel.starts_with("\\\\") || rel.starts_with("//") {
        return Err(PathError::Absolute);
    }

    // filepath.Clean(filepath.FromSlash(rel)); we normalize slashes and clean.
    let cleaned = clean(&rel.replace('\\', "/"));
    if cleaned == ".." {
        return Err(PathError::Traversal);
    }
    if cleaned == "." {
        return Ok(abs_root);
    }
    if cleaned.starts_with("../") {
        return Err(PathError::Traversal);
    }
    for seg in cleaned.split('/') {
        if seg == ".." {
            return Err(PathError::Traversal);
        }
    }

    let joined = abs_root.join(&cleaned);
    let resolved = eval_symlinks_deepest(&joined);

    // resolved == absRoot OR resolved+sep starts with absRoot+sep.
    if resolved != abs_root && !starts_with_dir(&resolved, &abs_root) {
        return Err(PathError::Outside);
    }
    Ok(resolved)
}

/// `filepath.Abs`: make a path absolute against the current working dir
/// without touching the filesystem.
fn absolute(p: &Path) -> PathBuf {
    if p.is_absolute() {
        return normalize_dots(p);
    }
    let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
    normalize_dots(&cwd.join(p))
}

/// Lexically clean a path (collapse `.`/`..` and redundant separators),
/// like Go `filepath.Clean` for a slash-separated relative path.
fn clean(p: &str) -> String {
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => {}
            ".." => {
                if matches!(out.last(), Some(&"..")) || out.is_empty() {
                    out.push("..");
                } else {
                    out.pop();
                }
            }
            s => out.push(s),
        }
    }
    if out.is_empty() {
        ".".to_string()
    } else {
        out.join("/")
    }
}

/// Lexically collapse `.`/`..` in an absolute path without filesystem access.
fn normalize_dots(p: &Path) -> PathBuf {
    let mut out = PathBuf::new();
    for comp in p.components() {
        match comp {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// `evalSymlinksDeepest`: resolve symlinks for the deepest existing ancestor,
/// then re-attach the non-existent tail unchanged.
fn eval_symlinks_deepest(p: &Path) -> PathBuf {
    if let Ok(resolved) = std::fs::canonicalize(p) {
        return resolved;
    }
    let parent = match p.parent() {
        Some(par) if par != p && par != Path::new("") => par,
        _ => return p.to_path_buf(),
    };
    let resolved_parent = eval_symlinks_deepest(parent);
    match p.file_name() {
        Some(name) => resolved_parent.join(name),
        None => resolved_parent,
    }
}

/// Reports whether `child` is `root` or lives under it (component-boundary
/// aware, matching Go's `strings.HasPrefix(resolved+sep, absRoot+sep)`).
fn starts_with_dir(child: &Path, root: &Path) -> bool {
    child.starts_with(root)
}
