//! `GET /api/symbols` — port of `internal/web/handlers.go` `handleSymbols`.
//! `GET /api/definition` — port of `internal/web/handlers.go` `handleDefinition`.
//!
//! Both routes reuse `ctx_symbols::extract` (native tree-sitter, byte-parity
//! verified) and `ctx_symbols::lookup::{resolve, sort_hits}`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::handlers::file::relative_to_root;
use crate::response;
use crate::safepath;
use crate::AppState;

// ── Wire types (field order + names MUST match Go structs in api.go) ─────────

/// Symbol wire type — mirrors `web.Symbol`. Lowercase field names to match Go's
/// JSON output. (ctx_symbols::Symbol uses capitalized `rename` for FFI/Go
/// compatibility; we must NOT reuse it here.)
/// Public so `file.rs` can reference it directly.
#[derive(Serialize)]
pub struct SymbolWire {
    pub name: String,
    pub kind: String,
    pub line: i32,
}

/// SymbolsResponse mirrors `web.SymbolsResponse`.
/// `files` is a map[string][]Symbol in Go — serialized as a JSON object.
/// Go serializes map keys in sorted order; we use BTreeMap for the same.
/// The value for a non-code file is `null` (Go's `convertSymbols(nil)` → `nil`
/// → JSON `null`). We use `Option<Vec<SymbolWire>>` where `None` → `null`.
#[derive(Serialize)]
struct SymbolsResponse {
    path: String,
    files: BTreeMap<String, Option<Vec<SymbolWire>>>,
}

/// DefinitionCandidate mirrors `web.DefinitionCandidate`.
#[derive(Serialize)]
struct DefinitionCandidate {
    path: String,
    line: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    column: Option<i32>,
    kind: String,
    symbol_name: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    file_role: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    file_tokens: i64,
}

fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

/// DefinitionResponse mirrors `web.DefinitionResponse`.
/// `candidates` is always an array (never null) — Go uses `make([]..., 0, ...)`.
#[derive(Serialize)]
struct DefinitionResponse {
    name: String,
    candidates: Vec<DefinitionCandidate>,
}

// ── /api/symbols ──────────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct SymbolsParams {
    #[serde(default)]
    path: String,
}

pub async fn handle_symbols(
    State(state): State<AppState>,
    Query(params): Query<SymbolsParams>,
) -> Response {
    crate::blocking::run(move || handle_symbols_sync(state, params)).await
}

fn handle_symbols_sync(state: AppState, params: SymbolsParams) -> Response {
    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let meta = match std::fs::metadata(&target) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return response::stat_not_found(&target);
            }
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "stat", &e.to_string());
        }
    };

    let root_str = &state.root;
    let mut files: BTreeMap<String, Option<Vec<SymbolWire>>> = BTreeMap::new();

    if !meta.is_dir() {
        // Single-file path
        let syms = match ctx_symbols::extract(&target) {
            Ok(s) => s,
            Err(e) => {
                return response::error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "symbols",
                    &e.to_string(),
                );
            }
        };
        let key = dotted_relative_to_root(root_str, &target);
        files.insert(key.clone(), convert_symbols(syms));
        return response::json(StatusCode::OK, &SymbolsResponse { path: key, files });
    }

    // Directory path: walk + extract like Go's handleSymbols directory branch
    collect_symbols_from_dir(&target, root_str, &mut files);

    let path_key = dotted_relative_to_root(root_str, &target);
    let resp = response::json(
        StatusCode::OK,
        &SymbolsResponse {
            path: path_key,
            files,
        },
    );
    // The walk parsed every file on the rayon pool (see `run_parallel`).
    crate::blocking::release_freed_memory();
    resp
}

// ── /api/definition ───────────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DefinitionParams {
    #[serde(default)]
    name: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    kind: String,
}

pub async fn handle_definition(
    State(state): State<AppState>,
    Query(params): Query<DefinitionParams>,
) -> Response {
    crate::blocking::run(move || handle_definition_sync(state, params)).await
}

fn handle_definition_sync(state: AppState, params: DefinitionParams) -> Response {
    if params.name.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "name is required");
    }

    // `from` is a hint only: silently drop on resolution failure (mirrors Go).
    let from = if params.from.is_empty() {
        String::new()
    } else {
        match safepath::resolve(&state.root, &params.from) {
            Ok(_) => params.from.clone(),
            Err(_) => String::new(),
        }
    };

    let root_str = &state.root;
    let root_path = match std::fs::canonicalize(root_str)
        .or_else(|_| std::path::absolute(Path::new(root_str)))
    {
        Ok(p) => p,
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "definition",
                &e.to_string(),
            );
        }
    };

    // Walk the repo and build a corpus of FileSymbols (mirrors LookupByName),
    // reusing a cached corpus when nothing under root has changed since it
    // was last built (see `corpus_cache`).
    let fingerprint = corpus_fingerprint(&root_path);
    let cached = corpus_cache().read().ok().and_then(|guard| {
        guard
            .get(&root_path)
            .filter(|entry| entry.fingerprint == fingerprint)
            .map(|entry| (Arc::clone(&entry.corpus), Arc::clone(&entry.meta_index)))
    });
    let (corpus, meta_index) = match cached {
        Some(pair) => pair,
        None => {
            let mut corpus: Vec<ctx_symbols::FileSymbols> = Vec::new();
            let mut meta_index: HashMap<String, FileMeta> = HashMap::new();
            collect_definition_corpus(&root_path, root_str, &mut corpus, &mut meta_index);
            // The build parsed every file on the rayon pool (see
            // `run_parallel`); only the cached corpus outlives it.
            crate::blocking::release_freed_memory();
            let corpus = Arc::new(corpus);
            let meta_index = Arc::new(meta_index);
            if let Ok(mut guard) = corpus_cache().write() {
                guard.insert(
                    root_path.clone(),
                    CorpusCacheEntry {
                        fingerprint,
                        corpus: Arc::clone(&corpus),
                        meta_index: Arc::clone(&meta_index),
                    },
                );
            }
            (corpus, meta_index)
        }
    };

    let args = ctx_symbols::LookupArgs {
        name: params.name.clone(),
        from: from.clone(),
        kind: params.kind.clone(),
    };
    let hits = ctx_symbols::resolve(corpus.as_slice(), &args);

    let candidates: Vec<DefinitionCandidate> = hits
        .into_iter()
        .map(|h| {
            let (file_role, file_tokens) = meta_index
                .get(&h.path)
                .map(|m| (m.role.clone(), m.tokens))
                .unwrap_or_default();
            DefinitionCandidate {
                path: h.path,
                line: h.line,
                column: None,
                kind: h.kind,
                symbol_name: h.symbol_name,
                file_role,
                file_tokens,
            }
        })
        .collect();

    response::json(
        StatusCode::OK,
        &DefinitionResponse {
            name: params.name,
            candidates,
        },
    )
}

// ── Shared helpers ────────────────────────────────────────────────────────────

/// Convert `ctx_symbols::Symbol` (capitalized serde names) to the wire type
/// (lowercase names). Returns `None` (→ JSON `null`) for empty input,
/// mirroring Go's `convertSymbols(nil) → nil → JSON null`.
pub fn convert_symbols(syms: Vec<ctx_symbols::Symbol>) -> Option<Vec<SymbolWire>> {
    if syms.is_empty() {
        return None;
    }
    Some(
        syms.into_iter()
            .map(|s| SymbolWire {
                name: s.name,
                kind: s.kind,
                line: s.line,
            })
            .collect(),
    )
}

/// `relative_to_root` with the served root ("") mapped to "." — the same
/// fallback `dir.rs`/`tree.rs` apply inline at their single call site; named
/// here because `/api/symbols` and `/api/definition` need it at four.
fn dotted_relative_to_root(root: &str, target: &Path) -> String {
    let s = relative_to_root(root, target);
    if s.is_empty() {
        ".".to_string()
    } else {
        s
    }
}

/// Walk `dir` recursively and fill `files` map. Mirrors Go's handleSymbols
/// directory branch: skip files with no symbols (errE != nil || len == 0).
fn collect_symbols_from_dir(
    dir: &Path,
    root: &str,
    files: &mut BTreeMap<String, Option<Vec<SymbolWire>>>,
) {
    // List the files sequentially, then parse them in parallel: each parse
    // reads only its own file, and the map is keyed by path, so the result
    // does not depend on the order the parses finish in.
    let mut paths = Vec::new();
    list_symbol_files(dir, &mut paths);
    let parsed: Vec<(PathBuf, Vec<ctx_symbols::Symbol>)> = paths
        .into_par_iter()
        .filter_map(|path| {
            // Go skips on errE != nil || len(syms) == 0.
            match ctx_symbols::extract(&path) {
                Ok(syms) if !syms.is_empty() => Some((path, syms)),
                _ => None,
            }
        })
        .collect();
    for (path, syms) in parsed {
        let key = dotted_relative_to_root(root, &path);
        files.insert(key, convert_symbols(syms));
    }
}

/// The files [`collect_symbols_from_dir`] parses, in walk order.
fn list_symbol_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // DirEntry::file_type does not follow symlinks, so symlinked dirs are
    // never recursed into (cyclic links would loop; absolute links would
    // leak outside root).
    let mut children: Vec<(PathBuf, bool)> = entries
        .flatten()
        .filter_map(|e| {
            let is_dir = e.file_type().ok()?.is_dir();
            Some((e.path(), is_dir))
        })
        .collect();
    // Sort for deterministic order matching Go's walk.Flatten DFS traversal.
    children.sort();

    for (path, is_dir) in children {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip hidden dirs/files and vendor-like dirs.
        if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "coverage" {
            continue;
        }

        if is_dir {
            list_symbol_files(&path, out);
            continue;
        }
        out.push(path);
    }
}

/// Per-file metadata for /api/definition enrichment — mirrors `fileMeta`.
#[derive(Default)]
struct FileMeta {
    role: String,
    tokens: i64,
}

/// One cached `/api/definition` corpus build, valid only while `fingerprint`
/// still matches a fresh [`corpus_fingerprint`] call.
struct CorpusCacheEntry {
    fingerprint: SystemTime,
    corpus: Arc<Vec<ctx_symbols::FileSymbols>>,
    meta_index: Arc<HashMap<String, FileMeta>>,
}

/// Process-lifetime cache for `/api/definition`'s repo-wide symbol corpus,
/// keyed by canonical root. Without it, `handle_definition_sync` re-walks and
/// tree-sitter-parses every file in the repo on every request; a fingerprint
/// hit skips straight to `ctx_symbols::resolve`. A global cache keyed by an
/// absolute path (rather than threading a field through `AppState`, as
/// `FileCache`/`DiffCache` do) follows `tree.rs`'s `cached_file_stats`
/// precedent in this same crate — safe here because entries are validated by
/// fingerprint before use, so a stale or unrelated root simply misses.
fn corpus_cache() -> &'static RwLock<HashMap<PathBuf, CorpusCacheEntry>> {
    static CACHE: OnceLock<RwLock<HashMap<PathBuf, CorpusCacheEntry>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Cheap fingerprint for the corpus cache: the max mtime seen across a
/// stat-only walk of the same file set `collect_definition_corpus` would
/// visit (same skip filter, no file reads or tree-sitter parsing). Any file
/// add/edit/delete under root bumps some directory's or file's mtime, so
/// comparing this single timestamp across requests is enough to detect
/// "nothing changed" without repeating the expensive walk.
fn corpus_fingerprint(root_path: &Path) -> SystemTime {
    let mut max = std::fs::metadata(root_path)
        .and_then(|m| m.modified())
        .unwrap_or(UNIX_EPOCH);
    fingerprint_walk(root_path, &mut max);
    max
}

fn fingerprint_walk(dir: &Path, max: &mut SystemTime) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.')
            || name_str == "node_modules"
            || name_str == "dist"
            || name_str == "coverage"
        {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if let Ok(mtime) = meta.modified() {
            if mtime > *max {
                *max = mtime;
            }
        }
        if meta.is_dir() {
            fingerprint_walk(&entry.path(), max);
        }
    }
}

/// Walk `root_path` and build both the symbol corpus and the file-meta index.
/// Mirrors `LookupByName` (walk + extract) + `buildFileMetaIndex` (walk + role/tokens).
fn collect_definition_corpus(
    root_path: &Path,
    root_str: &str,
    corpus: &mut Vec<ctx_symbols::FileSymbols>,
    meta_index: &mut HashMap<String, FileMeta>,
) {
    // List the files sequentially (walk order fixes the corpus order), then
    // stat and parse them in parallel; the indexed collect keeps that order.
    let mut paths = Vec::new();
    list_definition_files(root_path, &mut paths);
    let built: Vec<(String, FileMeta, Vec<ctx_symbols::Symbol>)> = paths
        .into_par_iter()
        .map(|path| {
            let rel_key = dotted_relative_to_root(root_str, &path);
            // Meta index entry: role + token estimate from file size.
            let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let meta = FileMeta {
                role: crate::handlers::role::infer_role(&rel_key),
                tokens: ctx_tokens::estimate_by_size(file_size as i64),
            };
            let syms = ctx_symbols::extract(&path).unwrap_or_default();
            (rel_key, meta, syms)
        })
        .collect();
    for (rel_key, meta, syms) in built {
        meta_index.insert(rel_key.clone(), meta);
        // Symbol corpus: files without symbols (or that failed) are skipped.
        if !syms.is_empty() {
            corpus.push(ctx_symbols::FileSymbols {
                path: rel_key,
                symbols: syms,
            });
        }
    }
}

/// The files [`collect_definition_corpus`] indexes, in walk order.
fn list_definition_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    // file_type does not follow symlinks — see collect_symbols_from_dir.
    let mut children: Vec<(PathBuf, bool)> = entries
        .flatten()
        .filter_map(|e| {
            let is_dir = e.file_type().ok()?.is_dir();
            Some((e.path(), is_dir))
        })
        .collect();
    children.sort();

    for (path, is_dir) in children {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "coverage" {
            continue;
        }

        if is_dir {
            list_definition_files(&path, out);
            continue;
        }
        out.push(path);
    }
}
