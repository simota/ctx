use std::path::{Path, PathBuf};
use std::time::SystemTime;

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
            Ok(path) => path.to_string_lossy().replace('\\', "/"),
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
    collect_pack_inputs_walk(root, path, ignore, out)?;
    out.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(())
}

fn collect_pack_inputs_walk(
    root: &Path,
    path: &Path,
    ignore: &PackIgnore,
    out: &mut Vec<ctx_pack::FileInput>,
) -> Result<(), String> {
    if path.is_file() {
        push_pack_input(root, path, ignore, out)?;
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
            collect_pack_inputs_walk(root, &path, ignore, out)?;
        } else {
            push_pack_input(root, &path, ignore, out)?;
        }
    }
    Ok(())
}

pub(crate) fn push_pack_input(
    root: &Path,
    path: &Path,
    ignore: &PackIgnore,
    out: &mut Vec<ctx_pack::FileInput>,
) -> Result<(), String> {
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
    if ignore.is_ignored_rel(&rel, path.is_dir()) {
        return Ok(());
    }
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(());
    };
    let Ok(body) = String::from_utf8(bytes.clone()) else {
        return Ok(());
    };
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
    out.push(ctx_pack::FileInput {
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
    });
    Ok(())
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
    for line in text.lines() {
        let fields: Vec<&str> = line.split_whitespace().collect();
        if fields.len() < 4 || fields[0] != "diff" || fields[1] != "--git" {
            continue;
        }
        for raw in &fields[2..4] {
            let path = clean_pack_diff_path(raw);
            if path.is_empty() || !seen.insert(path.clone()) {
                continue;
            }
            paths.push(path);
        }
    }
    paths
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
    let cleaned = cleaned.to_string_lossy().replace('\\', "/");
    if cleaned == "." {
        String::new()
    } else {
        cleaned
    }
}
