//! `ctx digest` must report the real path of every changed file: renames
//! count as a delete plus an add, and non-ASCII or quote-containing names
//! are neither C-quoted nor left with a zero token delta.

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;

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

fn digest_json(root: &Path) -> Value {
    let output = Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(["digest", "--since", "30d", "--format", "json"])
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("run ctx digest");
    assert!(
        output.status.success(),
        "ctx digest failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse digest JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    })
}

fn token_delta(payload: &Value, path: &str) -> Option<i64> {
    payload["hot_files"]
        .as_array()?
        .iter()
        .find(|item| item["path"] == path)
        .and_then(|item| item["token_delta"].as_i64())
}

#[test]
fn digest_reports_renamed_files_under_their_new_path() {
    let root = test_dir("digest-rename");
    fs::create_dir_all(root.join("src")).unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join("README.md"), "base\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    fs::write(
        root.join("src/a.ts"),
        "export function alpha() { return 1 }\n",
    )
    .unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "add a"]);
    fs::create_dir_all(root.join("src/deep")).unwrap();
    git(&root, &["mv", "src/a.ts", "src/deep/a.ts"]);
    git(&root, &["commit", "-qm", "move a"]);

    let payload = digest_json(&root);
    let new_delta = token_delta(&payload, "src/deep/a.ts")
        .unwrap_or_else(|| panic!("renamed path missing from digest: {payload}"));
    let old_delta = token_delta(&payload, "src/a.ts")
        .unwrap_or_else(|| panic!("old path missing from digest: {payload}"));
    assert!(new_delta > 0, "new path should gain tokens: {payload}");
    // `src/a.ts` was added and removed inside the period: net zero.
    assert_eq!(old_delta, 0, "{payload}");
    assert_eq!(payload["files_added"], 2, "{payload}");
    assert_eq!(payload["files_deleted"], 1, "{payload}");
}

#[test]
fn digest_keeps_non_ascii_and_quoted_paths_verbatim() {
    let root = test_dir("digest-quoted");
    fs::create_dir_all(&root).unwrap();
    git(&root, &["init", "-q"]);
    fs::write(root.join("日本語.txt"), "hello\n").unwrap();
    fs::write(root.join("quo\"te.txt"), "hello\n").unwrap();
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    fs::write(root.join("日本語.txt"), "hello\nmore words in this file\n").unwrap();
    fs::write(root.join("quo\"te.txt"), "hello\nmore words in this file\n").unwrap();
    git(&root, &["commit", "-qam", "grow"]);

    let payload = digest_json(&root);
    for path in ["日本語.txt", "quo\"te.txt"] {
        let delta = token_delta(&payload, path)
            .unwrap_or_else(|| panic!("{path:?} missing or escaped in digest: {payload}"));
        assert!(
            delta > 0,
            "{path:?} should have a positive delta: {payload}"
        );
    }
    assert_eq!(payload["files_changed"], 2, "{payload}");
}
