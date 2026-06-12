//! `GET /api/where` — port of `internal/web/handlers.go` `handleWhere`.
//!
//! Query params:
//!   q      (required) — search query
//!   limit  (optional) — max results, default 10
//!   path   (optional) — sub-root relative to server root (default: root)
//!
//! Reuses `ctx-where::search_with_options` for scoring. The walker is
//! reimplemented here to match Go's default walk semantics (gitignore-aware
//! skip of hidden dirs, node_modules, dist). For the non-code fixture files
//! used in parity tests, no symbols are extracted, so scores are based on
//! path/basename/content — identical to Go.

use std::ffi::OsStr;
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

    let files = match collect_files(&target, &target) {
        Ok(f) => f,
        Err(e) => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "where", &e);
        }
    };

    let opts = Options {
        limit,
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
fn collect_files(root: &Path, dir: &Path) -> Result<Vec<FileInput>, String> {
    let mut out = Vec::new();
    collect_files_inner(root, dir, &mut out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(out)
}

fn collect_files_inner(
    root: &Path,
    dir: &Path,
    out: &mut Vec<FileInput>,
) -> Result<(), String> {
    let entries =
        std::fs::read_dir(dir).map_err(|e| format!("walk {}: {e}", dir.display()))?;
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

        // Skip binary-like files (non-UTF-8).
        let Ok(body) = std::fs::read_to_string(&path) else {
            continue;
        };

        let lines: Vec<String> = body.lines().map(|l| l.to_string()).collect();
        let symbols = extract_symbols(&rel, &lines);

        out.push(FileInput {
            path: rel,
            is_dir: false,
            symbols,
            lines,
        });
    }
    Ok(())
}

/// Lightweight regex-free symbol extraction matching ctx-cli's heuristics.
/// For non-code files (the fixture) this produces no symbols — identical to Go.
fn extract_symbols(path: &str, lines: &[String]) -> Vec<SymbolInput> {
    let mut out = Vec::new();
    let ext = Path::new(path)
        .extension()
        .and_then(OsStr::to_str)
        .unwrap_or("");
    for (idx, line) in lines.iter().enumerate() {
        let line_no = (idx + 1) as i64;
        match ext {
            "go" => extract_go_symbol(line, line_no, &mut out),
            "rs" => extract_rust_symbol(line, line_no, &mut out),
            "ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs" => {
                extract_js_symbol(line, line_no, &mut out)
            }
            "py" => extract_python_symbol(line, line_no, &mut out),
            _ => {}
        }
    }
    out
}

fn sym(name: &str, kind: &str, line: i64) -> SymbolInput {
    SymbolInput { name: name.to_string(), kind: kind.to_string(), line }
}

fn first_ident(s: &str) -> Option<&str> {
    let s = s.trim_start();
    let end = s
        .find(|c: char| !c.is_alphanumeric() && c != '_')
        .unwrap_or(s.len());
    if end == 0 { None } else { Some(&s[..end]) }
}

fn extract_go_symbol(line: &str, line_no: i64, out: &mut Vec<SymbolInput>) {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("func ") {
        let name = rest
            .strip_prefix('(')
            .and_then(|tail| tail.split_once(')'))
            .and_then(|(_, after)| first_ident(after.trim_start()))
            .or_else(|| first_ident(rest));
        if let Some(n) = name {
            out.push(sym(n, "function", line_no));
        }
    } else if let Some(rest) = trimmed.strip_prefix("type ") {
        if let Some(n) = first_ident(rest) {
            out.push(sym(n, "type", line_no));
        }
    }
}

fn extract_rust_symbol(line: &str, line_no: i64, out: &mut Vec<SymbolInput>) {
    let trimmed = line.trim_start();
    let rest = trimmed.strip_prefix("pub ").unwrap_or(trimmed);
    for (prefix, kind) in [("fn ", "function"), ("struct ", "struct"), ("enum ", "enum"), ("trait ", "trait")] {
        if let Some(tail) = rest.strip_prefix(prefix) {
            if let Some(n) = first_ident(tail) {
                out.push(sym(n, kind, line_no));
            }
            break;
        }
    }
}

fn extract_js_symbol(line: &str, line_no: i64, out: &mut Vec<SymbolInput>) {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed
        .strip_prefix("export function ")
        .or_else(|| trimmed.strip_prefix("function "))
    {
        if let Some(n) = first_ident(rest) {
            out.push(sym(n, "function", line_no));
        }
    } else if let Some(rest) = trimmed
        .strip_prefix("export class ")
        .or_else(|| trimmed.strip_prefix("class "))
    {
        if let Some(n) = first_ident(rest) {
            out.push(sym(n, "class", line_no));
        }
    }
}

fn extract_python_symbol(line: &str, line_no: i64, out: &mut Vec<SymbolInput>) {
    let trimmed = line.trim_start();
    if let Some(rest) = trimmed.strip_prefix("def ") {
        if let Some(n) = first_ident(rest) {
            out.push(sym(n, "function", line_no));
        }
    } else if let Some(rest) = trimmed.strip_prefix("class ") {
        if let Some(n) = first_ident(rest) {
            out.push(sym(n, "class", line_no));
        }
    }
}
