//! `GET /api/where` — port of `internal/web/handlers.go` `handleWhere`.
//!
//! Query params:
//!   q      (required) — search query
//!   limit  (optional) — max results, default 10
//!   path   (optional) — sub-root relative to server root (default: root)
//!   all    (optional) — require every query term to match, default false
//!   literal (optional) — exact case-sensitive pattern, not normalized
//!
//! Reuses `ctx-where::search_with_options` for scoring. The walker is
//! reimplemented here to match Go's default walk semantics (gitignore-aware
//! skip of hidden dirs, node_modules, dist). For the non-code fixture files
//! used in parity tests, no symbols are extracted, so scores are based on
//! path/basename/content — identical to Go.

use std::path::{Path, PathBuf};

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use ctx_where::{FileInput, Options, SearchResult, SymbolInput};

use crate::response;
use crate::safepath;
use crate::AppState;

#[derive(Deserialize)]
pub struct WhereParams {
    #[serde(rename = "q", default)]
    query: String,
    #[serde(default)]
    limit: i64,
    #[serde(default)]
    path: String,
    #[serde(default)]
    all: bool,
    #[serde(default)]
    require_all: bool,
    #[serde(default)]
    literal: String,
}

/// WhereMatch mirrors `web.WhereMatch`. Field order matches Go struct.
#[derive(Serialize)]
struct WhereMatch {
    line: i64,
    column: i64,
    #[serde(rename = "type")]
    kind: String,
    text: String,
}

/// WhereResult mirrors `web.WhereResult`. Field order matches Go struct.
#[derive(Serialize)]
struct WhereResult {
    path: String,
    score: i64,
    reason: String,
    matches: Vec<WhereMatch>,
}

/// WhereResponse mirrors `web.WhereResponse`. Field order matches Go struct.
#[derive(Serialize)]
struct WhereResponse {
    query: String,
    results: Vec<WhereResult>,
}

pub async fn handle(
    State(state): State<AppState>,
    params: Result<Query<WhereParams>, QueryRejection>,
) -> Response {
    let Query(params) = match params {
        Ok(q) => q,
        Err(e) => return response::bad_query(e),
    };
    crate::blocking::run_parallel(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: WhereParams) -> Response {
    if params.query.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "q is required");
    }

    let limit = if params.limit <= 0 { 10 } else { params.limit };

    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let files = match collect_files(&target, &target) {
        Ok(f) => f,
        Err(e) => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "where", &e);
        }
    };

    let opts = Options {
        limit,
        require_all: params.all || params.require_all,
        literal: params.literal,
        ..Options::default()
    };
    let results = ctx_where::search_with_options(&files, &params.query, &opts);

    let out: Vec<WhereResult> = results
        .into_iter()
        .map(|r: SearchResult| {
            let matches = r
                .matches
                .into_iter()
                .map(|m| WhereMatch {
                    line: m.line,
                    column: m.column,
                    kind: m.kind,
                    text: m.text,
                })
                .collect();
            WhereResult {
                path: r.path,
                score: r.score,
                reason: r.reason,
                matches,
            }
        })
        .collect();

    response::json(
        StatusCode::OK,
        &WhereResponse {
            query: params.query,
            results: out,
        },
    )
}

/// Walk `dir` recursively (relative to `root`) collecting FileInput entries
/// for all non-directory files. Mirrors the minimal walk semantics of the
/// Go `where.Search` call — skips hidden dirs, `node_modules`, `dist`,
/// `coverage`, and reads file content as lines.
///
/// Two phases: a sequential walk lists the regular files (readdir only — it
/// is also the only phase that can fail), then each file is read and parsed
/// in parallel. Every file maps to a self-contained `FileInput` and the
/// result is sorted by path, so the parallel phase cannot change it.
fn collect_files(root: &Path, dir: &Path) -> Result<Vec<FileInput>, String> {
    let mut candidates = Vec::new();
    collect_files_inner(root, dir, &mut candidates)?;
    let mut out: Vec<FileInput> = candidates
        .into_par_iter()
        .filter_map(|(path, rel)| build_file_input(&path, rel))
        .collect();
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn collect_files_inner(
    root: &Path,
    dir: &Path,
    out: &mut Vec<(PathBuf, String)>,
) -> Result<(), String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("walk {}: {e}", dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        // Skip hidden dirs, vendor-like dirs, build artifacts — mirrors
        // Go where.SearchWithOptions walk.DefaultOptions semantics.
        if name_str.starts_with('.')
            || name_str == "node_modules"
            || name_str == "dist"
            || name_str == "coverage"
        {
            continue;
        }

        // DirEntry::file_type does not follow symlinks, so symlinked dirs are
        // never recursed into (cyclic links would loop; absolute links would
        // leak outside root).
        if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
            collect_files_inner(root, &path, out)?;
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        // Only regular files are read — the same lstat rule as the tree/dir
        // walks (`tree::cached_file_stats`). A symlinked file could point out
        // of root (its matching lines would be returned in the response), and
        // a FIFO or device would block or never reach EOF.
        if !entry.file_type().map(|t| t.is_file()).unwrap_or(false) {
            continue;
        }

        out.push((path, rel));
    }
    Ok(())
}

/// Repository-relative paths of the regular files the search walk visits
/// under `root` (no file is read) — what `/api/refs` resolves mentions
/// against.
pub(crate) fn list_files(root: &Path) -> Result<Vec<String>, String> {
    let mut candidates = Vec::new();
    collect_files_inner(root, root, &mut candidates)?;
    let mut out: Vec<String> = candidates.into_iter().map(|(_, rel)| rel).collect();
    out.sort();
    Ok(out)
}

/// Read and parse one listed file, or `None` when it is not UTF-8 text.
fn build_file_input(path: &Path, rel: String) -> Option<FileInput> {
    // Skip binary-like files (non-UTF-8).
    let body = std::fs::read_to_string(path).ok()?;

    // Parse the body just read: `extract` would stat and read the file again.
    let symbols = ctx_symbols::extract_source_on_worker(path, body.as_bytes())
        .into_iter()
        .map(|sym| SymbolInput {
            name: sym.name,
            kind: sym.kind,
            line: i64::from(sym.line),
        })
        .collect();
    // Split after the parse, so its tree is freed before the lines exist.
    let lines = ctx_where::split_lines(&body);

    Some(FileInput {
        path: rel,
        is_dir: false,
        symbols,
        lines,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;

    fn temp_dir(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ctx-web-where-{tag}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn walk_does_not_read_symlinked_files_out_of_root() {
        let outside = temp_dir("outside");
        std::fs::write(outside.join("secret.txt"), "needle TOP SECRET\n").unwrap();
        let root = temp_dir("root");
        std::fs::write(root.join("a.txt"), "needle here\n").unwrap();
        std::os::unix::fs::symlink(outside.join("secret.txt"), root.join("link.txt")).unwrap();

        let files = collect_files(&root, &root).unwrap();
        let paths: Vec<&str> = files.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(paths, vec!["a.txt"]);
        assert!(files
            .iter()
            .all(|f| f.lines.iter().all(|l| !l.contains("TOP SECRET"))));

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[test]
    fn fifo_does_not_block_the_walk() {
        let root = temp_dir("fifo");
        std::fs::write(root.join("a.txt"), "x\n").unwrap();
        let fifo = root.join("pipe");
        let c = std::ffi::CString::new(fifo.to_string_lossy().as_bytes()).unwrap();
        assert_eq!(unsafe { libc::mkfifo(c.as_ptr(), 0o600) }, 0);

        let (tx, rx) = std::sync::mpsc::channel();
        let r = root.clone();
        std::thread::spawn(move || {
            let _ = tx.send(collect_files(&r, &r).map(|f| f.len()));
        });
        let got = rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("walk blocked on a FIFO");
        assert_eq!(got.unwrap(), 1);
        let _ = std::fs::remove_dir_all(root);
    }
}
