use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

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
    let from_oid = resolve_revision(&git_dir, from_rev)
        .map_err(|err| GitError::new(format!("cannot resolve from={from_rev:?}: {err}")))?;
    let to_oid = resolve_revision(&git_dir, to_rev)
        .map_err(|err| GitError::new(format!("cannot resolve to={to_rev:?}: {err}")))?;

    let from_commit = read_commit(&git_dir, &from_oid)?;
    let to_commit = read_commit(&git_dir, &to_oid)?;
    let (before_content, before_binary, before_exists) =
        file_content_at_commit(&git_dir, &from_commit, slash_path)?;
    let (after_content, after_binary, after_exists) =
        file_content_at_commit(&git_dir, &to_commit, slash_path)?;

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

fn git_dir(repo_root: &Path) -> PathBuf {
    repo_root.join(".git")
}

fn resolve_revision(git_dir: &Path, rev: &str) -> Result<String> {
    if rev == "HEAD" {
        return read_head_oid(git_dir)?.ok_or_else(|| GitError::new("reference not found"));
    }
    if is_hex_prefix(rev) {
        if let Some(oid) = resolve_loose_oid_prefix(git_dir, rev)? {
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
    let compressed = fs::read(object_path)?;
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
