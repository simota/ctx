use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::map::estimate_where_tokens;
use crate::commands::where_cmd::where_files;
use crate::common::*;
use serde::Serialize;

#[derive(Debug)]
pub(crate) struct NoiseArgs {
    root: PathBuf,
    top: usize,
    apply: bool,
    format: String,
}

#[derive(Debug, Serialize)]
pub(crate) struct NoiseResult {
    root: String,
    candidates: Vec<NoiseCandidate>,
    total_tokens: i64,
    total_files: usize,
    noise_ratio: f64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct NoiseCandidate {
    path: String,
    tokens: i64,
    reason: String,
    size: i64,
    gitignore_status: String,
}

pub(crate) fn run_noise_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_noise_args(args)?;
    match noise_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_noise_args(args: &[OsString]) -> Option<NoiseArgs> {
    let mut saw_noise = false;
    let mut json = false;
    let mut top = 20usize;
    let mut apply = false;
    let mut format = "text".to_string();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--apply") {
            apply = true;
        } else if arg == OsStr::new("noise") {
            if saw_noise {
                return None;
            }
            saw_noise = true;
        } else if let Some(value) = flag_value(arg, "--top") {
            top = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--top") {
            i += 1;
            top = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if is_option(arg) {
            return None;
        } else if saw_noise {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_noise || positionals.len() > 1 {
        return None;
    }
    if json {
        format = "json".to_string();
    }
    Some(NoiseArgs {
        root: match positionals.as_slice() {
            [] => PathBuf::from("."),
            [root] => PathBuf::from(root),
            _ => return None,
        },
        top,
        apply,
        format,
    })
}

pub(crate) fn noise_command(args: NoiseArgs) -> Result<(), String> {
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        env::current_dir()
            .map_err(|err| err.to_string())?
            .join(&args.root)
    };
    let mut result = inspect_noise(
        &root,
        &args.root.to_string_lossy(),
        if args.apply { 0 } else { args.top },
    )?;
    if args.apply {
        render_noise_proposal(&result);
    } else if args.format == "json" {
        serde_json::to_writer_pretty(io::stdout(), &result).map_err(|err| err.to_string())?;
        println!();
    } else if args.format == "text" || args.format.is_empty() {
        render_noise_text(&result, args.top);
    } else {
        return Err(format!("unknown --format value {:?}", args.format));
    }
    result.candidates.clear();
    Ok(())
}

pub(crate) fn inspect_noise(
    root: &Path,
    display_root: &str,
    top: usize,
) -> Result<NoiseResult, String> {
    let files = where_files(root)?;
    let mut candidates = Vec::new();
    let mut total_repo_tokens = 0_i64;
    for fi in files {
        if fi.is_dir {
            continue;
        }
        let tokens = estimate_where_tokens(root, &fi);
        total_repo_tokens += tokens;
        let size: i64 = fi.lines.iter().map(|line| line.len() as i64 + 1).sum();
        let Some(reason) = classify_noise(&fi.path, size, fi.symbols.len()) else {
            continue;
        };
        candidates.push(NoiseCandidate {
            path: fi.path,
            tokens,
            reason: reason.to_string(),
            size,
            gitignore_status: "tracked".to_string(),
        });
    }
    candidates.sort_by(|a, b| b.tokens.cmp(&a.tokens).then_with(|| a.path.cmp(&b.path)));
    let total_tokens: i64 = candidates.iter().map(|c| c.tokens).sum();
    let total_files = candidates.len();
    if top > 0 && candidates.len() > top {
        candidates.truncate(top);
    }
    let noise_ratio = if total_repo_tokens > 0 {
        (total_tokens as f64 / total_repo_tokens as f64).min(1.0)
    } else {
        0.0
    };
    Ok(NoiseResult {
        root: display_root.to_string(),
        candidates,
        total_tokens,
        total_files,
        noise_ratio,
    })
}

pub(crate) fn classify_noise(path: &str, size: i64, symbols: usize) -> Option<&'static str> {
    let slash = path.replace('\\', "/");
    let base = slash.rsplit('/').next().unwrap_or(&slash);
    let ext = Path::new(base)
        .extension()
        .and_then(OsStr::to_str)
        .map(|s| format!(".{}", s.to_ascii_lowercase()))
        .unwrap_or_default();
    for suffix in [
        ".pb.go",
        ".pb.ts",
        ".gen.ts",
        ".gen.go",
        "_generated.py",
        "_pb2.py",
        ".min.js",
        ".bundle.js",
    ] {
        if base.ends_with(suffix) {
            return Some("generated");
        }
    }
    if contains_path_segment(&slash, &["dist", "build", ".next", "generated"]) {
        return Some("generated");
    }
    if matches!(
        base,
        "package-lock.json"
            | "pnpm-lock.yaml"
            | "yarn.lock"
            | "Cargo.lock"
            | "Gemfile.lock"
            | "go.sum"
            | "poetry.lock"
            | "composer.lock"
            | "Pipfile.lock"
    ) {
        return Some("lockfile");
    }
    if contains_path_segment(
        &slash,
        &["testdata", "fixtures", "__fixtures__", "__snapshots__"],
    ) {
        return Some("testdata");
    }
    if matches!(
        ext.as_str(),
        ".png"
            | ".jpg"
            | ".jpeg"
            | ".gif"
            | ".ico"
            | ".pdf"
            | ".zip"
            | ".tar"
            | ".gz"
            | ".so"
            | ".dylib"
            | ".dll"
            | ".exe"
            | ".bin"
            | ".wasm"
            | ".woff"
            | ".woff2"
            | ".ttf"
            | ".eot"
            | ".mp3"
            | ".mp4"
            | ".mov"
    ) {
        return Some("binary");
    }
    if ext == ".json" && size >= 50 * 1024 {
        return Some("huge-json");
    }
    if matches!(
        ext.as_str(),
        ".go" | ".ts" | ".tsx" | ".js" | ".jsx" | ".mjs" | ".py"
    ) && size >= 10 * 1024
        && (symbols as f64 / (size as f64 / 1024.0)) < 0.5
    {
        return Some("low-density");
    }
    None
}

pub(crate) fn contains_path_segment(path: &str, segments: &[&str]) -> bool {
    segments.iter().any(|seg| {
        path.strip_prefix(&format!("{seg}/")).is_some() || path.contains(&format!("/{seg}/"))
    })
}

pub(crate) fn render_noise_text(result: &NoiseResult, top: usize) {
    if result.candidates.is_empty() {
        println!("No noise candidates found.");
        return;
    }
    println!("Top noise candidates by token impact:");
    println!();
    println!(
        " {:<4} {:<9} {:<13} {:<11} {}",
        "#", "Tokens", "Reason", "Gitignore", "Path"
    );
    for (idx, c) in result.candidates.iter().enumerate() {
        println!(
            " {:<4} {:<9} {:<13} {:<11} {}",
            format!("{}.", idx + 1),
            format_number_i64(c.tokens),
            c.reason,
            c.gitignore_status,
            c.path
        );
    }
    println!();
    println!(
        "Total noise: {} tokens across {} files ({}% of repo).",
        format_k_tokens(result.total_tokens),
        result.total_files,
        (result.noise_ratio * 100.0) as i64
    );
    if result.total_files > top && top > 0 {
        println!(
            "Showing top {} of {}. Use --top {} to see all.",
            top, result.total_files, result.total_files
        );
    }
    println!("Run `ctx noise --apply > .ctxignore` to write the proposal.");
}

pub(crate) fn render_noise_proposal(result: &NoiseResult) {
    let today = current_date_utc();
    println!("# Generated by `ctx noise --apply` on {today}");
    println!("# Files identified as LLM-context noise (high tokens, low semantic value).");
    println!("# Review and edit as needed \u{2014} this is a proposal, not authoritative.");
    if result.candidates.is_empty() {
        println!("# (no noise candidates found)");
        return;
    }
    for reason in [
        "generated",
        "lockfile",
        "testdata",
        "binary",
        "huge-json",
        "low-density",
    ] {
        let cands: Vec<_> = result
            .candidates
            .iter()
            .filter(|c| c.reason == reason)
            .collect();
        if cands.is_empty() {
            continue;
        }
        println!();
        println!("# {reason}");
        let lines = aggregate_noise_globs(&cands);
        for line in lines {
            println!("{line}");
        }
    }
}

/// Mirrors Go's aggregateGlobs: collapses candidates sharing the same
/// parent dir + extension (3+ files) into a `dir/*.ext` glob pattern.
pub(crate) fn aggregate_noise_globs(cands: &[&NoiseCandidate]) -> Vec<String> {
    use std::collections::HashMap;

    // Group by (dir, ext)
    let mut groups: HashMap<(String, String), Vec<String>> = HashMap::new();
    for c in cands {
        let slash = c.path.replace('\\', "/");
        let dir = match slash.rfind('/') {
            Some(idx) => slash[..idx].to_string(),
            None => ".".to_string(),
        };
        let ext = match slash.rfind('.') {
            Some(idx) if idx > slash.rfind('/').unwrap_or(0) => slash[idx..].to_ascii_lowercase(),
            _ => String::new(),
        };
        groups.entry((dir, ext)).or_default().push(slash.clone());
    }

    // Emit lines preserving candidate order (de-dup via emitted set)
    let mut emitted: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut lines: Vec<String> = Vec::new();

    for c in cands {
        let slash = c.path.replace('\\', "/");
        let dir = match slash.rfind('/') {
            Some(idx) => slash[..idx].to_string(),
            None => ".".to_string(),
        };
        let ext = match slash.rfind('.') {
            Some(idx) if idx > slash.rfind('/').unwrap_or(0) => slash[idx..].to_ascii_lowercase(),
            _ => String::new(),
        };
        let key = (dir.clone(), ext.clone());
        let paths = groups.get(&key).map(|v| v.len()).unwrap_or(0);

        if paths >= 3 && !ext.is_empty() {
            let glob = if dir == "." {
                format!("*{ext}")
            } else {
                format!("{dir}/*{ext}")
            };
            if emitted.insert(glob.clone()) {
                lines.push(glob);
            }
        } else if emitted.insert(slash.clone()) {
            lines.push(slash);
        }
    }

    lines.sort();
    lines
}
