//! `ctx pack --base <REV>`: a branch-review pack of every file changed since
//! the merge base with REV — committed on the branch, staged, unstaged and
//! untracked — relative to the pack root.

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
            "parse pack JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn entries<'a>(pack: &'a Value, key: &str) -> Vec<(&'a str, &'a str)> {
    pack[key]
        .as_array()
        .unwrap_or_else(|| panic!("{key} array"))
        .iter()
        .map(|file| {
            (
                file["path"].as_str().expect("path"),
                file["reason"].as_str().unwrap_or(""),
            )
        })
        .collect()
}

/// main: keep.rs, old.rs, sub/lib.rs, docs.md. feature (checked out):
/// sub/feature.rs added and docs.md deleted in a commit; main then gains
/// main_only.rs; the work tree edits keep.rs and adds an untracked sub/new.rs.
fn feature_repo(name: &str) -> std::path::PathBuf {
    let root = test_dir(name);
    fs::create_dir_all(root.join("sub")).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    fs::write(root.join("keep.rs"), "fn keep() {}\n").unwrap();
    fs::write(root.join("old.rs"), "fn old() {}\n").unwrap();
    fs::write(root.join("sub/lib.rs"), "fn lib() {}\n").unwrap();
    fs::write(root.join("docs.md"), "# docs\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "initial"]);
    git(&root, &["checkout", "-qb", "feature"]);
    fs::write(
        root.join("sub/feature.rs"),
        "fn feature() {}\nfn more() {}\n",
    )
    .unwrap();
    git(&root, &["rm", "-q", "docs.md"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "feature"]);
    git(&root, &["checkout", "-q", "main"]);
    fs::write(root.join("main_only.rs"), "fn main_only() {}\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "main moves on"]);
    git(&root, &["checkout", "-q", "feature"]);
    fs::write(root.join("keep.rs"), "fn keep() -> u8 { 1 }\n").unwrap();
    fs::write(root.join("sub/new.rs"), "fn new() {}\n").unwrap();
    root
}

#[test]
fn pack_base_includes_branch_commits_and_work_tree_changes() {
    let root = feature_repo("pack-base-branch");
    let pack = json(&ctx(&root, &["pack", "--base", "main", "--json"]));
    let included = entries(&pack, "included");
    let mut paths: Vec<&str> = included.iter().map(|(path, _)| *path).collect();
    paths.sort();
    assert_eq!(paths, vec!["keep.rs", "sub/feature.rs", "sub/new.rs"]);
    let reason = |path: &str| included.iter().find(|(p, _)| *p == path).unwrap().1;
    assert!(
        reason("sub/feature.rs").starts_with("changed vs main (+2 -0, committed)"),
        "{}",
        reason("sub/feature.rs")
    );
    assert!(reason("keep.rs").contains("uncommitted"));
    let skipped = entries(&pack, "skipped");
    assert!(
        skipped.contains(&("docs.md", "deleted vs main")),
        "{skipped:?}"
    );

    // Plain --changed still sees only the work tree.
    let changed = json(&ctx(&root, &["pack", "--changed", "--json"]));
    let mut paths: Vec<&str> = entries(&changed, "included")
        .iter()
        .map(|(p, _)| *p)
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["keep.rs", "sub/new.rs"]);
}

#[test]
fn pack_base_on_a_subdirectory_root_uses_root_relative_paths() {
    let root = feature_repo("pack-base-subdir");
    let pack = json(&ctx(&root, &["pack", "sub", "--base=main", "--json"]));
    let mut paths: Vec<&str> = entries(&pack, "included").iter().map(|(p, _)| *p).collect();
    paths.sort();
    assert_eq!(paths, vec!["feature.rs", "new.rs"]);
}

#[test]
fn pack_base_respects_include_and_exclude() {
    let root = feature_repo("pack-base-filter");
    let pack = json(&ctx(
        &root,
        &["pack", "--base", "main", "--exclude", "sub", "--json"],
    ));
    let paths: Vec<&str> = entries(&pack, "included").iter().map(|(p, _)| *p).collect();
    assert_eq!(paths, vec!["keep.rs"]);
}

#[test]
fn pack_base_rejects_unknown_or_option_like_revisions() {
    let root = feature_repo("pack-base-errors");
    for base in ["no-such-branch", "--output=x"] {
        let output = ctx(&root, &["pack", &format!("--base={base}")]);
        assert!(!output.status.success(), "--base={base} must fail");
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("--base"), "{stderr}");
    }
    let output = ctx(&root, &["pack", "--base", "main", "--diff", "main..HEAD"]);
    assert!(!output.status.success());
}
