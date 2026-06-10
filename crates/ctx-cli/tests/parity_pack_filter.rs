use serde_json::Value;
use std::fs;
use std::process::Command;

mod common;
use common::*;

#[test]
fn native_pack_since_use_mtime_filters_without_go_delegate() {
    let root = test_dir("pack-since-mtime");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/new.go"),
        "package src\n\nfunc NewThing() {}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/old.go"),
        "package src\n\nfunc OldThing() {}\n",
    )
    .unwrap();
    set_file_mtime_yyyymmddhhmm(&root.join("src/old.go"), "202001010000");

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--since",
            "7d",
            "--use-mtime",
            "--no-contract",
            "--format",
            "json",
            "--goal",
            "new old thing",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack since use-mtime failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("parse pack since use-mtime JSON: {err}"));
    let included: Vec<String> = payload["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect();
    assert!(included.iter().any(|path| path == "src/new.go"));
    assert!(!included.iter().any(|path| path == "src/old.go"));
}

#[test]
fn native_pack_until_use_mtime_filters_without_go_delegate() {
    let root = test_dir("pack-until-mtime");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/new.go"),
        "package src\n\nfunc NewThing() {}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/old.go"),
        "package src\n\nfunc OldThing() {}\n",
    )
    .unwrap();
    set_file_mtime_yyyymmddhhmm(&root.join("src/old.go"), "202001010000");

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--until",
            "7d",
            "--use-mtime",
            "--no-contract",
            "--format",
            "json",
            "--goal",
            "new old thing",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack until use-mtime failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("parse pack until use-mtime JSON: {err}"));
    let included: Vec<String> = payload["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect();
    assert!(!included.iter().any(|path| path == "src/new.go"));
    assert!(included.iter().any(|path| path == "src/old.go"));
}

#[test]
fn native_pack_since_uses_git_commit_time_without_go_delegate() {
    let root = test_dir("pack-since-git-time");
    fs::create_dir_all(root.join("src")).unwrap();
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.email", "dev@example.com"]);
    run_git(&root, &["config", "user.name", "Dev"]);

    fs::write(
        root.join("src/old.go"),
        "package src\n\nfunc OldThing() {}\n",
    )
    .unwrap();
    run_git(&root, &["add", "."]);
    run_git_with_date(&root, &["commit", "-m", "old"], "2020-01-01T00:00:00Z");

    fs::write(
        root.join("src/new.go"),
        "package src\n\nfunc NewThing() {}\n",
    )
    .unwrap();
    run_git(&root, &["add", "."]);
    run_git_with_date(&root, &["commit", "-m", "new"], "2026-05-01T00:00:00Z");

    fs::write(
        root.join("src/untracked.go"),
        "package src\n\nfunc UntrackedThing() {}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--since",
            "365d",
            "--no-contract",
            "--format",
            "json",
            "--goal",
            "old new untracked thing",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack since git-time failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("parse pack since git-time JSON: {err}"));
    let included: Vec<String> = payload["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect();
    assert!(!included.iter().any(|path| path == "src/old.go"));
    assert!(included.iter().any(|path| path == "src/new.go"));
    assert!(included.iter().any(|path| path == "src/untracked.go"));
}

#[test]
fn native_pack_changed_filters_to_git_changes_without_go_delegate() {
    let root = test_dir("pack-changed");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.go"), "package src\n\nfunc Run() {}\n").unwrap();
    fs::write(
        root.join("src/other.go"),
        "package src\n\nfunc Other() {}\n",
    )
    .unwrap();
    Command::new("git")
        .args(["init"])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial"])
        .current_dir(&root)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() bool {\n    return true\n}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--changed",
            "--no-contract",
            "--format",
            "json",
            "--goal",
            "run other",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack changed failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack changed JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let included = payload["included"].as_array().expect("included");
    assert_eq!(included.len(), 1);
    assert_eq!(included[0]["path"], "src/app.go");
}
