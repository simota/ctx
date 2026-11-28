//! `ctx pack --profile NAME` and `ctx profiles`: named selection strategies
//! from the root's ctx.toml, overridden by options on the command line.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

mod common;
use common::test_dir;

fn git(root: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap_or_else(|err| panic!("run git {args:?}: {err}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ctx(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(args)
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap_or_else(|err| panic!("run ctx {args:?}: {err}"))
}

fn json(output: &Output) -> Value {
    assert!(
        output.status.success(),
        "ctx failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn included(pack: &Value) -> Vec<String> {
    pack["included"]
        .as_array()
        .expect("included")
        .iter()
        .map(|file| file["path"].as_str().unwrap().to_string())
        .collect()
}

const CTX_TOML: &str = r#"
[pack]
preset = ""

[profiles.review]
description = "Branch review with its tests"
base = "main"
tests = 5
budget = 9000
format = "json"

[profiles.web]
include = ["web/**"]
format = "json"
"#;

/// main: src/store.py, its test, src/other.py, web/app.ts.
/// feature branch: a committed edit to store.py and an uncommitted one to
/// other.py.
fn repo() -> std::path::PathBuf {
    let root = test_dir("pack-profile");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("web")).unwrap();
    fs::write(root.join("src/store.py"), "def load():\n    return 1\n").unwrap();
    fs::write(
        root.join("src/test_store.py"),
        "from store import load\n\ndef test_load():\n    assert load() == 1\n",
    )
    .unwrap();
    fs::write(root.join("src/other.py"), "def other():\n    pass\n").unwrap();
    fs::write(root.join("web/app.ts"), "export const app = 1;\n").unwrap();
    fs::write(root.join("ctx.toml"), CTX_TOML).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    git(&root, &["checkout", "-q", "-b", "feature"]);
    fs::write(root.join("src/store.py"), "def load():\n    return 2\n").unwrap();
    git(&root, &["commit", "-qam", "change store"]);
    fs::write(root.join("src/other.py"), "def other():\n    return 3\n").unwrap();
    root
}

#[test]
fn profile_applies_its_selection_budget_and_format() {
    let root = repo();
    let pack = json(&ctx(&root, &["pack", "--profile", "review"]));
    assert_eq!(pack["budget"], 9000);
    let files = included(&pack);
    assert!(files.contains(&"src/store.py".to_string()), "{files:?}");
    assert!(files.contains(&"src/other.py".to_string()), "{files:?}");
    assert!(
        files.contains(&"src/test_store.py".to_string()),
        "tests=5: {files:?}"
    );
    assert!(
        !files.contains(&"web/app.ts".to_string()),
        "base=main: {files:?}"
    );
}

#[test]
fn command_line_options_override_the_profile() {
    let root = repo();
    let pack = json(&ctx(
        &root,
        &["pack", "--budget", "4000", "--profile=review", "--tests=0"],
    ));
    assert_eq!(pack["budget"], 4000, "--budget before --profile still wins");
    assert!(!included(&pack).contains(&"src/test_store.py".to_string()));

    let pack = json(&ctx(&root, &["pack", "--profile", "web"]));
    assert_eq!(included(&pack), vec!["web/app.ts".to_string()]);
}

#[test]
fn unknown_profile_names_the_defined_ones() {
    let root = repo();
    let out = ctx(&root, &["pack", "--profile", "reveiw"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("unknown profile \"reveiw\" (defined: review, web)"),
        "{stderr}"
    );
}

#[test]
fn conflicting_options_say_the_profile_applied() {
    let root = repo();
    let out = ctx(&root, &["pack", "--profile", "review", "--from-where"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(
        stderr.contains("--base does not combine with --from-where"),
        "{stderr}"
    );
    assert!(stderr.contains("options from --profile review"), "{stderr}");
}

#[test]
fn invalid_profile_is_rejected_before_packing() {
    let root = repo();
    fs::write(root.join("ctx.toml"), "[profiles.x]\ndeps = 3\n").unwrap();
    let out = ctx(&root, &["pack", "--profile", "x"]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("needs a file selection"), "{stderr}");
}

#[test]
fn profiles_command_lists_profiles_with_their_options() {
    let root = repo();
    let out = ctx(&root, &["profiles"]);
    assert!(out.status.success());
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(
        stdout.contains("review  — Branch review with its tests\n  ctx pack --budget=9000 --format=json --base main --tests=5\n"),
        "{stdout}"
    );
    let listed = json(&ctx(&root, &["profiles", "--json"]));
    assert_eq!(listed[1]["name"], "web");
    assert_eq!(listed[1]["include"][0], "web/**");
}
