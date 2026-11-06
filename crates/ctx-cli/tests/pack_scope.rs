//! `ctx pack` records what it left out (`skipped` / "Omitted files" /
//! snapshot), scopes the walk with `--include` / `--exclude`, and with
//! `--overflow signatures` packs a signature digest of a relevant file that
//! does not fit the budget instead of dropping it.

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;
use common::test_dir;

fn ctx(root: &Path, args: &[&str], stdin: Option<&str>, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args(args)
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("spawn ctx");
    {
        let mut pipe = child.stdin.take().unwrap();
        if let Some(input) = stdin {
            pipe.write_all(input.as_bytes()).unwrap();
        }
    }
    child.wait_with_output().expect("wait ctx")
}

fn ok(out: Output) -> String {
    assert!(
        out.status.success(),
        "ctx failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn json(out: Output) -> Value {
    serde_json::from_str(&ok(out)).expect("pack json")
}

fn paths(v: &Value, key: &str) -> Vec<String> {
    v[key]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["path"].as_str().unwrap().to_string())
        .collect()
}

/// A repo with one large relevant file (`src/auth/session.rs`), small
/// relevant ones, tests and docs.
fn fixture(name: &str) -> PathBuf {
    let root = test_dir(name);
    fs::create_dir_all(root.join("src/auth")).unwrap();
    fs::create_dir_all(root.join("tests")).unwrap();
    fs::create_dir_all(root.join("docs")).unwrap();
    let big: String = (0..120)
        .map(|i| {
            format!(
                "pub fn session_fn_{i}(user: &str,\n    ttl: u64) -> bool {{\n    let x = {i};\n    x > 0 && user.len() > {i} && ttl > 3\n}}\n\n"
            )
        })
        .collect();
    fs::write(root.join("src/auth/session.rs"), big).unwrap();
    fs::write(
        root.join("src/auth/login.rs"),
        "pub fn login() -> bool { true }\n",
    )
    .unwrap();
    fs::write(
        root.join("tests/session_test.rs"),
        "#[test]\nfn session_works() {}\n",
    )
    .unwrap();
    fs::write(root.join("docs/notes.md"), "# notes\n").unwrap();
    root
}

#[test]
fn packs_record_budget_drops_in_json_and_markdown() {
    let root = fixture("pack-scope-omitted");
    let v = json(ctx(
        &root,
        &[
            "pack",
            "--goal",
            "session login",
            "--budget",
            "300",
            "--json",
            "--no-contract",
            "--no-warnings",
        ],
        None,
        &[],
    ));
    assert!(!paths(&v, "included").contains(&"src/auth/session.rs".to_string()));
    let dropped = v["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .find(|s| s["path"] == "src/auth/session.rs")
        .expect("budget drop recorded");
    assert_eq!(dropped["reason"], "budget exceeded");
    assert!(dropped["tokens"].as_i64().unwrap() > 300);

    let md = ok(ctx(
        &root,
        &[
            "pack",
            "--goal",
            "session login",
            "--budget",
            "300",
            "--no-contract",
            "--no-warnings",
        ],
        None,
        &[],
    ));
    assert!(md.contains("## Omitted files ("), "{md}");
    assert!(md.contains("- src/auth/session.rs ("), "{md}");
    assert!(md.contains("tokens) - budget exceeded\n"), "{md}");
    assert!(
        md.contains("Including every budget-exceeded file needs about"),
        "{md}"
    );
}

#[test]
fn a_pack_that_fits_has_no_omitted_section() {
    let root = fixture("pack-scope-fits");
    let md = ok(ctx(
        &root,
        &["pack", "--include", "src/auth/login.rs", "--no-contract"],
        None,
        &[],
    ));
    assert!(!md.contains("Omitted"), "{md}");
    assert!(md.contains("### src/auth/login.rs"), "{md}");
}

#[test]
fn include_and_exclude_scope_the_walk() {
    let root = fixture("pack-scope-filters");
    let v = json(ctx(
        &root,
        &[
            "pack",
            "--include",
            "src/**,*.md",
            "--exclude",
            "*login*",
            "--json",
            "--no-contract",
        ],
        None,
        &[],
    ));
    assert_eq!(
        paths(&v, "included"),
        vec!["docs/notes.md", "src/auth/session.rs"]
    );
    // Filtered-out files are out of scope, not omissions.
    assert!(v["skipped"].as_array().unwrap().is_empty());

    let v = json(ctx(
        &root,
        &[
            "pack",
            "--include=*.{rs}",
            "--exclude=src/auth",
            "--json",
            "--no-contract",
        ],
        None,
        &[],
    ));
    assert_eq!(paths(&v, "included"), vec!["tests/session_test.rs"]);
}

#[test]
fn explicit_selections_record_missing_and_filtered_files() {
    let root = fixture("pack-scope-stdin");
    let out = ok(ctx(
        &root,
        &[
            "pack",
            "--from-stdin",
            "--exclude",
            "tests",
            "--plain",
            "--no-contract",
        ],
        Some("src/auth/login.rs\nnope.rs\ntests/session_test.rs\n"),
        &[],
    ));
    assert!(out.contains("+ src/auth/login.rs  ("), "{out}");
    assert!(out.contains("- nope.rs  (not found or ignored)\n"), "{out}");
    assert!(
        out.contains("- tests/session_test.rs  (excluded by --include/--exclude)\n"),
        "{out}"
    );
}

#[test]
fn from_where_records_unreadable_and_filtered_paths() {
    let root = fixture("pack-scope-from-where");
    let where_json =
        r#"[{"path":"src/auth/login.rs"},{"path":"missing.rs"},{"path":"docs/notes.md"}]"#;
    let v = json(ctx(
        &root,
        &[
            "pack",
            "--from-where",
            "--exclude",
            "docs",
            "--json",
            "--no-contract",
            "--no-warnings",
        ],
        Some(where_json),
        &[],
    ));
    assert_eq!(paths(&v, "included"), vec!["src/auth/login.rs"]);
    assert_eq!(paths(&v, "skipped"), vec!["missing.rs", "docs/notes.md"]);
}

#[test]
fn overflow_signatures_keeps_the_api_of_a_file_that_does_not_fit() {
    let root = fixture("pack-scope-overflow");
    let args = [
        "pack",
        "--goal",
        "session login",
        "--budget",
        "3000",
        "--overflow",
        "signatures",
        "--no-contract",
        "--no-warnings",
    ];
    let md = ok(ctx(&root, &args, None, &[]));
    assert!(
        md.contains("- src/auth/session.rs (") && md.contains("tokens, signatures only)"),
        "{md}"
    );
    assert!(md.contains("[ctx: signatures only - src/auth/session.rs is "));
    assert!(md.contains("\n1: pub fn session_fn_0(user: &str,\n2:     ttl: u64) -> bool {\n"));
    assert!(!md.contains("let x = 0;"), "bodies are dropped: {md}");
    assert!(!md.contains("budget exceeded"), "{md}");

    // A budget too small even for the digest still drops (and records) it.
    let mut tight = args.to_vec();
    tight[4] = "200";
    tight.push("--json");
    let v = json(ctx(&root, &tight, None, &[]));
    assert!(paths(&v, "skipped").contains(&"src/auth/session.rs".to_string()));
}

#[test]
fn rejects_bad_overflow_and_patterns() {
    let root = fixture("pack-scope-errors");
    let out = ctx(&root, &["pack", "--overflow", "trim"], None, &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("unknown --overflow value"));
    let out = ctx(&root, &["pack", "--include", "*.{rs"], None, &[]);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--include/--exclude"));
    let out = ctx(
        &root,
        &["pack", "--overflow", "signatures", "--api-only"],
        None,
        &[],
    );
    assert!(!out.status.success());
}

#[test]
fn snapshots_record_skipped_files() {
    let root = fixture("pack-scope-snapshot");
    let state = root.join(".state");
    let state_s = state.to_string_lossy().into_owned();
    ok(ctx(
        &root,
        &[
            "pack",
            "--goal",
            "session login",
            "--budget",
            "300",
            "--snapshot",
            "s1",
            "--no-contract",
            "--no-warnings",
        ],
        None,
        &[("XDG_STATE_HOME", &state_s), ("HOME", &state_s)],
    ));
    let manifest = walk_find(&state, "s1.json").expect("snapshot written");
    let v: Value = serde_json::from_str(&fs::read_to_string(manifest).unwrap()).unwrap();
    let skipped = v["skipped"].as_array().expect("skipped array");
    assert!(skipped
        .iter()
        .any(|s| s["path"] == "src/auth/session.rs" && s["reason"] == "budget exceeded"));
}

fn walk_find(dir: &Path, name: &str) -> Option<PathBuf> {
    for entry in fs::read_dir(dir).ok()? {
        let path = entry.ok()?.path();
        if path.is_dir() {
            if let Some(found) = walk_find(&path, name) {
                return Some(found);
            }
        } else if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
    }
    None
}
