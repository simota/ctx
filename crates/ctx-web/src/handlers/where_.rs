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

use std::path::Path;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
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
    crate::blocking::run(move || handle_sync(state, params)).await
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

    let boundary = crate::handlers::file::canonical_root(&state.root);
    let files = match collect_files(&target, &boundary) {
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
/// Only regular files, and symlinks that resolve to a regular file inside
/// `boundary` (the canonical served root), are read — the CLI `ctx where`
/// walker's rule — so a link to a file outside the repository is not
/// returned in `matches[].text`, and a FIFO or device cannot block the
/// request.
fn collect_files(root: &Path, boundary: &Path) -> Result<Vec<FileInput>, String> {
    let mut out = Vec::new();
    collect_files_inner(root, root, boundary, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn collect_files_inner(
    root: &Path,
    dir: &Path,
    boundary: &Path,
    out: &mut Vec<FileInput>,
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
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            collect_files_inner(root, &path, boundary, out)?;
            continue;
        }
        let readable =
            file_type.is_file() || (file_type.is_symlink() && resolves_to_file_in(boundary, &path));
        if !readable {
            continue;
        }

        let rel = path
            .strip_prefix(root)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .replace('\\', "/");

        // Skip binary-like files (non-UTF-8).
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };

        let lines: Vec<String> = body.lines().map(|l| l.to_string()).collect();
        let symbols = ctx_symbols::extract(&path)
            .unwrap_or_default()
            .into_iter()
            .map(|sym| SymbolInput {
                name: sym.name,
                kind: sym.kind,
                line: i64::from(sym.line),
            })
            .collect();

        out.push(FileInput {
            path: rel,
            is_dir: false,
            symbols,
            lines,
        });
    }
    Ok(())
}

/// Whether `path`, after resolving every symlink, is a regular file inside
/// `canonical_boundary`.
fn resolves_to_file_in(canonical_boundary: &Path, path: &Path) -> bool {
    std::fs::canonicalize(path).is_ok_and(|target| {
        target.starts_with(canonical_boundary)
            && std::fs::metadata(&target).is_ok_and(|meta| meta.is_file())
    })
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;

    struct TempDir(PathBuf);
    impl TempDir {
        fn new(name: &str) -> Self {
            let dir =
                std::env::temp_dir().join(format!("ctx-web-where-{name}-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(&dir).unwrap();
            TempDir(dir.canonicalize().unwrap())
        }
    }
    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    fn collect_paths(root: PathBuf) -> Vec<(String, Vec<String>)> {
        // A worker thread turns a read that blocks on a FIFO into a test
        // failure instead of a hung suite.
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(collect_files(&root, &root).unwrap());
        });
        rx.recv_timeout(std::time::Duration::from_secs(10))
            .expect("collect_files blocked (read through a FIFO?)")
            .into_iter()
            .map(|f| (f.path, f.lines))
            .collect()
    }

    #[test]
    fn collect_files_skips_fifos_and_links_out_of_root() {
        let outside = TempDir::new("outside");
        std::fs::write(outside.0.join("secret.txt"), "TOPSECRET zebra\n").unwrap();
        let fx = TempDir::new("root");
        std::fs::write(fx.0.join("a.txt"), "zebra here\n").unwrap();
        std::os::unix::fs::symlink(outside.0.join("secret.txt"), fx.0.join("leak.txt")).unwrap();
        std::os::unix::fs::symlink("a.txt", fx.0.join("inside.txt")).unwrap();
        std::os::unix::fs::symlink(&outside.0, fx.0.join("outdir")).unwrap();
        let status = std::process::Command::new("mkfifo")
            .arg(fx.0.join("pipe"))
            .status()
            .unwrap();
        assert!(status.success());

        let got = collect_paths(fx.0.clone());
        assert_eq!(
            got,
            vec![
                ("a.txt".to_string(), vec!["zebra here".to_string()]),
                ("inside.txt".to_string(), vec!["zebra here".to_string()]),
            ]
        );
    }
}
