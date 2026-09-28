use std::path::{Path, PathBuf};
use std::time::SystemTime;

use rayon::prelude::*;

use super::*;
use crate::commands::where_cmd::extract_where_symbols;

pub(crate) fn read_pack_paths_ordered(
    root: &Path,
    paths: &[String],
    budget: i64,
    quiet: bool,
) -> Result<Vec<NativePackFile>, String> {
    let mut files = Vec::new();
    let mut used = 0_i64;
    for path in paths {
        let clean = ctx_pack::from_where::clean_input_path(path);
        if clean.is_empty() {
            continue;
        }
        let abs = root.join(&clean);
        let rel = match abs.strip_prefix(root) {
            Ok(path) => path_to_slash_lossy(path),
            Err(_) => clean.clone(),
        };
        let body = match std::fs::read_to_string(&abs) {
            Ok(body) => body,
            Err(_) => {
                if !quiet {
                    eprintln!("warning: --from-where: path not found or excluded: {clean}");
                }
                continue;
            }
        };
        let tokens = estimate_text_tokens(&body);
        if used + tokens > budget {
            continue;
        }
        used += tokens;
        let lines: Vec<String> = body.lines().map(ToString::to_string).collect();
        let symbols = extract_where_symbols(&rel, &lines)
            .into_iter()
            .map(|sym| sym.name)
            .collect();
        files.push(NativePackFile {
            path: rel,
            abs_path: abs.to_string_lossy().into_owned(),
            content: body,
            tokens,
            score: 0,
            relevance: "selected".to_string(),
            reason: "from-where".to_string(),
            symbols,
        });
    }
    Ok(files)
}

pub(crate) fn read_pack_root(
    root: &Path,
    args: &PackArgs,
    cfg: &PackCtxToml,
    only_paths: Option<&[String]>,
    respect_ctxignore: bool,
) -> Result<Vec<NativePackFile>, String> {
    let mut inputs = Vec::new();
    let base = if root.is_file() {
        root.parent().unwrap_or_else(|| Path::new("."))
    } else {
        root
    };
    let ignore = PackIgnore::load(base, &cfg.ignore.patterns, respect_ctxignore);
    collect_pack_inputs(base, root, &ignore, &mut inputs)?;
    apply_pack_time_filters(base, &mut inputs, args)?;
    if args.changed {
        let changed = git_changed_paths(base)?;
        inputs.retain(|input| changed.contains(&input.path));
    }
    if args.api_only {
        inputs.retain(|input| supports_api_only_light(&input.path));
    }
    if let Some(paths) = only_paths {
        let wanted: std::collections::BTreeSet<String> = paths
            .iter()
            .map(|path| clean_pack_input_path(path))
            .collect();
        inputs.retain(|input| wanted.contains(&input.path));
    }
    let ctx = ctx_pack::RelevanceContext::new(&args.goal, args.budget);
    let mut scored = Vec::new();
    let mut skipped = Vec::new();
    for input in inputs {
        let result = ctx_pack::relevance::score_relevance_with_ctx(&input, &ctx, input.tokens);
        if result.tier.is_empty() {
            skipped.push((input.path, result.reason));
        } else {
            scored.push((input, result));
        }
    }
    scored.sort_by(|a, b| {
        if a.1.score != b.1.score {
            b.1.score.cmp(&a.1.score)
        } else {
            a.0.path.cmp(&b.0.path)
        }
    });
    skipped.sort_by(|a, b| a.0.cmp(&b.0));

    let mut files = Vec::new();
    let mut used = 0_i64;
    for (input, result) in scored {
        if args.budget > 0 && used + input.tokens > args.budget {
            if !args.no_warnings {
                eprintln!("warning: pack: skipped {}: budget exceeded", input.path);
            }
            continue;
        }
        let body = read_native_pack_content(&input.path, &input.abs_path, args, cfg)?;
        let tokens = estimate_text_tokens(&body);
        let symbols = input
            .metadata
            .symbols
            .iter()
            .map(|sym| sym.name.clone())
            .collect();
        used += tokens;
        files.push(NativePackFile {
            path: input.path,
            abs_path: input.abs_path,
            content: body,
            tokens,
            score: result.score,
            relevance: result.tier,
            reason: result.reason,
            symbols,
        });
    }
    Ok(files)
}

pub(crate) fn apply_pack_time_filters(
    root: &Path,
    inputs: &mut Vec<ctx_pack::FileInput>,
    args: &PackArgs,
) -> Result<(), String> {
    if args.since.is_empty() && args.until.is_empty() {
        return Ok(());
    }
    let now = SystemTime::now();
    let since = if args.since.is_empty() {
        None
    } else {
        Some(parse_pack_time_filter(&args.since, now).map_err(|err| format!("--since: {err}"))?)
    };
    let until = if args.until.is_empty() {
        None
    } else {
        Some(parse_pack_time_filter(&args.until, now).map_err(|err| format!("--until: {err}"))?)
    };
    let git_times = if args.use_mtime {
        None
    } else {
        build_git_commit_time_index(root, since)
    };
    inputs.retain(|input| {
        let Some(modified) = pack_input_effective_time(input, git_times.as_ref()) else {
            return false;
        };
        if let Some(since) = since {
            if modified < since {
                return false;
            }
        }
        if let Some(until) = until {
            if modified > until {
                return false;
            }
        }
        true
    });
    Ok(())
}

pub(crate) fn pack_input_effective_time(
    input: &ctx_pack::FileInput,
    git_times: Option<&GitTimeIndex>,
) -> Option<SystemTime> {
    if let Some(git_times) = git_times {
        if let Some(time) = git_times.commit_times.get(&input.path) {
            return Some(*time);
        }
        if git_times.head_paths.contains(&input.path) {
            return None;
        }
    }
    let meta = std::fs::metadata(&input.abs_path).ok()?;
    meta.modified().ok()
}

pub(crate) fn collect_pack_inputs(
    root: &Path,
    path: &Path,
    ignore: &PackIgnore,
    out: &mut Vec<ctx_pack::FileInput>,
) -> Result<(), String> {
    // Two-phase: a cheap sequential walk enumerates candidate file paths
    // (readdir + symlink stat + directory pruning only — no file read or symbol
    // extraction), then the per-file work (read + symbol parse + token estimate)
    // runs in parallel. Each file maps to a self-contained `FileInput`, so the
    // result order is irrelevant — the `sort_by(path)` below restores the exact
    // deterministic order, keeping byte-parity with the sequential version.
    let mut candidates = Vec::new();
    collect_pack_input_paths(root, path, ignore, &mut candidates)?;
    let collected: Vec<ctx_pack::FileInput> = candidates
        .par_iter()
        .filter_map(|path| build_pack_input(root, path, ignore))
        .collect();
    out.extend(collected);
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(())
}

/// Enumerate candidate file paths under `path`, applying the same directory
/// pruning the original walk did. File-level ignore / read / UTF-8 filtering is
/// deferred to [`build_pack_input`] so it can run in parallel.
fn collect_pack_input_paths(
    root: &Path,
    path: &Path,
    ignore: &PackIgnore,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if path.is_file() {
        out.push(path.to_path_buf());
        return Ok(());
    }
    for entry in std::fs::read_dir(path).map_err(|err| format!("walk {}: {err}", path.display()))? {
        let entry = entry.map_err(|err| err.to_string())?;
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if matches!(
            name.as_ref(),
            ".git" | "node_modules" | "dist" | "coverage" | "target"
        ) {
            continue;
        }
        // symlink_metadata (NOT is_dir, which follows links): a symlinked dir
        // is never recursed into, so cyclic links cannot loop. Consistent
        // with tree/json.rs.
        let meta = std::fs::symlink_metadata(&path).map_err(|err| err.to_string())?;
        if meta.is_dir() {
            if ignore.is_ignored(root, &path, true) {
                continue;
            }
            collect_pack_input_paths(root, &path, ignore, out)?;
        } else {
            out.push(path);
        }
    }
    Ok(())
}

/// Read and parse a single candidate file into a `FileInput`, or `None` when it
/// is ignored, unreadable, or not valid UTF-8 (mirroring the original
/// `push_pack_input` skip cases). Pure given `(root, path, ignore)`, so it is
/// safe to call concurrently across files.
pub(crate) fn build_pack_input(
    root: &Path,
    path: &Path,
    ignore: &PackIgnore,
) -> Option<ctx_pack::FileInput> {
    let rel = path_to_slash_lossy(path.strip_prefix(root).unwrap_or(path));
    if ignore.is_ignored_rel(&rel, path.is_dir()) {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    let body = String::from_utf8(bytes.clone()).ok()?;
    let lines: Vec<String> = body.lines().map(ToString::to_string).collect();
    let symbols = extract_where_symbols(&rel, &lines)
        .into_iter()
        .map(|sym| ctx_pack::SymbolInput {
            name: sym.name,
            kind: sym.kind,
            line: sym.line,
        })
        .collect();
    let tokens = estimate_text_tokens(&body);
    Some(ctx_pack::FileInput {
        path: rel,
        abs_path: path.to_string_lossy().into_owned(),
        is_dir: false,
        tokens,
        role: String::new(),
        metadata: ctx_pack::MetadataInput {
            size: bytes.len() as i64,
            tokens_est: tokens,
            role: String::new(),
            symbols,
        },
        content_head: bytes.into_iter().take(512).collect(),
    })
}

pub(crate) fn parse_pack_stdin_paths(text: &str) -> Vec<String> {
    if text.contains("diff --git ") {
        return parse_pack_git_diff_paths(text);
    }
    parse_pack_path_list(text)
}

pub(crate) fn parse_pack_git_diff_paths(text: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut paths = Vec::new();

    let mut push = |raw: String| {
        let path = clean_pack_diff_path(&raw);
        if !path.is_empty() && seen.insert(path.clone()) {
            paths.push(path);
        }
    };

    for line in text.lines() {
        // Prefer the one-path-per-line headers when present. Unlike
        // `diff --git a/<old> b/<new>`, these stay unambiguous when a path
        // contains spaces.
        for prefix in ["--- ", "+++ "] {
            if let Some(raw) = line.strip_prefix(prefix).and_then(parse_git_path_line) {
                push(raw);
            }
        }
        for prefix in ["rename from ", "rename to ", "copy from ", "copy to "] {
            if let Some(raw) = line.strip_prefix(prefix).and_then(parse_git_path_line) {
                push(raw);
            }
        }

        if let Some(rest) = line.strip_prefix("diff --git ") {
            if let Some((before, after)) = parse_git_diff_header_paths(rest) {
                push(before);
                push(after);
            }
        }
    }
    paths
}

/// Decode one complete Git path field. Git C-quotes control bytes and
/// backslashes; ordinary paths (including spaces) are emitted verbatim.
fn parse_git_path_line(raw: &str) -> Option<String> {
    if raw.starts_with('"') {
        let (path, rest) = parse_git_quoted_path(raw)?;
        rest.trim().is_empty().then_some(path)
    } else {
        Some(raw.trim_end().to_string())
    }
}

/// Parse the two paths from `diff --git a/<old> b/<new>`.
///
/// Git does not quote spaces, so the separator is not always the first space.
/// When several ` b/` candidates exist, prefer the split whose a/ and b/
/// sides name the same path (the overwhelmingly common non-rename case).
/// Renames/copies with genuinely ambiguous headers are still recovered from
/// their unambiguous `rename from/to` or `copy from/to` lines.
fn parse_git_diff_header_paths(raw: &str) -> Option<(String, String)> {
    if raw.starts_with('"') {
        let (before, rest) = parse_git_quoted_path(raw)?;
        let rest = rest.trim_start();
        let (after, tail) = if rest.starts_with('"') {
            parse_git_quoted_path(rest)?
        } else {
            (rest.to_string(), "")
        };
        if !tail.trim().is_empty() {
            return None;
        }
        return Some((before, after));
    }

    let candidates: Vec<usize> = raw.match_indices(" b/").map(|(idx, _)| idx).collect();
    if candidates.is_empty() {
        return None;
    }
    for &idx in &candidates {
        let before = &raw[..idx];
        let after = &raw[idx + 1..];
        if clean_pack_diff_path(before) == clean_pack_diff_path(after) {
            return Some((before.to_string(), after.to_string()));
        }
    }
    if candidates.len() == 1 {
        let idx = candidates[0];
        return Some((raw[..idx].to_string(), raw[idx + 1..].to_string()));
    }
    None
}

/// Parse Git's double-quoted path syntax, including C escapes and octal bytes.
fn parse_git_quoted_path(input: &str) -> Option<(String, &str)> {
    let bytes = input.as_bytes();
    if bytes.first() != Some(&b'"') {
        return None;
    }
    let mut out = Vec::new();
    let mut i = 1usize;
    while i < bytes.len() {
        match bytes[i] {
            b'"' => {
                let path = String::from_utf8_lossy(&out).into_owned();
                return Some((path, &input[i + 1..]));
            }
            b'\\' => {
                i += 1;
                let escaped = *bytes.get(i)?;
                match escaped {
                    b'a' => out.push(0x07),
                    b'b' => out.push(0x08),
                    b't' => out.push(b'\t'),
                    b'n' => out.push(b'\n'),
                    b'v' => out.push(0x0b),
                    b'f' => out.push(0x0c),
                    b'r' => out.push(b'\r'),
                    b'\\' | b'"' => out.push(escaped),
                    b'0'..=b'7' => {
                        let mut value = u16::from(escaped - b'0');
                        let mut digits = 1;
                        while digits < 3
                            && i + 1 < bytes.len()
                            && matches!(bytes[i + 1], b'0'..=b'7')
                        {
                            i += 1;
                            digits += 1;
                            value = value * 8 + u16::from(bytes[i] - b'0');
                        }
                        if value > u16::from(u8::MAX) {
                            return None;
                        }
                        out.push(value as u8);
                    }
                    _ => return None,
                }
            }
            byte => out.push(byte),
        }
        i += 1;
    }
    None
}

pub(crate) fn parse_pack_path_list(text: &str) -> Vec<String> {
    let mut seen = std::collections::BTreeSet::new();
    let mut paths = Vec::new();
    for line in text.lines() {
        let path = clean_pack_input_path(line);
        if path.is_empty() || !seen.insert(path.clone()) {
            continue;
        }
        paths.push(path);
    }
    paths
}

pub(crate) fn clean_pack_diff_path(raw: &str) -> String {
    let path = raw.trim_matches('"');
    let path = path.strip_prefix("a/").unwrap_or(path);
    let path = path.strip_prefix("b/").unwrap_or(path);
    clean_pack_input_path(path)
}

pub(crate) fn clean_pack_input_path(raw: &str) -> String {
    let path = raw.trim().trim_matches('"');
    if path.is_empty() || path == "/dev/null" {
        return String::new();
    }
    let cleaned = Path::new(path).components().collect::<PathBuf>();
    let cleaned = path_to_slash_lossy(&cleaned);
    if cleaned == "." {
        String::new()
    } else {
        cleaned
    }
}

#[cfg(test)]
mod special_path_tests {
    use super::*;

    #[test]
    fn git_diff_path_parser_handles_spaces_and_c_escapes() {
        let patch = concat!(
            "diff --git a/with space.rs b/with space.rs\n",
            "--- a/with space.rs\n",
            "+++ b/with space.rs\n",
            "diff --git \"a/line\\nbreak.rs\" \"b/line\\nbreak.rs\"\n",
            "--- \"a/line\\nbreak.rs\"\n",
            "+++ \"b/line\\nbreak.rs\"\n",
            "diff --git \"a/literal\\\\name.rs\" \"b/literal\\\\name.rs\"\n",
            "--- \"a/literal\\\\name.rs\"\n",
            "+++ \"b/literal\\\\name.rs\"\n",
        );

        assert_eq!(
            parse_pack_git_diff_paths(patch),
            vec![
                "with space.rs".to_string(),
                "line\nbreak.rs".to_string(),
                r"literal\name.rs".to_string(),
            ]
        );
    }

    #[test]
    fn git_diff_path_parser_uses_rename_headers_for_spaced_paths() {
        let patch = concat!(
            "diff --git a/old name.rs b/new name.rs\n",
            "similarity index 100%\n",
            "rename from old name.rs\n",
            "rename to new name.rs\n",
        );
        assert_eq!(
            parse_pack_git_diff_paths(patch),
            vec!["old name.rs".to_string(), "new name.rs".to_string()]
        );
    }

    #[cfg(unix)]
    #[test]
    fn clean_pack_input_path_preserves_literal_backslash_on_unix() {
        assert_eq!(
            clean_pack_input_path(r"literal\name.rs"),
            r"literal\name.rs"
        );
    }
}
