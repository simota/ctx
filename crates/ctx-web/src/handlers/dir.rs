//! `GET /api/dir` — port of `internal/web/handlers.go` `handleDir`.
//!
//! Returns directory stats + immediate children sorted dirs-first then
//! name-ASC, plus README/doc.go preview. Git summary is returned as zero
//! (all fields 0) for directories without a git repo — the same result Go
//! produces when `ctxgit.New().Status(root)` returns an empty map. Note that
//! Go's `DirGitSummary` is a VALUE type (not pointer) in DirResponse, so even
//! when all counts are 0, the `json:"git,omitempty"` tag does NOT suppress it
//! (Go only omits pointer structs). We always emit `"git":{}`.

use std::path::Path;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Serialize;
use serde::Deserialize;

use crate::handlers::file::relative_to_root;
use crate::response;
use crate::safepath;
use crate::AppState;

const MAX_README_BYTES: usize = 64 << 10; // 64 KiB

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct DirParams {
    #[serde(default)]
    path: String,
}

/// Mirrors `web.DirChild`. Field order matches Go struct.
#[derive(Serialize)]
struct DirChild {
    path: String,
    name: String,
    is_dir: bool,
    size: i64,
    #[serde(skip_serializing_if = "is_zero_i64")]
    tokens: i64,
    #[serde(skip_serializing_if = "str::is_empty")]
    git: String,
}

/// Mirrors `web.DirGitSummary`. Fields use omitempty (zero int → omitted).
/// The PARENT field `git` in DirResponse does NOT use omitempty for struct
/// values — Go always serializes struct values. We always emit the `git` field.
#[derive(Serialize, Default)]
struct DirGitSummary {
    #[serde(skip_serializing_if = "is_zero_i32")]
    modified: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    added: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    deleted: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    untracked: i32,
}

/// Mirrors `web.DirResponse`. Field order matches Go struct.
/// `git` is ALWAYS present (struct value, not pointer; Go omitempty does not
/// suppress zero struct values for non-pointer fields).
#[derive(Serialize)]
struct DirResponse {
    path: String,
    name: String,
    tokens: i64,
    file_count: i32,
    dir_count: i32,
    git: DirGitSummary,
    #[serde(skip_serializing_if = "str::is_empty")]
    readme: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    readme_path: String,
    children: Vec<DirChild>,
}

fn is_zero_i32(v: &i32) -> bool { *v == 0 }
fn is_zero_i64(v: &i64) -> bool { *v == 0 }

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn handle(State(state): State<AppState>, Query(params): Query<DirParams>) -> Response {
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: DirParams) -> Response {
    let rel = if params.path.is_empty() { "." } else { &params.path };

    let target = match safepath::resolve(&state.root, rel) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let info = match std::fs::metadata(&target) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                // Match Go's `os.Stat` error string exactly:
                //   "stat <abs_path>: no such file or directory"
                return response::error(
                    StatusCode::NOT_FOUND,
                    "not_found",
                    &format!("stat {}: no such file or directory", target.display()),
                );
            }
            return response::error(StatusCode::INTERNAL_SERVER_ERROR, "stat", &e.to_string());
        }
    };
    if !info.is_dir() {
        return response::error(
            StatusCode::BAD_REQUEST,
            "not_a_dir",
            "path is not a directory",
        );
    }

    let dir_rel_raw = relative_to_root(&state.root, &target);
    // Convert "" (root) to "." to match Go relativeToRoot(".", absRoot) = ".".
    let dir_rel = if dir_rel_raw.is_empty() { ".".to_string() } else { dir_rel_raw };

    let name = if dir_rel == "." {
        // Root: use basename of the root dir, matching Go `filepath.Base(a.Root)`.
        Path::new(&state.root)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string())
    } else {
        target
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    // Walk recursively to gather total token/file/dir counts (mirrors Go's
    // walk.DefaultOptions walk + countTokens).
    let (total_tokens, file_count, dir_count) = walk_counts(&state.root, &target);

    // Git summary: DEFERRED (no native git-status). Go's ctxgit.New().Status()
    // returns an empty map when the directory is not a git repo, producing
    // DirGitSummary{} (all zeros). We always emit the `git` field.
    let git = DirGitSummary::default();

    // Immediate children sorted dirs-first then name-ASC (matches Go sort).
    let raw_entries: Vec<_> = match std::fs::read_dir(&target) {
        Ok(rd) => rd.flatten().collect(),
        Err(e) => {
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "read_dir",
                &e.to_string(),
            );
        }
    };

    let mut children: Vec<DirChild> = Vec::new();
    for entry in &raw_entries {
        let child_abs = entry.path();
        let child_info = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        let child_rel = relative_to_root(&state.root, &child_abs);
        let child_name = entry.file_name().to_string_lossy().into_owned();
        let is_dir = child_info.is_dir();
        let size = child_info.len() as i64;
        // Tokens: only for files, estimate by size (matches Go `tokens.EstimateBySize`).
        let tokens = if !is_dir {
            ctx_tokens::estimate_by_size(size)
        } else {
            0
        };
        children.push(DirChild {
            path: child_rel,
            name: child_name,
            is_dir,
            size,
            tokens,
            git: String::new(),
        });
    }

    // Sort: dirs first (is_dir DESC), then name ASC — matches Go's sort.SliceStable.
    children.sort_by(|a, b| {
        if a.is_dir != b.is_dir {
            // dirs first
            return b.is_dir.cmp(&a.is_dir);
        }
        a.name.cmp(&b.name)
    });

    // README/doc.go preview (mirrors Go `findReadme`).
    let (readme, readme_path) = find_readme(&target, &dir_rel, &raw_entries);

    response::json(
        StatusCode::OK,
        &DirResponse {
            path: dir_rel,
            name,
            tokens: total_tokens,
            file_count,
            dir_count,
            git,
            readme,
            readme_path,
            children,
        },
    )
}

// ---------------------------------------------------------------------------
// Walk helpers
// ---------------------------------------------------------------------------

/// Mirrors `walk.DefaultOptions` ExtraIgnore patterns.
/// Does NOT skip all hidden dirs — only those matching the extra-ignore list.
fn should_skip_dir(name: &str) -> bool {
    matches!(name, ".git" | "node_modules" | "dist" | "coverage")
}

/// Recursively count total tokens (tiktoken), files, and subdirectories under
/// `dir`, skipping extra-ignore list — mirrors Go's walk + countTokens.
/// Note: Go uses tiktoken exact count (CountFile) for the aggregate token sum.
/// Returns `(total_tokens, file_count, dir_count)`.
fn walk_counts(root: &str, dir: &Path) -> (i64, i32, i32) {
    let mut total_tokens: i64 = 0;
    let mut file_count: i32 = 0;
    let mut dir_count: i32 = 0;
    walk_counts_inner(root, dir, dir, &mut total_tokens, &mut file_count, &mut dir_count);
    (total_tokens, file_count, dir_count)
}

fn walk_counts_inner(
    root: &str,
    base_dir: &Path,
    dir: &Path,
    total_tokens: &mut i64,
    file_count: &mut i32,
    dir_count: &mut i32,
) {
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd.flatten().collect::<Vec<_>>(),
        Err(_) => return,
    };
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        let info = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if info.is_dir() {
            if should_skip_dir(&name_str) {
                continue;
            }
            // Count all subdirectories (including hidden like .ctx).
            if path != base_dir {
                *dir_count += 1;
            }
            walk_counts_inner(root, base_dir, &path, total_tokens, file_count, dir_count);
        } else {
            *file_count += 1;
            // Use tiktoken exact count (mirrors Go countTokens using counter.CountFile).
            let tok = match ctx_tokens::count_file(path.to_str().unwrap_or("")) {
                Ok(n) => n,
                Err(_) => ctx_tokens::estimate_by_size(info.len() as i64),
            };
            *total_tokens += tok;
        }
    }
}

// ---------------------------------------------------------------------------
// README finder
// ---------------------------------------------------------------------------

const README_CANDIDATES: &[&str] = &[
    "README.md",
    "README",
    "README.txt",
    "readme.md",
    "doc.go",
];

/// Mirrors Go `findReadme`: first matching candidate, truncated to
/// `MAX_README_BYTES`, with its root-relative path.
fn find_readme(
    abs_dir: &Path,
    dir_rel: &str,
    entries: &[std::fs::DirEntry],
) -> (String, String) {
    // Build a set of present non-directory file names.
    let present: std::collections::HashSet<String> = entries
        .iter()
        .filter(|e| !e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();

    for &candidate in README_CANDIDATES {
        if !present.contains(candidate) {
            continue;
        }
        let mut data = match std::fs::read(abs_dir.join(candidate)) {
            Ok(d) => d,
            Err(_) => continue,
        };
        if data.len() > MAX_README_BYTES {
            data.truncate(MAX_README_BYTES);
        }
        let content = String::from_utf8_lossy(&data).into_owned();
        let rel_path = if dir_rel == "." || dir_rel.is_empty() {
            candidate.to_string()
        } else {
            format!("{}/{}", dir_rel, candidate)
        };
        return (content, rel_path);
    }
    (String::new(), String::new())
}
