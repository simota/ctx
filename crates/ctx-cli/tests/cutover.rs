//! ADR-0005 Wave 3 cutover oracle — WHITE-BOX DISPATCH tests.
//!
//! These tests assert the *dispatch decision* (native vs. delegate-to-Go), NOT
//! output. Because the native MCP server and native web engine are byte-parity
//! with Go, a black-box output comparison cannot tell native from delegate.
//! Instead we exploit the delegation seam in `crates/ctx-cli/src/main.rs`:
//!
//!   main() -> try_run_native(args)  -> Some(code)  => Rust ran it
//!                                   -> None         => delegate_to_go(args)
//!
//! and `find_go_binary()` resolves the Go fallback from `CTX_GO_BIN` FIRST.
//! We point `CTX_GO_BIN` at a STUB that exits with a unique sentinel code
//! (`SENTINEL_EXIT`). Then:
//!
//!   * If a command DELEGATES, the stub runs and the process exits with the
//!     sentinel code (and prints the sentinel marker on stderr).
//!   * If a command runs NATIVE, the stub is never spawned, so the exit code is
//!     anything-but-sentinel.
//!
//! Post-cutover dispatch contract under test:
//!   1. `ctx mcp serve` runs NATIVE (must NOT delegate).         [RED now]
//!   2. `ctx browse` with no `--web-engine` flag and no env runs the RUST
//!      engine by default (must NOT delegate).                    [RED now]
//!   3. Same as (2) with `CTX_WEB_ENGINE` explicitly unset.       [RED now]
//!   4. `ctx tui` runs NATIVE (ctx-tui ratatui; must NOT delegate). [RED now]
//!   5. `ctx where <q>` still runs NATIVE (sanity, no regression).[GREEN now]
//!
//! The wiring that makes 1-4 green is the migration loop's job; this oracle
//! must be RED on those until it lands, and must NOT itself implement the cutover.

use std::io::Write;
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

/// Unique sentinel exit code emitted by the fake Go binary. Chosen to be
/// distinct from the native exit codes any case can produce (0, 1, 2, 127).
const SENTINEL_EXIT: i32 = 73;
const SENTINEL_MARKER: &str = "FAKE_GO_DELEGATE_SENTINEL_42";

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("ctx-cli crate should live under <repo>/crates/ctx-cli")
        .to_path_buf()
}

/// Create (once) a stub executable that stands in for the Go compatibility
/// binary. When `delegate_to_go` spawns it, it prints a sentinel marker and
/// exits with `SENTINEL_EXIT`, making a delegation attempt observable.
fn fake_go_binary() -> PathBuf {
    let dir = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("cutover-fake-go");
    std::fs::create_dir_all(&dir).expect("create fake-go dir");

    #[cfg(unix)]
    {
        let path = dir.join("ctx-go-stub.sh");
        let script = format!(
            "#!/bin/sh\n\
             echo \"{SENTINEL_MARKER}\" 1>&2\n\
             exit {SENTINEL_EXIT}\n"
        );
        std::fs::write(&path, script).expect("write stub script");
        use std::os::unix::fs::PermissionsExt;
        let mut perms = std::fs::metadata(&path).unwrap().permissions();
        perms.set_mode(0o755);
        std::fs::set_permissions(&path, perms).expect("chmod stub");
        path
    }

    #[cfg(not(unix))]
    {
        let path = dir.join("ctx-go-stub.bat");
        let script = format!("@echo off\r\necho {SENTINEL_MARKER} 1>&2\r\nexit /b {SENTINEL_EXIT}\r\n");
        std::fs::write(&path, script).expect("write stub script");
        path
    }
}

/// Run the built Rust `ctx` binary with the fake Go fallback installed, feeding
/// empty stdin (so any native stdio server — e.g. MCP — sees immediate EOF and
/// exits instead of blocking). Extra env vars may be supplied; an entry whose
/// value is `None` is *removed* from the child environment.
fn run_ctx(args: &[&str], envs: &[(&str, Option<&str>)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args(args)
        .current_dir(repo_root())
        .env("CTX_GO_BIN", fake_go_binary())
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (k, v) in envs {
        match v {
            Some(val) => {
                cmd.env(k, val);
            }
            None => {
                cmd.env_remove(k);
            }
        }
    }
    let mut child = cmd.spawn().expect("spawn ctx binary");
    // Close stdin immediately (write nothing) so stdio servers hit EOF and exit.
    drop(child.stdin.take());
    child.wait_with_output().expect("collect ctx output")
}

fn delegated(out: &Output) -> bool {
    out.status.code() == Some(SENTINEL_EXIT)
        || String::from_utf8_lossy(&out.stderr).contains(SENTINEL_MARKER)
}

fn describe(out: &Output) -> String {
    format!(
        "exit={:?}\nstdout:\n{}\nstderr:\n{}",
        out.status.code(),
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr),
    )
}

/// Bind a loopback listener and hand back its port while keeping the socket
/// alive. A native web server that tries to bind the same port will fail fast
/// (EADDRINUSE) instead of blocking — a non-blocking probe for "native ran".
fn occupied_loopback_port() -> (TcpListener, u16) {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind probe listener");
    let port = listener.local_addr().expect("probe addr").port();
    (listener, port)
}

// ---------------------------------------------------------------------------
// Case 1 — `ctx mcp serve` must dispatch NATIVE (RED now; GREEN post-cutover).
// ---------------------------------------------------------------------------
#[test]
fn cutover_mcp_serve_dispatches_native() {
    // Empty stdin => native MCP stdio server sees EOF and returns Ok(()) (exit 0).
    // Pre-cutover, `mcp` is not in try_run_native => delegates => sentinel.
    let out = run_ctx(
        &["mcp", "serve", "--root", "."],
        &[("CTX_WEB_ENGINE", None)],
    );
    assert!(
        !delegated(&out),
        "`ctx mcp serve` must run NATIVE after the Wave 3 cutover, but it \
         delegated to the Go fallback.\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------------
// Case 2 — `ctx browse` with NO --web-engine flag and no env defaults to RUST
// (RED now; GREEN post-cutover).
// ---------------------------------------------------------------------------
#[test]
fn cutover_browse_default_engine_is_rust() {
    let (_held, port) = occupied_loopback_port();
    let port_s = port.to_string();
    // Loopback bind passes the non-loopback guard; the native rust engine then
    // tries to bind an already-occupied port and fails FAST (exit 1) without
    // blocking. Pre-cutover the empty default engine delegates => sentinel.
    let out = run_ctx(
        &["browse", "--port", &port_s, "--no-open"],
        &[("CTX_WEB_ENGINE", None)],
    );
    assert!(
        !delegated(&out),
        "`ctx browse` with no --web-engine flag must default to the RUST web \
         engine after the cutover, but it delegated to Go.\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------------
// Case 3 — CTX_WEB_ENGINE explicitly unset => rust (the new default).
// (RED now; GREEN post-cutover.)
// ---------------------------------------------------------------------------
#[test]
fn cutover_browse_with_env_unset_is_rust() {
    let (_held, port) = occupied_loopback_port();
    let port_s = port.to_string();
    let out = run_ctx(
        &["browse", "--port", &port_s, "--no-open"],
        // Explicitly scrub the env var to prove the *default* (not an env
        // override) selects the rust engine.
        &[("CTX_WEB_ENGINE", None)],
    );
    assert!(
        !delegated(&out),
        "With CTX_WEB_ENGINE unset, `ctx browse` must select the RUST engine by \
         default after the cutover, but it delegated to Go.\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------------
// Case 4 — `ctx tui` must dispatch NATIVE (RED now; GREEN post-tui-cutover).
// tui was the last Go carve-out; once ctx-cli routes it to the native ctx-tui
// (ratatui) crate it must NOT delegate. The probe closes stdin (run_ctx) and
// runs in a non-TTY, so the native ctx-tui `run()` fails fast trying to enter
// the alternate screen / raw mode (ENOTTY) and exits WITHOUT touching the Go
// stub — proving the native path ran. Pre-cutover `tui` is absent from
// try_run_native, so it delegates => sentinel (RED now).
// ---------------------------------------------------------------------------
#[test]
fn cutover_tui_dispatches_native() {
    let out = run_ctx(&["tui"], &[("CTX_WEB_ENGINE", None)]);
    assert!(
        !delegated(&out),
        "`ctx tui` must run NATIVE (ctx-tui ratatui) after the tui cutover, but \
         it delegated to the Go fallback. (The native run() must fail fast on a \
         non-TTY rather than block.)\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------------
// Case 6 — root (no-subcommand) `--json` tree must dispatch NATIVE (RED now;
// GREEN after the JSONTree port). A flag-first non-symbols invocation currently
// falls through try_run_native (run_symbols_command only handles --symbols) →
// None → delegates to Go. The native JSONTree renderer makes it native.
// ---------------------------------------------------------------------------
#[test]
fn cutover_root_json_tree_dispatches_native() {
    let out = run_ctx(
        &["--git=false", "--json", "tests/where-fixtures/small_repo"],
        &[("CTX_WEB_ENGINE", None)],
    );
    assert!(
        !delegated(&out),
        "root `--json` tree must run NATIVE after the tree port, but it delegated to Go.\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------------
// Case 7 — root (no-subcommand) DEFAULT text tree must dispatch NATIVE (RED now;
// GREEN after the render.Tree port). `ctx [path]` with no --json/--budget falls
// through try_run_native → None → delegates. The native text-tree renderer
// (+ renderPlanFit footer) makes it native.
// ---------------------------------------------------------------------------
#[test]
fn cutover_root_text_tree_dispatches_native() {
    let out = run_ctx(
        &["--git=false", "tests/where-fixtures/small_repo"],
        &[("CTX_WEB_ENGINE", None)],
    );
    assert!(
        !delegated(&out),
        "root default text tree must run NATIVE after the tree port, but it delegated to Go.\n{}",
        describe(&out)
    );
}

// ---------------------------------------------------------------------------
// Case 8 — root with tree/budget FLAGS must dispatch NATIVE (RED now; GREEN
// after the root flag-variant port). Representative: --depth (and --budget).
// ---------------------------------------------------------------------------
#[test]
fn cutover_root_flags_dispatch_native() {
    for args in [
        &["--git=false", "--depth", "1", "tests/where-fixtures/small_repo"][..],
        &["--git=false", "--budget", "200", "tests/where-fixtures/small_repo"][..],
    ] {
        let out = run_ctx(args, &[("CTX_WEB_ENGINE", None)]);
        assert!(
            !delegated(&out),
            "root with flags {args:?} must run NATIVE after the flag-variant port, but it delegated.\n{}",
            describe(&out)
        );
    }
}

// ---------------------------------------------------------------------------
// Case 5 — SANITY: an already-native command (`where`) still runs native
// (must stay GREEN through the cutover).
// ---------------------------------------------------------------------------
#[test]
fn cutover_where_still_dispatches_native() {
    let out = run_ctx(
        &["where", "nonexistent-symbol-query", "--json"],
        &[("CTX_WEB_ENGINE", None)],
    );
    assert!(
        !delegated(&out),
        "`ctx where` is ported and must run NATIVE; it delegated to Go.\n{}",
        describe(&out)
    );
}
