use std::collections::HashMap;
use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

const MAX_WORKTREE_DIFF_BYTES: u64 = 1 << 20;
const MAX_WORKTREE_DIFF_LINES: usize = 5000;
const MAX_SUBJECT_RUNES: usize = 500;
const MAX_AUTHOR_RUNES: usize = 100;

#[derive(Debug)]
pub struct GitError {
    message: String,
}

impl GitError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for GitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GitError {}

impl From<std::io::Error> for GitError {
    fn from(err: std::io::Error) -> Self {
        Self::new(err.to_string())
    }
}

pub type Result<T> = std::result::Result<T, GitError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeDiffLine {
    pub typ: String,
    pub text: String,
    pub old_num: i32,
    pub new_num: i32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeFileDiff {
    pub path: String,
    pub added: bool,
    pub deleted: bool,
    pub binary: bool,
    pub no_change: bool,
    pub truncated: bool,
    pub lines: Vec<WorktreeDiffLine>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileLogEntry {
    pub hash: String,
    pub hash_full: String,
    pub author: String,
    pub author_email: String,
    pub subject: String,
    pub date: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoLogEntry {
    pub hash: String,
    pub hash_full: String,
    pub author: String,
    pub author_email: String,
    pub subject: String,
    pub date: i64,
    /// Full hashes of this commit's parents (1 for a normal commit, 2+ for a
    /// merge, 0 for the root). Drives the commit-graph lane rendering.
    pub parents: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    pub name: String,
    /// Short object id the branch points at.
    pub hash: String,
    /// True for the branch HEAD currently points at.
    pub current: bool,
    /// Subject of the branch's tip commit (first line).
    pub subject: String,
    /// Committer time (unix seconds) of the tip commit.
    pub date: i64,
    /// Commits this branch is ahead / behind the checked-out HEAD. Both zero
    /// when even, when the branch is HEAD, or when git is too old to report
    /// `ahead-behind` (best-effort — never breaks the listing).
    pub ahead: u32,
    pub behind: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tag {
    pub name: String,
    pub hash: String,
    /// Creator time (unix seconds) — tag date for annotated tags, commit date
    /// for lightweight tags.
    pub date: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    /// Branch short name, or `None` when detached/bare.
    pub branch: Option<String>,
    /// Short HEAD object id (empty for a bare worktree).
    pub head: String,
    pub bare: bool,
    pub detached: bool,
}

/// Local branches (`refs/heads`), each with its tip oid, subject, date, the
/// HEAD flag, and best-effort ahead/behind counts vs the checked-out HEAD.
/// Empty on an unborn branch / no branches.
pub fn branches(repo_root: impl AsRef<Path>) -> Result<Vec<Branch>> {
    let git_dir = git_dir(repo_root.as_ref());
    let output = git_output(
        &git_dir,
        &[
            "-c",
            "core.quotepath=false",
            "for-each-ref",
            "--format=%(refname:short)\x1f%(objectname:short)\x1f%(HEAD)\x1f%(committerdate:unix)\x1f%(contents:subject)",
            "refs/heads",
        ],
    )?;
    let text = String::from_utf8_lossy(&output);
    let mut out = Vec::new();
    for line in text.lines() {
        let mut fields = line.splitn(5, '\x1f');
        let Some(name) = fields.next() else { continue };
        if name.is_empty() {
            continue;
        }
        let hash = fields.next().unwrap_or("").to_string();
        // `%(HEAD)` is "*" for the checked-out branch, " " otherwise.
        let current = fields.next().unwrap_or("").trim() == "*";
        let date = fields
            .next()
            .unwrap_or("")
            .trim()
            .parse::<i64>()
            .unwrap_or(0);
        let subject = truncate_chars(fields.next().unwrap_or(""), MAX_SUBJECT_RUNES);
        out.push(Branch {
            name: name.to_string(),
            hash,
            current,
            subject,
            date,
            ahead: 0,
            behind: 0,
        });
    }

    // Best-effort ahead/behind in a separate call: the `ahead-behind` atom
    // needs git 2.41+, so an older git fails this command — we then leave the
    // counts at zero rather than breaking the whole listing.
    if let Ok(ab_out) = git_output(
        &git_dir,
        &[
            "for-each-ref",
            "--format=%(refname:short)\x1f%(ahead-behind:HEAD)",
            "refs/heads",
        ],
    ) {
        let ab_text = String::from_utf8_lossy(&ab_out);
        for line in ab_text.lines() {
            let mut fields = line.splitn(2, '\x1f');
            let Some(name) = fields.next() else { continue };
            let counts = fields.next().unwrap_or("");
            let mut nums = counts.split_whitespace();
            let ahead = nums.next().and_then(|s| s.parse::<u32>().ok());
            let behind = nums.next().and_then(|s| s.parse::<u32>().ok());
            if let (Some(ahead), Some(behind)) = (ahead, behind) {
                if let Some(b) = out.iter_mut().find(|b| b.name == name) {
                    b.ahead = ahead;
                    b.behind = behind;
                }
            }
        }
    }

    Ok(out)
}

/// Tags (`refs/tags`), each with its short target oid and creator date,
/// newest first. Empty when the repo has no tags.
pub fn tags(repo_root: impl AsRef<Path>) -> Result<Vec<Tag>> {
    let git_dir = git_dir(repo_root.as_ref());
    let output = git_output(
        &git_dir,
        &[
            "-c",
            "core.quotepath=false",
            "for-each-ref",
            "--sort=-creatordate",
            "--format=%(refname:short)\x1f%(objectname:short)\x1f%(creatordate:unix)",
            "refs/tags",
        ],
    )?;
    let text = String::from_utf8_lossy(&output);
    let mut out = Vec::new();
    for line in text.lines() {
        let mut fields = line.splitn(3, '\x1f');
        let Some(name) = fields.next() else { continue };
        if name.is_empty() {
            continue;
        }
        let hash = fields.next().unwrap_or("").to_string();
        let date = fields
            .next()
            .unwrap_or("")
            .trim()
            .parse::<i64>()
            .unwrap_or(0);
        out.push(Tag {
            name: name.to_string(),
            hash,
            date,
        });
    }
    Ok(out)
}

/// Linked worktrees (`git worktree list`), including the main one. `branch`
/// is the short name when on a branch, `None` when detached or bare.
pub fn worktrees(repo_root: impl AsRef<Path>) -> Result<Vec<Worktree>> {
    let git_dir = git_dir(repo_root.as_ref());
    let output = git_output(&git_dir, &["worktree", "list", "--porcelain"])?;
    let text = String::from_utf8_lossy(&output);

    let mut out = Vec::new();
    let mut cur: Option<Worktree> = None;
    for line in text.lines() {
        if line.is_empty() {
            if let Some(wt) = cur.take() {
                out.push(wt);
            }
            continue;
        }
        if let Some(path) = line.strip_prefix("worktree ") {
            if let Some(wt) = cur.take() {
                out.push(wt);
            }
            cur = Some(Worktree {
                path: path.to_string(),
                branch: None,
                head: String::new(),
                bare: false,
                detached: false,
            });
        } else if let Some(wt) = cur.as_mut() {
            if let Some(head) = line.strip_prefix("HEAD ") {
                wt.head = head.chars().take(7).collect();
            } else if let Some(branch) = line.strip_prefix("branch ") {
                wt.branch = Some(
                    branch
                        .strip_prefix("refs/heads/")
                        .unwrap_or(branch)
                        .to_string(),
                );
            } else if line == "bare" {
                wt.bare = true;
            } else if line == "detached" {
                wt.detached = true;
            }
        }
    }
    if let Some(wt) = cur.take() {
        out.push(wt);
    }
    Ok(out)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CommitFile {
    /// One of "added" | "modified" | "deleted" (other git statuses map to
    /// "modified" — the per-file commit diff renders them all the same way).
    pub status: String,
    pub path: String,
    /// Lines added / removed by this file in the commit (git `--numstat`).
    /// Both zero for a binary file (see `binary`).
    pub additions: u32,
    pub deletions: u32,
    pub binary: bool,
}

/// Recent repository-wide commit history (newest first), like `git log`.
/// Returns at most `limit` entries plus a `truncated` flag that is true when
/// more history exists beyond the window. Empty (and not truncated) on an
/// unborn branch.
///
/// `ref_name` selects the starting point (a branch, worktree HEAD, or commit
/// hash); `None` walks the default HEAD and keeps the git invocation identical
/// to the historic byte-for-byte behaviour.
pub fn repo_log(
    repo_root: impl AsRef<Path>,
    limit: usize,
    ref_name: Option<&str>,
) -> Result<(Vec<RepoLogEntry>, bool)> {
    let git_dir = git_dir(repo_root.as_ref());
    // The unborn-HEAD shortcut only applies to the default HEAD; with an
    // explicit ref we defer to git, which reports an invalid ref as an error.
    if ref_name.is_none() && read_head_oid(&git_dir)?.is_none() {
        return Ok((Vec::new(), false));
    }

    // One extra row tells us whether the history is truncated.
    let probe = limit.saturating_add(1);
    let n = format!("-n{probe}");
    // \x1f (unit separator) cannot appear in a hash/email/timestamp and is
    // stripped from subjects by git's %s (first line only), so it is a safe
    // field delimiter — far safer than whitespace for author names.
    let mut args = vec![
        "log",
        "--no-color",
        &n,
        "--format=%H\x1f%h\x1f%an\x1f%ae\x1f%ct\x1f%P\x1f%s",
    ];
    // Append the ref only when given so the default-HEAD arg list is unchanged.
    if let Some(ref_name) = ref_name {
        args.push(ref_name);
    }
    let output = git_output(&git_dir, &args)?;
    let text = String::from_utf8_lossy(&output);

    let mut entries = Vec::with_capacity(limit.min(probe));
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.splitn(7, '\x1f');
        let hash_full = fields.next().unwrap_or("").to_string();
        let hash = fields.next().unwrap_or("").to_string();
        let author = truncate_chars(fields.next().unwrap_or(""), MAX_AUTHOR_RUNES);
        let author_email = truncate_chars(fields.next().unwrap_or(""), MAX_AUTHOR_RUNES);
        let date = fields
            .next()
            .unwrap_or("")
            .trim()
            .parse::<i64>()
            .unwrap_or(0);
        let parents = fields
            .next()
            .unwrap_or("")
            .split_whitespace()
            .map(str::to_string)
            .collect();
        let subject = truncate_chars(fields.next().unwrap_or(""), MAX_SUBJECT_RUNES);
        if hash_full.is_empty() {
            continue;
        }
        entries.push(RepoLogEntry {
            hash,
            hash_full,
            author,
            author_email,
            subject,
            date,
            parents,
        });
    }

    let truncated = entries.len() > limit;
    if truncated {
        entries.truncate(limit);
    }
    Ok((entries, truncated))
}

/// Files changed by a single commit (`git diff-tree` against its first
/// parent; a root commit reports every file as "added"). Renames are not
/// detected — a rename surfaces as a delete + add so each `path` resolves
/// cleanly through [`commit_diff`] with `from = "<hash>^"`.
pub fn commit_files(repo_root: impl AsRef<Path>, hash: &str) -> Result<Vec<CommitFile>> {
    if hash.is_empty() {
        return Err(GitError::new("hash is required"));
    }
    let git_dir = git_dir(repo_root.as_ref());
    let status_args = [
        "-c",
        "core.quotepath=false",
        "diff-tree",
        "--no-commit-id",
        "--name-status",
        "--no-renames",
        "-r",
        "--root",
        hash,
    ];
    let status_out = git_output(&git_dir, &status_args)?;
    let status_text = String::from_utf8_lossy(&status_out);

    // `--numstat` gives `<additions>\t<deletions>\t<path>` per file (a binary
    // file reports `-\t-\t<path>`). Run it alongside name-status and merge by
    // path so each row carries both its A/M/D status and its +/- line counts.
    let numstat_args = [
        "-c",
        "core.quotepath=false",
        "diff-tree",
        "--no-commit-id",
        "--numstat",
        "--no-renames",
        "-r",
        "--root",
        hash,
    ];
    let numstat_out = git_output(&git_dir, &numstat_args)?;
    let numstat_text = String::from_utf8_lossy(&numstat_out);
    let mut stats: HashMap<String, (u32, u32, bool)> = HashMap::new();
    for line in numstat_text.lines() {
        let mut parts = line.splitn(3, '\t');
        let Some(add_raw) = parts.next() else {
            continue;
        };
        let Some(del_raw) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else { continue };
        if path.is_empty() {
            continue;
        }
        let binary = add_raw == "-" || del_raw == "-";
        let additions = add_raw.parse::<u32>().unwrap_or(0);
        let deletions = del_raw.parse::<u32>().unwrap_or(0);
        stats.insert(path.to_string(), (additions, deletions, binary));
    }

    let mut files = Vec::new();
    for line in status_text.lines() {
        let mut parts = line.splitn(2, '\t');
        let Some(status_raw) = parts.next() else {
            continue;
        };
        let Some(path) = parts.next() else {
            continue;
        };
        if status_raw.is_empty() || path.is_empty() {
            continue;
        }
        let status = match status_raw.chars().next() {
            Some('A') => "added",
            Some('D') => "deleted",
            _ => "modified",
        };
        let (additions, deletions, binary) = stats.get(path).copied().unwrap_or((0, 0, false));
        files.push(CommitFile {
            status: status.to_string(),
            path: path.to_string(),
            additions,
            deletions,
            binary,
        });
    }
    Ok(files)
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ChurnStat {
    /// Number of commits that touched this path within the window.
    pub commits: u32,
    /// Unix timestamp (committer time) of the most recent touching commit.
    pub last_commit_time: i64,
}

/// Per-file change frequency and most-recent commit time derived from
/// `git log`. Keys are repo-relative, '/'-separated paths matching the
/// worktree layout (`core.quotepath=false` keeps non-ASCII names literal).
/// `since` (a git approxidate such as `"90d"` or an ISO date) windows the
/// log when `Some`; `None` walks the full history. Returns an empty map on
/// an unborn branch (no commits yet).
pub fn file_churn(
    repo_root: impl AsRef<Path>,
    since: Option<&str>,
) -> Result<HashMap<String, ChurnStat>> {
    let git_dir = git_dir(repo_root.as_ref());
    if read_head_oid(&git_dir)?.is_none() {
        return Ok(HashMap::new());
    }

    let mut args: Vec<String> = vec![
        "-c".into(),
        "core.quotepath=false".into(),
        "log".into(),
        "--no-renames".into(),
        "--name-only".into(),
        "--format=%x00%ct".into(),
    ];
    if let Some(since) = since.filter(|s| !s.is_empty()) {
        args.push(format!("--since={since}"));
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git_output(&git_dir, &arg_refs)?;
    let text = String::from_utf8_lossy(&output);

    let mut churn: HashMap<String, ChurnStat> = HashMap::new();
    let mut commit_time = 0i64;
    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('\0') {
            commit_time = rest.trim().parse::<i64>().unwrap_or(0);
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let entry = churn.entry(line.to_string()).or_default();
        entry.commits = entry.commits.saturating_add(1);
        if commit_time > entry.last_commit_time {
            entry.last_commit_time = commit_time;
        }
    }
    Ok(churn)
}

/// Co-change graph node: one repo-relative, '/'-separated path that changed
/// within the scanned window. `commits` / `last_commit_time` are the same
/// per-file churn quantities `file_churn` reports, gathered in the same pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoChangeNode {
    pub path: String,
    pub commits: u32,
    pub last_commit_time: i64,
    /// Line count of the worktree file (`b'\n'` count), or 0 if unreadable/absent.
    pub lines: u32,
}

/// Co-change graph edge between two nodes. `source`/`target` index into the
/// `CoChangeGraph::nodes` vector; `weight` is the number of commits that
/// touched both endpoints together.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoChangeEdge {
    pub source: usize,
    pub target: usize,
    pub weight: u32,
}

/// Co-change graph for a window of history.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CoChangeGraph {
    pub nodes: Vec<CoChangeNode>,
    pub edges: Vec<CoChangeEdge>,
    /// Number of commits actually folded into the aggregation (≤ `limit`).
    pub commits_scanned: u32,
    /// True when history exceeded `limit` and the older commits were dropped.
    pub truncated: bool,
}

/// Commits whose change set is larger than this are treated as bulk edits
/// (mass reformat, vendoring, generated-file regen) and excluded from
/// pairwise edge aggregation, since every pair in such a commit is a false
/// co-change signal. The files still count toward per-node churn.
const MAX_FILES_PER_COMMIT: usize = 50;

/// Builds a co-change graph from a single `git log` pass: files changed in the
/// same commit get an edge whose weight is how often they co-changed. Edges
/// expose the implicit coupling (an API and its test, source and its snapshot,
/// a config and its loader) that the commit graph's time axis cannot show.
/// Paths are repo-relative and '/'-separated (`core.quotepath=false` keeps
/// non-ASCII names literal), matching the worktree layout and `file_churn`.
///
/// `limit` caps the number of commits aggregated (a probe of `limit + 1` sets
/// `truncated`); `since` is a git approxidate (e.g. `"90d"`) windowing the log
/// when `Some`; `min_weight` drops edges seen fewer than that many times.
/// Nodes that end up with no surviving edge are omitted to shrink the payload,
/// so every returned node is incident to at least one edge. Returns an empty
/// graph on an unborn branch (no commits yet).
pub fn co_change_graph(
    repo_root: impl AsRef<Path>,
    limit: usize,
    since: Option<&str>,
    min_weight: u32,
) -> Result<CoChangeGraph> {
    let git_dir = git_dir(repo_root.as_ref());
    if read_head_oid(&git_dir)?.is_none() {
        return Ok(CoChangeGraph::default());
    }

    // Same 1-pass shape as `file_churn`, but probe one extra commit so we can
    // tell whether history was truncated, and bound the scan with `-n`.
    let mut args: Vec<String> = vec![
        "-c".into(),
        "core.quotepath=false".into(),
        "log".into(),
        "--no-renames".into(),
        "--name-only".into(),
        "--format=%x00%ct".into(),
        format!("-n{}", limit.saturating_add(1)),
    ];
    if let Some(since) = since.filter(|s| !s.is_empty()) {
        args.push(format!("--since={since}"));
    }
    let arg_refs: Vec<&str> = args.iter().map(String::as_str).collect();
    let output = git_output(&git_dir, &arg_refs)?;
    let text = String::from_utf8_lossy(&output);

    // Intern paths to dense ids; per-id churn accumulates alongside, indexed
    // by id. `paths` is the reverse map (id -> path) for the final node build.
    let mut ids: HashMap<String, usize> = HashMap::new();
    let mut paths: Vec<String> = Vec::new();
    let mut churn: Vec<ChurnStat> = Vec::new();
    let mut pair_weights: HashMap<(usize, usize), u32> = HashMap::new();

    let mut commit_time = 0i64;
    let mut current: Vec<usize> = Vec::new();
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    let mut commits_scanned: u32 = 0;
    let mut truncated = false;
    let mut commit_open = false;

    for line in text.lines() {
        if let Some(rest) = line.strip_prefix('\0') {
            // A commit boundary closes the previous commit (if any).
            if commit_open {
                if commits_scanned as usize >= limit {
                    // The just-finished commit is the probe beyond `limit`;
                    // drop it and stop — older history is truncated.
                    truncated = true;
                    break;
                }
                fold_commit(&current, commit_time, &mut churn, &mut pair_weights);
                commits_scanned += 1;
            }
            commit_open = true;
            current.clear();
            seen.clear();
            commit_time = rest.trim().parse::<i64>().unwrap_or(0);
            continue;
        }
        if line.is_empty() {
            continue;
        }
        let id = *ids.entry(line.to_string()).or_insert_with(|| {
            let id = paths.len();
            paths.push(line.to_string());
            churn.push(ChurnStat::default());
            id
        });
        // Dedupe paths within a commit so a file listed twice counts once.
        if seen.insert(id) {
            current.push(id);
        }
    }
    // Fold the final commit (the log ends without a trailing `\0`), unless it
    // is the probe commit beyond `limit`.
    if commit_open {
        if commits_scanned as usize >= limit {
            truncated = true;
        } else {
            fold_commit(&current, commit_time, &mut churn, &mut pair_weights);
            commits_scanned += 1;
        }
    }

    // Keep edges at or above the weight floor, then compact node ids so the
    // returned nodes are exactly the surviving edges' endpoints (isolated
    // nodes are dropped to shrink the payload).
    let mut compact: HashMap<usize, usize> = HashMap::new();
    let mut nodes: Vec<CoChangeNode> = Vec::new();
    let mut edges: Vec<CoChangeEdge> = Vec::new();
    for (&(a, b), &weight) in &pair_weights {
        if weight < min_weight {
            continue;
        }
        let source = intern_node(
            repo_root.as_ref(),
            a,
            &paths,
            &churn,
            &mut compact,
            &mut nodes,
        );
        let target = intern_node(
            repo_root.as_ref(),
            b,
            &paths,
            &churn,
            &mut compact,
            &mut nodes,
        );
        edges.push(CoChangeEdge {
            source,
            target,
            weight,
        });
    }

    Ok(CoChangeGraph {
        nodes,
        edges,
        commits_scanned,
        truncated,
    })
}

/// Folds one commit's deduped file id set into per-id churn and pairwise
/// co-change weights. Bulk commits (> `MAX_FILES_PER_COMMIT`) still count
/// toward churn but are excluded from pairwise weights to avoid false signal.
fn fold_commit(
    current: &[usize],
    commit_time: i64,
    churn: &mut [ChurnStat],
    pair_weights: &mut HashMap<(usize, usize), u32>,
) {
    for &id in current {
        let stat = &mut churn[id];
        stat.commits = stat.commits.saturating_add(1);
        if commit_time > stat.last_commit_time {
            stat.last_commit_time = commit_time;
        }
    }
    if current.len() > MAX_FILES_PER_COMMIT {
        return;
    }
    for i in 0..current.len() {
        for j in (i + 1)..current.len() {
            let (a, b) = (current[i], current[j]);
            let key = if a < b { (a, b) } else { (b, a) };
            let w = pair_weights.entry(key).or_insert(0);
            *w = w.saturating_add(1);
        }
    }
}

/// Maps a churn id to its compacted node index, inserting the node on first
/// sight so the node vector only carries paths incident to a surviving edge.
fn intern_node(
    repo_root: &Path,
    id: usize,
    paths: &[String],
    churn: &[ChurnStat],
    compact: &mut HashMap<usize, usize>,
    nodes: &mut Vec<CoChangeNode>,
) -> usize {
    if let Some(&idx) = compact.get(&id) {
        return idx;
    }
    let idx = nodes.len();
    let path = &paths[id];
    // Count newlines in the worktree file; absent/unreadable files report 0.
    let lines = fs::read(repo_root.join(path))
        .map(|bytes| bytes.iter().filter(|&&b| b == b'\n').count() as u32)
        .unwrap_or(0);
    nodes.push(CoChangeNode {
        path: path.clone(),
        commits: churn[id].commits,
        last_commit_time: churn[id].last_commit_time,
        lines,
    });
    compact.insert(id, idx);
    idx
}

pub fn file_log(
    repo_root: impl AsRef<Path>,
    slash_path: &str,
    limit: usize,
) -> Result<(Vec<FileLogEntry>, bool)> {
    let git_dir = git_dir(repo_root.as_ref());
    let Some(mut commit_oid) = read_head_oid(&git_dir)? else {
        return Ok((Vec::new(), false));
    };

    let mut entries = Vec::with_capacity(limit.saturating_add(1));
    while entries.len() < limit.saturating_add(1) {
        let commit = read_commit(&git_dir, &commit_oid)?;
        let current_blob = tree_lookup_blob(&git_dir, &commit.tree, slash_path)?;
        let parent_blob = match commit.parents.first() {
            Some(parent) => {
                let parent_commit = read_commit(&git_dir, parent)?;
                tree_lookup_blob(&git_dir, &parent_commit.tree, slash_path)?
            }
            None => None,
        };

        if current_blob != parent_blob {
            entries.push(commit_to_entry(&commit));
        }

        let Some(parent) = commit.parents.first() else {
            break;
        };
        commit_oid = parent.clone();
    }

    let truncated = entries.len() > limit;
    if truncated {
        entries.truncate(limit);
    }
    Ok((entries, truncated))
}

/// Per-file commit history starting from an optional ref.
///
/// The default-HEAD case delegates to [`file_log`] to preserve the existing
/// object-walk behavior. An explicit ref uses `git log <ref> -- <path>`, giving
/// CLI callers a bounded path-history query without exposing raw git process
/// invocation at the command layer.
pub fn file_log_ref(
    repo_root: impl AsRef<Path>,
    slash_path: &str,
    limit: usize,
    ref_name: Option<&str>,
) -> Result<(Vec<FileLogEntry>, bool)> {
    let Some(ref_name) = ref_name else {
        return file_log(repo_root, slash_path, limit);
    };
    if slash_path.is_empty() {
        return Err(GitError::new("path is required"));
    }

    let git_dir = git_dir(repo_root.as_ref());
    let probe = limit.saturating_add(1);
    let n = format!("-n{probe}");
    let args = [
        "-c",
        "core.quotepath=false",
        "log",
        "--no-color",
        &n,
        "--format=%H\x1f%h\x1f%an\x1f%ae\x1f%ct\x1f%s",
        ref_name,
        "--",
        slash_path,
    ];
    let output = git_output(&git_dir, &args)?;
    let text = String::from_utf8_lossy(&output);

    let mut entries = Vec::with_capacity(limit.min(probe));
    for line in text.lines() {
        if line.is_empty() {
            continue;
        }
        let mut fields = line.splitn(6, '\x1f');
        let hash_full = fields.next().unwrap_or("").to_string();
        let hash = fields.next().unwrap_or("").to_string();
        let author = truncate_chars(fields.next().unwrap_or(""), MAX_AUTHOR_RUNES);
        let author_email = truncate_chars(fields.next().unwrap_or(""), MAX_AUTHOR_RUNES);
        let date = fields
            .next()
            .unwrap_or("")
            .trim()
            .parse::<i64>()
            .unwrap_or(0);
        let subject = truncate_chars(fields.next().unwrap_or(""), MAX_SUBJECT_RUNES);
        if hash_full.is_empty() {
            continue;
        }
        entries.push(FileLogEntry {
            hash,
            hash_full,
            author,
            author_email,
            subject,
            date,
        });
    }

    let truncated = entries.len() > limit;
    if truncated {
        entries.truncate(limit);
    }
    Ok((entries, truncated))
}

pub fn worktree_diff(repo_root: impl AsRef<Path>, slash_path: &str) -> Result<WorktreeFileDiff> {
    if slash_path.is_empty() {
        return Err(GitError::new("path is required"));
    }

    let repo_root = repo_root.as_ref();
    let git_dir = git_dir(repo_root);
    let (before_content, before_binary, before_exists) = read_head_file(&git_dir, slash_path)?;
    let full_path = repo_root.join(slash_path.replace('/', std::path::MAIN_SEPARATOR_STR));
    let (after_content, after_binary, after_exists) = read_worktree_file(&full_path)?;

    let mut result = WorktreeFileDiff {
        path: slash_path.to_string(),
        added: false,
        deleted: false,
        binary: false,
        no_change: false,
        truncated: false,
        lines: Vec::new(),
    };

    if !before_exists && !after_exists {
        result.no_change = true;
        return Ok(result);
    }
    if before_binary || after_binary {
        result.binary = true;
        result.added = !before_exists && after_exists;
        result.deleted = before_exists && !after_exists;
        return Ok(result);
    }
    if !before_exists {
        result.added = true;
    }
    if !after_exists {
        result.deleted = true;
    }
    if before_content == after_content {
        result.no_change = true;
        return Ok(result);
    }

    let (lines, truncated) =
        render_diff_lines(&before_content, &after_content, MAX_WORKTREE_DIFF_LINES);
    result.lines = lines;
    result.truncated = truncated;
    Ok(result)
}

pub fn commit_diff(
    repo_root: impl AsRef<Path>,
    from_rev: &str,
    to_rev: &str,
    slash_path: &str,
) -> Result<WorktreeFileDiff> {
    let git_dir = git_dir(repo_root.as_ref());
    let from_commit = resolve_commit_revision(&git_dir, from_rev, "from")?;
    let to_commit = resolve_commit_revision(&git_dir, to_rev, "to")?;
    let (before_content, before_binary, before_exists) = match from_commit {
        Some(commit) => file_content_at_commit(&git_dir, &commit, slash_path)?,
        None => (Vec::new(), false, false),
    };
    let (after_content, after_binary, after_exists) = match to_commit {
        Some(commit) => file_content_at_commit(&git_dir, &commit, slash_path)?,
        None => (Vec::new(), false, false),
    };

    let mut result = WorktreeFileDiff {
        path: slash_path.to_string(),
        added: false,
        deleted: false,
        binary: false,
        no_change: false,
        truncated: false,
        lines: Vec::new(),
    };

    if !before_exists && !after_exists {
        result.no_change = true;
        return Ok(result);
    }
    if before_binary || after_binary {
        result.binary = true;
        result.added = !before_exists && after_exists;
        result.deleted = before_exists && !after_exists;
        return Ok(result);
    }
    if !before_exists {
        result.added = true;
    }
    if !after_exists {
        result.deleted = true;
    }
    if before_content == after_content {
        result.no_change = true;
        return Ok(result);
    }

    let (lines, truncated) =
        render_diff_lines(&before_content, &after_content, MAX_WORKTREE_DIFF_LINES);
    result.lines = lines;
    result.truncated = truncated;
    Ok(result)
}

fn resolve_commit_revision(git_dir: &Path, rev: &str, label: &str) -> Result<Option<CommitInfo>> {
    if let Some(base_rev) = rev.strip_suffix('^') {
        let base_oid = resolve_revision(git_dir, base_rev)
            .map_err(|err| GitError::new(format!("cannot resolve {label}={rev:?}: {err}")))?;
        let base_commit = read_commit(git_dir, &base_oid)
            .map_err(|err| GitError::new(format!("cannot resolve {label}={rev:?}: {err}")))?;
        let Some(parent_oid) = base_commit.parents.first() else {
            return Ok(None);
        };
        return read_commit(git_dir, parent_oid)
            .map(Some)
            .map_err(|err| GitError::new(format!("cannot resolve {label}={rev:?}: {err}")));
    }

    let oid = resolve_revision(git_dir, rev)
        .map_err(|err| GitError::new(format!("cannot resolve {label}={rev:?}: {err}")))?;
    read_commit(git_dir, &oid)
        .map(Some)
        .map_err(|err| GitError::new(format!("cannot resolve {label}={rev:?}: {err}")))
}

/// Current `HEAD` object id for `repo_root`, or `None` on an unborn branch
/// (no commits yet). Resolves through `gitdir:` files (worktrees/submodules)
/// and packed refs. Cheap — a handful of small file reads, no object inflate —
/// so callers can use it as a cache-validity fingerprint for [`worktree_diff`].
pub fn head_oid(repo_root: impl AsRef<Path>) -> Result<Option<String>> {
    read_head_oid(&git_dir(repo_root.as_ref()))
}

fn git_dir(repo_root: &Path) -> PathBuf {
    let dot_git = repo_root.join(".git");
    if dot_git.is_dir() {
        return dot_git;
    }
    let Ok(raw) = fs::read_to_string(&dot_git) else {
        return dot_git;
    };
    let Some(path) = raw.trim().strip_prefix("gitdir:") else {
        return dot_git;
    };
    let path = path.trim();
    let git_dir = PathBuf::from(path);
    if git_dir.is_absolute() {
        git_dir
    } else {
        repo_root.join(git_dir)
    }
}

fn resolve_revision(git_dir: &Path, rev: &str) -> Result<String> {
    if rev == "HEAD" {
        return read_head_oid(git_dir)?.ok_or_else(|| GitError::new("reference not found"));
    }
    if is_hex_prefix(rev) {
        if let Some(oid) = resolve_loose_oid_prefix(git_dir, rev)? {
            return Ok(oid);
        }
        if let Some(oid) = resolve_commit_prefix_via_git(git_dir, rev)? {
            return Ok(oid);
        }
    }
    let ref_name = if rev.starts_with("refs/") {
        rev.to_string()
    } else {
        format!("refs/heads/{rev}")
    };
    if let Some(oid) = read_ref(git_dir, &ref_name)? {
        return Ok(oid);
    }
    Err(GitError::new("reference not found"))
}

fn is_hex_prefix(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_hexdigit())
}

fn resolve_loose_oid_prefix(git_dir: &Path, prefix: &str) -> Result<Option<String>> {
    if prefix.len() < 2 {
        return Ok(None);
    }
    let dir = git_dir.join("objects").join(&prefix[..2]);
    let suffix_prefix = &prefix[2..];
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let mut found = None;
    for entry in entries {
        let entry = entry?;
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with(suffix_prefix)
            && name.len() == 38
            && name.bytes().all(|b| b.is_ascii_hexdigit())
        {
            let oid = format!("{}{}", &prefix[..2], name);
            if found.replace(oid).is_some() {
                return Err(GitError::new("ambiguous revision"));
            }
        }
    }
    Ok(found)
}

fn resolve_commit_prefix_via_git(git_dir: &Path, prefix: &str) -> Result<Option<String>> {
    let rev = format!("{prefix}^{{commit}}");
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(git_dir)
        .args(["rev-parse", "--verify", &rev])
        .output()
        .map_err(|err| GitError::new(err.to_string()))?;
    if output.status.success() {
        let oid = String::from_utf8(output.stdout).map_err(|err| GitError::new(err.to_string()))?;
        let oid = oid.trim();
        if oid.len() == 40 && oid.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Ok(Some(oid.to_string()));
        }
        return Err(GitError::new("invalid resolved object id"));
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    if stderr.contains("ambiguous") {
        return Err(GitError::new("ambiguous revision"));
    }
    Ok(None)
}

fn read_ref(git_dir: &Path, reference: &str) -> Result<Option<String>> {
    let ref_path = git_dir.join(reference);
    match fs::read_to_string(&ref_path) {
        Ok(oid) => Ok(Some(oid.trim().to_string())),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            read_packed_ref(git_dir, reference)
        }
        Err(err) => Err(err.into()),
    }
}

fn read_head_file(git_dir: &Path, slash_path: &str) -> Result<(Vec<u8>, bool, bool)> {
    let Some(head) = read_head_oid(git_dir)? else {
        return Ok((Vec::new(), false, false));
    };
    let commit = read_object(git_dir, &head, "commit")?;
    let tree = commit_tree_oid(&commit)?;
    let Some(blob_oid) = tree_lookup_blob(git_dir, &tree, slash_path)? else {
        return Ok((Vec::new(), false, false));
    };
    let blob = read_object(git_dir, &blob_oid, "blob")?;
    if blob.len() as u64 > MAX_WORKTREE_DIFF_BYTES || is_binary_bytes(&blob) {
        return Ok((Vec::new(), true, true));
    }
    Ok((blob, false, true))
}

fn file_content_at_commit(
    git_dir: &Path,
    commit: &CommitInfo,
    slash_path: &str,
) -> Result<(Vec<u8>, bool, bool)> {
    let Some(blob_oid) = tree_lookup_blob(git_dir, &commit.tree, slash_path)? else {
        return Ok((Vec::new(), false, false));
    };
    let blob = read_object(git_dir, &blob_oid, "blob")?;
    if blob.len() as u64 > MAX_WORKTREE_DIFF_BYTES || is_binary_bytes(&blob) {
        return Ok((Vec::new(), true, true));
    }
    Ok((blob, false, true))
}

fn read_worktree_file(path: &Path) -> Result<(Vec<u8>, bool, bool)> {
    let metadata = match fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok((Vec::new(), false, false));
        }
        Err(err) => return Err(err.into()),
    };
    if metadata.is_dir() {
        return Err(GitError::new("path is a directory"));
    }
    if metadata.len() > MAX_WORKTREE_DIFF_BYTES {
        return Ok((Vec::new(), true, true));
    }
    let data = fs::read(path)?;
    if is_binary_bytes(&data) {
        return Ok((Vec::new(), true, true));
    }
    Ok((data, false, true))
}

fn read_head_oid(git_dir: &Path) -> Result<Option<String>> {
    let head_path = git_dir.join("HEAD");
    let head = match fs::read_to_string(&head_path) {
        Ok(head) => head,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    let head = head.trim();
    if let Some(reference) = head.strip_prefix("ref: ") {
        let ref_path = git_dir.join(reference);
        match fs::read_to_string(&ref_path) {
            Ok(oid) => Ok(Some(oid.trim().to_string())),
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
                read_packed_ref(git_dir, reference)
            }
            Err(err) => Err(err.into()),
        }
    } else if head.is_empty() {
        Ok(None)
    } else {
        Ok(Some(head.to_string()))
    }
}

fn read_packed_ref(git_dir: &Path, reference: &str) -> Result<Option<String>> {
    let path = git_dir.join("packed-refs");
    let data = match fs::read_to_string(path) {
        Ok(data) => data,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err.into()),
    };
    for line in data.lines() {
        if line.starts_with('#') || line.starts_with('^') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let Some(oid) = parts.next() else { continue };
        let Some(name) = parts.next() else { continue };
        if name == reference {
            return Ok(Some(oid.to_string()));
        }
    }
    Ok(None)
}

#[derive(Debug)]
struct CommitInfo {
    oid: String,
    tree: String,
    parents: Vec<String>,
    author: String,
    author_email: String,
    author_time: i64,
    message: String,
}

fn read_commit(git_dir: &Path, oid: &str) -> Result<CommitInfo> {
    let data = read_object(git_dir, oid, "commit")?;
    parse_commit(oid, &data)
}

fn parse_commit(oid: &str, data: &[u8]) -> Result<CommitInfo> {
    let text = std::str::from_utf8(data).map_err(|err| GitError::new(err.to_string()))?;
    let (headers, message) = text.split_once("\n\n").unwrap_or((text, ""));
    let mut tree = String::new();
    let mut parents = Vec::new();
    let mut author = String::new();
    let mut author_email = String::new();
    let mut author_time = 0i64;

    for line in headers.lines() {
        if let Some(value) = line.strip_prefix("tree ") {
            tree = value.to_string();
        } else if let Some(value) = line.strip_prefix("parent ") {
            parents.push(value.to_string());
        } else if let Some(value) = line.strip_prefix("author ") {
            let (name, email, timestamp) = parse_signature(value)?;
            author = name;
            author_email = email;
            author_time = timestamp;
        }
    }

    if tree.is_empty() {
        return Err(GitError::new("commit has no tree"));
    }

    Ok(CommitInfo {
        oid: oid.to_string(),
        tree,
        parents,
        author,
        author_email,
        author_time,
        message: message.to_string(),
    })
}

fn parse_signature(value: &str) -> Result<(String, String, i64)> {
    let email_end = value
        .rfind('>')
        .ok_or_else(|| GitError::new("invalid commit signature"))?;
    let before_email_end = &value[..email_end];
    let email_start = before_email_end
        .rfind('<')
        .ok_or_else(|| GitError::new("invalid commit signature"))?;
    let name = before_email_end[..email_start].trim_end().to_string();
    let email = before_email_end[email_start + 1..].to_string();
    let rest = value[email_end + 1..].trim_start();
    let timestamp = rest
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<i64>().ok())
        .ok_or_else(|| GitError::new("invalid commit timestamp"))?;
    Ok((name, email, timestamp))
}

fn commit_to_entry(commit: &CommitInfo) -> FileLogEntry {
    FileLogEntry {
        hash: commit.oid.chars().take(7).collect(),
        hash_full: commit.oid.clone(),
        author: truncate_chars(&commit.author, MAX_AUTHOR_RUNES),
        author_email: truncate_chars(&commit.author_email, MAX_AUTHOR_RUNES),
        subject: truncate_chars(&first_line(&commit.message), MAX_SUBJECT_RUNES),
        date: commit.author_time,
    }
}

fn first_line(s: &str) -> String {
    s.split_once('\n')
        .map(|(line, _)| line)
        .unwrap_or(s)
        .trim_end_matches([' ', '\t', '\r'])
        .to_string()
}

fn truncate_chars(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    s.chars().take(max).collect()
}

fn read_object(git_dir: &Path, oid: &str, expected_type: &str) -> Result<Vec<u8>> {
    if oid.len() < 3 {
        return Err(GitError::new("invalid object id"));
    }
    let object_path = git_dir.join("objects").join(&oid[..2]).join(&oid[2..]);
    let compressed = match fs::read(object_path) {
        Ok(compressed) => compressed,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return read_object_via_git(git_dir, oid, expected_type);
        }
        Err(err) => return Err(err.into()),
    };
    let decompressed = miniz_oxide::inflate::decompress_to_vec_zlib(&compressed)
        .map_err(|err| GitError::new(format!("zlib decompression failed: {err:?}")))?;
    let nul = decompressed
        .iter()
        .position(|b| *b == 0)
        .ok_or_else(|| GitError::new("invalid git object"))?;
    let header =
        std::str::from_utf8(&decompressed[..nul]).map_err(|err| GitError::new(err.to_string()))?;
    let typ = header
        .split_once(' ')
        .map(|(typ, _)| typ)
        .ok_or_else(|| GitError::new("invalid git object header"))?;
    if typ != expected_type {
        return Err(GitError::new(format!(
            "expected {expected_type} object, got {typ}"
        )));
    }
    Ok(decompressed[nul + 1..].to_vec())
}

fn read_object_via_git(git_dir: &Path, oid: &str, expected_type: &str) -> Result<Vec<u8>> {
    let typ = git_output(git_dir, &["cat-file", "-t", oid])?;
    let typ = String::from_utf8(typ).map_err(|err| GitError::new(err.to_string()))?;
    let typ = typ.trim();
    if typ != expected_type {
        return Err(GitError::new(format!(
            "expected {expected_type} object, got {typ}"
        )));
    }
    git_output(git_dir, &["cat-file", expected_type, oid])
}

fn git_output(git_dir: &Path, args: &[&str]) -> Result<Vec<u8>> {
    let output = Command::new("git")
        .arg("--git-dir")
        .arg(git_dir)
        .args(args)
        .output()
        .map_err(|err| GitError::new(err.to_string()))?;
    if output.status.success() {
        return Ok(output.stdout);
    }
    let stderr = String::from_utf8_lossy(&output.stderr);
    let message = stderr.trim();
    if message.is_empty() {
        Err(GitError::new(format!("git {} failed", args.join(" "))))
    } else {
        Err(GitError::new(message.to_string()))
    }
}

fn commit_tree_oid(commit: &[u8]) -> Result<String> {
    let text = std::str::from_utf8(commit).map_err(|err| GitError::new(err.to_string()))?;
    for line in text.lines() {
        if let Some(tree) = line.strip_prefix("tree ") {
            return Ok(tree.to_string());
        }
    }
    Err(GitError::new("commit has no tree"))
}

fn tree_lookup_blob(git_dir: &Path, tree_oid: &str, slash_path: &str) -> Result<Option<String>> {
    let mut current_tree = tree_oid.to_string();
    let mut parts = slash_path.split('/').peekable();
    while let Some(part) = parts.next() {
        let tree = read_object(git_dir, &current_tree, "tree")?;
        let Some(entry) = find_tree_entry(&tree, part)? else {
            return Ok(None);
        };
        if parts.peek().is_none() {
            if entry.mode.starts_with('4') {
                return Ok(None);
            }
            return Ok(Some(entry.oid));
        }
        if !entry.mode.starts_with('4') {
            return Ok(None);
        }
        current_tree = entry.oid;
    }
    Ok(None)
}

struct TreeEntry {
    mode: String,
    oid: String,
}

fn find_tree_entry(tree: &[u8], name: &str) -> Result<Option<TreeEntry>> {
    let mut i = 0;
    while i < tree.len() {
        let mode_end = tree[i..]
            .iter()
            .position(|b| *b == b' ')
            .ok_or_else(|| GitError::new("invalid tree entry"))?
            + i;
        let mode = std::str::from_utf8(&tree[i..mode_end])
            .map_err(|err| GitError::new(err.to_string()))?;
        let name_start = mode_end + 1;
        let name_end = tree[name_start..]
            .iter()
            .position(|b| *b == 0)
            .ok_or_else(|| GitError::new("invalid tree entry"))?
            + name_start;
        let entry_name = std::str::from_utf8(&tree[name_start..name_end])
            .map_err(|err| GitError::new(err.to_string()))?;
        let oid_start = name_end + 1;
        let oid_end = oid_start + 20;
        if oid_end > tree.len() {
            return Err(GitError::new("invalid tree entry"));
        }
        if entry_name == name {
            return Ok(Some(TreeEntry {
                mode: mode.to_string(),
                oid: hex_oid(&tree[oid_start..oid_end]),
            }));
        }
        i = oid_end;
    }
    Ok(None)
}

fn hex_oid(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(HEX[(b >> 4) as usize] as char);
        out.push(HEX[(b & 0x0f) as usize] as char);
    }
    out
}

fn is_binary_bytes(data: &[u8]) -> bool {
    data.iter().take(8000).any(|b| *b == 0)
}

#[derive(Clone, Copy)]
enum DiffOp {
    Eq(usize),
    Del(usize),
    Add(usize),
}

fn render_diff_lines(
    before_content: &[u8],
    after_content: &[u8],
    max_lines: usize,
) -> (Vec<WorktreeDiffLine>, bool) {
    let before = split_diff_lines(before_content);
    let after = split_diff_lines(after_content);
    let ops = line_diff_ops(&before, &after);

    let mut out = Vec::with_capacity(ops.len().min(max_lines));
    let mut old_num = 1;
    let mut new_num = 1;
    for op in ops {
        if out.len() >= max_lines {
            return (out, true);
        }
        match op {
            DiffOp::Eq(i) => {
                out.push(WorktreeDiffLine {
                    typ: "eq".to_string(),
                    text: String::from_utf8_lossy(before[i]).into_owned(),
                    old_num,
                    new_num,
                });
                old_num += 1;
                new_num += 1;
            }
            DiffOp::Del(i) => {
                out.push(WorktreeDiffLine {
                    typ: "del".to_string(),
                    text: String::from_utf8_lossy(before[i]).into_owned(),
                    old_num,
                    new_num: 0,
                });
                old_num += 1;
            }
            DiffOp::Add(i) => {
                out.push(WorktreeDiffLine {
                    typ: "add".to_string(),
                    text: String::from_utf8_lossy(after[i]).into_owned(),
                    old_num: 0,
                    new_num,
                });
                new_num += 1;
            }
        }
    }
    (out, false)
}

fn split_diff_lines(content: &[u8]) -> Vec<&[u8]> {
    if content.is_empty() {
        return Vec::new();
    }
    let body = content.strip_suffix(b"\n").unwrap_or(content);
    body.split(|b| *b == b'\n').collect()
}

fn line_diff_ops<'a>(before: &[&'a [u8]], after: &[&'a [u8]]) -> Vec<DiffOp> {
    let n = before.len();
    let m = after.len();

    // The LCS DP matrix is O(n×m); huge inputs (e.g. 1MB files) would
    // allocate multi-GB. Past this cap, emit a whole-file replace — the
    // callers truncate at MAX_WORKTREE_DIFF_LINES anyway.
    const MAX_LCS_CELLS: usize = 25_000_000;
    if (n + 1).saturating_mul(m + 1) > MAX_LCS_CELLS {
        let mut ops = Vec::with_capacity(n + m);
        ops.extend((0..n).map(DiffOp::Del));
        ops.extend((0..m).map(DiffOp::Add));
        return ops;
    }

    // u32 cells: a u16 LCS length would wrap past 65535 common lines.
    let mut lcs = vec![0u32; (n + 1) * (m + 1)];

    for i in (0..n).rev() {
        for j in (0..m).rev() {
            let idx = i * (m + 1) + j;
            lcs[idx] = if before[i] == after[j] {
                lcs[(i + 1) * (m + 1) + j + 1] + 1
            } else {
                lcs[(i + 1) * (m + 1) + j].max(lcs[i * (m + 1) + j + 1])
            };
        }
    }

    let mut ops = Vec::with_capacity(n + m);
    let mut i = 0;
    let mut j = 0;
    while i < n && j < m {
        if before[i] == after[j] {
            ops.push(DiffOp::Eq(i));
            i += 1;
            j += 1;
        } else if lcs[(i + 1) * (m + 1) + j] >= lcs[i * (m + 1) + j + 1] {
            ops.push(DiffOp::Del(i));
            i += 1;
        } else {
            ops.push(DiffOp::Add(j));
            j += 1;
        }
    }
    while i < n {
        ops.push(DiffOp::Del(i));
        i += 1;
    }
    while j < m {
        ops.push(DiffOp::Add(j));
        j += 1;
    }
    ops
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn worktree_diff_marks_identical_head_file_as_no_change() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("notes.txt"), "second file\n").expect("write notes");
        git(&root, &["add", "notes.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add notes"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());

        let diff = worktree_diff(&root, "notes.txt").expect("worktree diff");
        assert_eq!(diff.path, "notes.txt");
        assert!(diff.no_change);
        assert!(!diff.added);
        assert!(!diff.deleted);
        assert!(!diff.binary);
        assert!(!diff.truncated);
        assert!(diff.lines.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worktree_diff_emits_modified_lines_with_old_and_new_numbers() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("greeting.txt"), "alpha\nBETA\ngamma\n").expect("write greeting");
        git(&root, &["add", "greeting.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add greeting"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());
        fs::write(root.join("greeting.txt"), "alpha\nBETA-worktree\ngamma\n")
            .expect("modify greeting");

        let diff = worktree_diff(&root, "greeting.txt").expect("worktree diff");
        assert_eq!(diff.path, "greeting.txt");
        assert!(!diff.added);
        assert!(!diff.deleted);
        assert!(!diff.binary);
        assert!(!diff.no_change);
        assert!(!diff.truncated);
        assert_eq!(
            diff.lines,
            vec![
                WorktreeDiffLine {
                    typ: "eq".to_string(),
                    text: "alpha".to_string(),
                    old_num: 1,
                    new_num: 1,
                },
                WorktreeDiffLine {
                    typ: "del".to_string(),
                    text: "BETA".to_string(),
                    old_num: 2,
                    new_num: 0,
                },
                WorktreeDiffLine {
                    typ: "add".to_string(),
                    text: "BETA-worktree".to_string(),
                    old_num: 0,
                    new_num: 2,
                },
                WorktreeDiffLine {
                    typ: "eq".to_string(),
                    text: "gamma".to_string(),
                    old_num: 3,
                    new_num: 3,
                },
            ]
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worktree_diff_reads_packed_head_objects() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("greeting.txt"), "alpha\nBETA\ngamma\n").expect("write greeting");
        git(&root, &["add", "greeting.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add greeting"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());
        git(&root, &["gc", "--prune=now"]);
        fs::write(root.join("greeting.txt"), "alpha\nBETA-worktree\ngamma\n")
            .expect("modify greeting");

        let diff = worktree_diff(&root, "greeting.txt").expect("worktree diff");
        assert_eq!(diff.path, "greeting.txt");
        assert!(!diff.no_change);
        assert!(diff
            .lines
            .iter()
            .any(|line| line.typ == "del" && line.text == "BETA" && line.old_num == 2));
        assert!(diff
            .lines
            .iter()
            .any(|line| line.typ == "add" && line.text == "BETA-worktree" && line.new_num == 2));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worktree_diff_reads_gitdir_file_worktree() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        let worktree = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("greeting.txt"), "alpha\nBETA\ngamma\n").expect("write greeting");
        git(&root, &["add", "greeting.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add greeting"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());
        let worktree_str = worktree.to_string_lossy().to_string();
        git(
            &root,
            &["worktree", "add", "-q", "--detach", &worktree_str, "HEAD"],
        );
        fs::write(
            worktree.join("greeting.txt"),
            "alpha\nBETA-worktree\ngamma\n",
        )
        .expect("modify worktree greeting");

        let diff = worktree_diff(&worktree, "greeting.txt").expect("worktree diff");
        assert_eq!(diff.path, "greeting.txt");
        assert!(!diff.no_change);
        assert!(diff
            .lines
            .iter()
            .any(|line| line.typ == "del" && line.text == "BETA" && line.old_num == 2));
        assert!(diff
            .lines
            .iter()
            .any(|line| line.typ == "add" && line.text == "BETA-worktree" && line.new_num == 2));

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", &worktree_str])
            .current_dir(&root)
            .status();
        let _ = fs::remove_dir_all(worktree);
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_log_returns_empty_for_path_with_no_history() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("tracked.txt"), "tracked\n").expect("write tracked");
        git(&root, &["add", "tracked.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add tracked"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());

        let (entries, truncated) = file_log(&root, "nope.txt", 50).expect("file log");
        assert!(entries.is_empty());
        assert!(!truncated);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn commit_diff_bad_from_rev_matches_go_error_text() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("greeting.txt"), "alpha\n").expect("write greeting");
        git(&root, &["add", "greeting.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add greeting"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());

        let err = commit_diff(&root, "deadbeef", "HEAD", "greeting.txt")
            .expect_err("bad from rev should fail");
        assert_eq!(
            err.to_string(),
            "cannot resolve from=\"deadbeef\": reference not found"
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn commit_diff_accepts_first_parent_revision() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("greeting.txt"), "alpha\n").expect("write greeting");
        git(&root, &["add", "greeting.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add greeting"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());

        let diff = commit_diff(&root, "HEAD^", "HEAD", "greeting.txt")
            .expect("root parent should behave like an empty before side");
        assert!(diff.added);
        assert!(!diff.deleted);
        assert_eq!(diff.lines.len(), 1);
        assert_eq!(diff.lines[0].typ, "add");
        assert_eq!(diff.lines[0].text, "alpha");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn commit_diff_resolves_packed_full_hash_parent_revision() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("greeting.txt"), "alpha\n").expect("write greeting");
        git(&root, &["add", "greeting.txt"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", "Add greeting"])
            .current_dir(&root);
        pin_git_env(&mut commit);
        assert!(commit.status().expect("git commit").success());
        let head = git_capture(&root, &["rev-parse", "HEAD"]);
        let head = head.trim().to_string();
        git(&root, &["gc", "--prune=now"]);

        let from = format!("{head}^");
        let diff = commit_diff(&root, &from, &head, "greeting.txt")
            .expect("packed root parent should behave like an empty before side");
        assert!(diff.added);
        assert!(!diff.deleted);
        assert_eq!(diff.lines.len(), 1);
        assert_eq!(diff.lines[0].typ, "add");
        assert_eq!(diff.lines[0].text, "alpha");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_churn_counts_commits_and_tracks_recency_per_path() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);

        // hot.txt is touched by two commits, cold.txt by one.
        fs::write(root.join("hot.txt"), "v1\n").expect("write hot");
        fs::write(root.join("cold.txt"), "c1\n").expect("write cold");
        commit_all(&root, "Add hot and cold", "2020-01-02T03:04:05+00:00");
        fs::write(root.join("hot.txt"), "v2\n").expect("modify hot");
        commit_all(&root, "Touch hot again", "2021-06-07T08:09:10+00:00");

        let churn = file_churn(&root, None).expect("file churn");
        assert_eq!(churn.get("hot.txt").map(|c| c.commits), Some(2));
        assert_eq!(churn.get("cold.txt").map(|c| c.commits), Some(1));
        // Most recent commit time wins (second commit's committer date).
        let hot = churn.get("hot.txt").expect("hot stat");
        assert_eq!(hot.last_commit_time, 1623053350);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn co_change_graph_counts_pairwise_changes() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);

        // api.rs + api_test.rs co-change three times; solo.rs changes alone.
        for i in 0..3 {
            fs::write(root.join("api.rs"), format!("v{i}\n")).expect("write api");
            fs::write(root.join("api_test.rs"), format!("t{i}\n")).expect("write test");
            commit_all(&root, "touch api + test", "2020-01-02T03:04:05+00:00");
        }
        // A commit touching only one of the pair: bumps churn, not the edge.
        fs::write(root.join("api.rs"), "lone\n").expect("write api");
        commit_all(&root, "touch api alone", "2020-01-03T03:04:05+00:00");
        // An unrelated file pair that co-changes only once (below min_weight=2).
        fs::write(root.join("solo_a.rs"), "a\n").expect("write solo_a");
        fs::write(root.join("solo_b.rs"), "b\n").expect("write solo_b");
        commit_all(&root, "touch solo pair once", "2020-01-04T03:04:05+00:00");

        let graph = co_change_graph(&root, 100, None, 2).expect("co-change graph");
        assert!(!graph.truncated);
        assert_eq!(graph.commits_scanned, 5);

        // Exactly one edge survives min_weight=2: api.rs <-> api_test.rs (w=3).
        assert_eq!(graph.edges.len(), 1);
        let edge = &graph.edges[0];
        assert_eq!(edge.weight, 3);
        let endpoints: std::collections::HashSet<&str> = [
            graph.nodes[edge.source].path.as_str(),
            graph.nodes[edge.target].path.as_str(),
        ]
        .into_iter()
        .collect();
        assert!(endpoints.contains("api.rs"));
        assert!(endpoints.contains("api_test.rs"));

        // Only the two edge endpoints are emitted; the solo pair is cut.
        assert_eq!(graph.nodes.len(), 2);
        let api_node = graph
            .nodes
            .iter()
            .find(|n| n.path == "api.rs")
            .expect("api node");
        assert_eq!(api_node.commits, 4); // 3 paired + 1 solo
                                         // Worktree `api.rs` is "lone\n" (one newline) after the solo commit.
        assert_eq!(api_node.lines, 1);

        // Raising min_weight above the surviving weight empties the graph.
        let strict = co_change_graph(&root, 100, None, 4).expect("strict graph");
        assert!(strict.edges.is_empty());
        assert!(strict.nodes.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn co_change_graph_empty_for_unborn_branch() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);

        let graph = co_change_graph(&root, 100, None, 2).expect("co-change graph");
        assert!(graph.nodes.is_empty());
        assert!(graph.edges.is_empty());
        assert_eq!(graph.commits_scanned, 0);
        assert!(!graph.truncated);

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn file_churn_empty_for_unborn_branch() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);

        let churn = file_churn(&root, None).expect("file churn");
        assert!(churn.is_empty());

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repo_log_returns_recent_commits_newest_first_with_truncation() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("a.txt"), "1\n").expect("write a");
        commit_all(&root, "first commit", "2020-01-02T03:04:05+00:00");
        fs::write(root.join("b.txt"), "2\n").expect("write b");
        commit_all(&root, "second commit", "2021-06-07T08:09:10+00:00");

        let (all, truncated) = repo_log(&root, 50, None).expect("repo log");
        assert_eq!(all.len(), 2);
        assert!(!truncated);
        assert_eq!(all[0].subject, "second commit");
        assert_eq!(all[1].subject, "first commit");
        assert_eq!(all[0].date, 1623053350);
        assert_eq!(all[0].hash.len(), all[0].hash.len().min(12)); // short hash
        assert!(all[0].hash_full.starts_with(&all[0].hash));
        // "second commit"'s single parent is "first commit"; the root has none.
        assert_eq!(all[0].parents, vec![all[1].hash_full.clone()]);
        assert!(all[1].parents.is_empty());

        let (one, truncated) = repo_log(&root, 1, None).expect("repo log limit 1");
        assert_eq!(one.len(), 1);
        assert!(truncated);
        assert_eq!(one[0].subject, "second commit");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn repo_log_walks_an_explicit_ref() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("a.txt"), "1\n").expect("write a");
        commit_all(&root, "main commit", "2020-01-02T03:04:05+00:00");
        // A second branch with its own commit on top of main.
        git(&root, &["checkout", "-q", "-b", "feature"]);
        fs::write(root.join("b.txt"), "2\n").expect("write b");
        commit_all(&root, "feature commit", "2021-06-07T08:09:10+00:00");
        // Move HEAD back to main so the ref (not HEAD) drives the result.
        git(&root, &["checkout", "-q", "main"]);

        let (entries, truncated) = repo_log(&root, 50, Some("feature")).expect("repo log ref");
        assert!(!truncated);
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].subject, "feature commit");
        assert_eq!(entries[1].subject, "main commit");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn commit_files_lists_changed_paths_with_status() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("keep.txt"), "1\n").expect("write keep");
        fs::write(root.join("gone.txt"), "x\n").expect("write gone");
        commit_all(&root, "root commit", "2020-01-02T03:04:05+00:00");
        fs::write(root.join("keep.txt"), "2\n").expect("modify keep");
        fs::remove_file(root.join("gone.txt")).expect("rm gone");
        fs::write(root.join("new.txt"), "n\n").expect("write new");
        commit_all(&root, "second", "2021-06-07T08:09:10+00:00");

        let head = git_capture(&root, &["rev-parse", "HEAD"]);
        let files = commit_files(&root, head.trim()).expect("commit files");
        let by_path: std::collections::HashMap<&str, &CommitFile> =
            files.iter().map(|f| (f.path.as_str(), f)).collect();
        assert_eq!(
            by_path.get("keep.txt").map(|f| f.status.as_str()),
            Some("modified")
        );
        assert_eq!(
            by_path.get("gone.txt").map(|f| f.status.as_str()),
            Some("deleted")
        );
        assert_eq!(
            by_path.get("new.txt").map(|f| f.status.as_str()),
            Some("added")
        );
        // numstat: keep.txt swaps one line (+1/-1), gone.txt drops one (-1),
        // new.txt adds one (+1).
        let keep = by_path.get("keep.txt").expect("keep stat");
        assert_eq!((keep.additions, keep.deletions), (1, 1));
        assert_eq!(
            by_path.get("gone.txt").map(|f| (f.additions, f.deletions)),
            Some((0, 1))
        );
        assert_eq!(
            by_path.get("new.txt").map(|f| (f.additions, f.deletions)),
            Some((1, 0))
        );

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn branches_lists_locals_and_marks_current() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("a.txt"), "1\n").expect("write a");
        commit_all(&root, "init", "2020-01-02T03:04:05+00:00");
        git(&root, &["branch", "feature/x"]);
        // feature/x is one commit behind main after another commit on main.
        fs::write(root.join("b.txt"), "2\n").expect("write b");
        commit_all(&root, "second", "2021-06-07T08:09:10+00:00");

        let list = branches(&root).expect("branches");
        let by_name: std::collections::HashMap<&str, &Branch> =
            list.iter().map(|b| (b.name.as_str(), b)).collect();
        let main = by_name.get("main").expect("main");
        assert!(main.current);
        assert!(!main.hash.is_empty());
        assert_eq!(main.subject, "second");
        assert!(main.date > 0);
        // main == HEAD, so even.
        assert_eq!((main.ahead, main.behind), (0, 0));
        // feature/x sits at the first commit: 0 ahead, 1 behind HEAD.
        let feat = by_name.get("feature/x").expect("feature/x");
        assert!(!feat.current);
        assert_eq!((feat.ahead, feat.behind), (0, 1));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn tags_lists_newest_first() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("a.txt"), "1\n").expect("write a");
        commit_all(&root, "init", "2020-01-02T03:04:05+00:00");
        git(&root, &["tag", "v0.1.0"]);
        fs::write(root.join("a.txt"), "2\n").expect("write a2");
        commit_all(&root, "second", "2021-06-07T08:09:10+00:00");
        git(&root, &["tag", "v0.2.0"]);

        let list = tags(&root).expect("tags");
        let names: Vec<&str> = list.iter().map(|t| t.name.as_str()).collect();
        assert!(names.contains(&"v0.1.0"));
        assert!(names.contains(&"v0.2.0"));
        assert!(list.iter().all(|t| !t.hash.is_empty()));

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn worktrees_lists_main_and_linked() {
        if Command::new("git").arg("--version").output().is_err() {
            return;
        }
        let root = unique_temp_dir();
        let linked = unique_temp_dir();
        fs::create_dir_all(&root).expect("create temp repo");
        git(&root, &["init", "-q", "-b", "main", "."]);
        fs::write(root.join("a.txt"), "1\n").expect("write a");
        commit_all(&root, "init", "2020-01-02T03:04:05+00:00");
        let linked_str = linked.to_string_lossy().to_string();
        git(&root, &["worktree", "add", "-q", "-b", "wt", &linked_str]);

        let list = worktrees(&root).expect("worktrees");
        assert!(list.iter().any(|w| w.branch.as_deref() == Some("main")));
        let wt = list
            .iter()
            .find(|w| w.branch.as_deref() == Some("wt"))
            .expect("linked worktree present");
        assert!(!wt.head.is_empty());
        assert!(!wt.bare && !wt.detached);

        let _ = Command::new("git")
            .args(["worktree", "remove", "--force", &linked_str])
            .current_dir(&root)
            .status();
        let _ = fs::remove_dir_all(linked);
        let _ = fs::remove_dir_all(root);
    }

    fn git_capture(root: &Path, args: &[&str]) -> String {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(root);
        pin_git_env(&mut cmd);
        let out = cmd.output().expect("git capture");
        assert!(out.status.success(), "git {args:?}");
        String::from_utf8_lossy(&out.stdout).into_owned()
    }

    fn commit_all(root: &Path, message: &str, date: &str) {
        git(root, &["add", "-A"]);
        let mut commit = Command::new("git");
        commit
            .args(["commit", "-q", "-m", message])
            .current_dir(root);
        pin_git_env(&mut commit);
        commit
            .env("GIT_AUTHOR_DATE", date)
            .env("GIT_COMMITTER_DATE", date);
        assert!(commit.status().expect("git commit").success());
    }

    fn unique_temp_dir() -> PathBuf {
        // pid+nanos alone collide across parallel test threads (the system
        // clock resolution is coarser than a thread spawn), racing git init
        // against another test's remove_dir_all; the counter disambiguates.
        static SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let seq = SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("time")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "ctx_git_test_{}_{}_{}",
            std::process::id(),
            nanos,
            seq
        ))
    }

    fn git(root: &Path, args: &[&str]) {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(root);
        pin_git_env(&mut cmd);
        assert!(cmd.status().expect("git").success(), "git {args:?}");
    }

    fn pin_git_env(cmd: &mut Command) {
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0")
            .env("GIT_AUTHOR_NAME", "Parity Bot")
            .env("GIT_AUTHOR_EMAIL", "parity@example.com")
            .env("GIT_COMMITTER_NAME", "Parity Bot")
            .env("GIT_COMMITTER_EMAIL", "parity@example.com")
            .env("GIT_AUTHOR_DATE", "2020-01-02T03:04:05+00:00")
            .env("GIT_COMMITTER_DATE", "2020-01-02T03:04:05+00:00");
    }
}
