//! `GET /api/tree` — port of `internal/web/handlers.go` `handleTree`.
//!
//! Returns a recursive directory tree rooted at the requested path.
//! Query params: `path` (default "."), `depth` (int, 0=unlimited),
//! `tokens` (bool), `symbols` (bool, DEFERRED), `git` (bool),
//! `use_mtime` (bool), `since`, `until` (time-filter strings).
//!
//! Symbols are DEFERRED. Git status is loaded from `git status --porcelain`
//! when requested and aggregated up directories so the UI can filter changed
//! files while preserving ancestor rows.
//!
//! `since`/`until` filter file nodes by mtime. Files modified before `since`
//! or after `until` are omitted; a directory left with no surviving
//! descendants is pruned too (the UI hides folders a filter emptied), except
//! the root, which always renders. `use_mtime` is accepted so the route does
//! not 400, but it has no effect: the web walk always uses mtime.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
// Time-filter parser — ported from ctx-cli/src/commands/pack/timefilter.rs.
// Only the pure (no-I/O) subset is included; the git-log GitTimeIndex is not.
// Accepted formats: relative (Nd / Nw / Nmo / Ny / Nh / Nm / Ns) or
// absolute YYYY-MM-DD (UTC midnight). Private to this module.
// ---------------------------------------------------------------------------

fn parse_pack_time_filter(input: &str, now: SystemTime) -> Result<SystemTime, String> {
    if input.is_empty() {
        return Err("time filter: empty string".to_string());
    }
    if let Some(t) = parse_yyyy_mm_dd_utc(input) {
        return Ok(t);
    }
    let lower = input.to_ascii_lowercase();
    let calendar_units = [
        ("mo", 30_u64 * 24 * 60 * 60),
        ("w", 7_u64 * 24 * 60 * 60),
        ("d", 24_u64 * 60 * 60),
        ("y", 365_u64 * 24 * 60 * 60),
    ];
    for (suffix, seconds) in calendar_units {
        if lower.ends_with(suffix) {
            let number = &input[..input.len() - suffix.len()];
            let n = parse_positive_u64_filter(number, input)?;
            return subtract_filter_duration(now, n, seconds, input);
        }
    }
    let duration_units = [("h", 60_u64 * 60), ("m", 60_u64), ("s", 1_u64)];
    for (suffix, seconds) in duration_units {
        if lower.ends_with(suffix) {
            let number = &input[..input.len() - suffix.len()];
            let n = parse_positive_u64_filter(number, input)?;
            return subtract_filter_duration(now, n, seconds, input);
        }
    }
    Err(format!(
        "time filter {input:?}: unrecognised format (expected YYYY-MM-DD or relative like 7d/2w/1mo/1y)"
    ))
}

fn parse_positive_u64_filter(number: &str, original: &str) -> Result<u64, String> {
    if number.is_empty() {
        return Err(format!("time filter {original:?}: missing numeric part"));
    }
    if !number.bytes().all(|byte| byte.is_ascii_digit()) {
        return Err(format!(
            "time filter {original:?}: invalid numeric part {number:?}"
        ));
    }
    let value = number
        .parse::<u64>()
        .map_err(|err| format!("time filter {original:?}: {err}"))?;
    if value == 0 {
        return Err(format!(
            "time filter {original:?}: value must be positive, got 0"
        ));
    }
    Ok(value)
}

fn subtract_filter_duration(
    now: SystemTime,
    amount: u64,
    unit_seconds: u64,
    original: &str,
) -> Result<SystemTime, String> {
    let seconds = amount
        .checked_mul(unit_seconds)
        .ok_or_else(|| format!("time filter {original:?}: duration overflow"))?;
    now.checked_sub(Duration::from_secs(seconds))
        .ok_or_else(|| format!("time filter {original:?}: duration is before unix epoch"))
}

fn parse_yyyy_mm_dd_utc(input: &str) -> Option<SystemTime> {
    let mut parts = input.split('-');
    let year = parts.next()?.parse::<i64>().ok()?;
    let month = parts.next()?.parse::<u32>().ok()?;
    let day = parts.next()?.parse::<u32>().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let days = days_from_civil(year, month, day);
    if days < 0 {
        return None;
    }
    Some(UNIX_EPOCH + Duration::from_secs(days as u64 * 24 * 60 * 60))
}

fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = if year >= 0 { year } else { year - 399 } / 400;
    let yoe = year - era * 400;
    let month = month as i64;
    let day = day as i64;
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146097 + doe - 719468
}

// ---------------------------------------------------------------------------
// Request / response types
// ---------------------------------------------------------------------------

#[derive(Deserialize)]
pub struct TreeParams {
    #[serde(default)]
    path: String,
    #[serde(default)]
    depth: i32,
    #[serde(default)]
    tokens: bool,
    // symbols is DEFERRED — accepted but ignored so the route doesn't 400 on
    // unknown params.
    #[serde(default)]
    _symbols: bool,
    #[serde(default)]
    git: bool,
    // When true, entries matching the root `.gitignore` are pruned from the
    // walk (in addition to the always-on ExtraIgnore list). Default false so
    // the plain tree view keeps showing every tracked + untracked file; the
    // "Largest source files" view opts in via `gitignore=true`.
    #[serde(default)]
    gitignore: bool,
    // Accepted so the route does not 400; has no effect — the web walk always
    // filters by mtime regardless of this flag (no git-commit-time index).
    #[serde(default)]
    use_mtime: bool,
    // since/until time-filter strings. Empty → no filtering. Non-empty →
    // parsed as relative (Nd/Nw/Nmo/Ny/Nh/Nm/Ns) or YYYY-MM-DD (UTC).
    // Files whose mtime is outside [since, until] are excluded; directories
    // are always retained (CLI parity).
    #[serde(default)]
    since: String,
    #[serde(default)]
    until: String,
}

/// Mirrors `web.TreeNode`. Field order matches Go struct.
#[derive(Serialize)]
pub struct TreeNode {
    path: String,
    name: String,
    is_dir: bool,
    size: i64,
    #[serde(skip_serializing_if = "is_zero_i32")]
    lines: i32,
    #[serde(skip_serializing_if = "is_zero_i32")]
    tokens: i32,
    #[serde(skip_serializing_if = "str::is_empty")]
    role: String,
    #[serde(skip_serializing_if = "str::is_empty")]
    git: String,
    #[serde(skip_serializing_if = "is_zero_i64")]
    updated_at: i64,
    // symbols: DEFERRED — omitempty in Go, omit always here.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    children: Vec<TreeNode>,
}

/// Mirrors `web.TreeResponse`. Field order matches Go struct.
#[derive(Serialize)]
struct TreeResponse {
    root: String,
    abs_root: String,
    tree: TreeNode,
    total: i32,
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

pub async fn handle(
    State(state): State<AppState>,
    params: Result<Query<TreeParams>, QueryRejection>,
) -> Response {
    let Query(params) = match params {
        Ok(q) => q,
        Err(e) => return response::bad_query(e),
    };
    // `git status` + the recursive walk can take seconds on large repos; keep
    // them off the tokio workers so parallel SPA requests aren't starved.
    crate::blocking::run(move || handle_sync(state, params)).await
}

fn handle_sync(state: AppState, params: TreeParams) -> Response {
    let rel = if params.path.is_empty() {
        "."
    } else {
        &params.path
    };

    let target = match safepath::resolve(&state.root, rel) {
        Ok(t) => t,
        Err(e) => return response::bad_path(e),
    };

    let max_depth = if params.depth <= 0 {
        0
    } else {
        params.depth as usize
    };

    let git_status = if params.git {
        GitStatusMap::load(&state.root)
    } else {
        GitStatusMap::default()
    };

    // Optional `.gitignore` matcher (root file only, mirroring the CLI walk's
    // root-`.gitignore` handling). Compiled once and shared across the walk.
    // A missing or unparseable file yields `None` (no extra pruning).
    let ignore = if params.gitignore {
        let gi_path = Path::new(&state.root).join(".gitignore");
        ctx_gitignore::GitIgnore::from_file(&gi_path).ok()
    } else {
        None
    };

    // Parse since/until once. Empty string → None (no filtering).
    // Invalid format → HTTP 400.
    let now = SystemTime::now();
    let since_time: Option<SystemTime> = if params.since.is_empty() {
        None
    } else {
        match parse_pack_time_filter(&params.since, now) {
            Ok(t) => Some(t),
            Err(msg) => {
                return response::error(StatusCode::BAD_REQUEST, "invalid_since", &msg)
            }
        }
    };
    let until_time: Option<SystemTime> = if params.until.is_empty() {
        None
    } else {
        match parse_pack_time_filter(&params.until, now) {
            Ok(t) => Some(t),
            Err(msg) => {
                return response::error(StatusCode::BAD_REQUEST, "invalid_until", &msg)
            }
        }
    };
    // use_mtime is accepted to avoid 400 but has no effect — filtering is
    // always by mtime in the web walk (no git-commit-time index is built).
    let _ = params.use_mtime;

    let walk_opts = WalkOpts {
        max_depth,
        with_tokens: params.tokens,
        git_status: &git_status,
        ignore: ignore.as_ref(),
        since: since_time,
        until: until_time,
    };

    let root_node = match walk_tree(&state.root, &target, 0, walk_opts) {
        Ok(Some(n)) => n,
        Ok(None) => {
            // Root is always a directory and therefore always retained.
            // Returning None here would be a logic error.
            return response::error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "walk_init",
                "root directory was unexpectedly filtered",
            );
        }
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
                "walk_init",
                &e.to_string(),
            );
        }
    };

    // total: count nodes in the flattened tree (files + dirs).
    let total = count_nodes(&root_node);

    // root field: relative path of target vs served root. Go uses filepath.ToSlash(relativeToRoot(a.Root, target)).
    // relativeToRoot strips abs_root prefix; for target==root this yields "".
    // Go then uses this as-is for the Root field — "." when the walk starts from
    // the served root (because os.ReadDir(".") → relativeToRoot(".", absRoot) = ".").
    // We need to convert "" → "." to match Go's "." output for the root.
    let root_rel_raw = relative_to_root(&state.root, &target);
    let root_rel = if root_rel_raw.is_empty() {
        ".".to_string()
    } else {
        root_rel_raw
    };

    // abs_root: absolute path of the served root (not of `target`). Go uses
    // filepath.Abs(a.Root) for this field.
    let abs_root = std::fs::canonicalize(&state.root)
        .or_else(|_| std::path::absolute(Path::new(&state.root)))
        .unwrap_or_else(|_| PathBuf::from(&state.root));
    let abs_root = abs_root.to_string_lossy().replace('\\', "/");

    response::json(
        StatusCode::OK,
        &TreeResponse {
            root: root_rel,
            abs_root,
            tree: root_node,
            total,
        },
    )
}

// ---------------------------------------------------------------------------
// Walk implementation — mirrors Go walk.DefaultOptions + toTreeNode
// ---------------------------------------------------------------------------

/// Options threaded through the recursive walk to avoid an argument-count
/// lint. All fields are cheap to copy.
#[derive(Clone, Copy)]
struct WalkOpts<'a> {
    max_depth: usize,
    with_tokens: bool,
    git_status: &'a GitStatusMap,
    ignore: Option<&'a ctx_gitignore::GitIgnore>,
    since: Option<SystemTime>,
    until: Option<SystemTime>,
}

/// Extra-ignore list from Go `walk.DefaultOptions().ExtraIgnore`.
/// NOTE: Do NOT skip hidden dirs generally — Go only skips what the gitignore
/// patterns and ExtraIgnore list cover. `.ctx` etc. are walked normally.
fn should_skip(name: &str) -> bool {
    // ExtraIgnore: ".git/", "node_modules/", "dist/", "coverage/", "target/"
    // In the gitignore library these match exactly the dir name.
    matches!(
        name,
        ".git" | "node_modules" | "dist" | "coverage" | "target"
    )
}

/// Walk the tree rooted at `dir`, building TreeNode children sorted
/// alphabetically (matching Go `os.ReadDir` which returns entries sorted by
/// filename). `root_str` is the served root used for relative-path computation.
///
/// Returns `Ok(None)` for non-directory files that fall outside the
/// `[since, until]` mtime window. When a time filter is active, a non-root
/// directory that ends up with no surviving descendants is also pruned
/// (`Ok(None)`) so the UI never shows an empty folder for a filter that
/// matched nothing inside it. The root and depth-cutoff directories are kept
/// (their matches may simply be unexplored). With no filter, every directory
/// is retained regardless of emptiness (behaviour-preserving).
fn walk_tree(
    root_str: &str,
    dir: &Path,
    depth: usize,
    opts: WalkOpts<'_>,
) -> std::io::Result<Option<TreeNode>> {
    let meta = std::fs::symlink_metadata(dir)?;
    // Follow symlinks only for the requested root; deeper symlinked dirs keep
    // their lstat metadata so the walk never recurses through them (cyclic
    // links would loop forever and absolute links would leak outside root).
    let meta = if depth == 0 {
        std::fs::metadata(dir).unwrap_or(meta)
    } else {
        meta
    };

    let rel_raw = relative_to_root(root_str, dir);
    // Convert "" (root) to "." to match Go relativeToRoot behaviour.
    let rel = if rel_raw.is_empty() {
        ".".to_string()
    } else {
        rel_raw
    };

    let name = if rel == "." {
        // Root node: basename of the root dir (mirrors Go `filepath.Base(root)`)
        Path::new(root_str)
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| ".".to_string())
    } else {
        dir.file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default()
    };

    let is_dir = meta.is_dir();
    let size = meta.len() as i64;

    // updated_at: mtime as Unix timestamp (mirrors Go fileTime fallback to mtime).
    let mtime: Option<SystemTime> = if !is_dir { meta.modified().ok() } else { None };
    let updated_at = mtime
        .and_then(|mt| mt.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    // Mtime filter: drop non-directory files outside [since, until].
    // Directory nodes are always retained (CLI parity).
    if !is_dir && (opts.since.is_some() || opts.until.is_some()) {
        if let Some(mt) = mtime {
            if let Some(s) = opts.since {
                if mt < s {
                    return Ok(None);
                }
            }
            if let Some(u) = opts.until {
                if mt > u {
                    return Ok(None);
                }
            }
        }
    }

    let (lines, tokens) = if !is_dir {
        let l = count_lines_file(dir);
        let tok = if opts.with_tokens {
            cached_file_tokens(dir, &meta, size)
        } else {
            0
        };
        (l, tok)
    } else {
        (0, 0)
    };

    let role = if !is_dir {
        infer_role(&rel)
    } else {
        String::new()
    };

    let git = opts.git_status.status_for(&rel, is_dir);
    let mut node = TreeNode {
        path: rel,
        name,
        is_dir,
        size,
        lines,
        tokens,
        role,
        git,
        updated_at,
        children: Vec::new(),
    };

    // Recurse into directory children (sorted; mirrors Go `os.ReadDir`).
    if is_dir && (opts.max_depth == 0 || depth < opts.max_depth) {
        let mut entries: Vec<_> = match std::fs::read_dir(dir) {
            Ok(rd) => rd.flatten().collect(),
            Err(_) => vec![],
        };
        // Sort by file name to match Go `os.ReadDir` alphabetical order.
        entries.sort_by(|a, b| a.file_name().cmp(&b.file_name()));

        for entry in entries {
            let child_path = entry.path();
            let child_name = entry.file_name();
            let child_name_str = child_name.to_string_lossy();

            // Only skip directories that match the ExtraIgnore patterns — a
            // regular file named e.g. "dist" must stay visible.
            // NOTE: Do NOT skip all hidden entries — Go walks .ctx/, etc.
            let child_is_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if child_is_dir && should_skip(&child_name_str) {
                continue;
            }

            // Prune entries matched by the root `.gitignore` when active.
            // Directories are checked with a trailing "/" AND the bare path,
            // matching how the CLI walk applies `GitIgnore::matches_path`.
            if let Some(ig) = opts.ignore {
                let child_rel = relative_to_root(root_str, &child_path);
                if !child_rel.is_empty() {
                    let dir_form = format!("{child_rel}/");
                    if (child_is_dir && ig.matches_path(&dir_form)) || ig.matches_path(&child_rel) {
                        continue;
                    }
                }
            }

            match walk_tree(root_str, &child_path, depth + 1, opts) {
                Ok(Some(child_node)) => node.children.push(child_node),
                Ok(None) => continue, // filtered out (file out of window, or empty dir)
                Err(_) => continue,
            }
        }

        // Prune a directory that the filter emptied: no file or sub-directory
        // survived the [since, until] window, so the folder itself is hidden.
        // Root (depth 0) is always kept so an all-filtered tree still renders.
        // A dir cut off by max_depth never reaches here (the outer `if` is
        // false), so unexplored subtrees are never mistaken for empty.
        let filter_active = opts.since.is_some() || opts.until.is_some();
        if filter_active && depth > 0 && node.children.is_empty() {
            return Ok(None);
        }
    }

    Ok(Some(node))
}

/// Exact tiktoken token count for `path`, memoized across requests. `/api/tree`
/// recomputes the whole tree on every navigation, so without this each request
/// re-runs the BPE encoder over every file — by far the dominant cost of a
/// `tokens=true` tree. The cache is keyed by absolute path and validated by
/// (mtime, size); an edited file re-counts. Falls back to a size-based estimate
/// when the file cannot be read, matching the previous inline behaviour. Result
/// is identical to a fresh `ctx_tokens::count_file`, so JSON output is unchanged.
fn cached_file_tokens(path: &Path, meta: &std::fs::Metadata, size: i64) -> i32 {
    use std::collections::HashMap;
    use std::sync::{OnceLock, RwLock};
    use std::time::SystemTime;

    /// path → (mtime, size, token count). The first two validate the third.
    type TokenCache = HashMap<PathBuf, (SystemTime, u64, i32)>;
    /// Bounds memory during long browse sessions over large trees.
    const TOKEN_CACHE_CAP: usize = 1 << 16;
    static CACHE: OnceLock<RwLock<TokenCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| RwLock::new(HashMap::new()));

    let fingerprint = meta.modified().ok().map(|mtime| (mtime, meta.len()));
    if let Some((mtime, len)) = fingerprint {
        if let Ok(guard) = cache.read() {
            if let Some(&(cm, cl, tok)) = guard.get(path) {
                if cm == mtime && cl == len {
                    return tok;
                }
            }
        }
    }

    let tokens = match ctx_tokens::count_file(path.to_str().unwrap_or("")) {
        Ok(n) => n as i32,
        Err(_) => ctx_tokens::estimate_by_size(size) as i32,
    };

    if let Some((mtime, len)) = fingerprint {
        if let Ok(mut guard) = cache.write() {
            if guard.len() < TOKEN_CACHE_CAP || guard.contains_key(path) {
                guard.insert(path.to_path_buf(), (mtime, len, tokens));
            }
        }
    }
    tokens
}

#[derive(Default)]
struct GitStatusMap {
    by_path: BTreeMap<String, String>,
}

impl GitStatusMap {
    fn load(root: &str) -> Self {
        let output = Command::new("git")
            .args(["-C", root, "status", "--porcelain"])
            .output();
        let Ok(output) = output else {
            return Self::default();
        };
        if !output.status.success() {
            return Self::default();
        }

        let mut by_path = BTreeMap::new();
        let stdout = String::from_utf8_lossy(&output.stdout);
        for line in stdout.lines() {
            if line.len() < 4 {
                continue;
            }
            let code = normalize_git_status(&line[..2]);
            if code.is_empty() {
                continue;
            }
            let raw_path = &line[3..];
            let path = normalize_git_status_path(raw_path);
            if !path.is_empty() {
                by_path.insert(path, code);
            }
        }
        Self { by_path }
    }

    fn status_for(&self, rel: &str, is_dir: bool) -> String {
        if !is_dir {
            if let Some(s) = self.by_path.get(rel) {
                return s.clone();
            }
            // `git status --porcelain` collapses a wholly-untracked directory to
            // a single "<dir>/" entry and omits the files inside it. Those files
            // are still walked from disk, so without this they'd carry no status
            // and the changed-only tree filter would hide them. Inherit the
            // nearest untracked-ancestor's status so they surface.
            return self.untracked_ancestor_status(rel);
        }
        self.aggregate_dir(rel)
    }

    /// Status of the nearest ancestor directory recorded as untracked
    /// (`"<dir>/"` in porcelain output), or empty if none. Walks ancestor
    /// prefixes shallow→deep so the closest match wins.
    fn untracked_ancestor_status(&self, rel: &str) -> String {
        let mut idx = 0;
        while let Some(pos) = rel[idx..].find('/') {
            let end = idx + pos;
            if let Some(status) = self.by_path.get(&rel[..=end]) {
                return status.clone();
            }
            idx = end + 1;
        }
        String::new()
    }

    fn aggregate_dir(&self, rel: &str) -> String {
        let prefix = if rel == "." {
            String::new()
        } else {
            format!("{rel}/")
        };
        let mut best = "";
        let mut best_rank = 0;
        // BTreeMap is key-sorted, so all paths under the prefix form one
        // contiguous range — no need to scan unrelated dirty files.
        for (_, status) in self
            .by_path
            .range(prefix.clone()..)
            .take_while(|(path, _)| path.starts_with(&prefix))
        {
            let rank = git_status_rank(status);
            if rank > best_rank {
                best = status;
                best_rank = rank;
            }
        }
        best.to_string()
    }
}

pub(crate) fn normalize_git_status(status: &str) -> String {
    if status == "??" {
        return "?".to_string();
    }
    for ch in status.chars() {
        if matches!(ch, 'D' | 'A' | 'M' | 'R' | 'C' | 'T') {
            return ch.to_string();
        }
    }
    String::new()
}

pub(crate) fn normalize_git_status_path(raw: &str) -> String {
    let mut path = raw.trim();
    if let Some((_, new_path)) = path.split_once(" -> ") {
        path = new_path;
    }
    path.trim_matches('"').replace('\\', "/")
}

fn git_status_rank(status: &str) -> i32 {
    match status {
        "D" => 60,
        "A" => 50,
        "M" => 40,
        "R" => 30,
        "C" => 20,
        "T" => 10,
        "?" => 5,
        _ => 0,
    }
}

fn count_nodes(node: &TreeNode) -> i32 {
    let mut n = 1i32;
    for child in &node.children {
        n += count_nodes(child);
    }
    n
}

/// Count newline-delimited lines in a file; mirrors Go `countTextStats`.
fn count_lines_file(path: &Path) -> i32 {
    let data = match std::fs::read(path) {
        Ok(d) => d,
        Err(_) => return 0,
    };
    if data.is_empty() {
        return 0;
    }
    let mut n = data.iter().filter(|&&b| b == b'\n').count() as i32;
    if data.last() != Some(&b'\n') {
        n += 1;
    }
    n
}

/// `inferRole` — mirrors Go walk/walk.go `inferRole`.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn git_status_path_normalizes_rename_target() {
        assert_eq!(
            normalize_git_status_path("old/name.rs -> src/name.rs"),
            "src/name.rs"
        );
        assert_eq!(
            normalize_git_status_path("\"web/src/App.svelte\""),
            "web/src/App.svelte"
        );
    }

    #[test]
    fn git_status_code_normalizes_porcelain_status() {
        assert_eq!(normalize_git_status("??"), "?");
        assert_eq!(normalize_git_status(" M"), "M");
        assert_eq!(normalize_git_status("M "), "M");
        assert_eq!(normalize_git_status("A "), "A");
        assert_eq!(normalize_git_status(" D"), "D");
        assert_eq!(normalize_git_status("R "), "R");
        assert_eq!(normalize_git_status("  "), "");
    }

    #[test]
    fn git_status_map_direct_and_directory_aggregate() {
        let status = GitStatusMap {
            by_path: BTreeMap::from([
                ("crates/ctx-web/src/lib.rs".to_string(), "M".to_string()),
                ("web/src/App.svelte".to_string(), "A".to_string()),
                ("notes.txt".to_string(), "?".to_string()),
            ]),
        };

        assert_eq!(status.status_for("crates/ctx-web/src/lib.rs", false), "M");
        assert_eq!(status.status_for("crates/ctx-web", true), "M");
        assert_eq!(status.status_for("web", true), "A");
        assert_eq!(status.status_for(".", true), "A");
        assert_eq!(status.status_for("README.md", false), "");
    }

    // -----------------------------------------------------------------------
    // Time-filter parser tests
    // -----------------------------------------------------------------------

    #[test]
    fn time_filter_parser_relative_units() {
        let now = SystemTime::now();
        // 7d → 7 × 24 × 3600 seconds before now
        let t = parse_pack_time_filter("7d", now).unwrap();
        let expected = now.checked_sub(Duration::from_secs(7 * 24 * 3600)).unwrap();
        assert_eq!(t, expected);

        // 2w → 2 × 7 × 24 × 3600
        let t2 = parse_pack_time_filter("2w", now).unwrap();
        let expected2 = now.checked_sub(Duration::from_secs(2 * 7 * 24 * 3600)).unwrap();
        assert_eq!(t2, expected2);

        // 1mo → 30 × 24 × 3600
        let t3 = parse_pack_time_filter("1mo", now).unwrap();
        let expected3 = now.checked_sub(Duration::from_secs(30 * 24 * 3600)).unwrap();
        assert_eq!(t3, expected3);
    }

    #[test]
    fn time_filter_parser_absolute_date() {
        let now = SystemTime::now();
        let t = parse_pack_time_filter("2024-01-15", now).unwrap();
        // 2024-01-15 UTC midnight
        let days = days_from_civil(2024, 1, 15);
        let expected = UNIX_EPOCH + Duration::from_secs(days as u64 * 24 * 3600);
        assert_eq!(t, expected);
    }

    #[test]
    fn time_filter_parser_invalid_returns_err() {
        let now = SystemTime::now();
        assert!(parse_pack_time_filter("notadate", now).is_err());
        assert!(parse_pack_time_filter("", now).is_err());
        assert!(parse_pack_time_filter("0d", now).is_err());
        assert!(parse_pack_time_filter("-1d", now).is_err());
    }

    // -----------------------------------------------------------------------
    // Walk mtime filter tests
    // -----------------------------------------------------------------------

    /// Create a temp dir with two files and assert that `since = now` excludes
    /// them (they were created just before `now + epsilon`, so mtime < since).
    /// Without a way to set mtime portably in std, we use two windows:
    ///   - since = far future → both files excluded
    ///   - until = far past  → both files excluded
    ///   - no filter         → both files present (behaviour-preserving baseline)
    #[test]
    fn walk_tree_mtime_filter_excludes_old_files() {
        use std::fs;

        // Build a temp dir: <tmp>/ctx_tree_test_<pid>/
        let tmp = std::env::temp_dir().join(format!(
            "ctx_tree_mtime_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("a.txt"), b"hello").unwrap();
        fs::write(tmp.join("b.txt"), b"world").unwrap();

        let git = GitStatusMap::default();

        // Baseline: no filter → both files present.
        let node = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 0, with_tokens: false, git_status: &git, ignore: None, since: None, until: None },
        ).unwrap().unwrap();
        let names: Vec<&str> = node.children.iter().map(|c| c.name.as_str()).collect();
        assert!(names.contains(&"a.txt"), "baseline: a.txt must be present");
        assert!(names.contains(&"b.txt"), "baseline: b.txt must be present");

        // since = far future → all files excluded (mtime < since).
        let far_future = SystemTime::now()
            .checked_add(Duration::from_secs(365 * 24 * 3600))
            .unwrap();
        let node_ff = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 0, with_tokens: false, git_status: &git, ignore: None, since: Some(far_future), until: None },
        ).unwrap().unwrap();
        assert!(
            node_ff.children.is_empty(),
            "since=far_future must exclude all file children; got {:?}",
            node_ff.children.iter().map(|c| &c.name).collect::<Vec<_>>()
        );
        // Directory itself is retained (CLI parity).
        assert!(node_ff.is_dir);

        // until = far past → all files excluded (mtime > until).
        let far_past = UNIX_EPOCH + Duration::from_secs(1);
        let node_fp = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 0, with_tokens: false, git_status: &git, ignore: None, since: None, until: Some(far_past) },
        ).unwrap().unwrap();
        assert!(
            node_fp.children.is_empty(),
            "until=far_past must exclude all file children"
        );
        assert!(node_fp.is_dir);

        let _ = fs::remove_dir_all(&tmp);
    }

    /// `since = now - 1s` keeps recently-created files, `since = now + 1s`
    /// drops them. This test verifies that the comparison direction is correct.
    #[test]
    fn walk_tree_mtime_filter_keeps_recent_files() {
        use std::fs;

        let tmp = std::env::temp_dir().join(format!(
            "ctx_tree_recent_{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(&tmp).unwrap();
        fs::write(tmp.join("new.txt"), b"new").unwrap();

        let git = GitStatusMap::default();
        let now = SystemTime::now();

        // since = 1 second ago → file (just created) should be included.
        let since_past = now.checked_sub(Duration::from_secs(1)).unwrap();
        let node = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 0, with_tokens: false, git_status: &git, ignore: None, since: Some(since_past), until: None },
        ).unwrap().unwrap();
        let names: Vec<&str> = node.children.iter().map(|c| c.name.as_str()).collect();
        assert!(
            names.contains(&"new.txt"),
            "since=1s ago must keep a just-created file; got {:?}", names
        );

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A sub-directory whose only files fall outside the window is pruned from
    /// the tree entirely (folder hidden), but a sibling sub-directory with a
    /// surviving file is kept. The root is always retained.
    #[test]
    fn walk_tree_filter_prunes_emptied_directories() {
        use std::fs;

        let tmp = std::env::temp_dir().join(format!("ctx_tree_prune_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("keep")).unwrap();
        fs::create_dir_all(tmp.join("drop")).unwrap();
        fs::write(tmp.join("keep/a.txt"), b"a").unwrap();
        fs::write(tmp.join("drop/b.txt"), b"b").unwrap();

        let git = GitStatusMap::default();
        let now = SystemTime::now();

        // No filter → both dirs present (behaviour-preserving baseline).
        let base = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 0, with_tokens: false, git_status: &git, ignore: None, since: None, until: None },
        ).unwrap().unwrap();
        let base_dirs: Vec<&str> = base.children.iter().map(|c| c.name.as_str()).collect();
        assert!(base_dirs.contains(&"keep") && base_dirs.contains(&"drop"), "baseline keeps both dirs; got {base_dirs:?}");

        // since = far future → every file is out of window → BOTH dirs pruned,
        // root retained but empty.
        let far_future = now.checked_add(Duration::from_secs(365 * 24 * 3600)).unwrap();
        let all_pruned = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 0, with_tokens: false, git_status: &git, ignore: None, since: Some(far_future), until: None },
        ).unwrap().unwrap();
        assert!(all_pruned.is_dir, "root must always be retained");
        assert!(
            all_pruned.children.is_empty(),
            "dirs whose files are all filtered out must be pruned; got {:?}",
            all_pruned.children.iter().map(|c| &c.name).collect::<Vec<_>>()
        );

        // since = 1s ago → both files survive → both dirs kept.
        let recent = now.checked_sub(Duration::from_secs(1)).unwrap();
        let kept = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 0, with_tokens: false, git_status: &git, ignore: None, since: Some(recent), until: None },
        ).unwrap().unwrap();
        let kept_dirs: Vec<&str> = kept.children.iter().map(|c| c.name.as_str()).collect();
        assert!(kept_dirs.contains(&"keep") && kept_dirs.contains(&"drop"), "dirs with surviving files are kept; got {kept_dirs:?}");

        let _ = fs::remove_dir_all(&tmp);
    }

    /// A directory cut off by `max_depth` is NOT pruned even under an active
    /// filter — its contents are unexplored, not empty.
    #[test]
    fn walk_tree_filter_keeps_depth_cutoff_directories() {
        use std::fs;

        let tmp = std::env::temp_dir().join(format!("ctx_tree_depth_{}", std::process::id()));
        let _ = fs::remove_dir_all(&tmp);
        fs::create_dir_all(tmp.join("sub")).unwrap();
        fs::write(tmp.join("sub/deep.txt"), b"x").unwrap();

        let git = GitStatusMap::default();
        // max_depth = 1: root walks `sub` (depth 1) but `sub` does NOT walk its
        // children. With a far-future filter, `sub` has empty children due to
        // the cutoff — it must still be retained.
        let far_future = SystemTime::now().checked_add(Duration::from_secs(365 * 24 * 3600)).unwrap();
        let node = walk_tree(
            tmp.to_str().unwrap(), &tmp, 0,
            WalkOpts { max_depth: 1, with_tokens: false, git_status: &git, ignore: None, since: Some(far_future), until: None },
        ).unwrap().unwrap();
        let dirs: Vec<&str> = node.children.iter().map(|c| c.name.as_str()).collect();
        assert!(dirs.contains(&"sub"), "depth-cutoff dir must be retained; got {dirs:?}");

        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn git_status_propagates_untracked_dir_to_contained_files() {
        // `git status --porcelain` collapses a wholly-untracked directory to a
        // single "notes/" entry; the files inside it are never listed. Without
        // ancestor propagation those files get no status and the changed-only
        // tree filter hides them (the reported bug).
        let status = GitStatusMap {
            by_path: BTreeMap::from([
                ("notes/".to_string(), "?".to_string()),
                ("docs/guide/intro.md".to_string(), "M".to_string()),
            ]),
        };

        // Files inside the untracked dir inherit "?" (any depth).
        assert_eq!(status.status_for("notes/todo.md", false), "?");
        assert_eq!(status.status_for("notes/sub/deep.md", false), "?");
        // The untracked dir node itself still aggregates to "?".
        assert_eq!(status.status_for("notes", true), "?");
        // A tracked, individually-listed file keeps its own status.
        assert_eq!(status.status_for("docs/guide/intro.md", false), "M");
        // A file with no status and no untracked ancestor stays empty.
        assert_eq!(status.status_for("docs/guide/other.md", false), "");
        assert_eq!(status.status_for("README.md", false), "");
    }
}
