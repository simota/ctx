use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_doctor_emits_expected_json_shape() {
    let dir = test_dir("doctor");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    fs::write(
        dir.join("ctx.toml"),
        format!(
            "[audit]\nenabled = true\npath = \"{}\"\nquery_handling = \"mask\"\nmask_patterns = [\"token\"]\n\n[security]\nstrict_offline = true\nsecret_scan = true\n",
            dir.join("audit.log").display()
        ),
    )
    .unwrap_or_else(|err| panic!("write ctx.toml: {err}"));
    fs::write(dir.join(".ctxignore"), "\n# comment\ndist/\n*.log\n")
        .unwrap_or_else(|err| panic!("write .ctxignore: {err}"));

    let root = dir.to_string_lossy().to_string();
    let output = run_rust(&["doctor", &root, "--json"]);
    assert!(
        output.status.success(),
        "doctor failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse doctor JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });

    assert_eq!(
        report["system"]["platform"],
        format!("{}/{}", go_os_name(), go_arch_name())
    );
    assert_eq!(report["strict_offline"]["flag_value"], true);
    assert_eq!(report["configuration"]["ctxignore"], "present (2 rules)");
    assert_eq!(
        report["configuration"]["query_masking"],
        "mask (1 pattern(s))"
    );
    assert_eq!(report["browse"]["loopback"]["status"], "ok");
    assert!(report["components"]
        .as_array()
        .is_some_and(|items| items.len() >= 5));
}

// ── Wave-4 native-shape suite for `doctor` (ADR-0005) ────────────────────────
//
// `doctor` is NOW a NATIVE Rust command (wired into try_run_native), NOT a
// delegate-to-Go command. Unlike every other ported command it is NOT
// byte-parity-vs-Go: it INTROSPECTS the implementation, so the native build
// HONESTLY reports the Rust stack and legitimately diverges from Go's output:
//
//   - System: Go reports `go_version` (e.g. "go1.25.0"); native reports
//     `runtime` = "ctx-rust <version>" under a RENAMED, honest JSON key.
//   - Components: Go names go-tree-sitter (CGO) / go-git / tiktoken-go; native
//     names vendored C grammars (no CGO) / ctx-git (native object reader) /
//     ctx-tokens cl100k_base.
//   - Embedded UI: native verifies ctx-web's embedded `Dist` (index.html),
//     not internal/web/dist.
//
// Therefore these tests assert OUTPUT SHAPE against the NATIVE binary only
// (via run_rust_in), never byte-comparing against the Go oracle.

/// `doctor` text — assert the section headers and runtime line are present.
#[test]
fn native_doctor_text_shape() {
    let dir = test_dir("doctor-native-text");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));

    let rust = run_rust_in(&repo_root(), &["doctor", &dir.to_string_lossy()]);
    assert_eq!(
        rust.status.code(),
        Some(0),
        "doctor exit code\nstderr:\n{}",
        String::from_utf8_lossy(&rust.stderr)
    );
    let out = String::from_utf8_lossy(&rust.stdout);

    // Section headers (mirror renderDoctor's layout).
    for header in [
        "System",
        "Components",
        "Strict offline",
        "Configuration",
        "Browse readiness",
    ] {
        assert!(
            out.contains(header),
            "missing section header {header:?}\n{out}"
        );
    }
    // Honest native System line: "ctx-rust <version>", not a Go version.
    assert!(
        out.contains("Runtime:") && out.contains("ctx-rust "),
        "expected honest native runtime line\n{out}"
    );
    assert!(
        !out.contains("Go version:"),
        "native doctor must not report a Go version\n{out}"
    );
    // Honest native component (Rust stack, no CGO-via-Go).
    assert!(
        out.contains("ctx-git"),
        "expected native git backend in components\n{out}"
    );
}

/// `doctor --json` — assert the top-level keys + non-empty components + the
/// honest `runtime` System key (renamed from Go's `go_version`).
#[test]
fn native_doctor_json_shape() {
    let dir = test_dir("doctor-native-json");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    fs::write(
        dir.join("ctx.toml"),
        "[audit]\nenabled = false\nquery_handling = \"mask\"\nmask_patterns = [\"token\"]\n\n[security]\nstrict_offline = true\n",
    )
    .unwrap_or_else(|err| panic!("write ctx.toml: {err}"));
    fs::write(dir.join(".ctxignore"), "# comment\ndist/\n*.log\n")
        .unwrap_or_else(|err| panic!("write .ctxignore: {err}"));

    let rust = run_rust_in(&repo_root(), &["doctor", &dir.to_string_lossy(), "--json"]);
    assert_eq!(
        rust.status.code(),
        Some(0),
        "doctor --json exit code\nstderr:\n{}",
        String::from_utf8_lossy(&rust.stderr)
    );
    let report: Value = serde_json::from_slice(&rust.stdout).unwrap_or_else(|err| {
        panic!(
            "parse native doctor JSON: {err}\n{}",
            String::from_utf8_lossy(&rust.stdout)
        )
    });

    // Expected top-level keys.
    for key in [
        "system",
        "components",
        "strict_offline",
        "configuration",
        "browse",
    ] {
        assert!(
            report.get(key).is_some(),
            "missing top-level key {key:?}\n{report}"
        );
    }
    // System: honest renamed `runtime` key, NOT `go_version`.
    assert!(
        report["system"]["go_version"].is_null(),
        "native doctor must not emit go_version\n{report}"
    );
    assert!(
        report["system"]["runtime"]
            .as_str()
            .is_some_and(|s| s.starts_with("ctx-rust ")),
        "expected honest runtime value\n{report}"
    );
    assert_eq!(
        report["system"]["platform"],
        format!("{}/{}", go_os_name(), go_arch_name())
    );
    // Components is non-empty.
    assert!(
        report["components"]
            .as_array()
            .is_some_and(|c| !c.is_empty()),
        "components must be non-empty\n{report}"
    );
    // Native config/ctxignore/masking computed natively.
    assert_eq!(report["configuration"]["ctxignore"], "present (2 rules)");
    assert_eq!(
        report["configuration"]["query_masking"],
        "mask (1 pattern(s))"
    );
    // Embedded-UI check reuses ctx-web's embedded Dist → ok.
    assert_eq!(report["browse"]["embedded_ui"]["status"], "ok");
}

/// `doctor --strict-offline` — flag flips the strict_offline report fields.
#[test]
fn native_doctor_strict_offline() {
    let dir = test_dir("doctor-native-strict");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));

    let rust = run_rust_in(
        &repo_root(),
        &[
            "doctor",
            &dir.to_string_lossy(),
            "--strict-offline",
            "--json",
        ],
    );
    assert_eq!(
        rust.status.code(),
        Some(0),
        "doctor --strict-offline exit code\nstderr:\n{}",
        String::from_utf8_lossy(&rust.stderr)
    );
    let report: Value = serde_json::from_slice(&rust.stdout).unwrap_or_else(|err| {
        panic!(
            "parse native doctor JSON: {err}\n{}",
            String::from_utf8_lossy(&rust.stdout)
        )
    });

    assert_eq!(report["strict_offline"]["flag_value"], true);
    assert_eq!(report["browse"]["strict_offline"]["status"], "ok");
}

// ── Wave-1 byte-parity suite for `roots` ─────────────────────────────────────
//
// Goal: prove `roots add`, `roots list`, and `roots remove` are provably
// byte-identical to Go across their full surface.
//
// Registry isolation: CTX_ROOTS_FILE env var points each binary (Go and Rust)
// to a distinct tmp-dir file, seeded with identical operations before
// comparison. The paths registered are temp dirs created via test_dir() so
// both binaries canonicalize to the same absolute path.
//
// Go surface (internal/cli/roots.go):
//   - `roots add [path] [--name NAME]`   (aliases: register)
//   - `roots list`                       (aliases: ls)
//   - `roots remove <name-or-path>`      (aliases: rm)
//   - `roots open <name-or-path>`        — intentionally NOT tested: spawns a
//     long-running web server child process and depends on the unported browse
//     server. It is not a terminating command and cannot produce stable output.
//
// Not covered: `roots open` (long-running web server — not a terminating
// command). The `RootsCommand::Open` arm is handled by `roots_command` and
// returns Ok; `run_roots_command` always returns Some(ExitCode) for it too.
//
// `run_roots_command` has NO reachable `return None` for any valid invocation:
//   - `parse_roots_args` returns None only on: unknown flags, double-`roots`,
//     unknown subcommand, or invalid arg counts for each subcommand.
//   - All valid invocations of add/list/remove/open return Some(RootsArgs) and
//     thus always return Some(ExitCode) from run_roots_command.
//
// roots `open` exclusion:
//   - `open` / `o` spawns a detached `ctx browse` subprocess (a long-running
//     HTTP server). It is not a terminating command, depends on the unported
//     web server, and opens a browser window. It is intentionally excluded
//     from byte-parity tests. The arm still runs natively (no delegation);
//     it is excluded from testing only because it cannot produce stable,
//     byte-comparable output.
