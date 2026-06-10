use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_digest_json_reports_git_activity() {
    let root = test_dir("digest");
    fs::create_dir_all(root.join("src")).unwrap();
    run_git(&root, &["init"]);
    run_git(&root, &["config", "user.email", "dev@example.com"]);
    run_git(&root, &["config", "user.name", "Dev"]);
    fs::write(root.join("src/app.go"), "package main\n\nfunc main() {}\n").unwrap();
    run_git(&root, &["add", "."]);
    run_git(&root, &["commit", "-m", "initial"]);
    fs::write(
        root.join("src/app.go"),
        "package main\n\nfunc main() {}\nfunc Run() {}\n",
    )
    .unwrap();
    run_git(&root, &["add", "."]);
    run_git(&root, &["commit", "-m", "update app"]);

    let output = run_rust_in(&root, &["digest", "--since", "365d", "--format", "json"]);
    assert!(
        output.status.success(),
        "digest failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse digest JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert!(payload["commits"].as_u64().unwrap_or(0) >= 2);
    assert_eq!(payload["authors"], 1);
    assert!(payload["hot_files"]
        .as_array()
        .is_some_and(|items| items.iter().any(|item| item["path"] == "src/app.go")));
}

/// Byte-parity for `digest --format markdown` (default format).
#[test]
fn digest_parity_markdown() {
    let root = write_digest_fixture();
    // Use --since 730d to include both pinned commits (2024-06-01 and 2024-06-15).
    assert_delegated_parity_in(
        &root,
        &["digest", "--since", "730d", "--format", "markdown"],
    );
}

/// Byte-parity for `digest --format plain`.
#[test]
fn digest_parity_plain() {
    let root = write_digest_fixture();
    assert_delegated_parity_in(&root, &["digest", "--since", "730d", "--format", "plain"]);
}

/// Byte-parity for `digest --format json` — excludes `period.since` and
/// `period.until` timestamp fields (non-deterministic time-of-day, see above).
/// All other fields are byte-identical.
#[test]
fn digest_parity_json() {
    let root = write_digest_fixture();
    assert_digest_json_parity(&root, &["digest", "--since", "730d", "--format", "json"]);
}

/// Byte-parity for `digest` with no --format (defaults to markdown).
#[test]
fn digest_parity_default_format() {
    let root = write_digest_fixture();
    assert_delegated_parity_in(&root, &["digest", "--since", "730d"]);
}

/// Byte-parity for `digest --top 1` (limits hot files to top 1).
#[test]
fn digest_parity_top_flag() {
    let root = write_digest_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "digest", "--since", "730d", "--top", "1", "--format", "markdown",
        ],
    );
    assert_delegated_parity_in(
        &root,
        &[
            "digest", "--since", "730d", "--top", "1", "--format", "plain",
        ],
    );
    assert_digest_json_parity(
        &root,
        &[
            "digest", "--since", "730d", "--top", "1", "--format", "json",
        ],
    );
}

/// Byte-parity for `digest` on an empty window (no commits in period).
/// Tests the zero-commit path where both binaries output 0 across all fields.
#[test]
fn digest_parity_empty_window() {
    let root = write_digest_fixture();
    // --since 1h: the pinned commits are from 2024-06, way outside 1 hour
    assert_delegated_parity_in(&root, &["digest", "--since", "1h", "--format", "markdown"]);
    assert_delegated_parity_in(&root, &["digest", "--since", "1h", "--format", "plain"]);
    assert_digest_json_parity(&root, &["digest", "--since", "1h", "--format", "json"]);
}

/// Byte-parity for `digest --out FILE` (write output to a file instead of stdout).
/// Verifies exit code 0 and file content matches Go.
#[test]
fn digest_parity_out_file() {
    let root = write_digest_fixture();
    let go_out = test_dir("digest-out-go");
    let rust_out = test_dir("digest-out-rust");
    fs::create_dir_all(&go_out).unwrap();
    fs::create_dir_all(&rust_out).unwrap();
    let go_file = go_out.join("digest.md");
    let rust_file = rust_out.join("digest.md");

    let go = run_go_in(
        &root,
        &[
            "digest",
            "--since",
            "730d",
            "--format",
            "markdown",
            "--out",
            &go_file.to_string_lossy(),
        ],
    );
    let rust = run_rust_in(
        &root,
        &[
            "digest",
            "--since",
            "730d",
            "--format",
            "markdown",
            "--out",
            &rust_file.to_string_lossy(),
        ],
    );

    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "digest --out exit code mismatch"
    );
    // stdout should be empty (output goes to file)
    assert_eq!(
        rust.stdout, go.stdout,
        "digest --out stdout should be empty"
    );

    let go_content = fs::read(&go_file).unwrap_or_default();
    let rust_content = fs::read(&rust_file).unwrap_or_default();
    assert_eq!(
        rust_content,
        go_content,
        "digest --out file content mismatch\nGo:\n{}\nRust:\n{}",
        String::from_utf8_lossy(&go_content),
        String::from_utf8_lossy(&rust_content),
    );
}

/// Full flag matrix: format {markdown,plain,json} × since {730d,1h} × top {1,10}.
#[test]
fn digest_parity_full_flag_matrix() {
    let root = write_digest_fixture();

    // markdown and plain: byte-exact
    for format in &["markdown", "plain"] {
        for since in &["730d", "1h"] {
            for top in &["1", "10"] {
                assert_delegated_parity_in(
                    &root,
                    &["digest", "--since", since, "--top", top, "--format", format],
                );
            }
        }
    }

    // json: exclude timestamp fields
    for since in &["730d", "1h"] {
        for top in &["1", "10"] {
            assert_digest_json_parity(
                &root,
                &["digest", "--since", since, "--top", top, "--format", "json"],
            );
        }
    }
}
