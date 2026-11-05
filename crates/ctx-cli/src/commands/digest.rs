use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::ExitCode;

use crate::commands::where_cmd::extract_where_symbols;
use crate::common::*;

#[derive(Debug)]
pub(crate) struct DigestArgs {
    /// Repository directory; `None` digests the repository in the current
    /// directory as a whole.
    root: Option<PathBuf>,
    since: String,
    top: usize,
    out: String,
    format: String,
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
    if !saw_digest || positionals.len() > 1 {
        return None;
    }
    if json {
        format = "json".to_string();
    }
    Some(DigestArgs {
        root: positionals.first().map(PathBuf::from),
        since,
        top,
        out,
        format,
    })
}

pub(crate) fn digest_command(args: DigestArgs) -> Result<(), String> {
    if let Some(root) = &args.root {
        if !root.is_dir() {
            return Err(format!("digest: {}: not a directory", root.display()));
        }
    }
    let brief = ctx_digest::generate(ctx_digest::DigestOptions {
        root: args.root.as_deref(),
        since: &args.since,
        top: args.top,
        count_symbols: count_digest_symbols,
    })?;
    let rendered = match args.format.as_str() {
        "json" => ctx_digest::render_json(&brief)?,
        "plain" => ctx_digest::render_plain(&brief),
        "markdown" | "" => ctx_digest::render_markdown(&brief),
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

/// Digest of the repository in the current directory (whole history scope),
/// as used by `braid` `digest` strands.
pub(crate) fn generate_digest(since: &str, top: usize) -> Result<ctx_digest::DigestBrief, String> {
    ctx_digest::generate(ctx_digest::DigestOptions {
        root: None,
        since,
        top,
        count_symbols: count_digest_symbols,
    })
}

/// Counts named symbols with the regex extractor `ctx where` uses, which
/// keeps `ctx digest` byte-compatible with the Go implementation.
pub(crate) fn count_digest_symbols(path: &str, content: &str) -> i64 {
    if content.is_empty() {
        return 0;
    }
    let lines: Vec<&str> = content.lines().collect();
    extract_where_symbols(path, &lines).len() as i64
}
