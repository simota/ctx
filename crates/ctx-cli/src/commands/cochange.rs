//! `ctx cochange [PATH...]`: files that historically change in the same
//! commits as PATH (git co-change), or the most coupled file pairs when no
//! PATH is given.

use std::env;
use std::ffi::{OsStr, OsString};
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use crate::common::*;

pub(crate) const DEFAULT_COMMITS: usize = 500;
pub(crate) const DEFAULT_MIN_WEIGHT: u32 = 2;

#[derive(Debug)]
pub(crate) struct CoChangeArgs {
    paths: Vec<OsString>,
    since: String,
    commits: usize,
    min_weight: u32,
    top: usize,
    format: String,
}

#[derive(Debug, Serialize)]
struct PartnerOut {
    path: String,
    weight: u32,
    commits: u32,
    confidence: f64,
    with: Vec<String>,
    last_commit: String,
}

#[derive(Debug, Serialize)]
struct PartnersReport {
    repo: String,
    anchors: Vec<AnchorOut>,
    partners: Vec<PartnerOut>,
    commits_scanned: u32,
    truncated: bool,
}

#[derive(Debug, Serialize)]
struct AnchorOut {
    path: String,
    commits: u32,
}

#[derive(Debug, Serialize)]
struct PairOut {
    a: String,
    b: String,
    weight: u32,
}

#[derive(Debug, Serialize)]
struct PairsReport {
    repo: String,
    pairs: Vec<PairOut>,
    commits_scanned: u32,
    truncated: bool,
}

pub(crate) fn run_cochange_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_cochange_args(args)?;
    match cochange_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_cochange_args(args: &[OsString]) -> Option<CoChangeArgs> {
    let mut saw = false;
    let mut out = CoChangeArgs {
        paths: Vec::new(),
        since: String::new(),
        commits: DEFAULT_COMMITS,
        min_weight: DEFAULT_MIN_WEIGHT,
        top: 20,
        format: "text".to_string(),
    };
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("cochange") && !saw {
            saw = true;
        } else if arg == OsStr::new("--json") {
            out.format = "json".to_string();
        } else if arg == OsStr::new("--plain") {
            out.format = "plain".to_string();
        } else if let Some(value) = flag_value(arg, "--format") {
            out.format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            out.format = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--since") {
            out.since = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--since") {
            i += 1;
            out.since = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--commits") {
            out.commits = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--commits") {
            i += 1;
            out.commits = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--min-weight") {
            out.min_weight = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--min-weight") {
            i += 1;
            out.min_weight = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--top") {
            out.top = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--top") {
            i += 1;
            out.top = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if is_option(arg) {
            return None;
        } else if saw {
            out.paths.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw {
        return None;
    }
    Some(out)
}

/// `raw` (relative to `cwd`, or absolute) as a '/'-separated path relative
/// to the work tree `toplevel`. `cwd_prefix` is cwd's `--show-prefix`.
pub(crate) fn repo_relative(
    raw: &Path,
    toplevel: &Path,
    cwd_prefix: &str,
) -> Result<String, String> {
    let outside = || format!("{} is outside the repository", raw.display());
    if raw.is_absolute() {
        let canonical = std::fs::canonicalize(raw).unwrap_or_else(|_| raw.to_path_buf());
        let stripped = canonical
            .strip_prefix(toplevel)
            .or_else(|_| raw.strip_prefix(toplevel))
            .map_err(|_| outside())?;
        return clean_parts(Vec::new(), stripped).ok_or_else(outside);
    }
    let base = cwd_prefix
        .split('/')
        .filter(|part| !part.is_empty())
        .map(str::to_string)
        .collect();
    clean_parts(base, raw).ok_or_else(outside)
}

fn clean_parts(mut parts: Vec<String>, rel: &Path) -> Option<String> {
    for component in rel.components() {
        match component {
            Component::CurDir => {}
            Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
            Component::ParentDir => {
                parts.pop()?;
            }
            _ => return None,
        }
    }
    Some(parts.join("/"))
}

fn short_date(secs: i64) -> String {
    if secs <= 0 {
        "-".to_string()
    } else {
        ctx_pack::assemble::format_rfc3339_utc(secs)[..10].to_string()
    }
}

pub(crate) fn since_approxidate(since: &str) -> Result<Option<String>, String> {
    if since.is_empty() {
        Ok(None)
    } else {
        ctx_digest::git_since_arg(since)
            .map(Some)
            .map_err(|err| format!("--since: {err}"))
    }
}

pub(crate) fn cochange_command(args: CoChangeArgs) -> Result<(), String> {
    if !matches!(args.format.as_str(), "text" | "plain" | "json" | "paths") {
        return Err(format!(
            "unknown --format value {:?} (allowed: text, plain, json, paths)",
            args.format
        ));
    }
    let since = since_approxidate(&args.since)?;
    let cwd = env::current_dir().map_err(|err| format!("cannot read working directory: {err}"))?;
    let toplevel = ctx_git::show_toplevel(&cwd)
        .map_err(|err| format!("cochange needs a Git repository: {err}"))?;
    let toplevel = std::fs::canonicalize(&toplevel).unwrap_or(toplevel);
    let cwd_prefix = ctx_git::show_prefix(&cwd).map_err(|err| err.to_string())?;
    let repo = toplevel.display().to_string();
    let stdout = std::io::stdout();
    let mut out = stdout.lock();

    if args.paths.is_empty() {
        let top = ctx_git::co_change_top_pairs(
            &toplevel,
            args.commits,
            since.as_deref(),
            args.min_weight,
            args.top,
        )
        .map_err(|err| err.to_string())?;
        let report = PairsReport {
            repo,
            pairs: top
                .pairs
                .into_iter()
                .map(|pair| PairOut {
                    a: pair.a,
                    b: pair.b,
                    weight: pair.weight,
                })
                .collect(),
            commits_scanned: top.commits_scanned,
            truncated: top.truncated,
        };
        return write_pairs(&mut out, &report, &args.format).map_err(|err| err.to_string());
    }

    let anchors = args
        .paths
        .iter()
        .map(|raw| repo_relative(&PathBuf::from(raw), &toplevel, &cwd_prefix))
        .collect::<Result<Vec<_>, _>>()?;
    let found = ctx_git::co_change_partners(
        &toplevel,
        &anchors,
        args.commits,
        since.as_deref(),
        args.min_weight,
    )
    .map_err(|err| err.to_string())?;
    let mut partners: Vec<PartnerOut> = found
        .partners
        .into_iter()
        .map(|p| PartnerOut {
            path: p.path,
            weight: p.weight,
            commits: p.commits,
            confidence: (p.confidence * 100.0).round() / 100.0,
            with: p.anchors,
            last_commit: short_date(p.last_commit_time),
        })
        .collect();
    if args.top > 0 {
        partners.truncate(args.top);
    }
    let report = PartnersReport {
        repo,
        anchors: found
            .matched_anchors
            .into_iter()
            .map(|(path, commits)| AnchorOut { path, commits })
            .collect(),
        partners,
        commits_scanned: found.commits_scanned,
        truncated: found.truncated,
    };
    write_partners(&mut out, &report, &anchors, args.min_weight, &args.format)
        .map_err(|err| err.to_string())
}

fn scanned_note(scanned: u32, truncated: bool) -> String {
    if truncated {
        format!(
            "{scanned} commits scanned; older history not included, raise --commits to go further"
        )
    } else {
        format!("{scanned} commits scanned")
    }
}

fn write_partners(
    out: &mut impl Write,
    report: &PartnersReport,
    requested: &[String],
    min_weight: u32,
    format: &str,
) -> std::io::Result<()> {
    match format {
        "json" => {
            serde_json::to_writer_pretty(&mut *out, report)?;
            writeln!(out)
        }
        "paths" => {
            for p in &report.partners {
                writeln!(out, "{}", p.path)?;
            }
            Ok(())
        }
        _ => {
            let label = requested.join(", ");
            let plain = format == "plain";
            if report.anchors.is_empty() {
                return writeln!(
                    out,
                    "No co-change history for {label}: it never changed together with another file at least {min_weight} times ({}).",
                    scanned_note(report.commits_scanned, report.truncated)
                );
            }
            if report.partners.is_empty() {
                return writeln!(
                    out,
                    "No file changed together with {label} at least {min_weight} times ({}). It looks safe to change on its own.",
                    scanned_note(report.commits_scanned, report.truncated)
                );
            }
            if plain {
                writeln!(
                    out,
                    "{} files usually change together with {label}. {}. Paths are relative to {}.",
                    report.partners.len(),
                    scanned_note(report.commits_scanned, report.truncated),
                    report.repo
                )?;
                for (idx, p) in report.partners.iter().enumerate() {
                    writeln!(
                        out,
                        "{}. {}: changed together {} times, {:.0} percent confidence, last changed {}.",
                        idx + 1,
                        p.path,
                        p.weight,
                        p.confidence * 100.0,
                        p.last_commit
                    )?;
                }
                return Ok(());
            }
            writeln!(
                out,
                "Co-change partners of {label} ({})",
                scanned_note(report.commits_scanned, report.truncated)
            )?;
            writeln!(out)?;
            writeln!(out, "  #  Together  Conf.  Last change  Path")?;
            for (idx, p) in report.partners.iter().enumerate() {
                writeln!(
                    out,
                    "{:>3}  {:>8}  {:>4.0}%  {:<11}  {}",
                    idx + 1,
                    p.weight,
                    p.confidence * 100.0,
                    p.last_commit,
                    p.path
                )?;
            }
            writeln!(out)?;
            writeln!(
                out,
                "Conf. = share of the anchor's commits that also touched the file. Paths are relative to {}.",
                report.repo
            )?;
            writeln!(
                out,
                "Next: ctx cochange {} --format paths | ctx pack --from-stdin",
                requested.join(" ")
            )
        }
    }
}

fn write_pairs(out: &mut impl Write, report: &PairsReport, format: &str) -> std::io::Result<()> {
    match format {
        "json" => {
            serde_json::to_writer_pretty(&mut *out, report)?;
            writeln!(out)
        }
        "paths" => {
            let mut seen = std::collections::BTreeSet::new();
            for pair in &report.pairs {
                for path in [&pair.a, &pair.b] {
                    if seen.insert(path.clone()) {
                        writeln!(out, "{path}")?;
                    }
                }
            }
            Ok(())
        }
        _ => {
            if report.pairs.is_empty() {
                return writeln!(
                    out,
                    "No file pair changed together often enough ({}). Try --min-weight 1 or a larger --commits.",
                    scanned_note(report.commits_scanned, report.truncated)
                );
            }
            if format == "plain" {
                writeln!(
                    out,
                    "{} most coupled file pairs. {}.",
                    report.pairs.len(),
                    scanned_note(report.commits_scanned, report.truncated)
                )?;
                for (idx, pair) in report.pairs.iter().enumerate() {
                    writeln!(
                        out,
                        "{}. {} and {}: changed together {} times.",
                        idx + 1,
                        pair.a,
                        pair.b,
                        pair.weight
                    )?;
                }
                return Ok(());
            }
            writeln!(
                out,
                "Most coupled file pairs ({})",
                scanned_note(report.commits_scanned, report.truncated)
            )?;
            writeln!(out)?;
            writeln!(out, "  #  Together  Files")?;
            for (idx, pair) in report.pairs.iter().enumerate() {
                writeln!(
                    out,
                    "{:>3}  {:>8}  {}  <->  {}",
                    idx + 1,
                    pair.weight,
                    pair.a,
                    pair.b
                )?;
            }
            writeln!(out)?;
            writeln!(
                out,
                "Inspect one side with: ctx cochange <path>. Paths are relative to {}.",
                report.repo
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_paths_and_options() {
        let args = parse_cochange_args(&os(&[
            "cochange",
            "src/a.rs",
            "--since=90d",
            "--commits",
            "50",
            "--min-weight=3",
            "--top",
            "5",
            "--json",
        ]))
        .expect("parse");
        assert_eq!(args.paths, os(&["src/a.rs"]));
        assert_eq!(args.since, "90d");
        assert_eq!(args.commits, 50);
        assert_eq!(args.min_weight, 3);
        assert_eq!(args.top, 5);
        assert_eq!(args.format, "json");
        assert!(parse_cochange_args(&os(&["cochange", "--bogus"])).is_none());
    }

    #[test]
    fn repo_relative_resolves_against_the_cwd_prefix() {
        let top = Path::new("/repo");
        assert_eq!(
            repo_relative(Path::new("a.rs"), top, "src/").unwrap(),
            "src/a.rs"
        );
        assert_eq!(
            repo_relative(Path::new("../docs/x.md"), top, "src/").unwrap(),
            "docs/x.md"
        );
        assert!(repo_relative(Path::new("../../etc"), top, "src/").is_err());
        assert_eq!(
            repo_relative(Path::new("/repo/lib/b.rs"), top, "src/").unwrap(),
            "lib/b.rs"
        );
    }
}
