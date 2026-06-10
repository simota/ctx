use serde_json::Value;
use std::fs;
use std::process::Command;

mod common;
use common::*;

#[test]
fn native_pack_from_where_no_contract_does_not_require_go_delegate() {
    let root = test_dir("pack-from-where");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() {\n    Helper()\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/helper.go"),
        "package src\n\nfunc Helper() string {\n    return \"ok\"\n}\n",
    )
    .unwrap();

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args([
        "pack",
        "--from-where",
        "--no-contract",
        "--format",
        "json",
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
        stdin.write_all(b"src/app.go\nsrc/helper.go\n").unwrap();
    }
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "pack from-where failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(payload["budget"], 1000);
    assert_eq!(payload["included"][0]["path"], "src/app.go");
    assert_eq!(payload["included"][1]["path"], "src/helper.go");
}

#[test]
fn native_pack_normal_no_contract_does_not_require_go_delegate() {
    let root = test_dir("pack-normal");
    fs::create_dir_all(root.join("src/auth")).unwrap();
    fs::write(
        root.join("src/auth/login.go"),
        "package auth\n\nfunc Login() bool {\n    return true\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/other.go"),
        "package src\n\nfunc Helper() {}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--no-contract",
            "--format",
            "json",
            "--goal",
            "login auth",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack normal failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(payload["goal"], "login auth");
    assert!(payload["included"].as_array().is_some_and(|items| items
        .iter()
        .any(|item| item["path"] == "src/auth/login.go" && item["relevance"] == "High")));
}

#[test]
fn native_pack_normal_default_contract_does_not_require_go_delegate() {
    let root = test_dir("pack-normal-contract");
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
            "--format",
            "json",
            "--goal",
            "login auth",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack normal contract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse pack JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(payload["contract"]["schema_version"], 1);
    assert_eq!(payload["contract"]["files"][0]["path"], "src/auth/login.go");
    assert!(payload["contract"]["files"][0]["symbols"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item == "Login")));
}

#[test]
fn native_pack_xml_format_without_go_delegate() {
    let root = test_dir("pack-xml");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.go"), "package src\n\nfunc Run() {}\n").unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--format",
            "xml",
            "--no-contract",
            "--goal",
            "run",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack xml failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#"<context-pack goal="run""#));
    assert!(stdout.contains(r#"budget="1000""#));
    assert!(stdout.contains(r#"files="1""#));
    assert!(!stdout.contains("ctx:contract v1"));
}

#[test]
fn native_pack_xml_contract_without_go_delegate() {
    let root = test_dir("pack-xml-contract");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(root.join("src/app.go"), "package src\n\nfunc Run() {}\n").unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--format",
            "xml",
            "--contract",
            "--goal",
            "run",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack xml contract failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains(r#"<context-pack goal="run""#));
    assert!(stdout.contains("<!-- ctx:contract v1"));
}

#[test]
fn native_pack_preset_llm_emits_plain_contents_without_go_delegate() {
    let root = test_dir("pack-preset-llm");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() {\n    Helper()\n}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--preset",
            "llm",
            "--no-contract",
            "--goal",
            "run helper",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack preset llm failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== src/app.go ==="));
    assert!(stdout.contains("func Run()"));
    assert!(!stdout.contains("Context Pack"));
}

#[test]
fn native_pack_preset_blog_uses_frontmatter_and_omits_paths_without_go_delegate() {
    let root = test_dir("pack-preset-blog");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() bool {\n    return true\n}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--preset",
            "blog",
            "--no-contract",
            "--goal",
            "run",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack preset blog failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("---\ntitle: \"Context Pack\"\ndate: "));
    assert!(stdout.contains("## File contents"));
    assert!(stdout.contains("func Run() bool"));
    assert!(!stdout.contains("# Context Pack"));
    assert!(!stdout.contains("### src/app.go"));
}

#[test]
fn native_pack_reads_config_preset_and_ctxignore_without_go_delegate() {
    let root = test_dir("pack-config-ignore");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::create_dir_all(root.join("dist")).unwrap();
    fs::write(root.join("ctx.toml"), "[pack]\npreset = \"llm\"\n").unwrap();
    fs::write(root.join(".ctxignore"), "dist/\n").unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() bool {\n    return true\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("dist/generated.go"),
        "package dist\n\nfunc Generated() {}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--no-contract",
            "--goal",
            "run generated",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack config ignore failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("=== src/app.go ==="));
    assert!(!stdout.contains("generated.go"));
    assert!(!stdout.contains("Context Pack"));
}
