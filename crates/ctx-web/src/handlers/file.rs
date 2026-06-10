//! `GET /api/file` — port of `internal/web/handlers.go` `handleFile`.
//!
//! Returns file metadata + content. Token counting reuses the parity-fixed
//! `ctx-tokens` crate. Symbol extraction reuses `ctx_symbols::extract`
//! (native tree-sitter, byte-parity verified). For files with no symbols
//! the `symbols` field is omitted by both Go (`omitempty`) and this port.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::handlers::symbols::convert_symbols;
use crate::response;
use crate::safepath;
use crate::AppState;

const MAX_FILE_BYTES: usize = 1 << 20; // 1 MiB
const TRUNCATE_TARGET: usize = 256 << 10;
const MAX_EXACT_TOKEN_BYTES: usize = 64 << 10;

/// Soft cap on cached `/api/file` bodies. Once reached, new files are served
/// without being cached (existing entries stay valid). Bounds memory during
/// long browse sessions over large trees.
const FILE_CACHE_CAP: usize = 1024;

/// A memoized `/api/file` response body plus the file fingerprint that
/// validates it. `body` is the exact serialized bytes (including the trailing
/// newline), so serving it is byte-identical to a fresh computation.
pub struct FileCacheEntry {
    mtime: SystemTime,
    size: u64,
    body: Arc<Vec<u8>>,
}

/// Process-lifetime cache for `/api/file` bodies, keyed by resolved target
/// path. Shared across requests via [`AppState`].
pub type FileCache = Arc<RwLock<HashMap<PathBuf, FileCacheEntry>>>;

#[derive(Deserialize)]
pub struct FileParams {
    #[serde(default)]
    path: String,
}

/// FileResponse mirrors `web.FileResponse`. Field order MUST match the Go
/// struct for byte-identical JSON; `skip_serializing_if` mirrors `omitempty`.
///
/// The `symbols` field is populated via `ctx_symbols::extract` (native
/// tree-sitter). It is omitted when empty (`omitempty` in Go).
/// The `git` field is DEFERRED — the git crate is not yet ported; the field
/// is `omitempty` in Go so omitting it is byte-identical for worktree files.
#[derive(Serialize)]
struct FileResponse {
    path: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    lang: String,
    size: i64,
    #[serde(skip_serializing_if = "is_zero_i32")]
    lines: i32,
    #[serde(skip_serializing_if = "is_zero_i64")]
    tokens: i64,
    content: String,
    #[serde(skip_serializing_if = "is_false")]
    truncated: bool,
    /// Populated by ctx_symbols::extract; None/empty → omitted (omitempty).
    #[serde(skip_serializing_if = "Option::is_none")]
    symbols: Option<Vec<crate::handlers::symbols::SymbolWire>>,
    #[serde(skip_serializing_if = "str::is_empty")]
    git: String,
}

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}
fn is_false(v: &bool) -> bool {
    !*v
}

pub async fn handle(State(state): State<AppState>, Query(params): Query<FileParams>) -> Response {
    if params.path.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }
    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let meta = match std::fs::symlink_metadata(&target).or_else(|_| std::fs::metadata(&target)) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                // Match Go's `os.Stat` error string exactly.
                return response::error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!("stat {}: no such file or directory", target.display()),
                );
            }
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "stat", &e.to_string());
        }
    };
    // Re-stat through symlinks to mirror os.Stat (follows links).
    let meta = std::fs::metadata(&target).unwrap_or(meta);
    if meta.is_dir() {
        return response::error(
            StatusCode::BAD_REQUEST,
            "not_a_file",
            "path is a directory",
        );
    }

    // Cache fingerprint: mtime + size. When both match a prior response for this
    // path, replay the stored bytes and skip the read + tree-sitter parse +
    // token count entirely. If mtime is unavailable on this platform, fall
    // through to the uncached path (correctness over speed).
    let fingerprint = meta.modified().ok().map(|mtime| (mtime, meta.len()));
    if let Some((mtime, size)) = fingerprint {
        if let Some(body) = cache_get(&state.file_cache, &target, mtime, size) {
            return response::json_bytes(StatusCode::OK, body.as_ref().clone());
        }
    }

    let raw = match std::fs::read(&target) {
        Ok(d) => d,
        Err(e) => {
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "read_file", &e.to_string())
        }
    };

    let (data, truncated) = truncate_file_data(&raw);
    // Go does `string(data)`: invalid UTF-8 bytes become replacement runes.
    let content = String::from_utf8_lossy(data).into_owned();
    let lines = count_lines(&content);
    let tokens = count_file_tokens(&content);

    // Extract symbols for code files. Errors and empty results are silently
    // ignored — matches Go's `syms, _ := symbols.New().Extract(target)`.
    let symbols = ctx_symbols::extract(&target)
        .ok()
        .and_then(|s| convert_symbols(s));

    let resp = FileResponse {
        path: relative_to_root(&state.root, &target),
        lang: lang_for_ext(&target).to_string(),
        size: meta.len() as i64,
        lines,
        tokens,
        content,
        truncated,
        symbols,
        git: String::new(),
    };

    let body = Arc::new(response::to_json_bytes(&resp));
    if let Some((mtime, size)) = fingerprint {
        cache_put(&state.file_cache, &target, mtime, size, Arc::clone(&body));
    }
    response::json_bytes(StatusCode::OK, body.as_ref().clone())
}

/// Return the cached body for `target` if the stored fingerprint still matches.
fn cache_get(
    cache: &FileCache,
    target: &Path,
    mtime: SystemTime,
    size: u64,
) -> Option<Arc<Vec<u8>>> {
    let guard = cache.read().ok()?;
    let entry = guard.get(target)?;
    (entry.mtime == mtime && entry.size == size).then(|| Arc::clone(&entry.body))
}

/// Store `body` for `target`. Refreshes an existing entry (so an edited file
/// re-caches), but does not grow the map past [`FILE_CACHE_CAP`].
fn cache_put(cache: &FileCache, target: &Path, mtime: SystemTime, size: u64, body: Arc<Vec<u8>>) {
    let Ok(mut guard) = cache.write() else {
        return;
    };
    if guard.len() >= FILE_CACHE_CAP && !guard.contains_key(target) {
        return;
    }
    guard.insert(target.to_path_buf(), FileCacheEntry { mtime, size, body });
}

fn truncate_file_data(data: &[u8]) -> (&[u8], bool) {
    if data.len() <= MAX_FILE_BYTES {
        return (data, false);
    }
    (truncate_utf8(data, TRUNCATE_TARGET), true)
}

/// `truncateUTF8`: cut at `max`, backing up to a UTF-8 rune boundary.
fn truncate_utf8(data: &[u8], max: usize) -> &[u8] {
    if data.len() <= max {
        return data;
    }
    let mut cut = max;
    // Back up while the byte is a UTF-8 continuation byte (0b10xxxxxx).
    while cut > 0 && (data[cut] & 0xC0) == 0x80 {
        cut -= 1;
    }
    &data[..cut]
}

/// `countLines`: number of newline-delimited lines; a trailing partial line
/// (no final `\n`) counts as one.
fn count_lines(s: &str) -> i32 {
    if s.is_empty() {
        return 0;
    }
    let mut n = s.matches('\n').count() as i32;
    if !s.ends_with('\n') {
        n += 1;
    }
    n
}

/// `countFileResponseTokens`: exact tiktoken count below the size cap, else a
/// size-based estimate. Reuses the parity-fixed ctx-tokens crate.
fn count_file_tokens(content: &str) -> i64 {
    if content.len() > MAX_EXACT_TOKEN_BYTES {
        return ctx_tokens::estimate_by_size(content.len() as i64);
    }
    ctx_tokens::count_str(content)
}

/// `langForExt` — extension → language label.
fn lang_for_ext(path: &Path) -> &'static str {
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(|s| s.to_ascii_lowercase())
        .unwrap_or_default();
    match ext.as_str() {
        "go" => "go",
        "ts" => "typescript",
        "tsx" => "tsx",
        "js" => "javascript",
        "jsx" => "jsx",
        "mjs" => "javascript",
        "py" => "python",
        "rs" => "rust",
        "java" => "java",
        "rb" => "ruby",
        "sh" | "bash" => "bash",
        "json" => "json",
        "yml" | "yaml" => "yaml",
        "toml" => "toml",
        "md" => "markdown",
        "html" | "htm" => "html",
        "css" => "css",
        "svelte" => "svelte",
        "vue" => "vue",
        _ => "",
    }
}

/// `relativeToRoot(root, target)` returning slash-separated rel path.
pub fn relative_to_root(root: &str, target: &Path) -> String {
    let abs_root = canonical_root(root);
    match target.strip_prefix(&abs_root) {
        Ok(rel) => rel.to_string_lossy().replace('\\', "/"),
        Err(_) => target.to_string_lossy().replace('\\', "/"),
    }
}

/// Memoize the canonicalized root. `relative_to_root` is called on every
/// `/api/*` request that resolves a path; `canonicalize` is a syscall and the
/// root is stable for the server's lifetime, so cache it per root string. The
/// result is identical to calling `canonicalize` directly, preserving parity.
fn canonical_root(root: &str) -> PathBuf {
    use std::sync::OnceLock;
    static CACHE: OnceLock<RwLock<HashMap<String, PathBuf>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));
    if let Ok(guard) = cache.read() {
        if let Some(abs) = guard.get(root) {
            return abs.clone();
        }
    }
    let abs = std::fs::canonicalize(root)
        .or_else(|_| std::path::absolute(root))
        .unwrap_or_else(|_| PathBuf::from(root));
    if let Ok(mut guard) = cache.write() {
        guard.insert(root.to_string(), abs.clone());
    }
    abs
}
