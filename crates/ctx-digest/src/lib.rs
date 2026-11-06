//! Repository activity digest shared by `ctx digest` and the MCP
//! `ctx_digest` tool: commits, authors, per-file churn and the net
//! token / symbol delta over a time window, plus Markdown / plain / JSON
//! renderers.
//!
//! The digest is computed from `git log` and `git show`. Symbol counting is
//! injected by the caller ([`DigestOptions::count_symbols`]) so the CLI keeps
//! its byte-parity extractor while other front ends can use their own.

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::process::Command;

use serde::Serialize;

/// Maximum number of files for which token/symbol deltas are computed.
/// Mirrors Go's `maxDeltaFiles = 100`.
pub const MAX_DELTA_FILES: usize = 100;

/// Counts the named symbols in `content` (the file at `path`).
pub type SymbolCounter = fn(path: &str, content: &str) -> i64;

/// Inputs for [`generate`].
#[derive(Clone, Copy)]
pub struct DigestOptions<'a> {
    /// `None` runs git in the current directory over the whole repository
    /// (the historical `ctx digest` behaviour). `Some(root)` runs git in
    /// `root` and scopes the digest to it: only changes under `root` count
    /// and every reported path is relative to `root`.
    pub root: Option<&'a Path>,
    /// Lookback window such as `7d`, `2w`, `1mo`, `1y`, `48h`.
    pub since: &'a str,
    /// Max number of hot files to keep (0 keeps all).
    pub top: usize,
    pub count_symbols: SymbolCounter,
}

#[derive(Debug, Clone, Serialize)]
pub struct DigestBrief {
    pub period: DigestPeriod,
    pub commits: usize,
    pub authors: usize,
    pub files_changed: usize,
    pub files_added: usize,
    pub files_deleted: usize,
    pub token_delta: i64,
    pub symbol_delta: i64,
    pub hot_files: Vec<DigestHotFile>,
}

#[derive(Debug, Clone, Serialize)]
pub struct DigestPeriod {
    pub since: String,
    #[serde(skip)]
    pub since_date: String,
    pub until: String,
    #[serde(skip)]
    pub until_date: String,
    pub duration: String,
    pub since_ref: String,
    pub head_ref: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DigestHotFile {
    pub path: String,
    pub commits: usize,
    pub token_delta: i64,
    pub symbol_delta: i64,
}

/// Runs git (in `root` when given) and returns trimmed stdout.
fn git(root: Option<&Path>, args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    if let Some(root) = root {
        cmd.arg("-C").arg(root);
    }
    let output = cmd.args(args).output().map_err(|err| err.to_string())?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().to_string());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

/// Builds the digest described by `opts`.
pub fn generate(opts: DigestOptions<'_>) -> Result<DigestBrief, String> {
    let since = opts.since;
    let since_arg = git_since_arg(since)?;
    let since_date = since_date(since);
    let until_date = ctx_pack::assemble::current_date_utc();
    let until_rfc3339 = ctx_pack::assemble::current_rfc3339_utc();
    let head = git(opts.root, &["rev-parse", "HEAD"]).unwrap_or_else(|_| "N/A".to_string());
    let head_ref = head.trim().to_string();

    // One header per commit (hash, email, parents), plus name-status
    // records. `-z` keeps paths unquoted (non-ASCII, `"`, `\`) so they can be
    // fed back to `git show`, and `--no-renames` reports a rename as a delete
    // of the old path plus an add of the new one. A scoped digest adds
    // `--relative`, which drops changes outside `root` and names paths from
    // it.
    let since_flag = format!("--since={since_arg}");
    let mut log_args = vec![
        "log",
        since_flag.as_str(),
        "-z",
        "--no-renames",
        "--name-status",
        "--format=commit:%H%x1f%ae%x1f%P",
    ];
    // Only a root below the top level needs scoping; at the top level the
    // plain log keeps merge commits (which list no paths) in the count.
    let scoped = opts.root.is_some()
        && git(opts.root, &["rev-parse", "--show-prefix"]).is_ok_and(|prefix| !prefix.is_empty());
    if scoped {
        log_args.push("--relative");
    }
    let log = git(opts.root, &log_args).map_err(|err| format!("digest: git log failed: {err}"))?;
    let log = parse_log(&log, scoped);

    let files_added = log.files.values().filter(|(_, added, _)| *added).count();
    let files_deleted = log
        .files
        .values()
        .filter(|(_, _, deleted)| *deleted)
        .count();
    let files_changed = log.files.len();

    // since_ref: parent of the oldest commit in the period (or the commit
    // itself if it is the root commit). Mirrors Go's SinceRef logic.
    let since_ref = if log.oldest_hash.is_empty() {
        head_ref.clone()
    } else {
        git(opts.root, &["rev-parse", &format!("{}^", log.oldest_hash)])
            .unwrap_or_else(|_| log.oldest_hash.trim().to_string())
            .trim()
            .to_string()
    };

    let mut all_paths: Vec<_> = log.files.keys().cloned().collect();
    all_paths.sort();

    let mut total_token_delta: i64 = 0;
    let mut total_symbol_delta: i64 = 0;
    let mut path_deltas: HashMap<String, (i64, i64)> = HashMap::new();
    let can_compute_delta = since_ref != "N/A" && !since_ref.is_empty();
    if can_compute_delta {
        // Each path's delta needs two `git show` processes, which dominate
        // the time and are independent. Start a window of them at once and
        // collect their output in path order: the processes overlap, while
        // this thread still reads, counts and parses one file at a time.
        let delta_paths = &all_paths[..all_paths.len().min(MAX_DELTA_FILES)];
        let window = std::thread::available_parallelism().map_or(1, usize::from);
        let mut deltas: Vec<(i64, i64)> = Vec::with_capacity(delta_paths.len());
        for chunk in delta_paths.chunks(window) {
            let running: Vec<_> = chunk
                .iter()
                .map(|path| {
                    (
                        spawn_show(opts.root, &since_ref, path),
                        spawn_show(opts.root, &head_ref, path),
                    )
                })
                .collect();
            for (path, (old_show, new_show)) in chunk.iter().zip(running) {
                let old_content =
                    finish_show(old_show, opts.root, &since_ref, path).unwrap_or_default();
                let new_content =
                    finish_show(new_show, opts.root, &head_ref, path).unwrap_or_default();
                let td = ctx_tokens::count_str(&new_content) - ctx_tokens::count_str(&old_content);
                let sd = count(opts.count_symbols, path, &new_content)
                    - count(opts.count_symbols, path, &old_content);
                deltas.push((td, sd));
            }
        }
        for (path, (td, sd)) in delta_paths.iter().zip(deltas) {
            total_token_delta += td;
            total_symbol_delta += sd;
            path_deltas.insert(path.clone(), (td, sd));
        }
    }

    let mut hot_files: Vec<_> = log
        .files
        .into_iter()
        .map(|(path, (commits, _, _))| {
            let (td, sd) = path_deltas.get(&path).copied().unwrap_or((0, 0));
            DigestHotFile {
                path,
                commits,
                token_delta: td,
                symbol_delta: sd,
            }
        })
        .collect();
    hot_files.sort_by(|a, b| b.commits.cmp(&a.commits).then_with(|| a.path.cmp(&b.path)));
    if opts.top > 0 && hot_files.len() > opts.top {
        hot_files.truncate(opts.top);
    }

    Ok(DigestBrief {
        period: DigestPeriod {
            since: format!("{since_date}T00:00:00Z"),
            since_date,
            until: until_rfc3339,
            until_date,
            duration: since.to_string(),
            since_ref,
            head_ref,
        },
        commits: log.commits,
        authors: log.authors.len(),
        files_changed,
        files_added,
        files_deleted,
        token_delta: total_token_delta,
        symbol_delta: total_symbol_delta,
        hot_files,
    })
}

fn count(counter: SymbolCounter, path: &str, content: &str) -> i64 {
    if content.is_empty() {
        0
    } else {
        counter(path, content)
    }
}

#[derive(Debug, Default)]
pub struct DigestLog {
    pub commits: usize,
    pub authors: BTreeSet<String>,
    /// path → (commit_count, is_added, is_deleted)
    pub files: BTreeMap<String, (usize, bool, bool)>,
    pub oldest_hash: String,
}

/// Parses `git log -z --no-renames --name-status
/// --format=commit:%H%x1f%ae%x1f%P` output. Records are NUL-separated: a
/// `commit:` header, then `STATUS`, `path` pairs (the first status of a
/// commit is preceded by a newline).
///
/// With `skip_empty_commits` (scoped `--relative` logs), commits that touch
/// no path under the root are not counted: `git log --relative` still lists
/// their headers.
pub fn parse_log(raw: &str, skip_empty_commits: bool) -> DigestLog {
    let mut log = DigestLog::default();
    let mut current_is_root = false;
    let mut pending: Option<(String, String)> = None;
    let mut tokens = raw.split('\0');
    let mut commit_files = 0usize;

    fn close(
        log: &mut DigestLog,
        pending: &mut Option<(String, String)>,
        files: usize,
        skip_empty: bool,
    ) {
        if let Some((hash, email)) = pending.take() {
            if skip_empty && files == 0 {
                return;
            }
            log.commits += 1;
            // git log prints newest-first, so the last seen hash is oldest.
            log.oldest_hash = hash;
            if !email.is_empty() {
                log.authors.insert(email);
            }
        }
    }

    while let Some(token) = tokens.next() {
        let token = token.trim_start_matches('\n');
        if let Some(rest) = token.strip_prefix("commit:") {
            close(&mut log, &mut pending, commit_files, skip_empty_commits);
            commit_files = 0;
            let mut parts = rest.splitn(3, '\x1f');
            let hash = parts.next().unwrap_or("").trim().to_string();
            let email = parts.next().unwrap_or("").trim().to_string();
            let parents = parts.next().unwrap_or("").trim();
            current_is_root = parents.is_empty();
            pending = Some((hash, email));
            continue;
        }
        let status = token;
        if status.is_empty() {
            continue;
        }
        // The path always follows its status, so a file named `commit:…`
        // is never mistaken for a header.
        let Some(path) = tokens.next() else {
            break;
        };
        if path.is_empty() {
            continue;
        }
        commit_files += 1;
        let entry = log
            .files
            .entry(path.to_string())
            .or_insert((0, false, false));
        entry.0 += 1;
        // For root commits, Go iterates c.Files() without marking
        // added/deleted; only non-root commits set added.
        if !current_is_root && status.starts_with('A') {
            entry.1 = true;
        }
        if status.starts_with('D') {
            entry.2 = true;
        }
    }
    close(&mut log, &mut pending, commit_files, skip_empty_commits);
    log
}

/// Starts `git show git_ref:path` without waiting for it, so several can run
/// at once; [`finish_show`] collects the content. A scoped digest names paths
/// from `root`, so they are passed as `./path` (cwd-relative tree paths).
/// `None` if the process could not be started.
fn spawn_show(root: Option<&Path>, git_ref: &str, path: &str) -> Option<std::process::Child> {
    use std::process::Stdio;
    let spec = if root.is_some() {
        format!("{git_ref}:./{path}")
    } else {
        format!("{git_ref}:{path}")
    };
    let mut cmd = Command::new("git");
    if let Some(root) = root {
        cmd.arg("-C").arg(root);
    }
    // Only stdout is read; stderr is discarded, as it always was.
    cmd.args(["show", &spec])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()
}

/// File content at `git_ref:path` from a [`spawn_show`], or `None` when the
/// file is absent at that ref or is not UTF-8 (binary). A process that could
/// not be started (say, while a window of others is running) is retried on
/// its own, so starting several at once never changes the result.
fn finish_show(
    child: Option<std::process::Child>,
    root: Option<&Path>,
    git_ref: &str,
    path: &str,
) -> Option<String> {
    let child = match child {
        Some(child) => child,
        None => spawn_show(root, git_ref, path)?,
    };
    let output = child.wait_with_output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
}

const UNITS: [(&str, &str); 5] = [
    ("mo", "months"),
    ("w", "weeks"),
    ("d", "days"),
    ("y", "years"),
    ("h", "hours"),
];

/// The YYYY-MM-DD date `since` ago (UTC), computed with `date -u` (GNU, then
/// BSD syntax). Falls back to `1970-01-01`.
pub fn since_date(since: &str) -> String {
    let lower = since.to_ascii_lowercase();
    for (suffix, unit) in UNITS {
        let Some(num) = lower.strip_suffix(suffix) else {
            continue;
        };
        if num.is_empty() || !num.chars().all(|ch| ch.is_ascii_digit()) {
            continue;
        }
        let gnu = Command::new("date")
            .args(["-u", &format!("--date={num} {unit} ago"), "+%Y-%m-%d"])
            .output();
        if let Ok(out) = gnu {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim().to_string();
            }
        }
        let unit_char = match suffix {
            "mo" => "m",
            "h" => "H",
            other => other,
        };
        let mac = Command::new("date")
            .args(["-u", &format!("-v-{num}{unit_char}"), "+%Y-%m-%d"])
            .output();
        if let Ok(out) = mac {
            if out.status.success() {
                return String::from_utf8_lossy(&out.stdout).trim().to_string();
            }
        }
    }
    "1970-01-01".to_string()
}

/// `since` as a git approxidate (`7d` → `7 days ago`).
pub fn git_since_arg(since: &str) -> Result<String, String> {
    if since.is_empty() {
        return Err("parsing duration \"\": empty string".to_string());
    }
    let lower = since.to_ascii_lowercase();
    for (suffix, unit) in UNITS {
        if let Some(num) = lower.strip_suffix(suffix) {
            if num.is_empty() || !num.chars().all(|ch| ch.is_ascii_digit()) {
                return Err(format!("parsing duration {since:?}: invalid numeric part"));
            }
            return Ok(format!("{num} {unit} ago"));
        }
    }
    Ok(format!("{since} ago"))
}

/// Formats an integer with an explicit `+` for non-negative values.
pub fn signed_fmt(n: i64) -> String {
    if n >= 0 {
        format!("+{n}")
    } else {
        format!("{n}")
    }
}

/// Pretty JSON with a trailing newline.
pub fn render_json(b: &DigestBrief) -> Result<String, String> {
    let mut s = serde_json::to_string_pretty(b).map_err(|err| err.to_string())?;
    s.push('\n');
    Ok(s)
}

pub fn render_markdown(b: &DigestBrief) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "# Digest \u{2014} {} \u{2192} {} ({})\n\n",
        b.period.since_date, b.period.until_date, b.period.duration
    ));
    out.push_str(&format!(
        "- Commits: {} by {} author(s)\n",
        b.commits, b.authors
    ));
    out.push_str(&format!(
        "- Files changed: {} (added: {}, deleted: {})\n",
        b.files_changed, b.files_added, b.files_deleted
    ));
    out.push_str(&format!(
        "- Token delta: {} tokens (net)\n",
        signed_fmt(b.token_delta)
    ));
    out.push_str(&format!(
        "- Symbol delta: {} symbols (net)\n",
        signed_fmt(b.symbol_delta)
    ));
    if !b.hot_files.is_empty() {
        out.push_str("\n## Hot files\n\n");
        out.push_str("| # | Commits | Token \u{0394} | Symbol \u{0394} | Path |\n");
        out.push_str("|---|---|---|---|---|\n");
        for (idx, hf) in b.hot_files.iter().enumerate() {
            out.push_str(&format!(
                "| {} | {} | {} | {} | {} |\n",
                idx + 1,
                hf.commits,
                signed_fmt(hf.token_delta),
                signed_fmt(hf.symbol_delta),
                hf.path
            ));
        }
        render_next_reads(&mut out, &b.hot_files);
    }
    out
}

/// "Suggested next reads" for up to 3 hot files (Go's writeNextReads).
pub fn render_next_reads(out: &mut String, hot: &[DigestHotFile]) {
    if hot.is_empty() {
        return;
    }
    const MAX_ROWS: usize = 3;
    out.push_str("\n## Suggested next reads\n\n");
    for hf in &hot[..MAX_ROWS.min(hot.len())] {
        let quoted = serde_json::to_string(&hf.path).unwrap_or_else(|_| format!("{:?}", hf.path));
        if hf.symbol_delta != 0 {
            out.push_str(&format!(
                "- `ctx_focus {{\"anchor\":{quoted}}}` \u{2014} {} (symbols \u{0394} {})\n",
                hf.path,
                signed_fmt(hf.symbol_delta),
            ));
        } else {
            out.push_str(&format!(
                "- `ctx_skim {{\"path\":{quoted}}}` \u{2014} {} (tokens \u{0394} {})\n",
                hf.path,
                signed_fmt(hf.token_delta),
            ));
        }
    }
}

pub fn render_plain(b: &DigestBrief) -> String {
    let mut out = String::new();
    out.push_str(&format!(
        "Digest: {} \u{2192} {} ({})\n",
        b.period.since_date, b.period.until_date, b.period.duration
    ));
    out.push_str(&format!(
        "Commits: {} by {} authors\n",
        b.commits, b.authors
    ));
    out.push_str(&format!(
        "Files: {} changed (+{} added, -{} deleted)\n",
        b.files_changed, b.files_added, b.files_deleted
    ));
    out.push_str(&format!("Tokens: {} net\n", signed_fmt(b.token_delta)));
    out.push_str(&format!("Symbols: {} net\n", signed_fmt(b.symbol_delta)));
    if !b.hot_files.is_empty() {
        out.push_str(&format!("\nHot files (top {}):\n", b.hot_files.len()));
        for (idx, hf) in b.hot_files.iter().enumerate() {
            let padded = if hf.path.len() < 40 {
                format!("{}{}", hf.path, " ".repeat(40 - hf.path.len()))
            } else {
                hf.path.clone()
            };
            out.push_str(&format!(
                "  {}. {}  commits={:<3}  tokens={:<6}  symbols={}\n",
                idx + 1,
                padded,
                hf.commits,
                signed_fmt(hf.token_delta),
                signed_fmt(hf.symbol_delta),
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_log_counts_commits_files_and_authors() {
        let raw = "commit:bbb\x1fa@x\x1faaa\0\nM\0src/a.rs\0A\0src/b.rs\0commit:aaa\x1fb@x\x1f\0\nA\0src/a.rs\0";
        let log = parse_log(raw, false);
        assert_eq!(log.commits, 2);
        assert_eq!(log.authors.len(), 2);
        assert_eq!(log.oldest_hash, "aaa");
        assert_eq!(log.files["src/a.rs"], (2, false, false));
        assert_eq!(log.files["src/b.rs"], (1, true, false));
    }

    #[test]
    fn scoped_parse_log_skips_commits_without_paths_under_root() {
        // `git log --relative` keeps headers of commits outside the root.
        let raw = "commit:ccc\x1fa@x\x1fbbb\0commit:bbb\x1fb@x\x1faaa\0\nM\0a.rs\0";
        let scoped = parse_log(raw, true);
        assert_eq!(scoped.commits, 1);
        assert_eq!(scoped.authors.len(), 1);
        assert_eq!(scoped.oldest_hash, "bbb");
        let whole = parse_log(raw, false);
        assert_eq!(whole.commits, 2);
    }

    #[test]
    fn git_since_arg_maps_units() {
        assert_eq!(git_since_arg("7d").unwrap(), "7 days ago");
        assert_eq!(git_since_arg("1mo").unwrap(), "1 months ago");
        assert!(git_since_arg("xd").is_err());
        assert!(git_since_arg("").is_err());
    }
}
