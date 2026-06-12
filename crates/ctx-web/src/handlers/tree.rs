//! `GET /api/tree` — port of `internal/web/handlers.go` `handleTree`.
//!
//! Returns a recursive directory tree rooted at the requested path.
//! Query params: `path` (default "."), `depth` (int, 0=unlimited),
//! `tokens` (bool), `symbols` (bool, DEFERRED), `git` (bool),
//! `use_mtime` (bool), `since`, `until` (time-filter strings).
//!
//! Symbols are DEFERRED. Git status is loaded from `git status --porcelain`
//! when requested and aggregated up directories so the UI can filter changed
//! files while preserving ancestor rows.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::handlers::file::relative_to_root;
use crate::response;
use crate::safepath;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TreeParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    depth: i32,
    #[serde(default)]
    tokens: bool,
    // symbols is DEFERRED — accepted but ignored so the route doesn't 400 on
    // unknown params.
    #[serde(default)]
    _symbols: bool,
    #[serde(default)]
    git: bool,
    #[serde(default)]
    _use_mtime: bool,
    // since/until time-filter strings are accepted but DEFERRED (no git-log
    // access); when provided the handler falls through to a plain walk with
    // no time filtering — same file set, so parity holds for fixture paths.
    #[serde(default)]
    _since: String,
    #[serde(default)]
    _until: String,
}

/// Mirrors `web.TreeNode`. Field order matches Go struct.
#[derive(Serialize)]
pub struct TreeNode {
    path: String,
    name: String,
    is_dir: bool,
    size: i64,
    #[serde(skip_serializing_if = "is_zero_i32")]
    lines: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    tokens: i32,
    #[serde(skip_serializing_if = "str::is_empty")]
    role: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    git: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    updated_at: i64,
    // symbols: DEFERRED — omitempty in Go, omit always here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<TreeNode>,
}

/// Mirrors `web.TreeResponse`. Field order matches Go struct.
#[derive(Serialize)]
struct TreeResponse {
    root: String,
    abs_root: String,
    tree: TreeNode,
    total: i32,
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn handle(
    State(state): State<AppState>,
    params: Result<Query<TreeParams>, QueryRejection>,
) -> Response {
    let Query(params) = match params {
        Ok(q) => q,
        Err(e) => return response::bad_query(e),
    };
    // `git status` + the recursive walk can take seconds on large repos; keep
    // them off the tokio workers so parallel SPA requests aren't starved.
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: TreeParams) -> Response {
    let rel = if params.path.is_empty() {
        "."
    } else {
        &params.path
    };

    let target = match safepath::resolve(&state.root, rel) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let max_depth = if params.depth <= 0 {
        0
    } else {
        params.depth as usize
    };

    let git_status = if params.git {
        GitStatusMap::load(&state.root)
    } else {
        GitStatusMap::default()
    };

    let root_node = match walk_tree(
        &state.root,
        &target,
        0,
        max_depth,
        params.tokens,
        &git_status,
    ) {
        Ok(n) => n,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return response::error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!("stat {}: no such file or directory", target.display()),
                );
            }
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "walk_init",
                &e.to_string(),
            );
        }
    };

    // total: count nodes in the flattened tree (files + dirs).
    let total = count_nodes(&root_node);

    // root field: relative path of target vs served root. Go uses filepath.ToSlash(relativeToRoot(a.Root, target)).
    // relativeToRoot strips abs_root prefix; for target==root this yields "".
    // Go then uses this as-is for the Root field — "." when the walk starts from
    // the served root (because os.ReadDir(".") → relativeToRoot(".", absRoot) = ".").
    // We need to convert "" → "." to match Go's "." output for the root.
    let root_rel_raw = relative_to_root(&state.root, &target);
    let root_rel = if root_rel_raw.is_empty() {
        ".".to_string()
    } else {
        root_rel_raw
    };

    // abs_root: absolute path of the served root (not of `target`). Go uses
    // filepath.Abs(a.Root) for this field.
    let abs_root = std::fs::canonicalize(&state.root)
        .or_else(|_| std::path::absolute(Path::new(&state.root)))
        .unwrap_or_else(|_| PathBuf::from(&state.root));
    let abs_root = abs_root.to_string_lossy().replace('\\', "/");

    response::json(
        StatusCode::OK,
        &TreeResponse {
            root: root_rel,
            abs_root,
            tree: root_node,
            total,
        },
    )
}

// ---------------------------------------------------------------------------
// Walk implementation — mirrors Go walk.DefaultOptions + toTreeNode
// ---------------------------------------------------------------------------

/// Extra-ignore list from Go `walk.DefaultOptions().ExtraIgnore`.
/// NOTE: Do NOT skip hidden dirs generally — Go only skips what the gitignore
/// patterns and ExtraIgnore list cover. `.ctx` etc. are walked normally.
fn should_skip(name: &str) -> bool {
    // ExtraIgnore: ".git/", "node_modules/", "dist/", "coverage/", "target/"
    // In the gitignore library these match exactly the dir name.
    matches!(
        name,
        ".git" | "node_modules" | "dist" | "coverage" | "target"
    )
}

/// Walk the tree rooted at `dir`, building TreeNode children sorted
/// alphabetically (matching Go `os.ReadDir` which returns entries sorted by
/// filename). `root_str` is the served root used for relative-path computation.
fn walk_tree(
    root_str: &str,
    dir: &Path,
    depth: usize,
    max_depth: usize,
    with_tokens: bool,
    git_status: &GitStatusMap,
) -> std::io::Result<TreeNode> {
    let meta = std::fs::symlink_metadata(dir)?;
    // Follow symlinks only for the requested root; deeper symlinked dirs keep
    // their lstat metadata so the walk never recurses through them (cyclic
    // links would loop forever and absolute links would leak outside root).
    let meta = if depth == 0 {
        std::fs::metadata(dir).unwrap_or(meta)
    } else {
        meta
    };

    let rel_raw = relative_to_root(root_str, dir);
    // Convert "" (root) to "." to match Go relativeToRoot behaviour.
    let rel = if rel_raw.is_empty() {
        ".".to_string()
    } else {
        rel_raw
    };

    let name = if rel == "." {
        // Root node: basename of the root dir (mirrors Go `filepath.Base(root)`)
        Path::new(root_str)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string())
    } else {
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    let is_dir = meta.is_dir();
    let size = meta.len() as i64;

    // updated_at: mtime as Unix timestamp (mirrors Go fileTime fallback to mtime).
    let updated_at = if !is_dir {
        use std::time::SystemTime;
        meta.modified()
            .ok()
            .and_then(|mt| mt.duration_since(SystemTime::UNIX_EPOCH).ok())
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0)
    } else {
        0
    };

    let (lines, tokens) = if !is_dir {
        let l = count_lines_file(dir);
        let tok = if with_tokens {
            // Try tiktoken exact count; fall back to size-based estimate.
            match ctx_tokens::count_file(dir.to_str().unwrap_or("")) {
                Ok(n) => n as i32,
                Err(_) => ctx_tokens::estimate_by_size(size) as i32,
            }
        } else {
            0
        };
        (l, tok)
    } else {
        (0, 0)
    };

    let role = if !is_dir {
        infer_role(&rel)
    } else {
        String::new()
    };

    let git = git_status.status_for(&rel, is_dir);
    let mut node = TreeNode {
        path: rel,
        name,
        is_dir,
        size,
        lines,
        tokens,
        role,
        git,
        updated_at,
        children: Vec::new(),
    };

    // Recurse into directory children (sorted; mirrors Go `os.ReadDir`).
    if is_dir && (max_depth == 0 || depth < max_depth) {
        let mut entries: Vec<_> = match std::fs::read_dir(dir) {
            Ok(rd) => rd.flatten().collect(),
            Err(_) => vec![],
        };
        // Sort by file name to match Go `os.ReadDir` alphabetical order.
        entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in entries {
            let child_path = entry.path();
            let child_name = entry.file_name();
            let child_name_str = child_name.to_string_lossy();

            // Only skip directories that match the ExtraIgnore patterns — a
            // regular file named e.g. "dist" must stay visible.
            // NOTE: Do NOT skip all hidden entries — Go walks .ctx/, etc.
            let child_is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if child_is_dir && should_skip(&child_name_str) {
                continue;
            }

            match walk_tree(
                root_str,
                &child_path,
                depth + 1,
                max_depth,
                with_tokens,
                git_status,
            ) {
                Ok(child_node) => node.children.push(child_node),
                Err(_) => continue,
            }
        }
    }

    Ok(node)
}

#[derive(Default)]
struct GitStatusMap {
    by_path: BTreeMap<String, String>,
}

impl GitStatusMap {
    fn load(root: &str) -> Self {
        let output = Command::new("git")
            .args(["-C", root, "status", "--porcelain"])
            .output();
        let Ok(output) = output else {
            return Self::default();
        };
        if !output.status.success() {
            return Self::default();
        }

        let mut by_path = BTreeMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }
            let code = normalize_git_status(&line[..2]);
            if code.is_empty() {
                continue;
            }
            let raw_path = &line[3..];
            let path = normalize_git_status_path(raw_path);
            if !path.is_empty() {
                by_path.insert(path, code);
            }
        }
        Self { by_path }
    }

    fn status_for(&self, rel: &str, is_dir: bool) -> String {
        if !is_dir {
            return self.by_path.get(rel).cloned().unwrap_or_default();
        }
        self.aggregate_dir(rel)
    }

    fn aggregate_dir(&self, rel: &str) -> String {
        let prefix = if rel == "." {
            String::new()
        } else {
            format!("{rel}/")
        };
        let mut best = "";
        let mut best_rank = 0;
        // BTreeMap is key-sorted, so all paths under the prefix form one
        // contiguous range — no need to scan unrelated dirty files.
        for (_, status) in self
            .by_path
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
        {
            let rank = git_status_rank(status);
            if rank > best_rank {
                best = status;
                best_rank = rank;
            }
        }
        best.to_string()
    }
}

pub(crate) fn normalize_git_status(status: &str) -> String {
    if status == "??" {
        return "?".to_string();
    }
    for ch in status.chars() {
        if matches!(ch, 'D' | 'A' | 'M' | 'R' | 'C' | 'T') {
            return ch.to_string();
        }
    }
    String::new()
}

pub(crate) fn normalize_git_status_path(raw: &str) -> String {
    let mut path = raw.trim();
    if let Some((_, new_path)) = path.split_once(" -> ") {
        path = new_path;
    }
    path.trim_matches('"').replace('\\', "/")
}

fn git_status_rank(status: &str) -> i32 {
    match status {
        "D" => 60,
        "A" => 50,
        "M" => 40,
        "R" => 30,
        "C" => 20,
        "T" => 10,
        "?" => 5,
        _ => 0,
    }
}

fn count_nodes(node: &TreeNode) -> i32 {
    let mut n = 1i32;
    for child in &node.children {
        n += count_nodes(child);
    }
    n
}

/// Count newline-delimited lines in a file; mirrors Go `countTextStats`.
fn count_lines_file(path: &Path) -> i32 {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    if data.is_empty() {
        return 0;
    }
    let mut n = data.iter().filter(|&&b| b == b'\n').count() as i32;
    if data.last() != Some(&b'\n') {
        n += 1;
    }
    n
}

/// `inferRole` — mirrors Go walk/walk.go `inferRole`.
fn infer_role(rel_slash: &str) -> String {
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

fn is_core_extension(ext: &str) -> bool {
    matches!(ext, "ts" | "tsx" | "js" | "go" | "py" | "rs")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_status_path_normalizes_rename_target() {
        assert_eq!(
            normalize_git_status_path("old/name.rs -> src/name.rs"),
            "src/name.rs"
        );
        assert_eq!(
            normalize_git_status_path("\"web/src/App.svelte\""),
            "web/src/App.svelte"
        );
    }

    #[test]
    fn git_status_code_normalizes_porcelain_status() {
        assert_eq!(normalize_git_status("??"), "?");
        assert_eq!(normalize_git_status(" M"), "M");
        assert_eq!(normalize_git_status("M "), "M");
        assert_eq!(normalize_git_status("A "), "A");
        assert_eq!(normalize_git_status(" D"), "D");
        assert_eq!(normalize_git_status("R "), "R");
        assert_eq!(normalize_git_status("  "), "");
    }

    #[test]
    fn git_status_map_direct_and_directory_aggregate() {
        let status = GitStatusMap {
            by_path: BTreeMap::from([
                ("crates/ctx-web/src/lib.rs".to_string(), "M".to_string()),
                ("web/src/App.svelte".to_string(), "A".to_string()),
                ("notes.txt".to_string(), "?".to_string()),
            ]),
        };

        assert_eq!(status.status_for("crates/ctx-web/src/lib.rs", false), "M");
        assert_eq!(status.status_for("crates/ctx-web", true), "M");
        assert_eq!(status.status_for("web", true), "A");
        assert_eq!(status.status_for(".", true), "A");
        assert_eq!(status.status_for("README.md", false), "");
    }
}
