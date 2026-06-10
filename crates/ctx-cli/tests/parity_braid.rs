use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_braid_dry_run_json_does_not_require_go_delegate() {
    let root = test_dir("braid-dry-run");
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
    fs::write(
        root.join("braid.toml"),
        r#"
schema_version = 1

[[strand]]
name = "run"
source = "where Run --limit 5"
share = 1.0
policy = "merge"
"#,
    )
    .unwrap();

    let output = run_rust_in_with_env(
        &root,
        &["braid", "--dry-run", "--format", "json", "--budget", "1000"],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "braid dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse braid JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(payload["file"], "braid.toml");
    assert_eq!(payload["dry_run"], true);
    assert_eq!(payload["strands"][0]["name"], "run");
    assert!(payload["files"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["path"] == "src/app.go")));
}

#[test]
fn native_braid_non_dry_run_produces_pack_body_without_go_delegate() {
    // Structural test: non-dry-run braid runs native (no Go delegate) and produces
    // an allocation report followed by a pack body with file contents.
    let root = test_dir("braid-nondryrun");
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
    fs::write(
        root.join("braid.toml"),
        r#"
schema_version = 1

[[strand]]
name = "run"
source = "where Run --limit 5"
share = 1.0
policy = "merge"
"#,
    )
    .unwrap();

    // Run without Go delegate to verify the non-dry-run path is fully native.
    let output = run_rust_in_with_env(
        &root,
        &[
            "braid",
            "--format",
            "markdown",
            "--budget",
            "10000",
            "--no-contract",
        ],
        &[("CTX_GO_BIN", "/definitely/missing/ctx-go")],
    );
    assert!(
        output.status.success(),
        "braid non-dry-run failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Allocation report header
    assert!(
        stdout.contains("# CTX-BRAID:"),
        "missing allocation report header\n{stdout}"
    );
    assert!(
        stdout.contains("mode=pack"),
        "mode should be 'pack' for non-dry-run\n{stdout}"
    );
    // Pack body
    assert!(
        stdout.contains("## File contents"),
        "missing pack body '## File contents'\n{stdout}"
    );
    assert!(
        stdout.contains("### src/app.go"),
        "missing file header in pack body\n{stdout}"
    );
    assert!(
        stdout.contains("func Run()"),
        "missing file content in pack body\n{stdout}"
    );
    // No "Dry run" section
    assert!(
        !stdout.contains("## Dry run"),
        "should not contain dry-run notice in non-dry-run mode\n{stdout}"
    );
}

#[test]
fn native_braid_non_dry_run_byte_parity_with_go() {
    // Byte-exact parity: Rust braid (non-dry-run) must produce output identical
    // to the Go binary. Both now use cl100k_base token counts via ctx_tokens,
    // and the pack body is rendered without timestamps (NoMetadata=true).
    let root = test_dir("braid-nondryrun-parity");
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
    fs::write(
        root.join("braid.toml"),
        r#"
schema_version = 1

[[strand]]
name = "run"
source = "where Run --limit 5"
share = 1.0
policy = "merge"
"#,
    )
    .unwrap();

    assert_delegated_parity_in(
        &root,
        &[
            "braid",
            "--format",
            "markdown",
            "--budget",
            "10000",
            "--no-contract",
        ],
    );
}
