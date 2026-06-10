use std::fs;
use std::process::Command;

mod common;
use common::*;

// Wave 4 (ADR-0005, #2b slice 4): after Go elimination, subcommand `--help`
// renders via clap natively. clap's help formatting differs from cobra's, so
// this is an ACCEPTED divergence (help is human-facing, not a functional
// contract) — we assert the native shape (exit 0 + a Usage section), NOT
// byte-parity with Go.
#[test]
fn native_subcommand_help_renders() {
    let root = repo_root();
    for args in [
        &["pack", "--help"][..],
        &["audit", "verify", "--help"][..],
        &["contract", "verify", "--help"][..],
    ] {
        let out = run_rust_in(&root, args);
        assert_eq!(
            out.status.code(),
            Some(0),
            "`ctx {}` should exit 0 rendering clap help; stderr:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr),
        );
        let stdout = String::from_utf8_lossy(&out.stdout).to_lowercase();
        assert!(
            stdout.contains("usage"),
            "`ctx {}` clap help should contain a Usage section; stdout:\n{}",
            args.join(" "),
            String::from_utf8_lossy(&out.stdout),
        );
    }
}

// Wave 4 (ADR-0005, #2b slice 4): unknown-flag / missing-arg errors are
// framework-specific (cobra vs clap), so byte-parity with Go is infeasible.
// After de-delegation the binary NEVER spawns Go; it must surface a native,
// non-zero, non-empty error. We prove non-delegation by pointing CTX_GO_BIN at
// a sentinel binary that would emit a marker if ever spawned, then assert the
// marker is absent (and exit is non-zero with a non-empty stderr message).
#[test]
fn native_subcommand_errors_are_native() {
    const SENTINEL: &str = "CTX_GO_DELEGATE_SENTINEL_SHOULD_NEVER_APPEAR";
    let root = repo_root();

    // A stub "Go binary" that, if spawned, prints the sentinel and exits 73.
    let stub_dir = std::env::temp_dir().join("ctx-de-delegate-sentinel");
    fs::create_dir_all(&stub_dir).expect("create sentinel stub dir");
    let stub = stub_dir.join("ctx-go-stub.sh");
    fs::write(
        &stub,
        format!("#!/bin/sh\necho \"{SENTINEL}\" 1>&2\nexit 73\n"),
    )
    .expect("write sentinel stub");
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let mut perms = fs::metadata(&stub).unwrap().permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&stub, perms).expect("chmod sentinel stub");
    }

    for args in [&["pack", "--not-a-real-flag"][..], &["deps"][..]] {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
        cmd.args(args).env("CTX_GO_BIN", &stub).current_dir(&root);
        let out = cmd
            .output()
            .unwrap_or_else(|err| panic!("run Rust ctx {args:?}: {err}"));

        // Non-delegation: the sentinel stub must never have run.
        assert_ne!(
            out.status.code(),
            Some(73),
            "`ctx {}` delegated to Go (sentinel exit 73); it must run native",
            args.join(" "),
        );
        let stderr = String::from_utf8_lossy(&out.stderr);
        assert!(
            !stderr.contains(SENTINEL),
            "`ctx {}` spawned the Go sentinel; it must run native.\nstderr:\n{stderr}",
            args.join(" "),
        );

        // Native-honest error: non-zero exit + a non-empty error message.
        assert_ne!(
            out.status.code(),
            Some(0),
            "`ctx {}` should fail with a non-zero exit, not succeed",
            args.join(" "),
        );
        assert!(
            !stderr.trim().is_empty(),
            "`ctx {}` should emit a non-empty native error message",
            args.join(" "),
        );
    }
}
