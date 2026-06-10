//! `GET /api/symbols` — port of `internal/web/handlers.go` `handleSymbols`.
//! `GET /api/definition` — port of `internal/web/handlers.go` `handleDefinition`.
//!
//! Both routes reuse `ctx_symbols::extract` (native tree-sitter, byte-parity
//! verified) and `ctx_symbols::lookup::{resolve, sort_hits}`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

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
    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let meta = match std::fs::metadata(&target) {
        Ok(m) => m,
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
                "stat",
                &e.to_string(),
            );
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
        let key = relative_to_root(root_str, &target);
        files.insert(key.clone(), convert_symbols(syms));
        return response::json(
            StatusCode::OK,
            &SymbolsResponse { path: key, files },
        );
    }

    // Directory path: walk + extract like Go's handleSymbols directory branch
    collect_symbols_from_dir(&target, root_str, &mut files);

    let path_key = relative_to_root(root_str, &target);
    response::json(StatusCode::OK, &SymbolsResponse { path: path_key, files })
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

    // Walk the repo and build a corpus of FileSymbols (mirrors LookupByName).
    let mut corpus: Vec<ctx_symbols::FileSymbols> = Vec::new();
    let mut meta_index: HashMap<String, FileMeta> = HashMap::new();

    collect_definition_corpus(&root_path, root_str, &mut corpus, &mut meta_index);

    let args = ctx_symbols::LookupArgs {
        name: params.name.clone(),
        from: from.clone(),
        kind: params.kind.clone(),
    };
    let hits = ctx_symbols::resolve(&corpus, &args);

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

/// Relative slash-separated path from root to target; mirrors Go's
/// `relativeToRoot`.
fn relative_to_root(root: &str, target: &Path) -> String {
    let abs_root = std::fs::canonicalize(root)
        .or_else(|_| std::path::absolute(Path::new(root)))
        .unwrap_or_else(|_| PathBuf::from(root));
    match target.strip_prefix(&abs_root) {
        Ok(rel) => {
            let s = rel.to_string_lossy().replace('\\', "/");
            if s.is_empty() { ".".to_string() } else { s }
        }
        Err(_) => {
            // target IS the abs_root (strip_prefix fails when paths are equal
            // on some platforms). Fallback: compare directly.
            if *target == abs_root {
                ".".to_string()
            } else {
                target.to_string_lossy().replace('\\', "/")
            }
        }
    }
}

/// Walk `dir` recursively and fill `files` map. Mirrors Go's handleSymbols
/// directory branch: skip files with no symbols (errE != nil || len == 0).
fn collect_symbols_from_dir(
    dir: &Path,
    root: &str,
    files: &mut BTreeMap<String, Option<Vec<SymbolWire>>>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries
        .flatten()
        .map(|e| e.path())
        .collect();
    // Sort for deterministic order matching Go's walk.Flatten DFS traversal.
    children.sort();

    for path in children {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        // Skip hidden dirs/files and vendor-like dirs.
        if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "coverage" {
            continue;
        }

        if path.is_dir() {
            collect_symbols_from_dir(&path, root, files);
            continue;
        }

        // Go skips on errE != nil || len(syms) == 0.
        let syms = match ctx_symbols::extract(&path) {
            Ok(s) if !s.is_empty() => s,
            _ => continue,
        };
        let key = relative_to_root(root, &path);
        files.insert(key, convert_symbols(syms));
    }
}

/// Per-file metadata for /api/definition enrichment — mirrors `fileMeta`.
#[derive(Default)]
struct FileMeta {
    role: String,
    tokens: i64,
}

/// Walk `root_path` and build both the symbol corpus and the file-meta index.
/// Mirrors `LookupByName` (walk + extract) + `buildFileMetaIndex` (walk + role/tokens).
fn collect_definition_corpus(
    root_path: &Path,
    root_str: &str,
    corpus: &mut Vec<ctx_symbols::FileSymbols>,
    meta_index: &mut HashMap<String, FileMeta>,
) {
    collect_definition_inner(root_path, root_str, root_path, corpus, meta_index);
}

fn collect_definition_inner(
    root_path: &Path,
    root_str: &str,
    dir: &Path,
    corpus: &mut Vec<ctx_symbols::FileSymbols>,
    meta_index: &mut HashMap<String, FileMeta>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut children: Vec<PathBuf> = entries.flatten().map(|e| e.path()).collect();
    children.sort();

    for path in children {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if name.starts_with('.') || name == "node_modules" || name == "dist" || name == "coverage" {
            continue;
        }

        if path.is_dir() {
            collect_definition_inner(root_path, root_str, &path, corpus, meta_index);
            continue;
        }

        let rel_key = relative_to_root(root_str, &path);

        // Build meta index (role + token estimate from file size).
        let file_size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
        let role = infer_role(&rel_key);
        meta_index.insert(
            rel_key.clone(),
            FileMeta {
                role,
                tokens: ctx_tokens::estimate_by_size(file_size as i64),
            },
        );

        // Build symbol corpus (skip if no symbols).
        let syms = match ctx_symbols::extract(&path) {
            Ok(s) if !s.is_empty() => s,
            _ => continue,
        };
        corpus.push(ctx_symbols::FileSymbols {
            path: rel_key,
            symbols: syms,
        });
    }
}

/// Infer file role from path, mirroring Go's `inferRole` in internal/walk/walk.go.
fn infer_role(path: &str) -> String {
    let lower_path = path.to_lowercase();
    let base = path.rsplit('/').next().unwrap_or(path);
    let lower_base = base.to_lowercase();
    let ext = Path::new(base)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| format!(".{}", e.to_lowercase()))
        .unwrap_or_default();

    // Test
    if lower_path.starts_with("tests/")
        || lower_path.contains("/tests/")
        || lower_base.ends_with("_test.go")
        || is_dotted_test_name(&lower_base)
    {
        return "test".to_string();
    }
    // Doc
    if ext == ".md" || lower_base.starts_with("license") || lower_base.starts_with("readme") {
        return "doc".to_string();
    }
    // Config
    if is_config_file(&lower_base, &ext) {
        return "config".to_string();
    }
    // Entry
    if base == "main.ts"
        || base == "main.go"
        || base == "main.py"
        || base == "index.ts"
        || base == "index.tsx"
        || base == "index.js"
        || (path.starts_with("cmd/") && path.ends_with("/main.go"))
    {
        return "entry".to_string();
    }
    // Route
    if base.contains("router") || base.contains("route") || base.contains("Router") {
        return "route".to_string();
    }
    // Core
    if matches!(ext.as_str(), ".ts" | ".tsx" | ".js" | ".go" | ".py" | ".rs") {
        return "core".to_string();
    }
    // Unknown / no role → empty string (omitempty in Go)
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
        "package.json"
            | "go.mod"
            | "cargo.toml"
            | "pyproject.toml"
            | "dockerfile"
            | "makefile"
    ) || matches!(ext, ".toml" | ".yaml" | ".yml")
}
