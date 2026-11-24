//! In-process checks for the Git-backed MCP tools: `ctx_pack` with
//! `changed` / `companions`, `ctx_digest` and `ctx_cochange`. They need a
//! `git` binary but no Go oracle.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ctx_mcp::{serve, ServeOptions};
use serde_json::{json, Value};

fn temp_repo(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ctx-mcp-git-{tag}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(dir.join("src")).unwrap();
    let dir = std::fs::canonicalize(dir).unwrap();
    git(&dir, &["init", "-q", "-b", "main"]);
    git(&dir, &["config", "user.email", "dev@example.com"]);
    git(&dir, &["config", "user.name", "Dev"]);
    dir
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

fn git_available() -> bool {
    Command::new("git").arg("--version").output().is_ok()
}

/// api.rs and its test change together three times, then api.rs is edited
/// again in the work tree (uncommitted).
fn api_repo(tag: &str) -> PathBuf {
    let root = temp_repo(tag);
    for i in 0..3 {
        std::fs::write(
            root.join("src/api.rs"),
            format!("pub fn api() -> u32 {{ {i} }}\n"),
        )
        .unwrap();
        std::fs::write(
            root.join("src/api_test.rs"),
            format!("fn check() {{ assert_eq!(api(), {i}); }}\n"),
        )
        .unwrap();
        git(&root, &["add", "-A"]);
        git(&root, &["commit", "-q", "-m", &format!("api {i}")]);
    }
    std::fs::write(root.join("README.md"), "# demo\n").unwrap();
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "readme"]);
    std::fs::write(root.join("src/api.rs"), "pub fn api() -> u32 { 42 }\n").unwrap();
    root
}

fn call_tool(root: &Path, name: &str, args: Value) -> Value {
    let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":name,"arguments":args}});
    let mut out = Vec::new();
    serve(
        Cursor::new(format!("{req}\n").into_bytes()),
        &mut out,
        ServeOptions {
            root: root.to_path_buf(),
            allow_outside_root: false,
        },
    )
    .expect("serve");
    serde_json::from_slice(&out).expect("one JSON reply")
}

fn text(reply: &Value) -> String {
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text content: {reply}"))
        .to_string()
}

#[test]
fn pack_changed_includes_only_work_tree_changes() {
    if !git_available() {
        return;
    }
    let root = api_repo("changed");
    let body = text(&call_tool(
        &root,
        "ctx_pack",
        json!({"path": ".", "changed": true, "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&body).unwrap();
    let included: Vec<&str> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(included, vec!["src/api.rs"], "{body}");
}

#[test]
fn pack_companions_adds_files_that_change_with_the_changed_ones() {
    if !git_available() {
        return;
    }
    let root = api_repo("companions");
    let body = text(&call_tool(
        &root,
        "ctx_pack",
        json!({"path": ".", "changed": true, "companions": 3, "explain": true}),
    ));
    assert!(body.contains("assert_eq!(api(), 2)"), "{body}");
    assert!(
        body.contains("companion: changed together with src/api.rs in 3 commit(s)"),
        "{body}"
    );
    assert!(
        !body.contains("# demo"),
        "README is not a companion: {body}"
    );

    let refused = call_tool(&root, "ctx_pack", json!({"path": ".", "companions": 3}));
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
}

#[test]
fn pack_base_packs_the_whole_branch_since_the_merge_base() {
    if !git_available() {
        return;
    }
    // main: api.rs, api_test.rs, README.md. feature: a committed new file,
    // a deleted README, plus the uncommitted api.rs edit from api_repo.
    let root = api_repo("base");
    git(&root, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(root.join("src/feature.rs"), "pub fn feature() {}\n").unwrap();
    std::fs::remove_file(root.join("README.md")).unwrap();
    git(&root, &["add", "src/feature.rs", "README.md"]);
    git(&root, &["commit", "-q", "-m", "feature"]);

    let body = text(&call_tool(
        &root,
        "ctx_pack",
        json!({"path": ".", "base": "main", "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&body).unwrap();
    let included: Vec<(&str, &str)> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap(),
                f["reason"].as_str().unwrap_or(""),
            )
        })
        .collect();
    let paths: Vec<&str> = included.iter().map(|(p, _)| *p).collect();
    assert!(paths.contains(&"src/api.rs"), "{body}");
    assert!(paths.contains(&"src/feature.rs"), "{body}");
    assert!(!paths.contains(&"src/api_test.rs"), "{body}");
    let feature = included
        .iter()
        .find(|(p, _)| *p == "src/feature.rs")
        .unwrap();
    assert!(
        feature.1.starts_with("changed vs main (+1 -0, committed)"),
        "{body}"
    );
    assert!(
        body.contains("deleted vs main"),
        "README deletion is reported: {body}"
    );

    let bad = call_tool(
        &root,
        "ctx_pack",
        json!({"path": ".", "base": "--output=x"}),
    );
    assert_eq!(bad["error"]["code"], -32602, "{bad}");
    let unknown = call_tool(&root, "ctx_pack", json!({"path": ".", "base": "nope"}));
    assert!(
        unknown.to_string().contains("cannot resolve base"),
        "{unknown}"
    );
}

#[test]
fn cochange_ranks_partners_and_pairs() {
    if !git_available() {
        return;
    }
    let root = api_repo("cochange");
    let body = text(&call_tool(
        &root,
        "ctx_cochange",
        json!({"anchor": "src/api.rs", "format": "json"}),
    ));
    let report: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(report["partners"][0]["path"], "src/api_test.rs", "{body}");
    assert_eq!(report["partners"][0]["weight"], 3);
    assert_eq!(report["partners"][0]["confidence"], 1.0);
    assert_eq!(report["commits_scanned"], 4);

    let pairs = text(&call_tool(&root, "ctx_cochange", json!({})));
    assert!(
        pairs.contains("| 1 | 3 | src/api.rs | src/api_test.rs |"),
        "{pairs}"
    );

    let escape = call_tool(&root, "ctx_cochange", json!({"anchor": "../x"}));
    assert_eq!(escape["error"]["code"], -32602, "{escape}");
}

#[test]
fn digest_reports_repository_activity() {
    if !git_available() {
        return;
    }
    let root = api_repo("digest");
    let body = text(&call_tool(
        &root,
        "ctx_digest",
        json!({"since": "30d", "format": "json"}),
    ));
    let digest: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(digest["commits"], 4, "{body}");
    assert_eq!(digest["authors"], 1);
    let hot: Vec<&str> = digest["hot_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(hot[0], "src/api.rs", "{body}");

    let markdown = text(&call_tool(
        &root,
        "ctx_digest",
        json!({"top": 1, "page_size": 1}),
    ));
    assert!(markdown.starts_with("# Digest"), "{markdown}");
    assert!(markdown.contains("## Suggested next reads"), "{markdown}");

    let not_repo = call_tool(&root, "ctx_digest", json!({"path": "src"}));
    assert_eq!(not_repo["result"]["isError"], true, "{not_repo}");
}

/// A crate where `src/api.rs` uses `src/store.rs` and `src/cli.rs` uses
/// `src/api.rs`; `src/api.rs` is then edited in the work tree.
fn import_repo(tag: &str) -> PathBuf {
    let root = temp_repo(tag);
    for (path, body) in [
        ("Cargo.toml", "[package]\nname = \"demo\"\n"),
        ("src/lib.rs", "pub mod api;\npub mod cli;\npub mod store;\n"),
        ("src/store.rs", "pub fn load() -> u32 { 1 }\n"),
        (
            "src/api.rs",
            "use crate::store::load;\npub fn api() -> u32 { load() }\n",
        ),
        ("src/cli.rs", "pub fn run() { crate::api::api(); }\n"),
        ("src/other.rs", "pub fn other() {}\n"),
    ] {
        std::fs::write(root.join(path), body).unwrap();
    }
    git(&root, &["add", "-A"]);
    git(&root, &["commit", "-q", "-m", "init"]);
    std::fs::write(
        root.join("src/api.rs"),
        "use crate::store::load;\npub fn api() -> u32 { load() + 1 }\n",
    )
    .unwrap();
    root
}

#[test]
fn deps_tool_follows_imports_both_ways_and_by_depth() {
    if !git_available() {
        return;
    }
    let root = import_repo("deps-tool");
    let body = text(&call_tool(
        &root,
        "ctx_deps",
        json!({"anchor": "src/api.rs", "format": "json"}),
    ));
    let found: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(found["files"][0]["path"], "src/store.rs", "{body}");
    assert_eq!(found["total"], 1, "{body}");

    let body = text(&call_tool(
        &root,
        "ctx_deps",
        json!({"anchor": "src/store.rs", "direction": "importers", "depth": 2}),
    ));
    assert!(
        body.contains("| 1 | src/api.rs | 1 | src/store.rs |"),
        "{body}"
    );
    assert!(
        body.contains("| 2 | src/cli.rs | 2 | src/api.rs |"),
        "{body}"
    );

    let refused = call_tool(&root, "ctx_deps", json!({"anchor": "../x.rs"}));
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
    let refused = call_tool(
        &root,
        "ctx_deps",
        json!({"anchor": "src/api.rs", "direction": "up"}),
    );
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
}

#[test]
fn pack_changed_with_deps_and_dependents_adds_the_import_neighbourhood() {
    if !git_available() {
        return;
    }
    let root = import_repo("pack-deps");
    let body = text(&call_tool(
        &root,
        "ctx_pack",
        json!({"path": ".", "changed": true, "deps": 5, "dependents": 5, "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&body).unwrap();
    let included: Vec<(&str, &str)> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap(),
                f["reason"].as_str().unwrap_or(""),
            )
        })
        .collect();
    let paths: Vec<&str> = included.iter().map(|(p, _)| *p).collect();
    assert_eq!(
        paths,
        ["src/api.rs", "src/store.rs", "src/cli.rs"],
        "{body}"
    );
    assert!(
        included[1].1.contains("dependency: imported by src/api.rs"),
        "{body}"
    );
    assert!(
        included[2].1.contains("dependent: imports src/api.rs"),
        "{body}"
    );

    let refused = call_tool(&root, "ctx_pack", json!({"path": ".", "deps": 3}));
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
}

#[test]
fn tests_tool_and_pack_tests_pair_changed_sources_with_their_tests() {
    if !git_available() {
        return;
    }
    // `api_repo` edits src/api.rs in the work tree; src/api_test.rs is
    // named after it.
    let root = api_repo("tests-tool");
    let body = text(&call_tool(
        &root,
        "ctx_tests",
        json!({"changed": true, "format": "json"}),
    ));
    let report: Value = serde_json::from_str(&body).unwrap();
    assert_eq!(report["tests"][0]["path"], "src/api_test.rs", "{body}");
    assert_eq!(report["tests"][0]["subject"], "src/api.rs", "{body}");

    let body = text(&call_tool(
        &root,
        "ctx_tests",
        json!({"anchor": "src/api.rs"}),
    ));
    assert!(
        body.contains("| 1 | src/api_test.rs | named after src/api.rs |"),
        "{body}"
    );

    let body = text(&call_tool(
        &root,
        "ctx_pack",
        json!({"path": ".", "changed": true, "tests": 5, "format": "json"}),
    ));
    let pack: Value = serde_json::from_str(&body).unwrap();
    let included: Vec<(&str, &str)> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap(),
                f["reason"].as_str().unwrap_or(""),
            )
        })
        .collect();
    assert_eq!(included[0].0, "src/api.rs", "{body}");
    assert_eq!(
        included[1],
        ("src/api_test.rs", "test: named after src/api.rs"),
        "{body}"
    );

    let refused = call_tool(&root, "ctx_tests", json!({}));
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
    let refused = call_tool(&root, "ctx_pack", json!({"tests": 3}));
    assert_eq!(refused["error"]["code"], -32602, "{refused}");
}
