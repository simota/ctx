use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use crate::commands::where_cmd::{where_files_with, WalkIgnoreOptions};
use crate::common::*;

#[derive(Debug)]
pub(crate) struct MapArgs {
    root: PathBuf,
    depth: i64,
    top: i64,
    by: String,
    format: String,
    budget: i64,
    plain: bool,
    width: i64,
    height: i64,
}

pub(crate) fn run_map_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_map_args(args)?;
    match map_command(parsed) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("{err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_map_args(args: &[OsString]) -> Option<MapArgs> {
    let mut saw_map = false;
    let mut json = false;
    let mut plain = false;
    let mut depth = 2;
    let mut top = 0;
    let mut by = "tokens".to_string();
    let mut format = "ascii".to_string();
    let mut budget = 0;
    let mut width = 80;
    let mut height = 20;
    let mut positionals = Vec::new();
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--plain") {
            plain = true;
        } else if arg == OsStr::new("map") {
            if saw_map {
                return None;
            }
            saw_map = true;
        } else if let Some(value) = flag_value(arg, "--depth") {
            depth = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--depth") {
            i += 1;
            depth = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--top") {
            top = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--top") {
            i += 1;
            top = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--by") {
            by = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--by") {
            i += 1;
            by = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--format") {
            format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            format = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--budget") {
            budget = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--budget") {
            i += 1;
            budget = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--width") {
            width = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--width") {
            i += 1;
            width = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if let Some(value) = flag_value(arg, "--height") {
            height = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--height") {
            i += 1;
            height = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if flag_value(arg, "--heatmap-engine").is_some() {
        } else if arg == OsStr::new("--heatmap-engine") {
            i += 1;
            args.get(i)?;
        } else if is_option(arg) {
            return None;
        } else if saw_map {
            positionals.push(arg.clone());
        } else {
            return None;
        }
        i += 1;
    }
    if !saw_map || positionals.len() > 1 {
        return None;
    }
    if json {
        format = "json".to_string();
    }
    Some(MapArgs {
        root: match positionals.as_slice() {
            [] => PathBuf::from("."),
            [root] => PathBuf::from(root),
            _ => return None,
        },
        depth,
        top,
        by,
        format,
        budget,
        plain,
        width,
        height,
    })
}

pub(crate) fn map_command(args: MapArgs) -> Result<(), String> {
    validate_map_args(&args)?;
    let root = if args.root.is_absolute() {
        args.root.clone()
    } else {
        env::current_dir()
            .map_err(|err| err.to_string())?
            .join(&args.root)
    };
    let metrics = heatmap_metrics(&root)?;
    let buckets = ctx_heatmap::top_n(
        ctx_heatmap::aggregate(
            &metrics,
            &ctx_heatmap::AggregateOptions {
                by: args.by.clone(),
                depth: args.depth,
                top: args.top,
            },
        ),
        args.top,
    );
    if args.plain {
        print!(
            "{}",
            ctx_heatmap::render_plain(
                &buckets,
                &ctx_heatmap::PlainOptions {
                    root: args.root.to_string_lossy().into_owned(),
                    by: args.by,
                    budget: args.budget,
                },
            )
        );
        return Ok(());
    }
    let rects = ctx_heatmap::squarify(&buckets, args.width, args.height);
    match args.format.as_str() {
        "json" => {
            let budget = (args.budget > 0).then_some(args.budget);
            let bytes = ctx_heatmap::render_json(
                &rects,
                &ctx_heatmap::JsonOptions {
                    root: args.root.to_string_lossy().into_owned(),
                    by: args.by,
                    budget,
                },
            )
            .map_err(|err| err.to_string())?;
            io::Write::write_all(&mut io::stdout(), &bytes).map_err(|err| err.to_string())?;
        }
        "ascii" | "" => {
            print!(
                "{}",
                ctx_heatmap::render_ascii(
                    &rects,
                    &ctx_heatmap::AsciiOptions {
                        width: args.width,
                        height: args.height,
                        by: args.by,
                        root: args.root.to_string_lossy().into_owned(),
                        budget: args.budget,
                    },
                )
            );
        }
        "svg" => {
            let budget = args.budget;
            print!(
                "{}",
                ctx_heatmap::render_svg(
                    &rects,
                    &ctx_heatmap::SvgOptions {
                        width: args.width,
                        height: args.height,
                        by: args.by,
                        root: args.root.to_string_lossy().into_owned(),
                        budget,
                    },
                )
            );
        }
        other => {
            return Err(format!(
                "unknown --format value {other:?} (allowed: ascii, json, svg)"
            ))
        }
    }
    Ok(())
}

pub(crate) fn validate_map_args(args: &MapArgs) -> Result<(), String> {
    match args.by.as_str() {
        "tokens" | "files" | "symbols" => {}
        "churn" => {
            return Err(
                "--by churn is not yet supported (requires git log history); use tokens, files, or symbols"
                    .to_string(),
            )
        }
        other => {
            return Err(format!(
                "unknown --by value {other:?} (allowed: tokens, files, symbols)"
            ))
        }
    }
    match args.format.as_str() {
        "ascii" | "json" | "svg" | "" => Ok(()),
        other => Err(format!(
            "unknown --format value {other:?} (allowed: ascii, json, svg)"
        )),
    }
}

/// Mirror of Go `loadMapFiles` walk options: `ctx map` uses the ctx.toml
/// `[ignore]` config (RespectGitignore + Patterns from `config.Default()`
/// when ctx.toml is absent) and does NOT respect `.ctxignore`
/// (walk.Options.RespectCtxignore is left unset in Go's loadMapFiles).
pub(crate) fn map_walk_options(root: &Path) -> WalkIgnoreOptions {
    // Go config.Default() Ignore values.
    let mut respect_gitignore = true;
    let mut patterns: Vec<String> = [
        "node_modules/**",
        "dist/**",
        "coverage/**",
        "*.lock",
        ".git/**",
    ]
    .iter()
    .map(ToString::to_string)
    .collect();

    // Overlay ctx.toml [ignore] keys when present (BurntSushi toml.DecodeFile
    // only overrides fields that appear in the file).
    #[derive(Default, serde::Deserialize)]
    struct IgnoreToml {
        respect_gitignore: Option<bool>,
        patterns: Option<Vec<String>>,
    }
    #[derive(Default, serde::Deserialize)]
    struct CtxToml {
        #[serde(default)]
        ignore: IgnoreToml,
    }
    if let Ok(body) = std::fs::read_to_string(root.join("ctx.toml")) {
        if let Ok(cfg) = toml::from_str::<CtxToml>(&body) {
            if let Some(rg) = cfg.ignore.respect_gitignore {
                respect_gitignore = rg;
            }
            if let Some(p) = cfg.ignore.patterns {
                patterns = p;
            }
        }
    }
    WalkIgnoreOptions {
        respect_gitignore,
        respect_ctxignore: false,
        extra_ignore: patterns,
    }
}

pub(crate) fn heatmap_metrics(root: &Path) -> Result<Vec<ctx_heatmap::FileMetric>, String> {
    let files = where_files_with(root, &map_walk_options(root))?;
    Ok(files
        .into_iter()
        .map(|fi| {
            let tokens = estimate_where_tokens(root, &fi);
            // Mirror Go map's extractSymbols: tree-sitter extraction via
            // symbols.New() (NOT the where regex extractor — Go map counts 0
            // symbols for languages tree-sitter doesn't cover, e.g. Rust).
            // Extraction errors leave the count at 0, like Go's warn-and-skip.
            let symbols = ctx_symbols::extract(root.join(&fi.path))
                .map(|syms| syms.len() as i64)
                .unwrap_or(0);
            ctx_heatmap::FileMetric {
                path: fi.path,
                is_dir: fi.is_dir,
                tokens,
                symbols,
            }
        })
        .collect())
}

pub(crate) fn estimate_where_tokens(root: &Path, input: &ctx_where::FileInput) -> i64 {
    // Mirror Go's tokens.CountFile: count the raw on-disk file content,
    // including any trailing newline. Reconstructing from `input.lines`
    // via join("\n") would silently drop the trailing newline (and any
    // CRLF detail), diverging from Go by one token on files that end in
    // "\n". Empty files count 0, exactly like Go (the old `.max(1)` clamp
    // was a holdover from the pre-parity bytes/4 heuristic and made map /
    // noise totals diverge by +1 per empty file). Fall back to a line-join
    // estimate only if the file cannot be read (matches Go's EstimateBySize
    // fallback on CountFile error).
    let abs = root.join(&input.path);
    let abs_str = abs.to_string_lossy();
    match ctx_tokens::count_file(&abs_str) {
        Ok(n) => n,
        Err(_) => ctx_tokens::count_str(&input.lines.join("\n")),
    }
}
