//! `ctx log` run from a subdirectory reads the enclosing repository's
//! history, and `--path` is relative to the current directory.

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

fn ctx_log(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctx"))
        .arg("log")
        .args(args)
        .current_dir(cwd)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap_or_else(|err| panic!("run ctx log {args:?}: {err}"))
}

fn stdout_of(output: &Output) -> String {
    assert!(
        output.status.success(),
        "ctx log failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).into_owned()
}

fn fixture() -> std::path::PathBuf {
    let root = test_dir("log-subdir");
    fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join("top.txt"), "top\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "add top"]);
    fs::write(root.join("src/a.ts"), "export const a = 1\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "add a"]);
    root
}

#[test]
fn log_from_a_subdirectory_reads_the_repository_history() {
    let root = fixture();
    let out = stdout_of(&ctx_log(&root.join("src"), &["--plain"]));
    assert!(out.contains("add a"), "{out}");
    assert!(out.contains("add top"), "{out}");
}

#[test]
fn log_path_is_relative_to_the_current_directory() {
    let root = fixture();
    let src = root.join("src");

    let out = stdout_of(&ctx_log(&src, &["--plain", "--path", "a.ts"]));
    assert!(out.contains("src/a.ts"), "{out}");
    assert!(out.contains("add a"), "{out}");
    assert!(!out.contains("add top"), "{out}");

    let out = stdout_of(&ctx_log(&src, &["--plain", "--path", "../top.txt"]));
    assert!(out.contains("add top"), "{out}");
    assert!(!out.contains("add a"), "{out}");

    let out = ctx_log(&src, &["--plain", "--path", "../../elsewhere.txt"]);
    assert_eq!(out.status.code(), Some(2), "{out:?}");
}
