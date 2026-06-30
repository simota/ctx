use std::fs;

mod common;
use common::*;

fn write_log_fixture() -> std::path::PathBuf {
    let root = test_dir("log");
    fs::create_dir_all(root.join("src")).unwrap();
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.name", "Test"]);
    run_git(&root, &["config", "user.email", "test@example.com"]);

    fs::write(
        root.join("src/login.ts"),
        "export function login(): string {\n  return 'v1';\n}\n",
    )
    .unwrap();
    run_git(&root, &["add", "."]);
    run_git_with_date(
        &root,
        &["commit", "-m", "initial login"],
        "2024-01-01T00:00:00Z",
    );

    fs::write(
        root.join("src/login.ts"),
        "export function login(): string {\n  return 'v2';\n}\n",
    )
    .unwrap();
    fs::write(root.join("README.md"), "notes\n").unwrap();
    run_git(&root, &["add", "."]);
    run_git_with_date(
        &root,
        &["commit", "-m", "update login"],
        "2024-01-02T00:00:00Z",
    );

    fs::write(root.join("README.md"), "notes\nmore notes\n").unwrap();
    run_git(&root, &["add", "."]);
    run_git_with_date(
        &root,
        &["commit", "-m", "update docs"],
        "2024-01-03T00:00:00Z",
    );
    root
}

#[test]
fn native_log_non_tty_requires_tty() {
    let output = run_rust_in_with_env(
        &repo_root(),
        &["log"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ctx log: requires an interactive terminal (TTY)"));
}

#[test]
fn native_log_plain_outputs_recent_commits_without_tty() {
    let root = write_log_fixture();
    let output = run_rust_in_with_env(
        &root,
        &["log", "--plain", "--limit", "2"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "ctx log --plain failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("ctx log HEAD (repo)"));
    assert!(stdout.contains("update docs"));
    assert!(stdout.contains("update login"));
    assert!(!stdout.contains("initial login"));
}

#[test]
fn native_log_path_plain_narrows_history() {
    let root = write_log_fixture();
    let output = run_rust_in_with_env(
        &root,
        &["log", "--plain", "--path", "src/login.ts", "--limit", "3"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "ctx log --path failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("matched paths: src/login.ts"));
    assert!(stdout.contains("update login"));
    assert!(stdout.contains("initial login"));
    assert!(!stdout.contains("update docs"));
}

#[test]
fn native_log_query_json_reports_matched_paths() {
    let root = write_log_fixture();
    let output = run_rust_in_with_env(
        &root,
        &["log", "--json", "--query", "login", "--limit", "5"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "ctx log --query failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(payload["source"]["kind"], "query");
    let matched = payload["source"]["matched_paths"].as_array().unwrap();
    assert!(matched.iter().any(|value| value == "src/login.ts"));
}

#[test]
fn native_log_rejects_invalid_ref_before_git() {
    let output = run_rust_in_with_env(
        &repo_root(),
        &["log", "--plain", "--ref", "-n1"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("ctx log: invalid --ref"));
}
