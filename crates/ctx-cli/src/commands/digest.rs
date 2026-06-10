use std::ffi::{OsStr, OsString};
use std::process::{Command, ExitCode};

use crate::commands::where_cmd::extract_where_symbols;
use crate::common::*;
use serde::Serialize;

#[derive(Debug)]
pub(crate) struct DigestArgs {
    since: String,
    top: usize,
    out: String,
    format: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DigestBrief {
    pub(crate) period: DigestPeriod,
    pub(crate) commits: usize,
    pub(crate) authors: usize,
    pub(crate) files_changed: usize,
    pub(crate) files_added: usize,
    pub(crate) files_deleted: usize,
    pub(crate) token_delta: i64,
    pub(crate) symbol_delta: i64,
    pub(crate) hot_files: Vec<DigestHotFile>,
}

#[derive(Debug, Serialize)]
pub(crate) struct DigestPeriod {
    since: String,
    #[serde(skip)]
    since_date: String,
    until: String,
    #[serde(skip)]
    until_date: String,
    duration: String,
    since_ref: String,
    head_ref: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct DigestHotFile {
    pub(crate) path: String,
    pub(crate) commits: usize,
    pub(crate) token_delta: i64,
    pub(crate) symbol_delta: i64,
}

pub(crate) fn run_digest_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_digest_args(args)?;
    match digest_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_digest_args(args: &[OsString]) -> Option<DigestArgs> {
    let mut saw_digest = false;
    let mut since = "7d".to_string();
    let mut top = 10usize;
    let mut out = String::new();
    let mut format = "markdown".to_string();
    let mut json = false;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("digest") {
            if saw_digest {
                return None;
            }
            saw_digest = true;
        } else if let Some(value) = flag_value(arg, "--since") {
            since = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--since") {
            i += 1;
            since = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--top") {
            top = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--top") {
            i += 1;
            top = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--out") {
            out = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--out") {
            i += 1;
            out = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if is_option(arg) {
            return None;
        } else if saw_digest {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_digest || !positionals.is_empty() {
        return None;
    }
    if json {
        format = "json".to_string();
    }
    Some(DigestArgs {
        since,
        top,
        out,
        format,
    })
}

pub(crate) fn digest_command(args: DigestArgs) -> Result<(), String> {
    let brief = generate_digest(&args.since, args.top)?;
    let rendered = match args.format.as_str() {
        "json" => {
            let mut s = serde_json::to_string_pretty(&brief).map_err(|err| err.to_string())?;
            s.push('\n');
            s
        }
        "plain" => render_digest_plain(&brief),
        "markdown" | "" => render_digest_markdown(&brief),
        other => return Err(format!("unknown --format value {other:?}")),
    };
    if args.out.is_empty() {
        print!("{rendered}");
    } else {
        std::fs::write(&args.out, rendered)
            .map_err(|err| format!("digest: write {}: {err}", args.out))?;
    }
    Ok(())
}

/// Maximum number of files for which token/symbol deltas are computed.
/// Mirrors Go's `maxDeltaFiles = 100`.
pub(crate) const DIGEST_MAX_DELTA_FILES: usize = 100;

pub(crate) fn generate_digest(since: &str, top: usize) -> Result<DigestBrief, String> {
    let since_arg = git_since_arg(since)?;
    let since_date = digest_since_date(since);
    let until_date = current_date_utc();
    let until_rfc3339 = rfc3339_now_utc();
    let head = git_output(&["rev-parse", "HEAD"]).unwrap_or_else(|_| "N/A".to_string());
    let head_ref = head.trim().to_string();

    // Collect commits: one line per commit (hash\x00email), plus name-status lines.
    // We also track root commits (no-parent) via a separate pass to avoid marking
    // their initial files as "added" — matching Go's go-git behaviour.
    let log = git_output(&[
        "log",
        &format!("--since={since_arg}"),
        "--name-status",
        "--format=commit:%H%x00%ae%x00%P",
    ])
    .map_err(|err| format!("digest: git log failed: {err}"))?;

    let mut commit_count = 0usize;
    let mut authors = std::collections::BTreeSet::new();
    // (commit_count, is_added, is_deleted)
    let mut files: std::collections::BTreeMap<String, (usize, bool, bool)> =
        std::collections::BTreeMap::new();
    let mut oldest_hash = String::new();
    let mut current_is_root = false;

    for line in log.lines() {
        if let Some(rest) = line.strip_prefix("commit:") {
            commit_count += 1;
            // Format: hash\x00email\x00parents
            let mut parts = rest.splitn(3, '\0');
            let hash = parts.next().unwrap_or("").trim().to_string();
            let email = parts.next().unwrap_or("").trim().to_string();
            let parents = parts.next().unwrap_or("").trim().to_string();
            // git log prints newest-first, so last seen hash = oldest
            oldest_hash = hash;
            // A root commit has no parents
            current_is_root = parents.is_empty();
            if !email.is_empty() {
                authors.insert(email);
            }
            continue;
        }
        let mut fields = line.split('\t');
        let Some(status) = fields.next() else {
            continue;
        };
        let Some(path) = fields.next() else {
            continue;
        };
        if status.is_empty() || path.is_empty() {
            continue;
        }
        let entry = files.entry(path.to_string()).or_insert((0, false, false));
        entry.0 += 1;
        // For root commits, Go iterates c.Files() without marking added/deleted.
        // Only non-root commits (where from==nil means truly new file) set added.
        if !current_is_root && status.starts_with('A') {
            entry.1 = true;
        }
        if status.starts_with('D') {
            entry.2 = true;
        }
    }

    let files_added = files.values().filter(|(_, added, _)| *added).count();
    let files_deleted = files.values().filter(|(_, _, deleted)| *deleted).count();
    let files_changed = files.len();

    // Resolve since_ref: parent of oldest commit in the period (or the commit
    // itself if it is the root commit). Mirrors Go's SinceRef logic.
    let since_ref = if oldest_hash.is_empty() {
        // No commits in period — use HEAD as both refs.
        head_ref.clone()
    } else {
        // Try to get the parent of the oldest commit
        let parent_hash = git_output(&["rev-parse", &format!("{oldest_hash}^")])
            .unwrap_or_else(|_| oldest_hash.trim().to_string());
        parent_hash.trim().to_string()
    };

    // Compute token/symbol deltas for hot files (capped at DIGEST_MAX_DELTA_FILES).
    // Sort all paths deterministically, then compute deltas.
    let mut all_paths: Vec<_> = files.keys().cloned().collect();
    all_paths.sort();

    let mut total_token_delta: i64 = 0;
    let mut total_symbol_delta: i64 = 0;
    let mut path_deltas: std::collections::HashMap<String, (i64, i64)> =
        std::collections::HashMap::new();

    let can_compute_delta = since_ref != "N/A" && !since_ref.is_empty();
    let mut delta_count = 0;
    for path in &all_paths {
        if delta_count >= DIGEST_MAX_DELTA_FILES {
            break;
        }
        if !can_compute_delta {
            break;
        }
        let old_content = git_show_content(&since_ref, path).unwrap_or_default();
        let new_content = git_show_content(&head_ref, path).unwrap_or_default();
        let old_tokens = ctx_tokens::count_str(&old_content);
        let new_tokens = ctx_tokens::count_str(&new_content);
        let td = new_tokens - old_tokens;
        let old_syms = count_digest_symbols(path, &old_content);
        let new_syms = count_digest_symbols(path, &new_content);
        let sd = new_syms - old_syms;
        total_token_delta += td;
        total_symbol_delta += sd;
        path_deltas.insert(path.clone(), (td, sd));
        delta_count += 1;
    }

    let mut hot_files: Vec<_> = files
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
    if top > 0 && hot_files.len() > top {
        hot_files.truncate(top);
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
        commits: commit_count,
        authors: authors.len(),
        files_changed,
        files_added,
        files_deleted,
        token_delta: total_token_delta,
        symbol_delta: total_symbol_delta,
        hot_files,
    })
}

/// Fetch file content at a git ref:path using `git show`.
/// Returns empty string if the file doesn't exist at that ref.
pub(crate) fn git_show_content(git_ref: &str, path: &str) -> Option<String> {
    let output = Command::new("git")
        .args(["show", &format!("{git_ref}:{path}")])
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    // Skip binary files (non-UTF-8)
    std::str::from_utf8(&output.stdout)
        .ok()
        .map(|s| s.to_string())
}

/// Count named symbols in file content using simple regex-like extraction.
/// Mirrors Go's countSymbols which uses tree-sitter for Go/TS/JS/Python.
/// We use the same regex-based extractor used elsewhere in this binary.
pub(crate) fn count_digest_symbols(path: &str, content: &str) -> i64 {
    if content.is_empty() {
        return 0;
    }
    let lines: Vec<String> = content.lines().map(ToString::to_string).collect();
    let syms = extract_where_symbols(path, &lines);
    syms.len() as i64
}

/// Compute the YYYY-MM-DD date that is `since` duration ago from today (UTC).
/// Uses `date -u` to compute deterministically. Falls back to "1970-01-01".
pub(crate) fn digest_since_date(since: &str) -> String {
    let lower = since.to_ascii_lowercase();
    // Try to parse the duration and use `date` to subtract from today
    for (suffix, unit) in [
        ("mo", "months"),
        ("w", "weeks"),
        ("d", "days"),
        ("y", "years"),
        ("h", "hours"),
    ] {
        if let Some(num) = lower.strip_suffix(suffix) {
            if !num.is_empty() && num.chars().all(|ch| ch.is_ascii_digit()) {
                // macOS `date` uses -v; GNU `date` uses --date
                // Try GNU first, then macOS
                let gnu = Command::new("date")
                    .args(["-u", &format!("--date={num} {unit} ago"), "+%Y-%m-%d"])
                    .output();
                if let Ok(out) = gnu {
                    if out.status.success() {
                        return String::from_utf8_lossy(&out.stdout).trim().to_string();
                    }
                }
                // macOS BSD date: -v-{n}{unit_char}
                let unit_char = match suffix {
                    "mo" => "m",
                    "w" => "w",
                    "d" => "d",
                    "y" => "y",
                    "h" => "H",
                    _ => continue,
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
        }
    }
    "1970-01-01".to_string()
}

pub(crate) fn git_since_arg(since: &str) -> Result<String, String> {
    if since.is_empty() {
        return Err("parsing duration \"\": empty string".to_string());
    }
    let lower = since.to_ascii_lowercase();
    for (suffix, unit) in [
        ("mo", "months"),
        ("w", "weeks"),
        ("d", "days"),
        ("y", "years"),
        ("h", "hours"),
    ] {
        if let Some(num) = lower.strip_suffix(suffix) {
            if num.is_empty() || !num.chars().all(|ch| ch.is_ascii_digit()) {
                return Err(format!("parsing duration {since:?}: invalid numeric part"));
            }
            return Ok(format!("{num} {unit} ago"));
        }
    }
    Ok(format!("{since} ago"))
}

/// Format an integer with an explicit + for non-negative values.
/// Mirrors Go's `signedFmt`.
pub(crate) fn signed_fmt(n: i64) -> String {
    if n >= 0 {
        format!("+{n}")
    } else {
        format!("{n}")
    }
}

pub(crate) fn render_digest_markdown(b: &DigestBrief) -> String {
    let mut out = String::new();
    // Header: "# Digest \u{2014} {since_date} \u{2192} {until_date} ({dur})"
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
        // Suggested next reads (capped at 3, matching Go's writeNextReads)
        render_digest_next_reads(&mut out, &b.hot_files);
    }
    out
}

/// Mirrors Go's writeNextReads: emits "Suggested next reads" for up to 3 hot files.
pub(crate) fn render_digest_next_reads(out: &mut String, hot: &[DigestHotFile]) {
    if hot.is_empty() {
        return;
    }
    const MAX_ROWS: usize = 3;
    let rows = &hot[..MAX_ROWS.min(hot.len())];
    out.push_str("\n## Suggested next reads\n\n");
    for hf in rows {
        let abs_sym = hf.symbol_delta.unsigned_abs();
        if abs_sym > 0 {
            out.push_str(&format!(
                "- `ctx_focus {{\"anchor\":{}}}` \u{2014} {} (symbols \u{0394} {})\n",
                serde_json::to_string(&hf.path).unwrap_or_else(|_| format!("{:?}", hf.path)),
                hf.path,
                signed_fmt(hf.symbol_delta),
            ));
        } else {
            out.push_str(&format!(
                "- `ctx_skim {{\"path\":{}}}` \u{2014} {} (tokens \u{0394} {})\n",
                serde_json::to_string(&hf.path).unwrap_or_else(|_| format!("{:?}", hf.path)),
                hf.path,
                signed_fmt(hf.token_delta),
            ));
        }
    }
}

pub(crate) fn render_digest_plain(b: &DigestBrief) -> String {
    let mut out = String::new();
    // Header: "Digest: {since_date} \u{2192} {until_date} ({dur})"
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
            // Left-pad path to 40 chars, matching Go's format
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
