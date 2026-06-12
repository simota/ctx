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
use std::path::{Path, PathBuf};

use crate::response;
use crate::safepath;
use crate::AppState;

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

    match ctx_git::worktree_diff(&git_root, &git_rel_slash) {
        Ok(mut diff) => {
            diff.path = rel_slash;
            response::json(StatusCode::OK, &GitDiffResponse::from(diff))
        }
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_diff",
            &err.to_string(),
        ),
    }
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
        Err(err) => response::error(
            StatusCode::INTERNAL_SERVER_ERROR,
            "git_commit_diff",
            &err.to_string(),
        ),
    }
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
