//! `--help` is reachable from every form a user types it in.

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run the built binary without the Go oracle: help is native-only.
fn run_rust_in(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(args)
        .current_dir(root)
        .stdin(Stdio::null())
        .output()
        .unwrap_or_else(|err| panic!("run ctx {args:?}: {err}"))
}

/// `--help` must work wherever the user types it: directly after the command,
/// after other arguments (`ctx pack . --help`) and on `ctx mcp serve`. Each
/// form exits 0 and prints the same option reference.
#[test]
fn help_flag_after_arguments_shows_command_help() {
    let root = repo_root();
    for (args, expect) in [
        (&["browse", "--help"][..], "--allow-nonlocal"),
        (&["browse", ".", "--help"][..], "--allow-nonlocal"),
        (
            &["pack", ".", "--budget", "100", "--help"][..],
            "--budget <N>",
        ),
        (&["where", "needle", "-h"][..], "Usage: ctx where"),
        (&["map", ".", "--help"][..], "--depth <N>"),
        (&["roots", "open", "--help"][..], "--timeout <DURATION>"),
        (&["mcp", "--help"][..], "--allow-outside-root"),
        (&["mcp", "serve", "--help"][..], "--allow-outside-root"),
    ] {
        let out = run_rust_in(&root, args);
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert_eq!(
            out.status.code(),
            Some(0),
            "`ctx {}` should print help and exit 0; stderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        assert!(
            stdout.contains(expect),
            "`ctx {}` help lacks {expect:?}; stdout:\n{stdout}",
            args.join(" "),
        );
    }
}

/// The help shown for a trailing `--help` is byte-identical to the canonical
/// `ctx <command> --help` output.
#[test]
fn trailing_help_matches_canonical_help() {
    let root = repo_root();
    let canonical = run_rust_in(&root, &["pack", "--help"]);
    let trailing = run_rust_in(&root, &["pack", ".", "--changed", "--help"]);
    assert_eq!(canonical.stdout, trailing.stdout);
}

/// Malformed invocations still fail, and the usage hint they print now leads
/// to a help page that lists the options.
#[test]
fn malformed_invocation_still_fails_with_usage() {
    let root = repo_root();
    let out = run_rust_in(&root, &["browse", "--port", "not-a-number"]);
    assert_ne!(out.status.code(), Some(0));
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("Usage: ctx browse [PATH] [OPTIONS]"),
        "{stderr}"
    );
    assert!(stderr.contains("Run `ctx browse --help`"), "{stderr}");

    // `--` ends option parsing: a literal `--help` after it is not a help request.
    let out = run_rust_in(&root, &["noise", "--", "--help"]);
    assert_ne!(out.status.code(), Some(0));
}
