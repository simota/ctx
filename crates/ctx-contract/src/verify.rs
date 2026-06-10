// crates/ctx-contract/src/verify.rs
//
// Port of internal/contract/verify.go.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Component, Path, PathBuf};

use crate::hash::sha256_hex;
use crate::parse_refs::extract_references;
use crate::types::{
    Contract, File, Reference, Result as VerifyResult, StaleFile, VerifyOptions, Violation,
    ViolationKind, OK,
};

/// Cross-checks each reference in `response` against `c`. Returns a
/// `Result` whose `exit_code` is 0 when no violations are detected,
/// otherwise 1. Mirrors `contract.Verify`.
pub fn verify(c: &Contract, response: &[u8], opts: &VerifyOptions) -> VerifyResult {
    let refs = extract_references(response);

    // Pre-build lookup tables.
    let mut by_path: BTreeMap<String, File> = BTreeMap::new();
    for f in &c.files {
        by_path.insert(f.path.clone(), f.clone());
    }
    let mut all_symbols: std::collections::HashSet<String> = std::collections::HashSet::new();
    for f in &c.files {
        for s in &f.symbols {
            all_symbols.insert(s.clone());
        }
    }
    let mut cited_paths: BTreeMap<String, CitedFile> = BTreeMap::new();

    let mut res = VerifyResult {
        pack_file: String::new(),
        schema_version: c.schema_version,
        total_files_in_contract: c.files.len() as i32,
        references_found: refs.len() as i32,
        ..Default::default()
    };

    for r in &refs {
        match r.kind.as_str() {
            "file" => match lookup_path(&by_path, &r.path) {
                Some(f) => {
                    res.ok.push(OK {
                        kind: "file".into(),
                        path: f.path.clone(),
                        source_line: r.source_line,
                        ..Default::default()
                    });
                    cite_whole_file(&mut cited_paths, f);
                }
                None => res.violations.push(Violation {
                    kind: ViolationKind::OutOfContext,
                    path: r.path.clone(),
                    source_line: r.source_line,
                    message: "file path is not in pack contract".into(),
                    ..Default::default()
                }),
            },
            "line-range" => match lookup_path(&by_path, &r.path) {
                None => res.violations.push(Violation {
                    kind: ViolationKind::OutOfContext,
                    path: r.path.clone(),
                    line_start: r.line_start,
                    line_end: r.line_end,
                    source_line: r.source_line,
                    message: "file path is not in pack contract".into(),
                    ..Default::default()
                }),
                Some(f) => {
                    if range_contained(r.line_start, r.line_end, f.line_start, f.line_end) {
                        res.ok.push(OK {
                            kind: "line-range".into(),
                            path: f.path.clone(),
                            line_start: r.line_start,
                            line_end: r.line_end,
                            source_line: r.source_line,
                            ..Default::default()
                        });
                        cite_line_range(&mut cited_paths, f, r.clone());
                    } else {
                        res.violations.push(Violation {
                            kind: ViolationKind::StaleContent,
                            path: f.path.clone(),
                            line_start: r.line_start,
                            line_end: r.line_end,
                            source_line: r.source_line,
                            message: "referenced line range is outside the contract span".into(),
                            expected_sha: f.sha256.clone(),
                            ..Default::default()
                        });
                    }
                }
            },
            "symbol" => {
                if opts.no_symbols {
                    continue;
                }
                if all_symbols.contains(&r.symbol) {
                    res.ok.push(OK {
                        kind: "symbol".into(),
                        symbol: r.symbol.clone(),
                        source_line: r.source_line,
                        ..Default::default()
                    });
                    continue;
                }
                let base = last_dot_segment(&r.symbol);
                if base != r.symbol && all_symbols.contains(base) {
                    res.ok.push(OK {
                        kind: "symbol".into(),
                        symbol: r.symbol.clone(),
                        source_line: r.source_line,
                        ..Default::default()
                    });
                    continue;
                }
                res.violations.push(Violation {
                    kind: ViolationKind::PhantomSymbol,
                    symbol: r.symbol.clone(),
                    source_line: r.source_line,
                    message: "symbol referenced in response but not in pack".into(),
                    ..Default::default()
                });
            }
            "diff-header" => match lookup_path(&by_path, &r.path) {
                Some(f) => {
                    res.ok.push(OK {
                        kind: "diff-header".into(),
                        path: f.path.clone(),
                        source_line: r.source_line,
                        ..Default::default()
                    });
                    cite_whole_file(&mut cited_paths, f);
                }
                None => res.violations.push(Violation {
                    kind: ViolationKind::OutOfContext,
                    path: r.path.clone(),
                    source_line: r.source_line,
                    message: "diff +++ header references a path not in pack contract".into(),
                    ..Default::default()
                }),
            },
            _ => {}
        }
    }

    if !opts.worktree_root.is_empty() {
        add_worktree_staleness(&mut res, &opts.worktree_root, &cited_paths);
    }

    if opts.strict && res.violations.is_empty() && res.references_found == 0 {
        res.violations.push(Violation {
            kind: ViolationKind::OutOfContext,
            message: "strict: response contains no verifiable references".into(),
            ..Default::default()
        });
    }

    if !res.violations.is_empty() {
        res.exit_code = 1;
    }
    res
}

#[derive(Debug, Clone, Default)]
struct CitedFile {
    file: File,
    whole: bool,
    ranges: Vec<Reference>,
}

fn cite_whole_file(cited: &mut BTreeMap<String, CitedFile>, f: File) {
    let entry = cited.entry(f.path.clone()).or_default();
    entry.file = f;
    entry.whole = true;
}

fn cite_line_range(cited: &mut BTreeMap<String, CitedFile>, f: File, r: Reference) {
    let entry = cited.entry(f.path.clone()).or_default();
    entry.file = f;
    entry.ranges.push(r);
}

fn add_worktree_staleness(res: &mut VerifyResult, root: &str, cited: &BTreeMap<String, CitedFile>) {
    if cited.is_empty() {
        return;
    }
    let mut suggestions: BTreeSet<String> = res.repack_suggestions.iter().cloned().collect();
    // BTreeMap iteration is already path-sorted.
    for c in cited.values() {
        let f = &c.file;
        if !c.whole && !c.ranges.is_empty() && !f.line_hashes.is_empty() {
            add_line_range_staleness(res, root, f, &c.ranges, &mut suggestions);
            continue;
        }
        let (got, message) = worktree_sha(root, &f.path);
        if got == f.sha256 && message.is_empty() {
            continue;
        }
        let mut msg = message.clone();
        if msg.is_empty() {
            msg = "worktree file differs from pack contract".to_string();
        }
        let sf = StaleFile {
            path: f.path.clone(),
            expected_sha: f.sha256.clone(),
            got_sha: got.clone(),
            message: msg.clone(),
        };
        res.stale_files.push(sf);
        add_repack_suggestion(res, &mut suggestions, &f.path);
        res.violations.push(Violation {
            kind: ViolationKind::StaleContent,
            path: f.path.clone(),
            expected_sha: f.sha256.clone(),
            got_sha: got,
            message: msg,
            ..Default::default()
        });
    }
}

fn add_line_range_staleness(
    res: &mut VerifyResult,
    root: &str,
    f: &File,
    refs: &[Reference],
    suggestions: &mut BTreeSet<String>,
) {
    for r in refs {
        let (start, end) = normalise_range(r.line_start, r.line_end);
        let Some(expected) = contract_line_range_sha(f, start, end) else {
            add_repack_suggestion(res, suggestions, &f.path);
            res.violations.push(Violation {
                kind: ViolationKind::StaleContent,
                path: f.path.clone(),
                line_start: start,
                line_end: end,
                source_line: r.source_line,
                message: "contract is missing line hashes for referenced range".into(),
                ..Default::default()
            });
            continue;
        };
        let (got, mut message) = worktree_line_range_sha(root, &f.path, start, end);
        if got == expected && message.is_empty() {
            continue;
        }
        if message.is_empty() {
            message = "worktree line range differs from pack contract".into();
        }
        res.stale_files.push(StaleFile {
            path: f.path.clone(),
            expected_sha: expected.clone(),
            got_sha: got.clone(),
            message: message.clone(),
        });
        add_repack_suggestion(res, suggestions, &f.path);
        res.violations.push(Violation {
            kind: ViolationKind::StaleContent,
            path: f.path.clone(),
            line_start: start,
            line_end: end,
            expected_sha: expected,
            got_sha: got,
            source_line: r.source_line,
            message,
            ..Default::default()
        });
    }
}

fn add_repack_suggestion(res: &mut VerifyResult, seen: &mut BTreeSet<String>, path: &str) {
    if seen.insert(path.to_string()) {
        res.repack_suggestions.push(path.to_string());
    }
}

fn worktree_sha(root: &str, rel: &str) -> (String, String) {
    let Some(full) = clean_worktree_path(root, rel) else {
        return (
            String::new(),
            "contract path cannot be resolved inside worktree".to_string(),
        );
    };
    match std::fs::read(&full) {
        Ok(body) => (sha256_hex(&body), String::new()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (String::new(), "worktree file is missing".to_string())
        }
        Err(e) => (String::new(), e.to_string()),
    }
}

fn clean_worktree_path(root: &str, rel: &str) -> Option<PathBuf> {
    let rel_path = Path::new(rel);
    // Reject absolute paths up front.
    if rel_path.is_absolute() {
        return None;
    }
    // PARITY (F-02): Go runs `filepath.Clean(filepath.FromSlash(rel))`
    // first which collapses `a/../b` to `b` *before* checking for
    // traversal. We mirror that by cancelling `..` against the
    // preceding Normal component, only rejecting if a `..` remains
    // after cancellation (i.e. would escape `root`).
    let mut clean: Vec<std::ffi::OsString> = Vec::new();
    for comp in rel_path.components() {
        match comp {
            Component::Normal(p) => clean.push(p.to_os_string()),
            Component::CurDir => {}
            Component::ParentDir => {
                if clean.pop().is_none() {
                    return None;
                }
            }
            Component::RootDir | Component::Prefix(_) => {
                return None;
            }
        }
    }
    if clean.is_empty() {
        return None;
    }
    let mut cleaned = PathBuf::new();
    for c in &clean {
        cleaned.push(c);
    }
    Some(Path::new(root).join(&cleaned))
}

fn worktree_line_range_sha(root: &str, rel: &str, start: i32, end: i32) -> (String, String) {
    let Some(full) = clean_worktree_path(root, rel) else {
        return (
            String::new(),
            "contract path cannot be resolved inside worktree".to_string(),
        );
    };
    match std::fs::read(&full) {
        Ok(body) => {
            let lines = split_logical_lines(&body);
            if start <= 0 || end <= 0 || start as usize > lines.len() || end as usize > lines.len()
            {
                return (
                    String::new(),
                    "referenced line range cannot be resolved in worktree".to_string(),
                );
            }
            let hashes: Vec<String> = lines[(start as usize - 1)..(end as usize)]
                .iter()
                .map(|line| sha256_hex(line))
                .collect();
            (hash_line_hashes(&hashes), String::new())
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            (String::new(), "worktree file is missing".to_string())
        }
        Err(e) => (String::new(), e.to_string()),
    }
}

fn contract_line_range_sha(f: &File, start: i32, end: i32) -> Option<String> {
    if start <= 0 || end <= 0 {
        return None;
    }
    let by_line: BTreeMap<i32, String> = f
        .line_hashes
        .iter()
        .map(|h| (h.line, h.sha256.clone()))
        .collect();
    let mut hashes = Vec::with_capacity((end - start + 1) as usize);
    for line in start..=end {
        let h = by_line.get(&line)?;
        hashes.push(h.clone());
    }
    Some(hash_line_hashes(&hashes))
}

fn hash_line_hashes(hashes: &[String]) -> String {
    sha256_hex(hashes.join("\n").as_bytes())
}

fn split_logical_lines(b: &[u8]) -> Vec<&[u8]> {
    if b.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (i, byte) in b.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(&b[start..=i]);
            start = i + 1;
        }
    }
    if start < b.len() {
        lines.push(&b[start..]);
    }
    lines
}

fn lookup_path(by_path: &BTreeMap<String, File>, p: &str) -> Option<File> {
    let p = p.strip_prefix("./").unwrap_or(p);
    if let Some(f) = by_path.get(p) {
        return Some(f.clone());
    }
    // PARITY (F-07): Go's `strings.ToLower` is Unicode-aware (e.g.
    // `İ → i̇`). `to_ascii_lowercase` only touches `A`-`Z`, so non-ASCII
    // contract paths that should match would miss. Use `to_lowercase`
    // (full Unicode) on both sides of the comparison.
    let lp = p.to_lowercase();
    for (k, f) in by_path {
        if k.to_lowercase() == lp {
            return Some(f.clone());
        }
    }
    None
}

fn range_contained(a_start: i32, a_end: i32, b_start: i32, b_end: i32) -> bool {
    if a_start <= 0 || a_end <= 0 {
        return false;
    }
    let (a_start, a_end) = normalise_range(a_start, a_end);
    a_start >= b_start && a_end <= b_end
}

fn normalise_range(start: i32, end: i32) -> (i32, i32) {
    if end < start {
        (end, start)
    } else {
        (start, end)
    }
}

fn last_dot_segment(s: &str) -> &str {
    match s.rfind('.') {
        Some(i) => &s[i + 1..],
        None => s,
    }
}
