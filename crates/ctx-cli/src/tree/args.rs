use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::SystemTime;

use super::*;
use crate::commands::pack::parse_pack_time_filter;
use crate::common::*;

/// Native `ctx [PATH] --json` — the PLAIN enriched-tree JSON (mirrors Go's
/// `render.JSONTree` / `runRoot`'s `--json` non-symbols, non-budget branch).
///
/// Returns `None` (→ delegate to Go) for any invocation we do not reproduce
/// byte-identically: `--symbols`/`--budget`/`--tokens`/`--plan`, walk-affecting
/// flags (`--depth`/`--since`/`--until`/`--use-mtime`), git ENABLED (only
/// `--git=false` is honored here, matching the parity gate), and any unknown
/// flag or multiple positionals.
///
/// NOTE: Go's default config has `Display.Symbols = true`, so a plain `--json`
/// invocation (where `--symbols` is NOT explicitly set) still runs symbol
/// extraction and embeds the symbols in the tree metadata — but renders via
/// JSONTree (not JSONSymbols) because `symbolsRequested` is false. We reproduce
/// that: symbols ARE extracted and embedded.
/// Parsed flag surface for the native no-subcommand root invocation.
pub(crate) struct RootArgs {
    pub(crate) path: Option<String>,
    pub(crate) want_json: bool,
    pub(crate) show_git: bool,
    pub(crate) plain: bool,
    pub(crate) depth: i64,
    pub(crate) budget: i64,
    pub(crate) unit: String,
    pub(crate) plan: String,
    pub(crate) since: String,
    pub(crate) until: String,
    pub(crate) use_mtime: bool,
}

/// Parse the root flag surface. Returns `None` (→ delegate to Go) for any flag
/// whose semantics this native path does not reproduce (`--symbols`/`--tui`),
/// unknown flags, or multiple positionals. Recognised value flags accept both
/// `--flag value` and `--flag=value` forms (mirrors cobra).
pub(crate) fn parse_root_args(args: &[OsString]) -> Option<RootArgs> {
    let mut r = RootArgs {
        path: None,
        want_json: false,
        show_git: true, // --git default true (config Display.Git default is also true).
        plain: false,
        depth: 0,
        budget: 0,
        unit: "tokens".to_string(), // config Display.Unit default.
        plan: String::new(),
        since: String::new(),
        until: String::new(),
        use_mtime: false,
    };

    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        let s = arg.to_string_lossy();
        match s.as_ref() {
            "--json" | "--json=true" => r.want_json = true,
            "--json=false" => r.want_json = false,
            "--git" | "--git=true" => r.show_git = true,
            "--git=false" => r.show_git = false,
            "--plain" | "--plain=true" => r.plain = true,
            "--plain=false" => r.plain = false,
            "--tokens" | "--tokens=true" | "--tokens=false" => {
                // Token counts are always computed natively; the metric column
                // shows tokens by default (config Display.Tokens=true), so
                // `--tokens` is a no-op on output here, matching Go.
            }
            "--use-mtime" | "--use-mtime=true" => r.use_mtime = true,
            "--use-mtime=false" => r.use_mtime = false,
            // Output modes / walk semantics we do NOT reproduce here.
            "--symbols" | "--symbols=true" | "--tui" | "--tui=true" => return None,
            _ if int_root_flag(arg, &s, "--depth", "-L", &mut i, args, &mut r.depth)? => {}
            _ if int_root_flag(arg, &s, "--budget", "", &mut i, args, &mut r.budget)? => {}
            _ if string_root_flag(arg, &s, "--unit", &mut i, args, &mut r.unit)? => {}
            _ if string_root_flag(arg, &s, "--plan", &mut i, args, &mut r.plan)? => {}
            _ if string_root_flag(arg, &s, "--since", &mut i, args, &mut r.since)? => {}
            _ if string_root_flag(arg, &s, "--until", &mut i, args, &mut r.until)? => {}
            _ if s.starts_with("--symbols") || s.starts_with("--tui") => return None,
            _ if is_option(arg) => return None, // unknown flag → delegate.
            _ => {
                if r.path.is_some() {
                    return None; // multiple positionals → delegate.
                }
                r.path = Some(s.to_string());
            }
        }
        i += 1;
    }
    Some(r)
}

/// Match an integer-valued flag (`name` long form, optional `short` alias).
/// Returns `Some(true)` if matched (advancing `i` for the split-value form),
/// `Some(false)` if not this flag, or `None` to abort parsing (bad value /
/// missing argument).
pub(crate) fn int_root_flag(
    arg: &OsStr,
    s: &str,
    name: &str,
    short: &str,
    i: &mut usize,
    args: &[OsString],
    out: &mut i64,
) -> Option<bool> {
    if let Some(v) = flag_value(arg, name) {
        *out = v.to_string_lossy().parse().ok()?;
        return Some(true);
    }
    if !short.is_empty() {
        if let Some(rest) = s.strip_prefix(short) {
            if !rest.is_empty() {
                // -L1 (attached) form.
                *out = rest.parse().ok()?;
                return Some(true);
            }
            if s == short {
                *i += 1;
                *out = args.get(*i)?.to_string_lossy().parse().ok()?;
                return Some(true);
            }
        }
    }
    if s == name {
        *i += 1;
        *out = args.get(*i)?.to_string_lossy().parse().ok()?;
        return Some(true);
    }
    Some(false)
}

/// Match a string-valued flag (`--flag value` or `--flag=value`).
pub(crate) fn string_root_flag(
    arg: &OsStr,
    s: &str,
    name: &str,
    i: &mut usize,
    args: &[OsString],
    out: &mut String,
) -> Option<bool> {
    if let Some(v) = flag_value(arg, name) {
        *out = v.to_string_lossy().into_owned();
        return Some(true);
    }
    if s == name {
        *i += 1;
        *out = args.get(*i)?.to_string_lossy().into_owned();
        return Some(true);
    }
    Some(false)
}

/// Native no-subcommand root (`ctx [PATH] [flags]`). Reuses the slice-1/2
/// walk+enrich+render building blocks and routes the budget/json/text variants
/// in Go's `runRoot` dispatch order. Returns `None` to delegate for anything
/// outside the natively-supported flag surface (see `parse_root_args`).
pub(crate) fn run_root_command(args: &[OsString]) -> Option<ExitCode> {
    let r = parse_root_args(args)?;
    let root = PathBuf::from(r.path.as_deref().unwrap_or("."));

    // Build the walk options (depth + time-filter) shared by every variant.
    // Invalid --since/--until values are RUNTIME failures (the invocation
    // shape was recognised): report them, do not fall through to the
    // "unsupported invocation" path.
    let now = SystemTime::now();
    let since = match r.since.as_str() {
        "" => None,
        raw => match parse_pack_time_filter(raw, now) {
            Ok(t) => Some(t),
            Err(err) => {
                eprintln!("Error: --since: {err}");
                return Some(ExitCode::FAILURE);
            }
        },
    };
    let until = match r.until.as_str() {
        "" => None,
        raw => match parse_pack_time_filter(raw, now) {
            Ok(t) => Some(t),
            Err(err) => {
                eprintln!("Error: --until: {err}");
                return Some(ExitCode::FAILURE);
            }
        },
    };
    let _ = r.use_mtime; // non-git fixture → effective time is mtime regardless.
    let tree_opts = TreeBuildOpts {
        max_depth: r.depth,
        since,
        until,
    };

    // Dispatch order mirrors Go's runRoot: budget > 0 first, then JSON, then text.
    // Render errors are RUNTIME failures too: the args parsed fine, so report
    // the real error instead of returning None (= "shape not recognised").
    if r.budget > 0 {
        return match render_root_budget(&root, &tree_opts, r.budget, r.want_json, r.plain) {
            Ok(()) => Some(ExitCode::SUCCESS),
            Err(err) => {
                eprintln!("Error: {err}");
                Some(ExitCode::FAILURE)
            }
        };
    }

    if r.want_json {
        return match render_root_json_tree(&root, &tree_opts) {
            Ok(()) => Some(ExitCode::SUCCESS),
            Err(err) => {
                eprintln!("Error: {err}");
                Some(ExitCode::FAILURE)
            }
        };
    }

    let text_opts = TextTreeOptions {
        show_git: r.show_git,
        show_tokens: true,  // config Display.Tokens default = true.
        show_size: true,    // renderOpts.ShowSize is hard-coded true.
        show_lines: true,   // renderOpts.ShowLines is hard-coded true.
        show_symbols: true, // config Display.Symbols default = true.
        plain: r.plain,
        unit: normalize_text_unit(&r.unit),
    };
    match render_root_text_tree(&root, &tree_opts, &text_opts, &r.plan) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(ExitCode::FAILURE)
        }
    }
}
