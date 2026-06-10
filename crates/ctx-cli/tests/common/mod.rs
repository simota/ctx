#![allow(dead_code)]

use serde_json::Value;
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

pub static GO_BINARY: OnceLock<PathBuf> = OnceLock::new();

pub static TEST_DIR_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Byte-parity helper for roots commands with an isolated registry.
/// `go_ops` and `rust_ops` are called to pre-populate each registry before
/// the final assertion command is run.
pub fn assert_roots_parity_in_env(
    dir: &Path,
    go_roots_file: &Path,
    rust_roots_file: &Path,
    setup_args: &[&[&str]],
    assert_args: &[&str],
) {
    // Run setup operations on both registries to get them to the same state.
    for &op in setup_args {
        let go_out = run_go_in_with_env(
            &repo_root(),
            op,
            &[("CTX_ROOTS_FILE", &go_roots_file.to_string_lossy())],
        );
        let rust_out = run_rust_in_with_env(
            &repo_root(),
            op,
            &[("CTX_ROOTS_FILE", &rust_roots_file.to_string_lossy())],
        );
        assert_eq!(
            rust_out.status.code(),
            go_out.status.code(),
            "setup op {op:?} exit code mismatch\nGo: {}\nRust: {}",
            String::from_utf8_lossy(&go_out.stderr),
            String::from_utf8_lossy(&rust_out.stderr),
        );
    }
    // Now run the assertion command.
    let go = run_go_in_with_env(
        &repo_root(),
        assert_args,
        &[("CTX_ROOTS_FILE", &go_roots_file.to_string_lossy())],
    );
    let rust = run_rust_in_with_env(
        &repo_root(),
        assert_args,
        &[("CTX_ROOTS_FILE", &rust_roots_file.to_string_lossy())],
    );
    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch for {assert_args:?}\nGo stderr:\n{}\nRust stderr:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
    assert_eq!(
        rust.stdout,
        go.stdout,
        "stdout mismatch for {assert_args:?}\nGo:\n{}\nRust:\n{}",
        String::from_utf8_lossy(&go.stdout),
        String::from_utf8_lossy(&rust.stdout),
    );
    assert_eq!(
        rust.stderr,
        go.stderr,
        "stderr mismatch for {assert_args:?}\nGo:\n{}\nRust:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
    let _ = dir;
}

/// Returns a fixture with 3 deterministic snapshots in .ctx/replay/:
///   psnap-a: created 2026-01-01, no goal ("" → "-" in list text)
///   psnap-b: created 2026-01-02, goal="auth"
///   psnap-c: created 2026-01-03, goal="auth deploy"
/// psnap-a has 1 entry (Medium), psnap-b has 2 entries (Medium+High),
/// psnap-c has 1 entry (High) — non-trivial diff across b→c.
pub fn write_replay_parity_fixture() -> PathBuf {
    let root = test_dir("replay-parity");
    let replay_dir = root.join(".ctx").join("replay");
    fs::create_dir_all(&replay_dir).unwrap();
    fs::write(
        replay_dir.join("psnap-a.json"),
        r#"{
  "schema_version": 1,
  "id": "psnap-a",
  "created_at": "2026-01-01T00:00:00Z",
  "ctx_version": "test",
  "budget": 1000,
  "used": 80,
  "root": ".",
  "format": "markdown",
  "entries": [
    {"path": "cmd/main.go", "sha256": "aaa", "tokens": 80, "relevance": "Medium", "score": 5}
  ]
}"#,
    )
    .unwrap();
    fs::write(
        replay_dir.join("psnap-b.json"),
        r#"{
  "schema_version": 1,
  "id": "psnap-b",
  "created_at": "2026-01-02T00:00:00Z",
  "ctx_version": "test",
  "goal": "auth",
  "budget": 1000,
  "used": 200,
  "root": ".",
  "format": "markdown",
  "entries": [
    {"path": "cmd/main.go", "sha256": "aaa", "tokens": 80, "relevance": "High", "score": 15},
    {"path": "internal/auth.go", "sha256": "bbb", "tokens": 120, "relevance": "Medium", "score": 8}
  ]
}"#,
    )
    .unwrap();
    fs::write(
        replay_dir.join("psnap-c.json"),
        r#"{
  "schema_version": 1,
  "id": "psnap-c",
  "created_at": "2026-01-03T00:00:00Z",
  "ctx_version": "test",
  "goal": "auth deploy",
  "budget": 2000,
  "used": 300,
  "root": ".",
  "format": "markdown",
  "entries": [
    {"path": "cmd/main.go", "sha256": "aaa", "tokens": 80, "relevance": "High", "score": 20},
    {"path": "internal/deploy.go", "sha256": "ccc", "tokens": 220, "relevance": "High", "score": 18}
  ]
}"#,
    )
    .unwrap();
    root
}

// ── replay list ── ────────────────────────────────────────────────────────────

pub fn write_skim_fixture() -> PathBuf {
    let root = test_dir("skim-parity");
    fs::create_dir_all(root.join("src")).unwrap();
    // app.go: 11 cl100k_base tokens (verified against Go)
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() {\n    Helper()\n}\n",
    )
    .unwrap();
    // helper.go: 14 cl100k_base tokens
    fs::write(
        root.join("src/helper.go"),
        "package src\n\nfunc Helper() string {\n    return \"ok\"\n}\n",
    )
    .unwrap();
    root
}

pub fn write_onboarding_fixture() -> PathBuf {
    // Use the same multi-file fixture the existing native test uses, but
    // as a fresh test_dir so snapshot is stable.
    let root = test_dir("onboarding-parity");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("main.go"), "package main\n\nfunc main() {}\n").unwrap();
    fs::write(
        root.join("core.go"),
        "package main\n\n// Core is the domain core.\nfunc Core() {}\nfunc Helper() {}\n",
    )
    .unwrap();
    // test file — must be excluded by both Go and Rust
    fs::write(
        root.join("core_test.go"),
        "package main\n\nfunc TestCore() {}\n",
    )
    .unwrap();
    root
}

/// ULP-tolerant JSON comparison for onboarding JSON output.
///
/// `score_breakdown.symbol_count` and `score` contain the result of
/// `log2(symbols+1)*3`. Go's `math.Log2` and Rust's `f64::log2()` implement
/// the same IEEE 754 operation but with different last-bit rounding, producing
/// a 1-ULP difference (relative error ~1.3e-16). The difference is:
///
///   Go:   log2(3)*3 = 4.754887502163469 (0x4013015c8528fffc)
///   Rust: log2(3)*3 = 4.754887502163468 (0x4013015c8528fffb)
///
/// This is STABLE per side (not run-to-run non-deterministic) but unavoidably
/// different. Tolerance: 1e-12 relative / 1e-12 absolute — same as echo.
/// Every non-score-float field is compared byte-exact.
pub fn assert_onboarding_json_parity_in(root: &Path, args: &[&str]) {
    let go = run_go_in(root, args);
    let rust = run_rust_in(root, args);

    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch for {args:?}\nGo stderr:\n{}\nRust stderr:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr)
    );
    assert_eq!(
        rust.stderr,
        go.stderr,
        "stderr mismatch for {args:?}\nGo:\n{}\nRust:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr)
    );

    let go_json: Value = serde_json::from_slice(&go.stdout).unwrap_or_else(|err| {
        panic!(
            "parse Go onboarding JSON: {err}\nargs={args:?}\n{}",
            String::from_utf8_lossy(&go.stdout)
        )
    });
    let rust_json: Value = serde_json::from_slice(&rust.stdout).unwrap_or_else(|err| {
        panic!(
            "parse Rust onboarding JSON: {err}\nargs={args:?}\n{}",
            String::from_utf8_lossy(&rust.stdout)
        )
    });

    if !echo_json_equal(&go_json, &rust_json) {
        panic!(
            "JSON mismatch for args {args:?}\nGo:\n{}\nRust:\n{}",
            serde_json::to_string_pretty(&go_json).unwrap_or_default(),
            serde_json::to_string_pretty(&rust_json).unwrap_or_default(),
        );
    }
}

/// Write a minimal noise fixture: go.sum (lockfile), app.min.js (generated),
/// keep.go (clean). Returns the fixture root.
pub fn write_noise_fixture() -> PathBuf {
    let root = test_dir("noise-parity");
    fs::create_dir_all(&root).unwrap_or_else(|err| panic!("create {}: {err}", root.display()));
    fs::write(root.join("go.sum"), "module fake\ngo 1.21\n")
        .unwrap_or_else(|err| panic!("write go.sum: {err}"));
    fs::write(root.join("app.min.js"), "function minified(){}\n")
        .unwrap_or_else(|err| panic!("write app.min.js: {err}"));
    fs::write(root.join("keep.go"), "package main\n\nfunc main() {}\n")
        .unwrap_or_else(|err| panic!("write keep.go: {err}"));
    root
}

/// Write a noise fixture with 3 generated .min.js files in src/ to exercise
/// glob aggregation in --apply.
pub fn write_noise_glob_fixture() -> PathBuf {
    let root = test_dir("noise-glob");
    fs::create_dir_all(root.join("src")).unwrap_or_else(|err| panic!("create src/: {err}"));
    for name in &["a.min.js", "b.min.js", "c.min.js"] {
        fs::write(
            root.join("src").join(name),
            format!("function {}(){{}}\n", name.replace('.', "_")),
        )
        .unwrap_or_else(|err| panic!("write {name}: {err}"));
    }
    root
}

/// Create a pinned fixture git repo for digest tests.
/// Uses fixed GIT_AUTHOR_DATE / GIT_COMMITTER_DATE so commit hashes are reproducible.
pub fn write_digest_fixture() -> PathBuf {
    let root = test_dir("digest-parity");
    fs::create_dir_all(root.join("src")).unwrap_or_else(|err| panic!("create src/: {err}"));
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.email", "bot@ctx.dev"]);
    run_git(&root, &["config", "user.name", "CtxBot"]);

    // Commit 1: initial (root commit, pinned to 2024-06-01)
    fs::write(root.join("src/app.go"), "package main\n\nfunc main() {}\n")
        .unwrap_or_else(|err| panic!("write src/app.go: {err}"));
    run_git(&root, &["add", "."]);
    run_git_with_date(&root, &["commit", "-m", "initial"], "2024-06-01T00:00:00Z");

    // Commit 2: add Foo function (non-root, adds a symbol)
    fs::write(
        root.join("src/app.go"),
        "package main\n\nfunc main() {}\nfunc Foo() {}\n",
    )
    .unwrap_or_else(|err| panic!("write src/app.go: {err}"));
    run_git(&root, &["add", "."]);
    run_git_with_date(&root, &["commit", "-m", "add Foo"], "2024-06-15T00:00:00Z");

    root
}

/// Byte-parity helper for `digest --format json`.
///
/// Excludes `period.since` and `period.until` from comparison because:
/// - Go stores the time at nanosecond precision (e.g. "2024-06-01T11:15:41.385642Z")
/// - Rust uses midnight for `since` ("2024-06-01T00:00:00Z") and second-precision
///   for `until` ("2026-06-01T11:15:41Z").
/// Both agree on the DATE portion; the time-of-day differs. All other fields are
/// compared exactly (commits, authors, files, deltas, hot_files, since_ref, head_ref).
pub fn assert_digest_json_parity(root: &Path, args: &[&str]) {
    let go = run_go_in(root, args);
    let rust = run_rust_in(root, args);

    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch for digest json {args:?}\nGo stderr:\n{}\nRust stderr:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
    assert_eq!(
        rust.stderr, go.stderr,
        "stderr mismatch for digest json {args:?}",
    );

    // Parse both JSON outputs
    let mut go_json: Value = serde_json::from_slice(&go.stdout).unwrap_or_else(|err| {
        panic!(
            "parse Go digest JSON: {err}\n{}",
            String::from_utf8_lossy(&go.stdout)
        )
    });
    let mut rust_json: Value = serde_json::from_slice(&rust.stdout).unwrap_or_else(|err| {
        panic!(
            "parse Rust digest JSON: {err}\n{}",
            String::from_utf8_lossy(&rust.stdout)
        )
    });

    // Assert date prefix of `period.since` and `period.until` match (YYYY-MM-DD)
    // even though full timestamp differs.
    let go_since = go_json["period"]["since"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let rust_since = rust_json["period"]["since"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert_eq!(
        &go_since[..10.min(go_since.len())],
        &rust_since[..10.min(rust_since.len())],
        "period.since date prefix mismatch for {args:?}: Go={go_since:?} Rust={rust_since:?}",
    );
    let go_until = go_json["period"]["until"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let rust_until = rust_json["period"]["until"]
        .as_str()
        .unwrap_or("")
        .to_string();
    assert_eq!(
        &go_until[..10.min(go_until.len())],
        &rust_until[..10.min(rust_until.len())],
        "period.until date prefix mismatch for {args:?}: Go={go_until:?} Rust={rust_until:?}",
    );

    // Remove non-deterministic timestamp fields before structural comparison
    if let Some(period) = go_json.get_mut("period") {
        if let Some(obj) = period.as_object_mut() {
            obj.remove("since");
            obj.remove("until");
        }
    }
    if let Some(period) = rust_json.get_mut("period") {
        if let Some(obj) = period.as_object_mut() {
            obj.remove("since");
            obj.remove("until");
        }
    }

    assert_eq!(
        rust_json,
        go_json,
        "JSON mismatch (period.since/until excluded) for digest {args:?}\nGo:\n{}\nRust:\n{}",
        serde_json::to_string_pretty(&go_json).unwrap_or_default(),
        serde_json::to_string_pretty(&rust_json).unwrap_or_default(),
    );
}

pub fn assert_delegated_parity(args: &[&str]) {
    assert_delegated_parity_in(&repo_root(), args);
}

pub fn assert_delegated_parity_in(root: &Path, args: &[&str]) {
    let go = run_go_in(root, args);
    let rust = run_rust_in(root, args);

    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch for args {args:?}\nGo stderr:\n{}\nRust stderr:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
    assert_eq!(
        rust.stdout,
        go.stdout,
        "stdout mismatch for args {args:?}\nGo stdout:\n{}\nRust stdout:\n{}",
        String::from_utf8_lossy(&go.stdout),
        String::from_utf8_lossy(&rust.stdout),
    );
    assert_eq!(
        rust.stderr,
        go.stderr,
        "stderr mismatch for args {args:?}\nGo stderr:\n{}\nRust stderr:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
}

pub fn write_valid_audit_log(path: &Path) {
    let line1 = r#"{"command":"pack","exit":0,"prev_hash":null}"#;
    let line2 = format!(
        r#"{{"command":"pack","exit":0,"prev_hash":"{}"}}"#,
        sha256_hex(line1)
    );
    fs::write(path, format!("{line1}\n{line2}\n"))
        .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

pub fn write_broken_audit_log(path: &Path) {
    let line1 = r#"{"command":"pack","exit":0,"prev_hash":null}"#;
    let line2 = r#"{"command":"pack","exit":0,"prev_hash":"deadbeefdeadbeefdeadbeefdeadbeef"}"#;
    fs::write(path, format!("{line1}\n{line2}\n"))
        .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

/// A 4-line audit log where lines 2-3 are broken (bad prev_hash) and line 4 resumes
/// a valid chain from line 3. Produces "broken range: 2-3" output (broken_end > broken_at).
pub fn write_broken_range_audit_log(path: &Path) {
    // line1: genesis (prev_hash null) — valid
    let line1 = r#"{"command":"pack","exit":0,"prev_hash":null}"#;
    // line2: bad prev_hash (not sha256 of line1) — broken
    let line2 = r#"{"command":"pack","exit":0,"prev_hash":"deadbeefdeadbeefdeadbeefdeadbeef"}"#;
    // line3: bad prev_hash (not sha256 of line2) — broken (extends range)
    let line3 = r#"{"command":"pack","exit":0,"prev_hash":"badc0ffeebadc0ffeebadc0ffeebadc0"}"#;
    // line4: correct prev_hash of line3 — resumes valid chain (ends broken range at 3)
    let line4 = format!(
        r#"{{"command":"pack","exit":0,"prev_hash":"{}"}}"#,
        sha256_hex(line3)
    );
    fs::write(path, format!("{line1}\n{line2}\n{line3}\n{line4}\n"))
        .unwrap_or_else(|err| panic!("write {}: {err}", path.display()));
}

pub fn run_go_in_with_env(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(go_binary());
    cmd.args(args).current_dir(root);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output()
        .unwrap_or_else(|err| panic!("run Go ctx with args {args:?}: {err}"))
}

pub fn sha256_hex(raw: &str) -> String {
    format!("{:x}", Sha256::digest(raw.as_bytes()))
}

pub fn test_dir(name: &str) -> PathBuf {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    let seq = TEST_DIR_COUNTER.fetch_add(1, Ordering::Relaxed);
    repo_root()
        .join("crates")
        .join("ctx-cli")
        .join("target")
        .join("testdata")
        .join(format!("{name}-{}-{now}-{seq}", std::process::id()))
}

pub fn set_file_mtime_yyyymmddhhmm(path: &Path, timestamp: &str) {
    let status = Command::new("touch")
        .args(["-t", timestamp])
        .arg(path)
        .status()
        .unwrap_or_else(|err| panic!("touch {}: {err}", path.display()));
    assert!(
        status.success(),
        "touch {} failed: {status}",
        path.display()
    );
}

pub fn run_go_in(root: &Path, args: &[&str]) -> Output {
    Command::new(go_binary())
        .args(args)
        .current_dir(root)
        .output()
        .unwrap_or_else(|err| panic!("run Go ctx with args {args:?}: {err}"))
}

pub fn run_rust(args: &[&str]) -> Output {
    run_rust_in(&repo_root(), args)
}

pub fn run_rust_in(root: &Path, args: &[&str]) -> Output {
    run_rust_in_with_env(root, args, &[])
}

pub fn run_rust_in_with_env(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args(args)
        .env("CTX_GO_BIN", go_binary())
        .current_dir(root);
    for (key, value) in envs {
        cmd.env(key, value);
    }
    cmd.output()
        .unwrap_or_else(|err| panic!("run Rust ctx with args {args:?}: {err}"))
}

pub fn run_git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .status()
        .unwrap_or_else(|err| panic!("run git {args:?}: {err}"));
    assert!(status.success(), "git {args:?} failed with {status}");
}

pub fn run_git_with_date(root: &Path, args: &[&str], date: &str) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .status()
        .unwrap_or_else(|err| panic!("run git {args:?}: {err}"));
    assert!(status.success(), "git {args:?} failed with {status}");
}

pub fn go_binary() -> &'static Path {
    GO_BINARY.get_or_init(|| {
        // Wave 4 (ADR-0005): the Go oracle is the FROZEN `go-oracle/v1` tag, not
        // the working tree — so the parity gate survives Go deletion from main.
        // Honor CTX_GO_BIN, else build the frozen oracle via ci/build-go-oracle.sh
        // (materializes the tag + builds + caches; its cmd/ctx is byte-identical
        // to the pre-deletion tree).
        if let Ok(p) = std::env::var("CTX_GO_BIN") {
            let p = PathBuf::from(p);
            if p.exists() {
                return p;
            }
        }
        let repo = repo_root();
        let out = Command::new("bash")
            .arg(repo.join("ci/build-go-oracle.sh"))
            .output()
            .unwrap_or_else(|err| panic!("run ci/build-go-oracle.sh: {err}"));
        assert!(
            out.status.success(),
            "ci/build-go-oracle.sh failed:\n{}",
            String::from_utf8_lossy(&out.stderr)
        );
        let bin = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
        assert!(
            bin.exists(),
            "frozen Go oracle not produced at {}",
            bin.display()
        );
        bin
    })
}

pub fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(Path::parent)
        .expect("ctx-cli crate should live under <repo>/crates/ctx-cli")
        .to_path_buf()
}

pub fn write_relation_fixture() -> PathBuf {
    let root = test_dir("relations");
    fs::create_dir_all(root.join("cmd")).unwrap();
    fs::create_dir_all(root.join("lib")).unwrap();
    fs::write(root.join("go.mod"), "module example.com/app\n\ngo 1.25\n").unwrap();
    fs::write(
        root.join("cmd/main.go"),
        "package main\n\nimport _ \"example.com/app/lib\"\n\nfunc main() {}\n",
    )
    .unwrap();
    fs::write(root.join("lib/lib.go"), "package lib\n\nfunc Helper() {}\n").unwrap();
    root
}

/// Multi-file Go project for non-trivial deps/impact graph.
///
/// Graph:
///   main.go  →  lib/a.go (via "example.com/m/lib")
///            →  lib/b.go (via "example.com/m/lib" — same package)
///            →  lib/sub/c.go (via "example.com/m/lib/sub")
///
/// This gives us:
///   - deps(main.go)     = [lib/a.go, lib/b.go, lib/sub/c.go]  (non-empty)
///   - deps(lib/a.go)    = []                                    (empty leaf)
///   - deps(lib/b.go)    = []                                    (empty leaf)
///   - deps(lib/sub/c.go)= []                                    (empty leaf)
///   - impact(lib/a.go)  = [main.go]                             (non-empty)
///   - impact(lib/sub/c.go) = [main.go]                          (non-empty)
///   - impact(main.go)   = []                                    (empty root)
pub fn write_go_project_fixture() -> PathBuf {
    let root = test_dir("go-project");
    fs::create_dir_all(root.join("lib/sub")).unwrap();
    fs::write(root.join("go.mod"), "module example.com/m\n\ngo 1.22\n").unwrap();
    fs::write(
        root.join("main.go"),
        "package main\n\nimport (\n\t\"fmt\"\n\t\"example.com/m/lib\"\n\tother \"example.com/m/lib/sub\"\n)\n\nfunc main() {\n\t_ = fmt.Sprint(\"\")\n\t_ = lib.X\n\t_ = other.Y\n}\n",
    )
    .unwrap();
    fs::write(root.join("lib/a.go"), "package lib\n\nvar X = 1\n").unwrap();
    fs::write(root.join("lib/b.go"), "package lib\n\nvar Y = 2\n").unwrap();
    fs::write(root.join("lib/sub/c.go"), "package sub\n\nvar Y = 3\n").unwrap();
    root
}

pub fn write_where_fixture() -> PathBuf {
    let root = test_dir("where");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() {\n    Helper()\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/helper.go"),
        "package src\n\nfunc Helper() string {\n    return \"ok\"\n}\n",
    )
    .unwrap();
    root
}

pub fn write_map_fixture() -> PathBuf {
    let root = test_dir("map");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() {\n    Helper()\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/helper.go"),
        "package src\n\nfunc Helper() string {\n    return \"ok\"\n}\n",
    )
    .unwrap();
    fs::write(root.join("docs/readme.md"), "# Notes\n\nSmall fixture.\n").unwrap();
    root
}

pub fn write_replay_fixture() -> PathBuf {
    let root = test_dir("replay");
    let replay_dir = root.join(".ctx").join("replay");
    fs::create_dir_all(&replay_dir).unwrap();
    fs::write(
        replay_dir.join("snap-a.json"),
        r#"{
  "schema_version": 1,
  "id": "snap-a",
  "created_at": "2026-01-01T00:00:00Z",
  "ctx_version": "test",
  "goal": "ship",
  "budget": 1000,
  "used": 100,
  "root": ".",
  "format": "markdown",
  "entries": [
    {"path": "src/app.go", "sha256": "a", "tokens": 100, "relevance": "Medium", "score": 10}
  ]
}"#,
    )
    .unwrap();
    fs::write(
        replay_dir.join("snap-b.json"),
        r#"{
  "schema_version": 1,
  "id": "snap-b",
  "created_at": "2026-01-02T00:00:00Z",
  "ctx_version": "test",
  "goal": "ship",
  "budget": 1000,
  "used": 170,
  "root": ".",
  "format": "markdown",
  "entries": [
    {"path": "src/app.go", "sha256": "a", "tokens": 120, "relevance": "High", "score": 20},
    {"path": "src/new.go", "sha256": "b", "tokens": 50, "relevance": "Medium", "score": 12}
  ]
}"#,
    )
    .unwrap();
    root
}

pub fn go_os_name() -> &'static str {
    match std::env::consts::OS {
        "macos" => "darwin",
        other => other,
    }
}

pub fn go_arch_name() -> &'static str {
    match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "amd64",
        other => other,
    }
}

/// Echo fixture: a multi-paragraph, multi-file pack body.
/// Three files with distinct paragraph and symbol boundaries so all three
/// chunk strategies (paragraph, symbol, fixed) produce non-trivial results.
/// The goal "rate limit burst handler" has clear relevance to limit.go and
/// weaker relevance to auth.go/intro.md — this exercises BM25 ranking,
/// tie-break ordering, and concentration stats.
pub fn write_echo_fixture() -> PathBuf {
    let root = test_dir("echo-parity");
    fs::create_dir_all(root.join("middleware")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();

    // Pack file at root — the cli takes a file path, not a directory.
    fs::write(
        root.join("pack.md"),
        concat!(
            "# Context Pack\n\n",
            "**Goal**: rate limit burst handler\n\n",
            "## File contents\n\n",
            "### middleware/limit.go\n\n",
            "```go\n",
            "package middleware\n\n",
            "// Limiter enforces rate limiting with burst tolerance.\n",
            "type Limiter struct {\n",
            "\trate  int\n",
            "\tburst int\n",
            "}\n\n",
            "// NewLimiter creates a new rate limiter.\n",
            "func NewLimiter(rate, burst int) *Limiter {\n",
            "\treturn &Limiter{rate: rate, burst: burst}\n",
            "}\n\n",
            "// Allow returns true if the request is within the rate limit.\n",
            "func (l *Limiter) Allow() bool {\n",
            "\tif l.burst > 0 {\n",
            "\t\tl.burst--\n",
            "\t\treturn true\n",
            "\t}\n",
            "\treturn l.rate > 0\n",
            "}\n",
            "```\n\n",
            "### middleware/auth.go\n\n",
            "```go\n",
            "package middleware\n\n",
            "// Auth validates request credentials.\n",
            "func Auth(token string) bool {\n",
            "\treturn token != \"\"\n",
            "}\n\n",
            "// Handler wraps an HTTP handler with auth.\n",
            "func Handler(next func()) func() {\n",
            "\treturn func() {\n",
            "\t\tif Auth(\"x\") {\n",
            "\t\t\tnext()\n",
            "\t\t}\n",
            "\t}\n",
            "}\n",
            "```\n\n",
            "### docs/intro.md\n\n",
            "```\n",
            "# Introduction\n\n",
            "This package provides rate limiting and burst handling.\n\n",
            "Use NewLimiter to create a limiter with a rate and burst value.\n",
            "```\n",
        ),
    )
    .unwrap();
    root
}

/// ULP-tolerant JSON comparison for echo JSON output.
///
/// BM25 `score` fields in the JSON output cannot be asserted byte-for-byte
/// against Go. There are TWO independent, empirically-verified causes — both
/// confined to the last 1-2 ULP of each f64 score (relative error ~1.3e-16,
/// i.e. one ULP at f64 machine epsilon):
///
///   1. DOMINANT, deterministic: `math.Log` (Go stdlib) and `f64::ln` (Rust
///      stdlib) implement the natural logarithm with different algorithms and
///      disagree in the last bit. The BM25 idf term is `ln(1 + (N-df+0.5)/
///      (df+0.5))`, so every score inherits this 1-ULP idf difference. This
///      difference is STABLE across runs on each side — Go is deterministic
///      here — but Go and Rust produce different last bits. Verified: for
///      `medium_pack --goal middleware --chunk-by paragraph` Go emits
///      3.3358682934662056 and Rust 3.335868293466205 (rel diff 1.33e-16),
///      identically on every run.
///
///   2. SECONDARY, run-to-run non-deterministic in Go ONLY: when a chunk
///      matches 2+ distinct goal tokens, Go sums `idf[t]*(numer/denom)` while
///      iterating a `map[string]int` in randomised order. f64 addition is not
///      associative, so Go's OWN output varies in the last ULP across repeated
///      runs (verified: `small_pack --goal "rate limit burst handler"
///      --chunk-by paragraph` yields 7 distinct byte outputs over 20 Go runs;
///      `--chunk-by symbol` yields 3). Single-token-per-chunk inputs do NOT
///      trigger this (no summation), but still hit cause (1).
///
/// Because Go itself is non-deterministic (cause 2) AND Go≠Rust even when Go
/// is deterministic (cause 1), byte-equality on score fields is impossible and
/// would make the test flaky. The markdown and plain renderers round scores to
/// %.2f / %.4f, which absorbs both effects — those formats ARE asserted
/// byte-for-byte (see the markdown/plain tests above). Only the raw-f64 JSON
/// score field needs tolerance.
///
/// Tolerance is 1e-12 relative / 1e-12 absolute — four orders of magnitude
/// above the observed ~1e-16 ULP noise (so genuine ULP differences pass) yet
/// nine-plus orders below any real scorer regression (a wrong BM25 constant,
/// tokenization drift, or chunk-boundary bug changes a score by >1e-3, which
/// this still catches). Every NON-score field (pack_file, goal, chunks_total,
/// chunks_covered, exit_code, threshold, coverage_score/spread_index when
/// they are whole numbers, matches counts, path, line_start, line_end,
/// concentration, null-vs-[] shape) is compared EXACTLY — no tolerance.
pub fn assert_echo_json_parity_in(root: &Path, args: &[&str]) {
    let go = run_go_in(root, args);
    let rust = run_rust_in(root, args);

    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch for args {args:?}\nGo stderr:\n{}\nRust stderr:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );
    assert_eq!(
        rust.stderr,
        go.stderr,
        "stderr mismatch for args {args:?}\nGo stderr:\n{}\nRust stderr:\n{}",
        String::from_utf8_lossy(&go.stderr),
        String::from_utf8_lossy(&rust.stderr),
    );

    let go_json: Value = serde_json::from_slice(&go.stdout).unwrap_or_else(|err| {
        panic!(
            "parse Go echo JSON: {err}\nargs={args:?}\n{}",
            String::from_utf8_lossy(&go.stdout)
        )
    });
    let rust_json: Value = serde_json::from_slice(&rust.stdout).unwrap_or_else(|err| {
        panic!(
            "parse Rust echo JSON: {err}\nargs={args:?}\n{}",
            String::from_utf8_lossy(&rust.stdout)
        )
    });

    if !echo_json_equal(&go_json, &rust_json) {
        panic!(
            "JSON mismatch for args {args:?}\nGo:\n{}\nRust:\n{}",
            serde_json::to_string_pretty(&go_json).unwrap_or_default(),
            serde_json::to_string_pretty(&rust_json).unwrap_or_default(),
        );
    }
}

/// Recursive JSON equality with a TIGHT float tolerance (1e-12 relative /
/// 1e-12 absolute — only the BM25 score f64 ULP noise from math.Log≠f64::ln
/// and Go map-order summation passes; any real regression is >>1e-3) and
/// null/[] aliasing for slice fields that Go emits as null when nil but we
/// emit as []. Integer-valued numbers are compared EXACTLY (no tolerance).
pub fn echo_json_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Null, Value::Null) => true,
        // Go emits `null` for nil slices; serde emits `[]` for empty Vec.
        (Value::Null, Value::Array(v)) | (Value::Array(v), Value::Null) => v.is_empty(),
        (Value::Bool(x), Value::Bool(y)) => x == y,
        (Value::Number(x), Value::Number(y)) => {
            if let (Some(xi), Some(yi)) = (x.as_i64(), y.as_i64()) {
                return xi == yi;
            }
            if let (Some(xf), Some(yf)) = (x.as_f64(), y.as_f64()) {
                if xf == yf {
                    return true;
                }
                let diff = (xf - yf).abs();
                let scale = xf.abs().max(yf.abs()).max(1.0);
                return diff <= 1e-12 || diff <= 1e-12 * scale;
            }
            false
        }
        (Value::String(x), Value::String(y)) => x == y,
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y.iter()).all(|(a, b)| echo_json_equal(a, b))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|v2| echo_json_equal(v, v2)))
        }
        _ => false,
    }
}
