use std::env;
use std::ffi::{OsStr, OsString};
use std::process::ExitCode;

use clap::{Arg, ArgAction, Command as ClapCommand};

mod commands;
mod common;
mod gitignore;
mod tree;

use crate::commands::audit::run_audit_verify;
use crate::commands::braid::run_braid_command;
use crate::commands::browse::run_browse_command;
use crate::commands::browse::run_mcp_command;
use crate::commands::browse::run_tui_command;
use crate::commands::contract::run_contract_verify;
use crate::commands::digest::run_digest_command;
use crate::commands::doctor::run_doctor;
use crate::commands::echo::run_echo_command;
use crate::commands::focus::run_focus_command;
use crate::commands::map::run_map_command;
use crate::commands::noise::run_noise_command;
use crate::commands::onboarding::run_onboarding_command;
use crate::commands::pack::run_pack_command;
use crate::commands::relations::run_relations_command;
use crate::commands::replay::run_replay_command;
use crate::commands::roots::run_roots_command;
use crate::commands::skim::run_skim_command;
use crate::commands::symbols::run_symbols_command;
use crate::commands::where_cmd::run_where_command;
use crate::tree::run_root_command;

const COMMANDS: &[(&str, &str)] = &[
    ("pack", "Generate an AI-ready context bundle"),
    ("doctor", "Diagnose ctx runtime capabilities"),
    (
        "where",
        "Find files and symbols matching a natural-language query",
    ),
    ("mcp", "Run MCP server operations"),
    ("deps", "Show dependencies for a file"),
    ("impact", "Show import impact for a file"),
    ("map", "Render repository heatmap"),
    ("tui", "Open interactive TUI"),
    ("replay", "Manage pack replay snapshots"),
    ("browse", "Start the local web browser UI"),
    ("roots", "Manage known repository roots"),
    ("skim", "Render skim-friendly file summaries"),
    ("audit", "Audit log operations"),
    ("focus", "Build a symbol-anchored mini-pack"),
    ("noise", "Inspect low-signal files"),
    ("digest", "Summarize recent repository changes"),
    ("onboarding", "Generate onboarding guidance"),
    ("echo", "Evaluate pack answerability"),
    ("contract", "Verify pack-as-contract evidence"),
    ("braid", "Compose a pack from braid.toml strands"),
];

fn main() -> ExitCode {
    let args: Vec<OsString> = env::args_os().collect();

    match cli().try_get_matches_from(args.clone()) {
        Ok(matches) => {
            if matches.get_flag("version") {
                println!("ctx-rust {}", env!("CARGO_PKG_VERSION"));
                return ExitCode::SUCCESS;
            }
            if let Some(code) = try_run_native(&args[1..]) {
                return code;
            }
            // The binary is fully native: every clap-accepted arg-shape is
            // handled by `try_run_native`. If we reach here, a parse-accepted
            // command hit a shape no native handler claimed. There is no Go
            // fallback — fail honestly rather than delegate.
            let invocation = args
                .iter()
                .skip(1)
                .map(|a| a.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            eprintln!("Error: unsupported invocation: ctx {invocation}");
            ExitCode::FAILURE
        }
        Err(err) => {
            let _ = err.print();
            ExitCode::from(err.exit_code() as u8)
        }
    }
}

/// Flag-first form (`ctx --json pack`): the SECOND arg names the command only
/// when the first is a flag. A non-flag first arg is a positional belonging to
/// some OTHER command (`ctx where pack` is a `where` query, not `pack`), so it
/// must not steal the dispatch.
fn flag_then_command(args: &[OsString], name: &str) -> bool {
    args.first()
        .is_some_and(|arg| arg.to_string_lossy().starts_with('-'))
        && args.get(1).is_some_and(|arg| arg == OsStr::new(name))
}

fn try_run_native(args: &[OsString]) -> Option<ExitCode> {
    if args.len() >= 2 && args[0] == OsStr::new("audit") && args[1] == OsStr::new("verify") {
        return run_audit_verify(&args[2..]);
    }
    if args.len() >= 2 && args[0] == OsStr::new("contract") && args[1] == OsStr::new("verify") {
        return run_contract_verify(&args[2..]);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("pack"))
        || flag_then_command(args, "pack")
    {
        return run_pack_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("browse"))
        || flag_then_command(args, "browse")
    {
        return run_browse_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("mcp")) {
        return run_mcp_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("deps"))
        || args.first().is_some_and(|arg| arg == OsStr::new("impact"))
        || flag_then_command(args, "deps")
        || flag_then_command(args, "impact")
    {
        return run_relations_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("where"))
        || flag_then_command(args, "where")
    {
        return run_where_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("map"))
        || flag_then_command(args, "map")
    {
        return run_map_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("focus"))
        || flag_then_command(args, "focus")
    {
        return run_focus_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("echo"))
        || flag_then_command(args, "echo")
    {
        return run_echo_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("replay"))
        || flag_then_command(args, "replay")
    {
        return run_replay_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("noise"))
        || flag_then_command(args, "noise")
    {
        return run_noise_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("digest"))
        || flag_then_command(args, "digest")
    {
        return run_digest_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("skim"))
        || flag_then_command(args, "skim")
    {
        return run_skim_command(args);
    }
    if args
        .first()
        .is_some_and(|arg| arg == OsStr::new("onboarding"))
        || flag_then_command(args, "onboarding")
    {
        return run_onboarding_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("roots"))
        || flag_then_command(args, "roots")
    {
        return run_roots_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("braid"))
        || flag_then_command(args, "braid")
    {
        return run_braid_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("tui")) {
        return run_tui_command(args);
    }
    if args.first().is_some_and(|arg| arg == OsStr::new("doctor")) {
        return run_doctor(args);
    }
    if let Some(code) = run_symbols_command(args) {
        return Some(code);
    }
    if let Some(code) = run_root_command(args) {
        return Some(code);
    }
    None
}

fn cli() -> ClapCommand {
    let mut cmd = ClapCommand::new("ctx")
        .about("ctx — AI-era tree: visualise and pack your codebase for LLMs")
        .long_about(
            "ctx is a CLI/TUI tool that shows directory structure enriched with\n\
             token counts, Git status, file roles, and symbols — and can pack\n\
             selected files into an AI-ready context bundle.",
        )
        .disable_version_flag(true)
        .arg(
            Arg::new("path")
                .value_name("path")
                .help("Repository path")
                .num_args(0..=1),
        )
        .arg(
            Arg::new("version")
                .long("version")
                .help("Print the Rust entrypoint version")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("git")
                .long("git")
                .help("Show Git status")
                .num_args(0..=1)
                .require_equals(true)
                .default_missing_value("true")
                .default_value("true")
                .action(ArgAction::Set),
        )
        .arg(
            Arg::new("tokens")
                .long("tokens")
                .help("Show token estimates")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("symbols")
                .long("symbols")
                .help("Show extracted symbols")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("tui")
                .long("tui")
                .help("Open interactive TUI")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("depth")
                .short('L')
                .long("depth")
                .value_name("N")
                .help("Max directory depth (0 = unlimited)"),
        )
        .arg(
            Arg::new("budget")
                .long("budget")
                .value_name("N")
                .help("Show context budget plan for N tokens"),
        )
        .arg(
            Arg::new("unit")
                .long("unit")
                .value_name("tokens|chars|pages")
                .help("Display unit")
                .default_value("tokens"),
        )
        .arg(
            Arg::new("plain")
                .long("plain")
                .help("Use screen-reader friendly plain output")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("json")
                .long("json")
                .help("Output JSON")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("plan")
                .long("plan")
                .value_name("NAME")
                .help("Show fit for a model plan"),
        )
        .arg(
            Arg::new("strict-offline")
                .long("strict-offline")
                .help("Disable features that may perform external network calls")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("no-audit")
                .long("no-audit")
                .help("Exclude this invocation from audit log")
                .action(ArgAction::SetTrue),
        )
        .arg(
            Arg::new("since")
                .long("since")
                .value_name("DATE|DURATION")
                .help("Include only files updated after this date/duration"),
        )
        .arg(
            Arg::new("until")
                .long("until")
                .value_name("DATE|DURATION")
                .help("Include only files updated before this date/duration"),
        )
        .arg(
            Arg::new("use-mtime")
                .long("use-mtime")
                .help("Use file mtime instead of git last-commit time")
                .action(ArgAction::SetTrue),
        )
        .allow_external_subcommands(true)
        .subcommand_required(false);

    for (name, about) in COMMANDS {
        cmd = cmd.subcommand(
            ClapCommand::new(*name)
                .about(*about)
                .allow_external_subcommands(true)
                .arg(
                    Arg::new("args")
                        .num_args(0..)
                        .trailing_var_arg(true)
                        .allow_hyphen_values(true),
                ),
        );
    }
    cmd
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::commands::roots::{extract_browse_url, parse_duration_arg};
    use std::time::Duration;

    fn os_args(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn subcommand_help_renders_natively_via_clap() {
        // After Go elimination, subcommand `--help` renders via clap (not Go).
        // clap surfaces help by returning an Err whose kind is DisplayHelp.
        let err = cli()
            .try_get_matches_from(os_args(&["ctx", "pack", "--help"]))
            .expect_err("clap should emit help as a DisplayHelp error");
        assert_eq!(err.kind(), clap::error::ErrorKind::DisplayHelp);
    }

    #[test]
    fn command_tree_contains_expected_subcommands() {
        let names: Vec<_> = cli()
            .get_subcommands()
            .map(|cmd| cmd.get_name().to_string())
            .collect();
        assert!(names.contains(&"pack".to_string()));
        assert!(names.contains(&"contract".to_string()));
        assert!(names.contains(&"braid".to_string()));
    }

    #[test]
    fn git_flag_accepts_cobra_bool_shapes() {
        assert!(cli().try_get_matches_from(["ctx", "--git"]).is_ok());
        assert!(cli().try_get_matches_from(["ctx", "--git=false"]).is_ok());
    }

    #[test]
    fn browse_url_extraction_trims_terminal_punctuation() {
        assert_eq!(
            extract_browse_url("ctx browse: serving . at http://127.0.0.1:54321/."),
            Some("http://127.0.0.1:54321/".to_string())
        );
        assert_eq!(
            extract_browse_url("ready at https://localhost:8443/path,"),
            Some("https://localhost:8443/path".to_string())
        );
        assert_eq!(extract_browse_url("ctx browse: starting"), None);
    }

    #[test]
    fn duration_arg_accepts_common_go_style_units() {
        assert_eq!(
            parse_duration_arg("100ms"),
            Some(Duration::from_millis(100))
        );
        assert_eq!(parse_duration_arg("10s"), Some(Duration::from_secs(10)));
        assert_eq!(parse_duration_arg("2m"), Some(Duration::from_secs(120)));
        assert_eq!(parse_duration_arg("1h"), Some(Duration::from_secs(3600)));
        assert_eq!(parse_duration_arg("10"), Some(Duration::from_secs(10)));
        assert_eq!(parse_duration_arg("bad"), None);
    }
}
