use serde_json::Value;
use std::fs;
use std::process::Command;

mod common;
use common::*;

#[test]
fn native_pack_from_mix_applies_recipe_without_go_delegate() {
    let root = test_dir("pack-from-mix");
    let xdg = test_dir("pack-from-mix-xdg");
    let store = xdg.join("ctx/mixes");
    fs::create_dir_all(&store).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(".ctxignore"), "src/secret.go\n").unwrap();
    fs::write(root.join("src/auth.go"), "package src\n\nfunc Login() {}\n").unwrap();
    fs::write(
        root.join("src/secret.go"),
        "package src\n\nfunc Secret() {}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/other.go"),
        "package src\n\nfunc Other() {}\n",
    )
    .unwrap();
    fs::write(
        store.join("auth.mix.json"),
        r#"{
  "schema_version": 1,
  "id": "auth",
  "name": "auth",
  "goal": "saved login secret goal",
  "created": "2026-05-31T00:00:00Z",
  "files": ["src/auth.go", "src/secret.go"],
  "budget": { "limit": 12345 }
}
"#,
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--from-mix",
            "auth",
            "--no-contract",
            "--format",
            "json",
        ],
        &[
            ("CTX_GO_BIN", "/definitely/missing/ctx-go"),
            ("XDG_STATE_HOME", xdg.to_str().unwrap()),
        ],
    );
    assert!(
        output.status.success(),
        "pack from-mix failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("parse pack from-mix JSON: {err}"));
    assert_eq!(payload["goal"], "saved login secret goal");
    assert_eq!(payload["budget"], 12345);
    let included: Vec<String> = payload["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect();
    assert!(included.iter().any(|path| path == "src/auth.go"));
    assert!(included.iter().any(|path| path == "src/secret.go"));
    assert!(!included.iter().any(|path| path == "src/other.go"));
}

#[test]
fn native_pack_from_mix_rejects_other_stdin_modes_without_go_delegate() {
    let root = test_dir("pack-from-mix-exclusive");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("main.go"), "package main\n").unwrap();

    let output = run_rust_in_with_env(
        &root,
        &["pack", "--from-mix", "auth", "--from-stdin"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("mutually exclusive"));
}

#[test]
fn native_pack_api_only_and_redaction_without_go_delegate() {
    let root = test_dir("pack-api-redact");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/api.go"),
        "package src\n\nfunc Public() bool {\n    secret := \"hidden\"\n    return secret != \"\"\n}\n\nfunc private() {}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/secret.env"),
        "OPENAI_API_KEY=example_secret_abcdef0123456789\n",
    )
    .unwrap();

    let api_output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--api-only",
            "--no-contract",
            "--preset",
            "llm",
            "--goal",
            "public private",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        api_output.status.success(),
        "pack api-only failed: {}",
        String::from_utf8_lossy(&api_output.stderr)
    );
    let api_stdout = String::from_utf8_lossy(&api_output.stdout);
    assert!(api_stdout.contains("func Public() bool"));
    assert!(!api_stdout.contains("secret := "));
    assert!(!api_stdout.contains("func private"));

    let redact_output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--no-contract",
            "--preset",
            "llm",
            "--goal",
            "api key secret env",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        redact_output.status.success(),
        "pack redaction failed: {}",
        String::from_utf8_lossy(&redact_output.stderr)
    );
    let redact_stdout = String::from_utf8_lossy(&redact_output.stdout);
    assert!(redact_stdout.contains("[REDACTED"));
    assert!(!redact_stdout.contains("sk-abcdef0123456789"));
}

#[test]
fn native_pack_from_stdin_paths_bypass_ctxignore_without_go_delegate() {
    let root = test_dir("pack-from-stdin-paths");
    fs::create_dir_all(root.join("generated")).unwrap();
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join(".ctxignore"), "generated/\n").unwrap();
    fs::write(
        root.join("generated/schema.go"),
        "package generated\n\nfunc Schema() {}\n",
    )
    .unwrap();
    fs::write(root.join("src/app.go"), "package src\n\nfunc Run() {}\n").unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args([
        "pack",
        "--from-stdin",
        "--no-contract",
        "--format",
        "json",
        "--goal",
        "schema run",
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
            .write_all(b"generated/schema.go\ngenerated/schema.go\n/dev/null\n")
            .unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "pack from-stdin paths failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack from-stdin JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let included = payload["included"].as_array().expect("included");
    assert_eq!(included.len(), 1);
    assert_eq!(included[0]["path"], "generated/schema.go");
}
