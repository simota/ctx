use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::where_cmd::extract_where_symbols;
use crate::common::*;

#[derive(Debug)]
pub(crate) struct SkimArgs {
    path: PathBuf,
    budget: i64,
    unit: String,
    lang: String,
    tier: String,
}

#[derive(Debug)]
pub(crate) struct SkimMeta {
    tier: String,
    tokens: i64,
    budget: i64,
    path: String,
    lang: String,
    unit: String,
    degraded: bool,
    overflow: bool,
}

pub(crate) fn run_skim_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_skim_args(args)?;
    match skim_command(parsed) {
        Ok((meta, body)) => {
            // Mirror Go's skim.FormatMeta: "(over budget)" goes inside the
            // tokens string, not at the end of the line.
            let tok_str = if meta.overflow {
                format!("{}/{} (over budget)", meta.tokens, meta.budget)
            } else {
                format!("{}/{}", meta.tokens, meta.budget)
            };
            println!(
                "# tier={} tokens={} path={} lang={}",
                meta.tier, tok_str, meta.path, meta.lang,
            );
            println!();
            print!("{body}");
            if meta.degraded {
                eprintln!(
                    "warning: skim degraded to tier={} to fit budget of {} {}",
                    meta.tier, meta.budget, meta.unit
                );
            }
            if meta.overflow {
                eprintln!(
                    "warning: skim output exceeds budget ({} > {} {})",
                    meta.tokens, meta.budget, meta.unit
                );
                // Mirror Go's cobra ExitError{"", Code}: cobra prints "Error: \n"
                // to stderr after the command body when RunE returns an ExitError.
                print_cobra_empty_error();
                Some(ExitCode::from(2))
            } else {
                Some(ExitCode::SUCCESS)
            }
        }
        Err(err) => {
            // Mirror Go's cobra + main.go double-print pattern:
            //   cobra prints:   "Error: <msg>\n"
            //   main.go prints: "<msg>\n"
            eprintln!("Error: {err}");
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_skim_args(args: &[OsString]) -> Option<SkimArgs> {
    let mut saw_skim = false;
    let mut budget = 1000;
    let mut unit = "tokens".to_string();
    let mut lang = "auto".to_string();
    let mut tier = String::new();
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("skim") {
            if saw_skim {
                return None;
            }
            saw_skim = true;
        } else if let Some(value) = flag_value(arg, "--budget") {
            budget = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--budget") {
            i += 1;
            budget = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--unit") {
            unit = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--unit") {
            i += 1;
            unit = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--lang") {
            lang = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--lang") {
            i += 1;
            lang = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--tier") {
            tier = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--tier") {
            i += 1;
            tier = args.get(i)?.to_string_lossy().into_owned();
        } else if is_option(arg) {
            return None;
        } else if saw_skim {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_skim {
        return None;
    }
    Some(SkimArgs {
        path: match positionals.as_slice() {
            [path] => PathBuf::from(path),
            _ => return None,
        },
        budget,
        unit,
        lang,
        tier,
    })
}

pub(crate) fn skim_command(args: SkimArgs) -> Result<(SkimMeta, String), String> {
    // Match Go's os.Stat error format: "skim: stat <path>: no such file or directory"
    // (lowercase OS message, "stat" verb prefix).
    if let Err(err) = std::fs::metadata(&args.path) {
        let path_s = args.path.to_string_lossy();
        let msg = if err.kind() == io::ErrorKind::NotFound {
            "no such file or directory".to_string()
        } else {
            err.to_string()
        };
        return Err(format!("skim: stat {path_s}: {msg}"));
    }
    let body = std::fs::read_to_string(&args.path)
        .map_err(|err| format!("skim: {}: {err}", args.path.display()))?;
    let lang = detect_skim_lang(&args.path, &args.lang);
    let rel = args.path.to_string_lossy().replace('\\', "/");
    let lines: Vec<String> = body.lines().map(ToString::to_string).collect();
    let symbols = extract_where_symbols(&rel, &lines);
    let forced = !args.tier.is_empty();
    let tiers: Vec<&str> = if forced {
        vec![args.tier.as_str()]
    } else {
        vec!["full", "api+doc", "signatures", "outline"]
    };
    let mut last: Option<(String, String, i64)> = None;
    for tier in tiers {
        let rendered = render_skim_tier(tier, &body, &rel, &symbols)?;
        let tokens = measure_skim(&rendered, &args.unit);
        last = Some((tier.to_string(), rendered.clone(), tokens));
        if forced || tokens <= args.budget || tier == "outline" {
            return Ok((
                SkimMeta {
                    tier: tier.to_string(),
                    tokens,
                    budget: args.budget,
                    path: rel,
                    lang,
                    unit: args.unit,
                    degraded: tier != "full" && !forced,
                    overflow: tokens > args.budget,
                },
                rendered,
            ));
        }
    }
    let (tier, rendered, tokens) = last.ok_or_else(|| "skim: no tier rendered".to_string())?;
    Ok((
        SkimMeta {
            tier,
            tokens,
            budget: args.budget,
            path: rel,
            lang,
            unit: args.unit,
            degraded: !forced,
            overflow: tokens > args.budget,
        },
        rendered,
    ))
}

pub(crate) fn render_skim_tier(
    tier: &str,
    body: &str,
    path: &str,
    symbols: &[ctx_where::SymbolInput],
) -> Result<String, String> {
    match tier {
        "full" => Ok(body.to_string()),
        "api+doc" | "signatures" => {
            let mut out = String::new();
            for sym in symbols {
                out.push_str(&format!(
                    "{}:{} {} {}\n",
                    path, sym.line, sym.kind, sym.name
                ));
            }
            if out.is_empty() {
                out.push_str("(no public symbols)\n");
            }
            Ok(out)
        }
        "outline" => {
            if symbols.is_empty() {
                return Ok("(no symbols)\n".to_string());
            }
            let mut out = String::new();
            for sym in symbols {
                out.push_str(&format!(
                    "{}:{} {} {}\n",
                    path, sym.line, sym.kind, sym.name
                ));
            }
            Ok(out)
        }
        other => Err(format!("skim: unknown tier {other:?}")),
    }
}

pub(crate) fn measure_skim(body: &str, unit: &str) -> i64 {
    if unit == "chars" {
        body.chars().count() as i64
    } else {
        ctx_tokens::count_str(body).max(1)
    }
}

pub(crate) fn detect_skim_lang(path: &Path, requested: &str) -> String {
    if requested != "auto" && !requested.is_empty() {
        return requested.to_string();
    }
    match path.extension().and_then(OsStr::to_str).unwrap_or("") {
        "go" => "go",
        "rs" => "rust",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" | "mjs" => "javascript",
        "py" => "python",
        "rb" => "ruby",
        _ => "text",
    }
    .to_string()
}
