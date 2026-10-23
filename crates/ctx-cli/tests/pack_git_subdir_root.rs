//! `ctx pack <subdir>` options backed by git (`--since`, `--changed`,
//! `--diff`) must name paths relative to the pack root, not the work tree.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::{Command, Output};

mod common;
use common::test_dir;

fn git(root: &Path, args: &[&str], date: &str) {
    let output = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .env("GIT_AUTHOR_DATE", date)
        .env("GIT_COMMITTER_DATE", date)
        .output()
        .unwrap_or_else(|err| panic!("run git {args:?}: {err}"));
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

fn ctx(root: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args(args)
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let output = cmd
        .output()
        .unwrap_or_else(|err| panic!("run ctx {args:?}: {err}"));
    assert!(
        output.status.success(),
        "ctx {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

fn included_paths(output: &Output) -> Vec<String> {
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    payload["included"]
        .as_array()
        .expect("included")
        .iter()
        .map(|file| file["path"].as_str().expect("path").to_string())
        .collect()
}

/// A repo whose `sub/` holds old files (one with a non-ASCII name) and a
/// recently committed one, plus a recent change outside `sub/`.
fn write_repo(name: &str) -> std::path::PathBuf {
    let root = test_dir(name);
    fs::create_dir_all(root.join("sub")).unwrap();
    git(&root, &["init", "-q"], "2001-01-01T00:00:00Z");
    fs::write(root.join("sub/old.rs"), "fn old() {}\n").unwrap();
    fs::write(root.join("sub/fresh.rs"), "fn fresh() {}\n").unwrap();
    fs::write(root.join("top.rs"), "fn top() {}\n").unwrap();
    fs::write(root.join("sub/ノート.rs"), "fn note() {}\n").unwrap();
    git(&root, &["add", "."], "2001-01-01T00:00:00Z");
    git(&root, &["commit", "-qm", "initial"], "2001-01-01T00:00:00Z");
    fs::write(root.join("sub/fresh.rs"), "fn fresh() -> u8 { 1 }\n").unwrap();
    fs::write(root.join("top.rs"), "fn top() -> u8 { 2 }\n").unwrap();
    git(&root, &["commit", "-qam", "update"], "2030-01-01T00:00:00Z");
    root
}

#[test]
fn pack_since_on_a_subdirectory_root_uses_git_commit_times() {
    let root = write_repo("pack-subdir-since");
    let output = ctx(
        &root,
        &[
            "pack",
            "sub",
            "--since",
            "2020-01-01",
            "--no-contract",
            "--format",
            "json",
        ],
        &[],
    );
    assert_eq!(included_paths(&output), vec!["fresh.rs".to_string()]);
}

#[test]
fn pack_since_matches_non_ascii_names_to_their_commit_time() {
    let root = write_repo("pack-since-non-ascii");
    // `sub/ノート.rs` was committed long ago but written to disk just now:
    // its commit time must win, which needs its name to match git's
    // (otherwise quoted) output.
    let output = ctx(
        &root,
        &[
            "pack",
            ".",
            "--since",
            "2020-01-01",
            "--no-contract",
            "--format",
            "json",
        ],
        &[],
    );
    assert_eq!(
        included_paths(&output),
        vec!["sub/fresh.rs".to_string(), "top.rs".to_string()]
    );
}

#[test]
fn pack_changed_on_a_subdirectory_root_lists_its_changed_files() {
    let root = write_repo("pack-subdir-changed");
    fs::write(root.join("sub/old.rs"), "fn old() -> u8 { 3 }\n").unwrap();
    fs::write(root.join("sub/new.rs"), "fn new() {}\n").unwrap();
    fs::write(root.join("top.rs"), "fn top() -> u8 { 4 }\n").unwrap();
    let output = ctx(
        &root,
        &[
            "pack",
            "sub",
            "--changed",
            "--no-contract",
            "--format",
            "json",
        ],
        &[],
    );
    assert_eq!(
        included_paths(&output),
        vec!["new.rs".to_string(), "old.rs".to_string()]
    );
}

#[test]
fn pack_diff_on_a_subdirectory_root_packs_only_its_patches() {
    let root = write_repo("pack-subdir-diff");
    let output = ctx(
        &root,
        &[
            "pack",
            "sub",
            "--diff",
            "HEAD~1..HEAD",
            "--layout",
            "unified",
            "--no-contract",
        ],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("+++ b/fresh.rs"), "{stdout}");
    assert!(stdout.contains("+fn fresh() -> u8 { 1 }"), "{stdout}");
    assert!(!stdout.contains("top.rs"), "{stdout}");

    let output = ctx(
        &root,
        &["pack", "sub", "--diff", "HEAD~1..HEAD", "--no-contract"],
        &[],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("### fresh.rs"), "{stdout}");
    assert!(stdout.contains("fn fresh() {}"), "{stdout}");
    assert!(stdout.contains("fn fresh() -> u8 { 1 }"), "{stdout}");
}

#[test]
fn pack_diff_ignores_external_diff_and_color_config() {
    let root = write_repo("pack-diff-user-config");
    let output = ctx(
        &root,
        &[
            "pack",
            ".",
            "--diff",
            "HEAD~1..HEAD",
            "--layout",
            "unified",
            "--no-contract",
        ],
        &[
            ("GIT_CONFIG_COUNT", "2"),
            ("GIT_CONFIG_KEY_0", "diff.external"),
            ("GIT_CONFIG_VALUE_0", "echo external-diff-ran"),
            ("GIT_CONFIG_KEY_1", "color.ui"),
            ("GIT_CONFIG_VALUE_1", "always"),
        ],
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("+fn top() -> u8 { 2 }"), "{stdout}");
    assert!(!stdout.contains("external-diff-ran"), "{stdout}");
    assert!(!stdout.contains('\u{1b}'), "{stdout}");
}
