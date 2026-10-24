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
    let canonical_root = crate::common::canonical_walk_root(root);
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
        // The where JSON may be hand-written or saved: like the walker, read
        // only regular files inside the repository (no absolute or `..`
        // escapes, no links leaving it, no FIFOs).
        let body = match crate::common::resolves_to_file_in(&canonical_root, &abs)
            .then(|| std::fs::read_to_string(&abs).ok())
            .flatten()
        {
            Some(body) => body,
            None => {
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
    let canonical_root = crate::common::canonical_walk_root(root);
    collect_pack_input_paths(root, &canonical_root, path, ignore, &mut candidates)?;
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
/// deferred to [`build_pack_input`] so it can run in parallel. Entries are
/// classified by [`crate::common::walk_entry_kind`], so symlinks leaving
/// `canonical_root` and special files never reach the reader.
fn collect_pack_input_paths(
    root: &Path,
    canonical_root: &Path,
    path: &Path,
    ignore: &PackIgnore,
    out: &mut Vec<PathBuf>,
) -> Result<(), String> {
    if path.is_file() {
        // A single-file root (`ctx pack README.md`) can itself be a link
        // leaving the repository.
        if crate::common::resolves_to_file_in(canonical_root, path) {
            out.push(path.to_path_buf());
        }
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
        // file_type does not follow links: a symlinked dir is never recursed
        // into, so cyclic links cannot loop. Consistent with tree/json.rs.
        let file_type = entry.file_type().map_err(|err| err.to_string())?;
        match crate::common::walk_entry_kind(canonical_root, &path, file_type) {
            crate::common::WalkEntryKind::Dir => {
                if ignore.is_ignored(root, &path, true) {
                    continue;
                }
                collect_pack_input_paths(root, canonical_root, &path, ignore, out)?;
            }
            crate::common::WalkEntryKind::File => out.push(path),
            crate::common::WalkEntryKind::Skip => {}
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
    let rel = path
        .strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/");
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

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::common::test_support::{finishes_within, TempDir};

    fn walk(root: &Path) -> Vec<String> {
        let root = root.to_path_buf();
        finishes_within(30, move || {
            let ignore = PackIgnore::load(&root, &[], true);
            let mut inputs = Vec::new();
            collect_pack_inputs(&root, &root, &ignore, &mut inputs).unwrap();
            inputs.into_iter().map(|input| input.path).collect()
        })
    }

    #[test]
    fn pack_walk_keeps_in_root_links_and_skips_links_leaving_root() {
        let outside = TempDir::new("pack-walk-outside");
        outside.write("secret.txt", "OUTSIDE_SECRET=1\n");
        let fx = TempDir::new("pack-walk-links");
        fx.write("src/main.rs", "fn main() {}\n");
        fx.symlink("main.rs", "src/alias.rs");
        fx.symlink(outside.0.join("secret.txt"), "leak.rs");
        fx.symlink(".", "loop");

        assert_eq!(walk(&fx.0), vec!["src/alias.rs", "src/main.rs"]);
    }

    #[test]
    fn pack_single_file_root_skips_a_link_leaving_root() {
        let outside = TempDir::new("pack-file-root-outside");
        outside.write("secret.txt", "OUTSIDE_SECRET=1\n");
        let fx = TempDir::new("pack-file-root");
        fx.write("main.rs", "fn main() {}\n");
        fx.symlink("main.rs", "alias.rs");
        fx.symlink(outside.0.join("secret.txt"), "leak.rs");

        let walk_file = |rel: &str| {
            let ignore = PackIgnore::load(&fx.0, &[], true);
            let mut inputs = Vec::new();
            collect_pack_inputs(&fx.0, &fx.0.join(rel), &ignore, &mut inputs).unwrap();
            inputs
                .into_iter()
                .map(|input| input.path)
                .collect::<Vec<_>>()
        };
        assert_eq!(walk_file("main.rs"), vec!["main.rs"]);
        assert_eq!(walk_file("alias.rs"), vec!["alias.rs"]);
        assert!(walk_file("leak.rs").is_empty());
    }

    #[test]
    fn pack_walk_applies_ctxignore_with_a_non_utf8_line() {
        let fx = TempDir::new("pack-walk-ctxignore-sjis");
        std::fs::write(fx.0.join(".ctxignore"), b"# \x83R\x83\x81\r\nsecrets/\r\n").unwrap();
        fx.write("main.rs", "fn main() {}\n");
        fx.write("secrets/key.txt", "API_KEY=1\n");

        // .ctxignore itself is skipped as binary (non-UTF-8 content).
        assert_eq!(walk(&fx.0), vec!["main.rs"]);
    }

    #[test]
    fn pack_walk_skips_fifos() {
        let fx = TempDir::new("pack-walk-fifo");
        fx.write("main.rs", "fn main() {}\n");
        fx.mkfifo("pipe");
        fx.symlink("pipe", "pipe-link");

        assert_eq!(walk(&fx.0), vec!["main.rs"]);
    }

    #[test]
    fn pack_walk_of_a_subdirectory_keeps_links_to_the_rest_of_the_repo() {
        let fx = TempDir::new("pack-walk-subdir");
        let status = std::process::Command::new("git")
            .args(["init", "-q"])
            .arg(&fx.0)
            .status()
            .expect("run git init");
        assert!(status.success());
        fx.write("lib/util.rs", "pub fn util() {}\n");
        fx.write("src/main.rs", "fn main() {}\n");
        fx.symlink("../lib/util.rs", "src/util_link.rs");

        assert_eq!(walk(&fx.0.join("src")), vec!["main.rs", "util_link.rs"]);
    }

    fn from_where(root: &Path, paths: &[String]) -> Vec<String> {
        let root = root.to_path_buf();
        let paths = paths.to_vec();
        finishes_within(30, move || {
            read_pack_paths_ordered(&root, &paths, 10_000, true)
                .unwrap()
                .into_iter()
                .map(|file| file.path)
                .collect()
        })
    }

    #[test]
    fn from_where_skips_paths_leaving_the_root() {
        let outside = TempDir::new("from-where-outside");
        outside.write("secret.txt", "OUTSIDE_SECRET=1\n");
        let fx = TempDir::new("from-where");
        fx.write("main.rs", "fn main() {}\n");
        fx.symlink(outside.0.join("secret.txt"), "leak.rs");

        let paths = [
            "leak.rs".to_string(),
            outside.0.join("secret.txt").to_string_lossy().into_owned(),
            "../ctx-cli-from-where-outside-{pid}/secret.txt"
                .replace("{pid}", &std::process::id().to_string()),
            "main.rs".to_string(),
        ];
        assert_eq!(from_where(&fx.0, &paths), vec!["main.rs"]);
    }

    #[test]
    fn from_where_skips_fifos() {
        let fx = TempDir::new("from-where-fifo");
        fx.write("main.rs", "fn main() {}\n");
        fx.mkfifo("pipe");

        let paths = ["pipe".to_string(), "main.rs".to_string()];
        assert_eq!(from_where(&fx.0, &paths), vec!["main.rs"]);
    }
}
