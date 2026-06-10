use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_pack_snapshot_shared_saves_manifest_without_go_delegate() {
    let root = test_dir("pack-snapshot-shared");
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
            "--snapshot",
            "snap1",
            "--shared",
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
        "pack snapshot failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Saved snapshot snap1 ->"));

    let manifest_path = root.join(".ctx/replay/snap1.json");
    let manifest: Value = serde_json::from_slice(
        &fs::read(&manifest_path)
            .unwrap_or_else(|err| panic!("read {}: {err}", manifest_path.display())),
    )
    .unwrap_or_else(|err| panic!("parse {}: {err}", manifest_path.display()));
    assert_eq!(manifest["schema_version"], 1);
    assert_eq!(manifest["id"], "snap1");
    assert_eq!(manifest["goal"], "login auth");
    assert_eq!(manifest["budget"], 1000);
    assert_eq!(manifest["format"], "json");
    assert_eq!(manifest["entries"][0]["path"], "src/auth/login.go");
    assert!(manifest["entries"][0]["sha256"]
        .as_str()
        .is_some_and(|value| value.len() == 64));
}

#[test]
fn native_pack_since_snapshot_shared_narrows_json_without_go_delegate() {
    let root = test_dir("pack-since-snapshot-shared");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() string {\n    return \"old\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/keep.go"),
        "package src\n\nfunc Keep() string {\n    return \"same\"\n}\n",
    )
    .unwrap();

    let baseline = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--snapshot",
            "base",
            "--shared",
            "--no-contract",
            "--format",
            "json",
            "--goal",
            "app keep",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        baseline.status.success(),
        "pack baseline failed: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );

    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() string {\n    return \"new\"\n}\n",
    )
    .unwrap();
    fs::write(
        root.join("src/new.go"),
        "package src\n\nfunc NewThing() string {\n    return \"added\"\n}\n",
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--since-snapshot",
            "base",
            "--shared",
            "--no-contract",
            "--format",
            "json",
            "--goal",
            "app keep new",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack since-snapshot failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout)
        .unwrap_or_else(|err| panic!("parse pack output: {err}"));
    assert_eq!(payload["replay"]["base"], "base");
    assert!(payload["replay"]["added"].as_i64().unwrap_or_default() >= 1);
    assert!(payload["replay"]["modified"].as_i64().unwrap_or_default() >= 1);

    let included: Vec<String> = payload["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect();
    assert!(included.iter().any(|path| path == "src/app.go"));
    assert!(included.iter().any(|path| path == "src/new.go"));
    assert!(!included.iter().any(|path| path == "src/keep.go"));
}

#[test]
fn native_pack_since_snapshot_xml_emits_replay_header_without_go_delegate() {
    let root = test_dir("pack-since-snapshot-xml");
    fs::create_dir_all(root.join("src")).unwrap();
    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() string { return \"old\" }\n",
    )
    .unwrap();

    let baseline = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--snapshot",
            "base",
            "--shared",
            "--no-contract",
            "--format",
            "xml",
            "--goal",
            "run",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        baseline.status.success(),
        "pack xml baseline failed: {}",
        String::from_utf8_lossy(&baseline.stderr)
    );

    fs::write(
        root.join("src/app.go"),
        "package src\n\nfunc Run() string { return \"new\" }\n",
    )
    .unwrap();
    let output = run_rust_in_with_env(
        &root,
        &[
            "pack",
            "--since-snapshot",
            "base",
            "--shared",
            "--no-contract",
            "--format",
            "xml",
            "--goal",
            "run",
            "--budget",
            "1000",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "pack since-snapshot xml failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.starts_with("<!-- base=base added=0 modified=1 removed=0 token-delta="));
    assert!(stdout.contains(r#"<context-pack goal="run""#));
}
