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

use crate::handlers::file::relative_to_root;
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

/// A memoized `/api/git/co-change` body plus the HEAD oid that validates it.
/// The co-change graph is a pure function of (HEAD history, params), so the
/// body is stale only when HEAD moves; the cache key folds the params in.
pub struct CoChangeCacheEntry {
    head_oid: Option<String>,
    body: Arc<Vec<u8>>,
}

/// Process-lifetime cache for `/api/git/co-change` bodies, keyed by a string
/// of the request params (`limit|since|min_weight`). Shared via [`AppState`].
pub type CoChangeCache = Arc<RwLock<HashMap<String, CoChangeCacheEntry>>>;

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
    /// Parent full-hashes — populated for repo-level log (drives the commit
    /// graph), empty (and omitted) for the file-scoped log.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    parents: Vec<String>,
}

#[derive(Serialize)]
struct FileLogResponse {
    path: String,
    commits: Vec<FileLogEntry>,
    truncated: bool,
}

#[derive(Deserialize)]
pub struct RepoLogParams {
    #[serde(default)]
    limit: String,
    #[serde(default, rename = "ref")]
    ref_name: String,
}

#[derive(Serialize)]
struct RepoLogResponse {
    commits: Vec<FileLogEntry>,
    truncated: bool,
}

#[derive(Deserialize)]
pub struct CommitFilesParams {
    #[serde(default)]
    hash: String,
}

#[derive(Deserialize)]
pub struct ChangedFilesParams {
    #[serde(default)]
    base: String,
    #[serde(default)]
    head: String,
    #[serde(default)]
    mode: String,
}

#[derive(Deserialize)]
pub struct CoChangeParams {
    #[serde(default)]
    limit: String,
    #[serde(default)]
    since: String,
    #[serde(default)]
    min_weight: String,
}

#[derive(Serialize)]
struct CoChangeNodeResp {
    path: String,
    commits: u32,
    last_commit_time: i64,
    #[serde(skip_serializing_if = "is_zero_u32")]
    lines: u32,
}

#[derive(Serialize)]
struct CoChangeEdgeResp {
    source: usize,
    target: usize,
    weight: u32,
}

#[derive(Serialize)]
struct CoChangeResponse {
    nodes: Vec<CoChangeNodeResp>,
    edges: Vec<CoChangeEdgeResp>,
    commits_scanned: u32,
    truncated: bool,
}

#[derive(Serialize)]
struct BranchEntry {
    name: String,
    hash: String,
    #[serde(skip_serializing_if = "is_false")]
    current: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    subject: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    date: i64,
    #[serde(skip_serializing_if = "is_zero_u32")]
    ahead: u32,
    #[serde(skip_serializing_if = "is_zero_u32")]
    behind: u32,
}

#[derive(Serialize)]
struct BranchesResponse {
    branches: Vec<BranchEntry>,
}

#[derive(Serialize)]
struct TagEntry {
    name: String,
    hash: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    date: i64,
}

#[derive(Serialize)]
struct TagsResponse {
    tags: Vec<TagEntry>,
}

#[derive(Serialize)]
struct WorktreeEntry {
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    branch: Option<String>,
    #[serde(skip_serializing_if = "String::is_empty")]
    head: String,
    #[serde(skip_serializing_if = "is_false")]
    bare: bool,
    #[serde(skip_serializing_if = "is_false")]
    detached: bool,
}

#[derive(Serialize)]
struct WorktreesResponse {
    worktrees: Vec<WorktreeEntry>,
}

#[derive(Serialize)]
struct CommitFileEntry {
    status: String,
    path: String,
    #[serde(skip_serializing_if = "is_zero_u32")]
    additions: u32,
    #[serde(skip_serializing_if = "is_zero_u32")]
    deletions: u32,
    #[serde(skip_serializing_if = "is_false")]
    binary: bool,
}

#[derive(Serialize)]
struct CommitFilesResponse {
    hash: String,
    files: Vec<CommitFileEntry>,
}

#[derive(Serialize)]
struct ChangedFilesSummaryResp {
    files: u32,
    additions: u32,
    deletions: u32,
    binary_files: u32,
}

#[derive(Serialize)]
struct ChangedFileEntryResp {
    status: String,
    path: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    old_path: Option<String>,
    additions: u32,
    deletions: u32,
    binary: bool,
    #[serde(skip_serializing_if = "String::is_empty")]
    raw_status: String,
}

#[derive(Serialize)]
struct ChangedFilesResponse {
    requested_base: String,
    requested_head: String,
    mode: String,
    effective_base: String,
    effective_head: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    merge_base: Option<String>,
    summary: ChangedFilesSummaryResp,
    limit: usize,
    truncated: bool,
    files: Vec<ChangedFileEntryResp>,
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
    let limit = crate::handlers::limit::parse_limit(&params.limit, 50, 1, 200);

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
                        parents: Vec::new(),
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

/// A ref is accepted when it does not start with `-` (which git would read as
/// an option) and every character is from the conservative branch/hash/tag set.
fn is_valid_ref(ref_name: &str) -> bool {
    !ref_name.starts_with('-')
        && ref_name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '/' | '@' | '-'))
}

fn is_valid_range_ref(ref_name: &str) -> bool {
    if ref_name.is_empty() || ref_name.len() > 255 {
        return false;
    }
    if ref_name.starts_with('-') || ref_name.ends_with('/') || ref_name.ends_with('.') {
        return false;
    }
    if ref_name.contains("..") || ref_name.contains("@{") || ref_name.contains("//") {
        return false;
    }
    ref_name
        .bytes()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'/' | b'@' | b'-'))
}

pub async fn handle_repo_log(
    State(state): State<AppState>,
    Query(params): Query<RepoLogParams>,
) -> Response {
    crate::blocking::run(move || handle_repo_log_sync(state, params)).await
}

fn handle_repo_log_sync(state: AppState, params: RepoLogParams) -> Response {
    let git_root = git_root_only(&state.root);
    let limit = crate::handlers::limit::parse_limit(&params.limit, 50, 1, 200);

    // Empty ref keeps the default-HEAD behaviour; a non-empty ref is validated
    // before it reaches git so a leading dash can't smuggle in an option.
    let ref_opt = if params.ref_name.is_empty() {
        None
    } else if is_valid_ref(&params.ref_name) {
        Some(params.ref_name.as_str())
    } else {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "invalid ref");
    };

    match ctx_git::repo_log(&git_root, limit as usize, ref_opt) {
        Ok((entries, truncated)) => response::json(
            StatusCode::OK,
            &RepoLogResponse {
                commits: entries
                    .into_iter()
                    .map(|entry| FileLogEntry {
                        hash: entry.hash,
                        hash_full: entry.hash_full,
                        author: entry.author,
                        author_email: entry.author_email,
                        subject: entry.subject,
                        date: entry.date,
                        parents: entry.parents,
                    })
                    .collect(),
                truncated,
            },
        ),
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_log",
            &err.to_string(),
        ),
    }
}

pub async fn handle_branches(State(state): State<AppState>) -> Response {
    crate::blocking::run(move || handle_branches_sync(state)).await
}

fn handle_branches_sync(state: AppState) -> Response {
    let git_root = git_root_only(&state.root);
    match ctx_git::branches(&git_root) {
        Ok(list) => response::json(
            StatusCode::OK,
            &BranchesResponse {
                branches: list
                    .into_iter()
                    .map(|b| BranchEntry {
                        name: b.name,
                        hash: b.hash,
                        current: b.current,
                        subject: b.subject,
                        date: b.date,
                        ahead: b.ahead,
                        behind: b.behind,
                    })
                    .collect(),
            },
        ),
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_branches",
            &err.to_string(),
        ),
    }
}

pub async fn handle_tags(State(state): State<AppState>) -> Response {
    crate::blocking::run(move || handle_tags_sync(state)).await
}

fn handle_tags_sync(state: AppState) -> Response {
    let git_root = git_root_only(&state.root);
    match ctx_git::tags(&git_root) {
        Ok(list) => response::json(
            StatusCode::OK,
            &TagsResponse {
                tags: list
                    .into_iter()
                    .map(|t| TagEntry {
                        name: t.name,
                        hash: t.hash,
                        date: t.date,
                    })
                    .collect(),
            },
        ),
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_tags",
            &err.to_string(),
        ),
    }
}

pub async fn handle_worktrees(State(state): State<AppState>) -> Response {
    crate::blocking::run(move || handle_worktrees_sync(state)).await
}

fn handle_worktrees_sync(state: AppState) -> Response {
    let git_root = git_root_only(&state.root);
    match ctx_git::worktrees(&git_root) {
        Ok(list) => response::json(
            StatusCode::OK,
            &WorktreesResponse {
                worktrees: list
                    .into_iter()
                    .map(|w| WorktreeEntry {
                        path: w.path,
                        branch: w.branch,
                        head: w.head,
                        bare: w.bare,
                        detached: w.detached,
                    })
                    .collect(),
            },
        ),
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_worktrees",
            &err.to_string(),
        ),
    }
}

pub async fn handle_commit_files(
    State(state): State<AppState>,
    Query(params): Query<CommitFilesParams>,
) -> Response {
    crate::blocking::run(move || handle_commit_files_sync(state, params)).await
}

fn handle_commit_files_sync(state: AppState, params: CommitFilesParams) -> Response {
    if params.hash.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "hash is required");
    }
    let git_root = git_root_only(&state.root);

    match ctx_git::commit_files(&git_root, &params.hash) {
        Ok(files) => response::json(
            StatusCode::OK,
            &CommitFilesResponse {
                hash: params.hash,
                files: files
                    .into_iter()
                    .map(|f| CommitFileEntry {
                        status: f.status,
                        path: f.path,
                        additions: f.additions,
                        deletions: f.deletions,
                        binary: f.binary,
                    })
                    .collect(),
            },
        ),
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_commit_files",
            &err.to_string(),
        ),
    }
}

pub async fn handle_changed_files(
    State(state): State<AppState>,
    Query(params): Query<ChangedFilesParams>,
) -> Response {
    crate::blocking::run(move || handle_changed_files_sync(state, params)).await
}

fn handle_changed_files_sync(state: AppState, params: ChangedFilesParams) -> Response {
    const CHANGED_FILES_LIMIT: usize = 1000;

    if params.base.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "base is required");
    }
    if params.head.is_empty() {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "head is required");
    }
    if !is_valid_range_ref(&params.base) || !is_valid_range_ref(&params.head) {
        return response::error(StatusCode::BAD_REQUEST, "invalid_ref", "invalid ref");
    }
    let mode = match params.mode.as_str() {
        "" | "merge-base" => ctx_git::ChangedFilesMode::MergeBase,
        "direct" => ctx_git::ChangedFilesMode::Direct,
        _ => {
            return response::error(StatusCode::BAD_REQUEST, "bad_request", "invalid mode");
        }
    };

    let served_root = crate::handlers::file::canonical_root(&state.root);
    let git_root = git_root_only(&state.root);
    match ctx_git::changed_files_between(
        &git_root,
        &params.base,
        &params.head,
        mode,
        CHANGED_FILES_LIMIT,
    ) {
        Ok(manifest) => response::json(
            StatusCode::OK,
            &changed_files_response(manifest, &git_root, &served_root),
        ),
        Err(err) => git_changed_files_error(&err.to_string()),
    }
}

fn changed_files_response(
    manifest: ctx_git::ChangedFilesManifest,
    git_root: &Path,
    served_root: &Path,
) -> ChangedFilesResponse {
    let mut files: Vec<ChangedFileEntryResp> = manifest
        .files
        .into_iter()
        .filter_map(|file| changed_file_response(file, git_root, served_root))
        .collect();
    let summary = ChangedFilesSummaryResp {
        files: files.len() as u32,
        additions: files.iter().map(|f| f.additions).sum(),
        deletions: files.iter().map(|f| f.deletions).sum(),
        binary_files: files.iter().filter(|f| f.binary).count() as u32,
    };
    files.sort_by(|a, b| a.path.cmp(&b.path));
    ChangedFilesResponse {
        requested_base: manifest.requested_base,
        requested_head: manifest.requested_head,
        mode: manifest.mode,
        effective_base: manifest.effective_base,
        effective_head: manifest.effective_head,
        merge_base: manifest.merge_base,
        summary,
        limit: manifest.limit,
        truncated: manifest.truncated,
        files,
    }
}

fn changed_file_response(
    file: ctx_git::ChangedFile,
    git_root: &Path,
    served_root: &Path,
) -> Option<ChangedFileEntryResp> {
    let path = repo_path_to_served_path(git_root, served_root, &file.path)?;
    let old_path = file
        .old_path
        .as_deref()
        .and_then(|p| repo_path_to_served_path(git_root, served_root, p));
    let raw_status = if matches!(
        (file.status.as_str(), file.raw_status.as_str()),
        ("added", "A") | ("modified", "M") | ("deleted", "D")
    ) || (file.status == "renamed" && file.raw_status.starts_with('R'))
    {
        String::new()
    } else {
        file.raw_status
    };
    Some(ChangedFileEntryResp {
        status: file.status,
        path,
        old_path,
        additions: file.additions,
        deletions: file.deletions,
        binary: file.binary,
        raw_status,
    })
}

fn repo_path_to_served_path(
    git_root: &Path,
    served_root: &Path,
    repo_path: &str,
) -> Option<String> {
    let abs = git_root.join(repo_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    abs.strip_prefix(served_root).ok().map(slash_path)
}

fn git_changed_files_error(message: &str) -> Response {
    if is_revision_error(message)
        || message == "reference not found"
        || message == "merge base not found"
        || message.ends_with(" has no commit")
    {
        return response::error(StatusCode::BAD_REQUEST, "invalid_revision", message);
    }
    response::error(
        StatusCode::INTERNAL_SERVER_ERROR,
        "git_changed_files",
        message,
    )
}

pub async fn handle_co_change(
    State(state): State<AppState>,
    Query(params): Query<CoChangeParams>,
) -> Response {
    crate::blocking::run(move || handle_co_change_sync(state, params)).await
}

fn handle_co_change_sync(state: AppState, params: CoChangeParams) -> Response {
    let git_root = git_root_only(&state.root);

    // Co-change wants a wide window, so the limit cap is larger than the
    // commit-log handler's; default 500, default min_weight 2 drops one-off
    // accidental pairs server-side.
    let limit = crate::handlers::limit::parse_limit(&params.limit, 500, 1, 2000) as usize;
    let min_weight = params.min_weight.parse::<u32>().unwrap_or(2);
    let since = if params.since.is_empty() {
        None
    } else {
        Some(params.since.as_str())
    };

    // Cache key folds the params; the entry is validated against HEAD oid so
    // a moved HEAD invalidates without an explicit purge.
    let cache_key = format!("{limit}|{}|{min_weight}", params.since);
    let head_oid = ctx_git::head_oid(&git_root).ok().flatten();
    if let Some(body) = co_change_cache_get(&state.co_change_cache, &cache_key, &head_oid) {
        return response::json_bytes(StatusCode::OK, body.as_ref().clone());
    }

    match ctx_git::co_change_graph(&git_root, limit, since, min_weight) {
        Ok(graph) => {
            let response = CoChangeResponse {
                nodes: graph
                    .nodes
                    .into_iter()
                    .map(|n| CoChangeNodeResp {
                        path: n.path,
                        commits: n.commits,
                        last_commit_time: n.last_commit_time,
                        lines: n.lines,
                    })
                    .collect(),
                edges: graph
                    .edges
                    .into_iter()
                    .map(|e| CoChangeEdgeResp {
                        source: e.source,
                        target: e.target,
                        weight: e.weight,
                    })
                    .collect(),
                commits_scanned: graph.commits_scanned,
                truncated: graph.truncated,
            };
            let body = Arc::new(response::to_json_bytes(&response));
            co_change_cache_put(
                &state.co_change_cache,
                cache_key,
                head_oid,
                Arc::clone(&body),
            );
            response::json_bytes(StatusCode::OK, body.as_ref().clone())
        }
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_co_change",
            &err.to_string(),
        ),
    }
}

/// Return the cached co-change body if its stored params key and HEAD oid match.
fn co_change_cache_get(
    cache: &CoChangeCache,
    key: &str,
    head_oid: &Option<String>,
) -> Option<Arc<Vec<u8>>> {
    let guard = cache.read().ok()?;
    let entry = guard.get(key)?;
    (entry.head_oid == *head_oid).then(|| Arc::clone(&entry.body))
}

/// Store `body` under `key`, refreshing any existing entry for the same params.
fn co_change_cache_put(
    cache: &CoChangeCache,
    key: String,
    head_oid: Option<String>,
    body: Arc<Vec<u8>>,
) {
    let Ok(mut guard) = cache.write() else {
        return;
    };
    guard.insert(key, CoChangeCacheEntry { head_oid, body });
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

fn is_zero_u32(value: &u32) -> bool {
    *value == 0
}

fn is_zero_i64(value: &i64) -> bool {
    *value == 0
}

/// Resolve the enclosing git repository root for repo-level queries (log,
/// commit files) that have no per-file target. Falls back to the served root
/// when the served directory is not inside a git repo — `ctx_git` then reports
/// an empty history rather than erroring.
fn git_root_only(root: &str) -> PathBuf {
    let served_root = crate::handlers::file::canonical_root(root);
    find_git_root(&served_root).unwrap_or(served_root)
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
