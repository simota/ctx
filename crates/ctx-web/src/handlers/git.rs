//! `GET /api/git/*` — incremental port of Go git route boundaries.
//!
//! The git producers themselves are still being ported. This module first
//! mirrors the request validation that happens before any git work in Go, so
//! individual parity cases can turn green without touching the oracle tests.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use serde::Serialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};
use std::time::SystemTime;

use crate::response;
use crate::safepath;
use crate::AppState;

/// Soft cap on cached `/api/git/diff` bodies. Mirrors `FILE_CACHE_CAP`: once
/// reached, new diffs are served uncached (existing entries stay valid).
const DIFF_CACHE_CAP: usize = 1024;

/// Diffs whose serialized body exceeds this are never cached, bounding
/// worst-case cache memory to `DIFF_CACHE_CAP * MAX_CACHED_DIFF_BYTES`.
const MAX_CACHED_DIFF_BYTES: usize = 256 << 10;

/// A memoized `/api/git/diff` body plus the fingerprint that validates it.
/// `worktree_diff` compares the HEAD blob against the working-tree file, so the
/// body is stale only when the working file changes (mtime/size) or HEAD moves
/// (oid) — index transitions and unrelated commits never affect a single file's
/// worktree diff.
pub struct DiffCacheEntry {
    mtime: SystemTime,
    size: u64,
    head_oid: Option<String>,
    body: Arc<Vec<u8>>,
}

/// Process-lifetime cache for `/api/git/diff` bodies, keyed by resolved target
/// path. Shared across requests via [`AppState`].
pub type DiffCache = Arc<RwLock<HashMap<PathBuf, DiffCacheEntry>>>;

#[derive(Deserialize)]
pub struct GitDiffParams {
    #[serde(default)]
    path: String,
}

#[derive(Deserialize)]
pub struct FileLogParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    limit: String,
}

#[derive(Deserialize)]
pub struct CommitDiffParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    from: String,
    #[serde(default)]
    to: String,
}

#[derive(Serialize)]
struct GitDiffLine {
    #[serde(rename = "type")]
    typ: String,
    text: String,
    #[serde(skip_serializing_if = "is_zero")]
    old_num: i32,
    #[serde(skip_serializing_if = "is_zero")]
    new_num: i32,
}

#[derive(Serialize)]
struct GitDiffResponse {
    path: String,
    #[serde(skip_serializing_if = "is_false")]
    added: bool,
    #[serde(skip_serializing_if = "is_false")]
    deleted: bool,
    #[serde(skip_serializing_if = "is_false")]
    binary: bool,
    #[serde(skip_serializing_if = "is_false")]
    no_change: bool,
    #[serde(skip_serializing_if = "is_false")]
    truncated: bool,
    lines: Vec<GitDiffLine>,
}

#[derive(Serialize)]
struct FileLogEntry {
    hash: String,
    hash_full: String,
    author: String,
    author_email: String,
    subject: String,
    date: i64,
}

#[derive(Serialize)]
struct FileLogResponse {
    path: String,
    commits: Vec<FileLogEntry>,
    truncated: bool,
}

pub async fn handle_diff(
    State(state): State<AppState>,
    Query(params): Query<GitDiffParams>,
) -> Response {
    crate::blocking::run(move || handle_diff_sync(state, params)).await
}

fn handle_diff_sync(state: AppState, params: GitDiffParams) -> Response {
    if params.path.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }
    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(target) => target,
        Err(err) => return response::bad_path(err),
    };
    let rel_slash = relative_to_root(&state.root, &target);
    let (git_root, git_rel_slash) = git_context(&state.root, &target, &rel_slash);

    // Cache fingerprint: working-tree (mtime, size) + HEAD oid. Both reads are
    // cheap stats relative to the diff itself (HEAD blob inflate + LCS). A miss
    // recomputes the full diff; a hit returns the exact bytes it would produce.
    let fingerprint = std::fs::metadata(&target)
        .ok()
        .and_then(|m| Some((m.modified().ok()?, m.len())));
    let head_oid = ctx_git::head_oid(&git_root).ok().flatten();
    if let Some((mtime, size)) = fingerprint {
        if let Some(body) = diff_cache_get(&state.diff_cache, &target, mtime, size, &head_oid) {
            return response::json_bytes(StatusCode::OK, body.as_ref().clone());
        }
    }

    match ctx_git::worktree_diff(&git_root, &git_rel_slash) {
        Ok(mut diff) => {
            diff.path = rel_slash;
            let body = Arc::new(response::to_json_bytes(&GitDiffResponse::from(diff)));
            if let Some((mtime, size)) = fingerprint {
                diff_cache_put(
                    &state.diff_cache,
                    &target,
                    mtime,
                    size,
                    head_oid,
                    Arc::clone(&body),
                );
            }
            response::json_bytes(StatusCode::OK, body.as_ref().clone())
        }
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_diff",
            &err.to_string(),
        ),
    }
}

/// Return the cached diff body for `target` if its stored fingerprint matches.
fn diff_cache_get(
    cache: &DiffCache,
    target: &Path,
    mtime: SystemTime,
    size: u64,
    head_oid: &Option<String>,
) -> Option<Arc<Vec<u8>>> {
    let guard = cache.read().ok()?;
    let entry = guard.get(target)?;
    (entry.mtime == mtime && entry.size == size && entry.head_oid == *head_oid)
        .then(|| Arc::clone(&entry.body))
}

/// Store `body` for `target`. Refreshes an existing entry, bounded by
/// [`DIFF_CACHE_CAP`]; never stores bodies over [`MAX_CACHED_DIFF_BYTES`].
fn diff_cache_put(
    cache: &DiffCache,
    target: &Path,
    mtime: SystemTime,
    size: u64,
    head_oid: Option<String>,
    body: Arc<Vec<u8>>,
) {
    if body.len() > MAX_CACHED_DIFF_BYTES {
        return;
    }
    let Ok(mut guard) = cache.write() else {
        return;
    };
    if guard.len() >= DIFF_CACHE_CAP && !guard.contains_key(target) {
        return;
    }
    guard.insert(
        target.to_path_buf(),
        DiffCacheEntry {
            mtime,
            size,
            head_oid,
            body,
        },
    );
}

pub async fn handle_file_log(
    State(state): State<AppState>,
    Query(params): Query<FileLogParams>,
) -> Response {
    crate::blocking::run(move || handle_file_log_sync(state, params)).await
}

fn handle_file_log_sync(state: AppState, params: FileLogParams) -> Response {
    if params.path.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }
    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(target) => target,
        Err(err) => return response::bad_path(err),
    };
    let rel_slash = relative_to_root(&state.root, &target);
    let (git_root, git_rel_slash) = git_context(&state.root, &target, &rel_slash);
    let mut limit = params.limit.parse::<i32>().unwrap_or(50);
    limit = limit.clamp(1, 200);

    match ctx_git::file_log(&git_root, &git_rel_slash, limit as usize) {
        Ok((entries, truncated)) => response::json(
            StatusCode::OK,
            &FileLogResponse {
                path: rel_slash,
                commits: entries
                    .into_iter()
                    .map(|entry| FileLogEntry {
                        hash: entry.hash,
                        hash_full: entry.hash_full,
                        author: entry.author,
                        author_email: entry.author_email,
                        subject: entry.subject,
                        date: entry.date,
                    })
                    .collect(),
                truncated,
            },
        ),
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_file_log",
            &err.to_string(),
        ),
    }
}

pub async fn handle_commit_diff(
    State(state): State<AppState>,
    Query(params): Query<CommitDiffParams>,
) -> Response {
    crate::blocking::run(move || handle_commit_diff_sync(state, params)).await
}

fn handle_commit_diff_sync(state: AppState, params: CommitDiffParams) -> Response {
    if params.path.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "path is required");
    }
    if params.from.is_empty() || params.to.is_empty() {
        return response::error(
            StatusCode::BAD_REQUEST,
            "bad_request",
            "from and to are required",
        );
    }

    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(target) => target,
        Err(err) => return response::bad_path(err),
    };
    let rel_slash = relative_to_root(&state.root, &target);
    let (git_root, git_rel_slash) = git_context(&state.root, &target, &rel_slash);

    match ctx_git::commit_diff(&git_root, &params.from, &params.to, &git_rel_slash) {
        Ok(mut diff) => {
            diff.path = rel_slash;
            response::json(StatusCode::OK, &GitDiffResponse::from(diff))
        }
        Err(err) => git_commit_diff_error(&err.to_string()),
    }
}

fn git_commit_diff_error(message: &str) -> Response {
    if is_revision_error(message) {
        return response::error(StatusCode::BAD_REQUEST, "invalid_revision", message);
    }
    response::error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "git_commit_diff",
        message,
    )
}

fn is_revision_error(message: &str) -> bool {
    message.starts_with("cannot resolve ")
}

impl From<ctx_git::WorktreeFileDiff> for GitDiffResponse {
    fn from(diff: ctx_git::WorktreeFileDiff) -> Self {
        Self {
            path: diff.path,
            added: diff.added,
            deleted: diff.deleted,
            binary: diff.binary,
            no_change: diff.no_change,
            truncated: diff.truncated,
            lines: diff
                .lines
                .into_iter()
                .map(|line| GitDiffLine {
                    typ: line.typ,
                    text: line.text,
                    old_num: line.old_num,
                    new_num: line.new_num,
                })
                .collect(),
        }
    }
}

fn is_false(value: &bool) -> bool {
    !*value
}

fn is_zero(value: &i32) -> bool {
    *value == 0
}

fn relative_to_root(root: &str, target: &Path) -> String {
    // Memoized in file.rs — the root is stable for the server's lifetime.
    let abs_root = crate::handlers::file::canonical_root(root);
    target
        .strip_prefix(&abs_root)
        .unwrap_or(target)
        .to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

fn git_context(root: &str, target: &Path, fallback_rel_slash: &str) -> (PathBuf, String) {
    let served_root = crate::handlers::file::canonical_root(root);
    let Some(git_root) = find_git_root(&served_root) else {
        return (served_root, fallback_rel_slash.to_string());
    };

    let git_rel = target
        .strip_prefix(&git_root)
        .map(slash_path)
        .unwrap_or_else(|_| fallback_rel_slash.to_string());
    (git_root, git_rel)
}

fn find_git_root(start: &Path) -> Option<PathBuf> {
    for dir in start.ancestors() {
        if dir.join(".git").exists() {
            return Some(dir.to_path_buf());
        }
    }
    None
}

fn slash_path(path: &Path) -> String {
    path.to_string_lossy()
        .replace(std::path::MAIN_SEPARATOR, "/")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn body(text: &str) -> Arc<Vec<u8>> {
        Arc::new(text.as_bytes().to_vec())
    }

    #[test]
    fn diff_cache_hits_only_on_matching_fingerprint() {
        let cache: DiffCache = DiffCache::default();
        let target = PathBuf::from("/repo/src/main.rs");
        let mtime = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let head = Some("abc123".to_string());
        diff_cache_put(&cache, &target, mtime, 42, head.clone(), body("diff"));

        // Identical fingerprint hits.
        assert!(diff_cache_get(&cache, &target, mtime, 42, &head).is_some());

        // A moved HEAD (same worktree mtime/size, e.g. after commit) misses —
        // the diff base changed even though the file on disk did not.
        let moved = Some("def456".to_string());
        assert!(diff_cache_get(&cache, &target, mtime, 42, &moved).is_none());

        // An edited working file (new mtime or size) misses.
        let later = mtime + Duration::from_secs(1);
        assert!(diff_cache_get(&cache, &target, later, 42, &head).is_none());
        assert!(diff_cache_get(&cache, &target, mtime, 43, &head).is_none());
    }

    #[test]
    fn diff_cache_skips_oversized_bodies() {
        let cache: DiffCache = DiffCache::default();
        let target = PathBuf::from("/repo/big.rs");
        let mtime = SystemTime::UNIX_EPOCH;
        let oversized = Arc::new(vec![0u8; MAX_CACHED_DIFF_BYTES + 1]);
        diff_cache_put(&cache, &target, mtime, 1, None, oversized);
        assert!(diff_cache_get(&cache, &target, mtime, 1, &None).is_none());
    }
}
