// crates/ctx-relations/src/cache.rs
//
// Port of internal/relations/cache.go.
//
// BuildCached + InvalidateCache memoise the result of Build() per
// absolute root path. The cache is invalidated when any file's
// (size, mtime) signature differs from the snapshot taken on the
// previous build — added/removed/modified files all force a rebuild.
//
// The cache is process-local. It is NOT persisted across restarts and
// NOT shared between processes — same scope as the Go source.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex, RwLock};

use once_cell::sync::Lazy;

use crate::build;
use crate::types::Index;
use crate::walk::walk;

/// (size, mtime) fingerprint used to detect changes. mtime is unix
/// nanoseconds since epoch, matching the Go source.
#[derive(Debug, Clone, PartialEq, Eq)]
struct FileSig {
    size: i64,
    mtime_nanos: i64,
}

struct CacheEntry {
    sigs: HashMap<String, FileSig>,
    index: Arc<Index>,
}

static CACHE: Lazy<RwLock<HashMap<PathBuf, Arc<Mutex<Option<CacheEntry>>>>>> =
    Lazy::new(|| RwLock::new(HashMap::new()));

/// Mirror of `relations.BuildCached(root)`.
///
/// FALLBACK POLICY (matches Go): any error in path-resolution or sig
/// snapshotting falls back to an uncached `Build(root)` — the cache is
/// an optimisation, not a correctness contract.
pub fn build_cached(root: &str) -> std::io::Result<Index> {
    let abs_root = match fs::canonicalize(root) {
        Ok(p) => p,
        Err(_) => return build::build(root),
    };
    let sigs = match snapshot_sigs(&abs_root.to_string_lossy()) {
        Ok(s) => s,
        Err(_) => return build::build(root),
    };

    // Per-root lock pair so two roots can build concurrently. The outer
    // RwLock guards the (root → entry) map; the inner Mutex serialises
    // (build, store, lookup) for one root.
    let entry_lock = {
        let mut map = CACHE.write().unwrap();
        map.entry(abs_root.clone())
            .or_insert_with(|| Arc::new(Mutex::new(None)))
            .clone()
    };

    let mut guard = entry_lock.lock().unwrap();
    if let Some(entry) = guard.as_ref() {
        if sigs_equal(&entry.sigs, &sigs) {
            return Ok((*entry.index).clone());
        }
    }
    let idx = build::build(root)?;
    let arc_idx = Arc::new(idx.clone());
    *guard = Some(CacheEntry {
        sigs,
        index: arc_idx,
    });
    Ok(idx)
}

/// Mirror of `relations.InvalidateCache(root)`.
pub fn invalidate_cache(root: &str) {
    let abs = match fs::canonicalize(root) {
        Ok(p) => p,
        Err(_) => return,
    };
    let mut map = CACHE.write().unwrap();
    map.remove(&abs);
}

/// Mirror of `snapshotSigs(root)`.
fn snapshot_sigs(root: &str) -> std::io::Result<HashMap<String, FileSig>> {
    let walked = walk(root)?;
    let mut out = HashMap::with_capacity(walked.len());
    for fi in &walked {
        if fi.is_dir {
            continue;
        }
        let sig = fs::metadata(&fi.abs_path)
            .ok()
            .map(|m| FileSig {
                size: m.len() as i64,
                mtime_nanos: m
                    .modified()
                    .ok()
                    .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                    .map(|d| d.as_nanos() as i64)
                    .unwrap_or(0),
            })
            .unwrap_or(FileSig {
                size: 0,
                mtime_nanos: 0,
            });
        out.insert(fi.path.clone(), sig);
    }
    Ok(out)
}

fn sigs_equal(a: &HashMap<String, FileSig>, b: &HashMap<String, FileSig>) -> bool {
    if a.len() != b.len() {
        return false;
    }
    for (k, va) in a {
        match b.get(k) {
            Some(vb) if vb == va => continue,
            _ => return false,
        }
    }
    true
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
    fn cached_returns_same_index_on_unchanged_tree() {
        let dir = std::env::temp_dir().join(format!(
            "rel-cache-{}-{}",
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
                ("go.mod", "module example.com/m\n"),
                (
                    "main.go",
                    "package main\nimport \"example.com/m/lib\"\nfunc main() {}\n",
                ),
                ("lib/a.go", "package lib\n"),
            ],
        );
        let root = dir.to_string_lossy().to_string();
        let first = build_cached(&root).unwrap();
        let second = build_cached(&root).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn invalidate_drops_entry() {
        let dir = std::env::temp_dir().join(format!(
            "rel-cache-inv-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        write_tree(&dir, &[("go.mod", "module example.com/m\n")]);
        let root = dir.to_string_lossy().to_string();
        let _ = build_cached(&root).unwrap();
        invalidate_cache(&root);
        let abs = std::fs::canonicalize(&dir).unwrap();
        let map = CACHE.read().unwrap();
        assert!(!map.contains_key(&abs));
    }
}
