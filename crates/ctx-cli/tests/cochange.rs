//! `ctx cochange`, `ctx pack --companions` and `ctx digest ROOT`: features
//! built on git co-change history. No Go oracle needed.

use serde_json::Value;
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

fn ctx(dir: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args(args)
        .current_dir(dir)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1");
    if let Some(input) = stdin {
        use std::io::Write;
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = cmd.spawn().expect("spawn ctx");
        child
            .stdin
            .take()
            .unwrap()
            .write_all(input.as_bytes())
            .unwrap();
        return child.wait_with_output().expect("wait ctx");
    }
    cmd.output().expect("run ctx")
}

fn ok_stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "ctx failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// `app/src/api.rs` changes with its test three times and with the schema
/// twice; a README commit stands alone. `api.rs` is then edited again
/// without committing.
fn fixture(tag: &str) -> std::path::PathBuf {
    let root = test_dir(tag);
    fs::create_dir_all(root.join("app/src")).unwrap();
    git(&root, &["init", "-q", "-b", "main"]);
    for i in 0..3 {
        fs::write(
            root.join("app/src/api.rs"),
            format!("pub fn api() -> u32 {{ {i} }}\n"),
        )
        .unwrap();
        fs::write(
            root.join("app/src/api_test.rs"),
            format!("fn check() {{ assert_eq!(api(), {i}); }}\n"),
        )
        .unwrap();
        if i < 2 {
            fs::write(root.join("app/schema.json"), format!("{{\"v\": {i}}}\n")).unwrap();
        }
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", &format!("api {i}")]);
    }
    fs::write(root.join("README.md"), "# demo\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "readme"]);
    fs::write(root.join("app/src/api.rs"), "pub fn api() -> u32 { 42 }\n").unwrap();
    root
}

#[test]
fn cochange_lists_partners_relative_to_the_repository() {
    let root = fixture("cochange-partners");
    // Run from a subdirectory: the anchor resolves against the cwd.
    let out = ok_stdout(&ctx(
        &root.join("app"),
        &["cochange", "src/api.rs", "--json"],
        None,
    ));
    let report: Value = serde_json::from_str(&out).unwrap();
    let partners: Vec<(&str, u64)> = report["partners"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| (p["path"].as_str().unwrap(), p["weight"].as_u64().unwrap()))
        .collect();
    assert_eq!(
        partners,
        vec![("app/src/api_test.rs", 3), ("app/schema.json", 2)],
        "{out}"
    );
    assert_eq!(report["anchors"][0]["path"], "app/src/api.rs");
    assert_eq!(report["commits_scanned"], 4);

    let paths = ok_stdout(&ctx(
        &root,
        &[
            "cochange",
            "app/src/api.rs",
            "--format",
            "paths",
            "--top",
            "1",
        ],
        None,
    ));
    assert_eq!(paths, "app/src/api_test.rs\n");

    let text = ok_stdout(&ctx(&root, &["cochange", "app/src/api.rs"], None));
    assert!(
        text.contains("Co-change partners of app/src/api.rs"),
        "{text}"
    );
    assert!(text.contains("100%  "), "{text}");

    let none = ok_stdout(&ctx(&root, &["cochange", "README.md"], None));
    assert!(
        none.starts_with("No co-change history for README.md"),
        "{none}"
    );
}

#[test]
fn cochange_without_paths_ranks_coupled_pairs() {
    let root = fixture("cochange-pairs");
    let out = ok_stdout(&ctx(&root, &["cochange", "--plain"], None));
    assert!(
        out.contains("1. app/src/api.rs and app/src/api_test.rs: changed together 3 times."),
        "{out}"
    );
    let bad = ctx(&root, &["cochange", "--format", "yaml"], None);
    assert!(!bad.status.success());
}

#[test]
fn pack_changed_with_companions_adds_co_changed_files_after_the_changes() {
    let root = fixture("pack-companions");
    let out = ok_stdout(&ctx(
        &root,
        &[
            "pack",
            "--changed",
            "--companions",
            "--no-contract",
            "--json",
        ],
        None,
    ));
    let pack: Value = serde_json::from_str(&out).unwrap();
    let included: Vec<(&str, &str)> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["path"].as_str().unwrap(), f["reason"].as_str().unwrap()))
        .collect();
    assert_eq!(included.len(), 3, "{out}");
    assert_eq!(included[0].0, "app/src/api.rs");
    assert_eq!(included[1].0, "app/src/api_test.rs");
    assert!(
        included[1]
            .1
            .starts_with("companion: changed together with app/src/api.rs in 3 commit(s)"),
        "{out}"
    );
    assert_eq!(included[2].0, "app/schema.json");

    // `--companions=N` caps the additions.
    let one = ok_stdout(&ctx(
        &root,
        &[
            "pack",
            "--changed",
            "--companions=1",
            "--no-contract",
            "--json",
        ],
        None,
    ));
    let pack: Value = serde_json::from_str(&one).unwrap();
    assert_eq!(pack["included"].as_array().unwrap().len(), 2, "{one}");

    // A pack rooted in a subdirectory names paths from it.
    let sub = ok_stdout(&ctx(
        &root.join("app"),
        &[
            "pack",
            ".",
            "--changed",
            "--companions",
            "--no-contract",
            "--json",
        ],
        None,
    ));
    assert!(sub.contains("\"path\": \"src/api_test.rs\""), "{sub}");

    // Without a selection there is nothing to find companions for.
    let refused = ctx(&root, &["pack", "--companions"], None);
    assert!(!refused.status.success());
    assert!(String::from_utf8_lossy(&refused.stderr).contains("needs a file selection"));
}

#[test]
fn pack_from_where_with_companions_appends_partners() {
    let root = fixture("pack-where-companions");
    let out = ok_stdout(&ctx(
        &root,
        &[
            "pack",
            "--from-where",
            "--companions=1",
            "--no-contract",
            "--json",
        ],
        Some(r#"[{"path":"app/src/api.rs","score":3}]"#),
    ));
    let pack: Value = serde_json::from_str(&out).unwrap();
    let included: Vec<&str> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(
        included,
        vec!["app/src/api.rs", "app/src/api_test.rs"],
        "{out}"
    );
}

#[test]
fn digest_accepts_a_repository_directory() {
    let root = fixture("digest-root");
    let elsewhere = test_dir("digest-cwd");
    fs::create_dir_all(&elsewhere).unwrap();
    let out = ok_stdout(&ctx(
        &elsewhere,
        &["digest", root.to_str().unwrap(), "--since", "30d", "--json"],
        None,
    ));
    let digest: Value = serde_json::from_str(&out).unwrap();
    assert_eq!(digest["commits"], 4, "{out}");

    // A subdirectory scopes the digest to it, with paths relative to it.
    let scoped = ok_stdout(&ctx(
        &elsewhere,
        &[
            "digest",
            root.join("app/src").to_str().unwrap(),
            "--since",
            "30d",
            "--json",
        ],
        None,
    ));
    let digest: Value = serde_json::from_str(&scoped).unwrap();
    assert_eq!(digest["commits"], 3, "{scoped}");
    let hot: Vec<&str> = digest["hot_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(hot, vec!["api.rs", "api_test.rs"], "{scoped}");
}
