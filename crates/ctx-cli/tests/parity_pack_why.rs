use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_pack_why_json_without_go_delegate() {
    let root = test_dir("pack-why-json");
    fs::create_dir_all(root.join("src/auth")).unwrap();
    fs::write(
        root.join("src/auth/login.go"),
        "package auth\n\nfunc Login() bool {\n    return true\n}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--why",
            "src/auth/login.go",
            "--format",
            "json",
            "--goal",
            "login auth",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack why json failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack why JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let item = &payload.as_array().expect("why payload array")[0];
    assert_eq!(item["path"], "src/auth/login.go");
    assert_eq!(item["exists"], true);
    assert_eq!(item["decision"], "included");
    assert_eq!(item["tier"], "high");
    assert!(item["score"].as_i64().unwrap_or(0) >= 10);
}

#[test]
fn native_pack_why_missing_path_reports_and_exits_nonzero_without_go_delegate() {
    let root = test_dir("pack-why-missing");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/real.go"), "package src\n\nfunc Real() {}\n").unwrap();

    let output = run_rust_in_with_env(
        &root,
        &["pack", "--why", "src/missing.go", "--goal", "real"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stdout.contains("File: src/missing.go"));
    assert!(stdout.contains("Decision: outside_scope"));
    assert!(stderr.contains("pack --why: path not found in repo: src/missing.go"));
}

#[test]
fn native_pack_why_rejects_explain_without_go_delegate() {
    let root = test_dir("pack-why-explain");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.go"), "package src\n\nfunc Run() {}\n").unwrap();

    let output = run_rust_in_with_env(
        &root,
        &["pack", "--why", "src/app.go", "--explain", "--goal", "run"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mutually exclusive"));
}
