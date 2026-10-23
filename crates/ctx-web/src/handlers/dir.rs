//! `GET /api/dir` — port of `internal/web/handlers.go` `handleDir`.
//!
//! Returns directory stats + immediate children sorted dirs-first then
//! name-ASC, plus README/doc.go preview. Git summary is returned as zero
//! (all fields 0) for directories without a git repo — the same result Go
//! produces when `ctxgit.New().Status(root)` returns an empty map. Note that
//! Go's `DirGitSummary` is a VALUE type (not pointer) in DirResponse, so even
//! when all counts are 0, the `json:"git,omitempty"` tag does NOT suppress it
//! (Go only omits pointer structs). We always emit `"git":{}`.

use std::io::Read;
use std::path::Path;

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::Deserialize;
use serde::Serialize;

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

fn is_zero_i32(v: &i32) -> bool {
    *v == 0
}
fn is_zero_i64(v: &i64) -> bool {
    *v == 0
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn handle(State(state): State<AppState>, Query(params): Query<DirParams>) -> Response {
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: DirParams) -> Response {
    let rel = if params.path.is_empty() {
        "."
    } else {
        &params.path
    };

    let target = match safepath::resolve(&state.root, rel) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let info = match std::fs::metadata(&target) {
        Ok(m) => m,
        Err(e) => {
            if e.kind() == std::io::ErrorKind::NotFound {
                return response::stat_not_found(&target);
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
    let dir_rel = if dir_rel_raw.is_empty() {
        ".".to_string()
    } else {
        dir_rel_raw
    };

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
    let (readme, readme_path) = find_readme(&state.root, &target, &dir_rel, &raw_entries);

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
    walk_counts_inner(
        root,
        dir,
        dir,
        &mut total_tokens,
        &mut file_count,
        &mut dir_count,
    );
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
            // Reuses the same (mtime, size)-keyed cache as `/api/tree` so a file
            // already seen by either route isn't re-read/re-encoded (PERF-4).
            let size = info.len() as i64;
            let (_, tok) = crate::handlers::tree::cached_file_stats(&path, &info, size, true);
            *total_tokens += tok as i64;
        }
    }
}

// ---------------------------------------------------------------------------
// README finder
// ---------------------------------------------------------------------------

const README_CANDIDATES: &[&str] = &["README.md", "README", "README.txt", "readme.md", "doc.go"];

/// Mirrors Go `findReadme`: first matching candidate, truncated to
/// `MAX_README_BYTES`, with its root-relative path.
///
/// The candidate is canonicalized from `abs_dir` (not re-parsed from the
/// display path, which would reinterpret `\\` or `c:` in directory names) and
/// must stay inside the canonical root, so a `README.md` symlink to a file
/// outside root is skipped. The canonical path is opened non-blocking and
/// only a regular file (checked on the open handle) is read, at most
/// `MAX_README_BYTES`, so a link to `/dev/zero` or a FIFO named `README`
/// cannot pin the request.
fn find_readme(
    root: &str,
    abs_dir: &Path,
    dir_rel: &str,
    entries: &[std::fs::DirEntry],
) -> (String, String) {
    // Build a set of present non-directory file names.
    let present: std::collections::HashSet<String> = entries
        .iter()
        .filter(|e| e.file_type().map(|t| !t.is_dir()).unwrap_or(false))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    let canonical_root = crate::handlers::file::canonical_root(root);

    for &candidate in README_CANDIDATES {
        if !present.contains(candidate) {
            continue;
        }
        let Ok(path) = std::fs::canonicalize(abs_dir.join(candidate)) else {
            continue;
        };
        if !path.starts_with(&canonical_root) {
            continue;
        }
        let Some(data) = read_regular_prefix(&path, MAX_README_BYTES) else {
            continue;
        };
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

/// Read at most `limit` bytes of `path` if it is a regular file. Opened
/// non-blocking so a FIFO swapped in after the caller's checks cannot block;
/// the type is checked on the open handle.
fn read_regular_prefix(path: &Path, limit: usize) -> Option<Vec<u8>> {
    let mut opts = std::fs::OpenOptions::new();
    opts.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        opts.custom_flags(libc::O_NONBLOCK);
    }
    let file = opts.open(path).ok()?;
    if !file.metadata().ok()?.is_file() {
        return None;
    }
    let mut data = Vec::new();
    file.take(limit as u64).read_to_end(&mut data).ok()?;
    Some(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::time::{Duration, SystemTime};

    fn temp_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "ctx-web-dir-{tag}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// GET /api/dir?path=<rel> under `root`, failing instead of hanging when
    /// the handler blocks on a read.
    fn get_dir(root: &Path, rel: &str) -> serde_json::Value {
        let state = AppState {
            root: root.to_string_lossy().into_owned(),
            bind: String::new(),
            audit: false,
            file_cache: Default::default(),
            diff_cache: Default::default(),
            co_change_cache: Default::default(),
            loopback_only: true,
        };
        let params = DirParams {
            path: rel.to_string(),
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(handle_sync(state, params));
        });
        let resp = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("/api/dir did not answer within 10s");
        assert_eq!(resp.status(), StatusCode::OK);
        let rt = tokio::runtime::Builder::new_current_thread()
            .build()
            .unwrap();
        let body = rt
            .block_on(axum::body::to_bytes(resp.into_body(), usize::MAX))
            .unwrap();
        serde_json::from_slice(&body).unwrap()
    }

    #[cfg(unix)]
    #[test]
    fn readme_symlink_outside_root_is_not_served() {
        let outside = temp_dir("outside");
        std::fs::write(outside.join("secret"), "TOP SECRET\n").unwrap();
        let root = temp_dir("root-escape");
        std::os::unix::fs::symlink(outside.join("secret"), root.join("README.md")).unwrap();

        let v = get_dir(&root, "");
        assert!(
            !v.to_string().contains("TOP SECRET"),
            "outside file leaked: {v}"
        );
        assert!(v.get("readme").is_none(), "{v}");
        assert!(v.get("readme_path").is_none(), "{v}");

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[cfg(unix)]
    #[test]
    fn readme_symlink_inside_root_is_still_served() {
        let root = temp_dir("root-inside");
        std::fs::create_dir_all(root.join("docs")).unwrap();
        std::fs::write(root.join("docs/intro.md"), "# Intro\n").unwrap();
        std::os::unix::fs::symlink("docs/intro.md", root.join("README.md")).unwrap();

        let v = get_dir(&root, "");
        assert_eq!(v["readme"], "# Intro\n", "{v}");
        assert_eq!(v["readme_path"], "README.md", "{v}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn readme_found_when_root_is_reached_through_symlink() {
        let real = temp_dir("root-real");
        std::fs::create_dir_all(real.join("pkg")).unwrap();
        std::fs::write(real.join("pkg/README.md"), "pkg docs\n").unwrap();
        let link = real.with_extension("link");
        std::os::unix::fs::symlink(&real, &link).unwrap();

        let v = get_dir(&link, "pkg");
        assert_eq!(v["readme"], "pkg docs\n", "{v}");
        assert_eq!(v["readme_path"], "pkg/README.md", "{v}");

        let _ = std::fs::remove_file(link);
        let _ = std::fs::remove_dir_all(real);
    }

    #[cfg(unix)]
    #[test]
    fn readme_follows_actual_dir_when_name_looks_like_separator_or_drive() {
        let root = temp_dir("root-oddnames");
        std::fs::create_dir_all(root.join("a\\b")).unwrap();
        std::fs::write(root.join("a\\b/README.md"), "RIGHT\n").unwrap();
        std::fs::create_dir_all(root.join("a/b")).unwrap();
        std::fs::write(root.join("a/b/README.md"), "WRONG\n").unwrap();
        std::os::unix::fs::symlink("a\\b", root.join("lnk")).unwrap();
        std::fs::create_dir_all(root.join("c:")).unwrap();
        std::fs::write(root.join("c:/README.md"), "DRIVE\n").unwrap();
        std::os::unix::fs::symlink("c:", root.join("lnk2")).unwrap();

        let v = get_dir(&root, "lnk");
        assert_eq!(v["readme"], "RIGHT\n", "{v}");
        let v = get_dir(&root, "lnk2");
        assert_eq!(v["readme"], "DRIVE\n", "{v}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn readme_is_truncated_to_cap() {
        let root = temp_dir("root-big");
        std::fs::create_dir_all(root.join("sub")).unwrap();
        let text = "readme line\n".repeat(MAX_README_BYTES / 12 + 10);
        std::fs::write(root.join("sub/README"), &text).unwrap();

        let v = get_dir(&root, "sub");
        assert_eq!(v["readme"], text[..MAX_README_BYTES], "{v}");
        assert_eq!(v["readme_path"], "sub/README", "{v}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn fifo_readme_does_not_block() {
        let root = temp_dir("root-fifo");
        let status = std::process::Command::new("mkfifo")
            .arg(root.join("README"))
            .status()
            .expect("mkfifo");
        assert!(status.success());
        std::fs::write(root.join("README.txt"), "fallback\n").unwrap();

        let v = get_dir(&root, "");
        assert_eq!(v["readme"], "fallback\n", "{v}");
        assert_eq!(v["readme_path"], "README.txt", "{v}");

        let _ = std::fs::remove_dir_all(root);
    }

    #[cfg(unix)]
    #[test]
    fn walk_does_not_follow_symlinks_out_of_root() {
        let outside = temp_dir("walk-outside");
        std::fs::create_dir_all(outside.join("tree/deep")).unwrap();
        for i in 0..5 {
            std::fs::write(outside.join(format!("tree/deep/f{i}.txt")), "x\n").unwrap();
        }
        std::fs::write(outside.join("big.txt"), "word ".repeat(4000)).unwrap();
        let root = temp_dir("walk-root");
        std::fs::write(root.join("a.txt"), "hello\n").unwrap();
        std::os::unix::fs::symlink(outside.join("tree"), root.join("linkdir")).unwrap();
        std::os::unix::fs::symlink(outside.join("big.txt"), root.join("linkfile")).unwrap();

        let v = get_dir(&root, "");
        // a.txt + the two links as leaf entries; nothing behind linkdir.
        assert_eq!(v["file_count"], 3, "{v}");
        assert_eq!(v["dir_count"], 0, "{v}");
        let link_len = std::fs::symlink_metadata(root.join("linkfile"))
            .unwrap()
            .len() as i64;
        let dir_len = std::fs::symlink_metadata(root.join("linkdir"))
            .unwrap()
            .len() as i64;
        let a_tokens = ctx_tokens::count_file(&root.join("a.txt").to_string_lossy()).unwrap();
        assert_eq!(
            v["tokens"],
            a_tokens
                + ctx_tokens::estimate_by_size(link_len)
                + ctx_tokens::estimate_by_size(dir_len),
            "{v}"
        );

        let _ = std::fs::remove_dir_all(root);
        let _ = std::fs::remove_dir_all(outside);
    }

    #[cfg(target_os = "linux")]
    #[test]
    fn device_symlink_is_not_read() {
        let root = temp_dir("root-devzero");
        std::os::unix::fs::symlink("/dev/zero", root.join("README.md")).unwrap();
        std::os::unix::fs::symlink("/dev/zero", root.join("zero.bin")).unwrap();

        let v = get_dir(&root, "");
        assert!(v.get("readme").is_none(), "{v}");
        assert_eq!(v["file_count"], 2, "{v}");

        let _ = std::fs::remove_dir_all(root);
    }
}
