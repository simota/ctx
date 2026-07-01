//! `GET /api/budget` — port of `internal/web/handlers.go` `handleBudget`.
//!
//! Returns a greedy file-budget plan: given a token limit, selects which files
//! fit and which are excluded (too large, binary, generated, or budget exceeded).
//!
//! Query params:
//!   path   (optional) — sub-root relative to server root (default: root)
//!   budget (required) — positive integer token budget
//!
//! Reuses `ctx-tokens` for token counting. Budget planning logic is ported
//! inline from `internal/budget/budget.go` (no Rust crate equivalent yet).
//!
//! ## Role priority mapping (mirrors `internal/budget/budget.go:rolePriority`)
//!   priority 0 — entry, core, route, config
//!   priority 1 — test, util
//!   priority 2 — doc, unknown, ""
//!
//! ## Sort key (mirrors `sort.SliceStable`)
//!   (priority ASC, tokens ASC, path ASC)  — stable sort
//!
//! ## Exclusion rules (in order, mirrors Go)
//!   1. tokens == 0 → reason = "binary"
//!   2. role == "generated" → reason = "generated"
//!   3. tokens > budget/2 → reason = "too large"
//!   else candidate; if candidate does not fit → reason = "budget exceeded"
//!
//! ## Group assignment (mirrors `internal/budget/budget.go:assignGroups`)
//!   A file gets group = its parent dir when 3+ files in that dir share
//!   the same included/excluded set.

use std::path::Path;

use axum::extract::rejection::QueryRejection;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde::{Deserialize, Serialize};

use crate::handlers::file::relative_to_root;
use crate::response;
use crate::safepath;
use crate::AppState;

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct BudgetParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    budget: i64,
}

/// `BudgetItem` mirrors `web.BudgetItem`. Field order matches Go struct.
/// `reason` and `group` are omitted when empty (`omitempty` in Go).
#[derive(Serialize)]
struct BudgetItem {
    path: String,
    tokens: i64,
    #[serde(skip_serializing_if = "str::is_empty")]
    reason: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    group: String,
}

/// `BudgetResponse` mirrors `web.BudgetResponse`. Field order matches Go struct.
#[derive(Serialize)]
struct BudgetResponse {
    budget: i64,
    used: i64,
    included: Vec<BudgetItem>,
    excluded: Vec<BudgetItem>,
}

// ---------------------------------------------------------------------------
// Handler
// ---------------------------------------------------------------------------

pub async fn handle(
    State(state): State<AppState>,
    params: Result<Query<BudgetParams>, QueryRejection>,
) -> Response {
    let Query(params) = match params {
        Ok(q) => q,
        Err(e) => return response::bad_query(e),
    };
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: BudgetParams) -> Response {
    if params.budget <= 0 {
        return response::error(StatusCode::BAD_REQUEST, "bad_request", "budget must be > 0");
    }

    let target = match safepath::resolve(&state.root, &params.path) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    // Collect all files under target (mirrors walk.New + walk.Flatten).
    let mut raw_files: Vec<FileInfo> = Vec::new();
    collect_files(&state.root, &target, &mut raw_files);

    // Sort by path to match Go's os.ReadDir alphabetical order within each dir.
    // (collect_files already produces entries in readdir order per dir, but
    // since we recurse depth-first and readdir is alphabetical, this is already
    // sorted. We sort again to be safe.)
    raw_files.sort_by(|a, b| a.rel_path.cmp(&b.rel_path));

    // Count tokens for each file (mirrors countTokens + tokenCount in Go).
    for fi in &mut raw_files {
        fi.tokens = count_tokens_for_file(&fi.abs_path, fi.size);
    }

    // Budget planning (mirrors internal/budget/budget.go:Plan).
    let budget = params.budget;
    let mut included: Vec<BudgetItem> = Vec::new();
    let mut excluded: Vec<BudgetItem> = Vec::new();
    let mut candidates: Vec<(i64, String, i64, String)> = Vec::new(); // (priority, path, tokens, reason_for_group)

    for fi in &raw_files {
        let tokens = fi.tokens;
        let role = &fi.role;
        let path = fi.rel_path.clone();

        // Rule 1: binary (0 tokens).
        if tokens == 0 {
            excluded.push(BudgetItem {
                path,
                tokens: 0,
                reason: "binary".to_string(),
                group: String::new(),
            });
            continue;
        }
        // Rule 2: generated files.
        if role == "generated" {
            excluded.push(BudgetItem {
                path,
                tokens,
                reason: "generated".to_string(),
                group: String::new(),
            });
            continue;
        }
        // Rule 3: too large (> budget/2).
        if tokens > budget / 2 {
            excluded.push(BudgetItem {
                path,
                tokens,
                reason: "too large".to_string(),
                group: String::new(),
            });
            continue;
        }
        // Candidate: sort by (priority, tokens, path).
        let priority = role_priority(role);
        candidates.push((priority, path, tokens, String::new()));
    }

    // Stable sort: (priority ASC, tokens ASC, path ASC).
    // We use sort_by (stable) on a tuple that naturally compares this way.
    candidates.sort_by(|a, b| a.0.cmp(&b.0).then(a.2.cmp(&b.2)).then(a.1.cmp(&b.1)));

    let mut used: i64 = 0;
    for (_, path, tokens, _) in candidates {
        if used + tokens > budget {
            excluded.push(BudgetItem {
                path,
                tokens,
                reason: "budget exceeded".to_string(),
                group: String::new(),
            });
            continue;
        }
        // reason is "" for included items (omitted by omitempty).
        used += tokens;
        included.push(BudgetItem {
            path,
            tokens,
            reason: String::new(),
            group: String::new(),
        });
    }

    // Group assignment: if 3+ files in a dir share a list, set group = dir.
    assign_groups(&mut included);
    assign_groups(&mut excluded);

    response::json(
        StatusCode::OK,
        &BudgetResponse {
            budget,
            used,
            included,
            excluded,
        },
    )
}

// ---------------------------------------------------------------------------
// Budget helpers — port of internal/budget/budget.go
// ---------------------------------------------------------------------------

/// Maps role string to priority integer.
/// Priority 0 (highest): entry, core, route, config
/// Priority 1: test, util
/// Priority 2 (lowest): doc, unknown, ""
fn role_priority(role: &str) -> i64 {
    match role {
        "entry" | "core" | "route" | "config" => 0,
        "test" | "util" => 1,
        _ => 2, // doc, unknown, ""
    }
}

/// Assign `group` to each item whose parent dir has 3+ items in the same list.
/// Mirrors `internal/budget/budget.go:assignGroups`.
fn assign_groups(items: &mut Vec<BudgetItem>) {
    // Count items per dir (skip root-level "." files).
    let mut counts: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    for item in items.iter() {
        let dir = parent_dir_slash(&item.path);
        if dir == "." {
            continue;
        }
        *counts.entry(dir).or_insert(0) += 1;
    }
    // Assign group where count >= 3.
    for item in items.iter_mut() {
        let dir = parent_dir_slash(&item.path);
        if dir == "." {
            continue;
        }
        if counts.get(&dir).copied().unwrap_or(0) >= 3 {
            item.group = dir;
        }
    }
}

/// Return the parent directory of a slash-separated path.
/// Mirrors `filepath.ToSlash(filepath.Dir(item.Path))` in Go.
fn parent_dir_slash(path: &str) -> String {
    match path.rfind('/') {
        Some(idx) => path[..idx].to_string(),
        None => ".".to_string(),
    }
}

// ---------------------------------------------------------------------------
// File info and walk
// ---------------------------------------------------------------------------

struct FileInfo {
    /// Relative path from served root, slash-separated.
    rel_path: String,
    abs_path: String,
    size: i64,
    /// Token count (set after walking).
    tokens: i64,
    /// Role string (from inferRole).
    role: String,
}

/// Walk `dir` recursively, collecting non-dir file entries.
/// Mirrors Go `walk.New(target, walk.DefaultOptions())` + `walk.Flatten`.
/// Skips: `.git`, `node_modules`, `dist`, `coverage` (ExtraIgnore defaults).
fn collect_files(root: &str, dir: &Path, out: &mut Vec<FileInfo>) {
    let entries = match std::fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return,
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    // Sort alphabetically to match Go's os.ReadDir output.
    entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name_str = name.to_string_lossy();

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };

        if meta.is_dir() {
            // Skip ExtraIgnore dirs (directories only — a regular file named
            // e.g. "dist" must stay visible).
            if matches!(
                name_str.as_ref(),
                ".git" | "node_modules" | "dist" | "coverage"
            ) {
                continue;
            }
            collect_files(root, &path, out);
        } else {
            let abs_path = path.to_string_lossy().to_string();
            let rel_raw = relative_to_root(root, &path);
            // filter out root node (".")
            if rel_raw == "." || rel_raw.is_empty() {
                continue;
            }
            let rel_path = rel_raw.replace('\\', "/");
            let size = meta.len() as i64;
            let role = infer_role(&rel_path);

            out.push(FileInfo {
                rel_path,
                abs_path,
                size,
                tokens: 0,
                role,
            });
        }
    }
}

/// Count tokens for a file, falling back to size estimate on error.
/// Mirrors Go `countTokens` + `tokenCount` logic.
fn count_tokens_for_file(abs_path: &str, size: i64) -> i64 {
    // ctx_tokens::count_file counts tiktoken tokens; returns 0 for binary/error.
    match ctx_tokens::count_file(abs_path) {
        Ok(n) => n as i64,
        Err(_) => ctx_tokens::estimate_by_size(size) as i64,
    }
}

/// Infer role for a file, mirroring `internal/walk/walk.go:inferRole`.
/// Returns "" for unknown/unclassified (same as Go returning "").
fn infer_role(rel_slash: &str) -> String {
    let base = rel_slash.rsplit('/').next().unwrap_or(rel_slash);
    let lower_path = rel_slash.to_ascii_lowercase();
    let lower_base = base.to_ascii_lowercase();
    let ext = if lower_base.contains('.') {
        lower_base.rsplit('.').next().unwrap_or("")
    } else {
        ""
    };

    if lower_path.starts_with("tests/")
        || lower_path.contains("/tests/")
        || lower_base.ends_with("_test.go")
        || is_dotted_test_name(&lower_base)
    {
        return "test".to_string();
    }
    if ext == "md" || lower_base.starts_with("license") || lower_base.starts_with("readme") {
        return "doc".to_string();
    }
    if is_config_file(&lower_base, ext) {
        return "config".to_string();
    }
    if base == "main.ts"
        || base == "main.go"
        || base == "main.py"
        || base == "index.ts"
        || base == "index.tsx"
        || base == "index.js"
        || (rel_slash.starts_with("cmd/") && rel_slash.ends_with("/main.go"))
    {
        return "entry".to_string();
    }
    if base.contains("router") || base.contains("route") || base.contains("Router") {
        return "route".to_string();
    }
    if is_core_extension(ext) {
        return "core".to_string();
    }
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
        "package.json" | "go.mod" | "cargo.toml" | "pyproject.toml" | "dockerfile" | "makefile"
    ) || matches!(ext, "toml" | "yaml" | "yml")
}

fn is_core_extension(ext: &str) -> bool {
    matches!(ext, "ts" | "tsx" | "js" | "go" | "py" | "rs")
}
