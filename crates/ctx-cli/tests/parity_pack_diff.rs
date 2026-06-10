use serde_json::Value;
use std::fs;
use std::process::Command;

mod common;
use common::*;

#[test]
fn native_pack_from_stdin_git_diff_paths_without_go_delegate() {
    let root = test_dir("pack-from-stdin-diff");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.go"), "package src\n\nfunc Run() {}\n").unwrap();
    fs::write(
        root.join("src/helper.go"),
        "package src\n\nfunc Helper() {}\n",
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args([
        "pack",
        "--from-stdin",
        "--no-contract",
        "--format",
        "json",
        "--goal",
        "run helper",
        "--budget",
        "1000",
    ])
    .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
    .current_dir(&root)
    .stdin(std::process::Stdio::piped())
    .stdout(std::process::Stdio::piped())
    .stderr(std::process::Stdio::piped());
    let mut child = cmd.spawn().unwrap();
    {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin");
        stdin
            .write_all(b"diff --git a/src/app.go b/src/app.go\nindex 1..2 100644\n")
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "pack from-stdin diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack from-stdin diff JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let included = payload["included"].as_array().expect("included");
    assert_eq!(included.len(), 1);
    assert_eq!(included[0]["path"], "src/app.go");
}

#[test]
fn native_pack_diff_unified_without_go_delegate() {
    let root = test_dir("pack-diff-unified");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.go"), "package src\n\nfunc Run() {}\n").unwrap();
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
    Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "update"])
        .current_dir(&root)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--diff",
            "HEAD~1..HEAD",
            "--layout",
            "unified",
            "--no-contract",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack diff unified failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("```diff"));
    assert!(stdout.contains("diff --git"));
    assert!(stdout.contains("+func Run() bool"));
}

#[test]
fn native_pack_diff_sequential_api_only_without_go_delegate() {
    let root = test_dir("pack-diff-api-only");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/login.ts"),
        "export function login(): Promise<void> { return Promise.resolve(); }\nfunction privateImpl() {}\n",
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
        root.join("src/login.ts"),
        "export function login(email: string): Promise<Session> { return load(email); }\nfunction privateImpl() {}\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "update"])
        .current_dir(&root)
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .output()
        .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--diff",
            "HEAD~1..HEAD",
            "--api-only",
            "--no-contract",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack diff api-only failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("### src/login.ts"));
    assert!(stdout.contains("**Before**"));
    assert!(stdout.contains("**After**"));
    assert!(stdout.contains("export function login(): Promise<void>"));
    assert!(stdout.contains("export function login(email: string): Promise<Session>"));
    assert!(!stdout.contains("privateImpl"));
    assert!(!stdout.contains("return load"));
}
