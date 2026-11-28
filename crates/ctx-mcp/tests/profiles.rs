//! `ctx_profiles` and `ctx_pack {"profile": …}`: named selection strategies
//! from the repository's ctx.toml.

use std::io::Cursor;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

use ctx_mcp::{serve, ServeOptions};
use serde_json::{json, Value};

fn temp(tag: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!(
        "ctx-mcp-profile-{tag}-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::canonicalize(dir).unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(["-c", "user.email=t@example.com", "-c", "user.name=t"])
        .args(args)
        .current_dir(root)
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?}");
}

const PROFILES: &str = r#"
[profiles.wip]
description = "Uncommitted work with its tests"
changed = true
tests = 5
budget = 7000
goal = "store load"

[profiles.docs]
include = ["docs/**"]
format = "plain"
"#;

/// A repository with a committed source, its test, a doc, and an
/// uncommitted edit to the source.
fn repo(tag: &str) -> PathBuf {
    let root = temp(tag);
    std::fs::create_dir_all(root.join("src")).unwrap();
    std::fs::create_dir_all(root.join("docs")).unwrap();
    std::fs::write(root.join("src/store.py"), "def load():\n    return 1\n").unwrap();
    std::fs::write(
        root.join("src/test_store.py"),
        "from store import load\n\ndef test_load():\n    assert load() == 1\n",
    )
    .unwrap();
    std::fs::write(root.join("src/other.py"), "def other():\n    pass\n").unwrap();
    std::fs::write(root.join("docs/guide.md"), "# Guide\n").unwrap();
    std::fs::write(root.join("ctx.toml"), PROFILES).unwrap();
    git(&root, &["init", "-q"]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "init"]);
    std::fs::write(root.join("src/store.py"), "def load():\n    return 2\n").unwrap();
    root
}

fn call(root: &Path, tool: &str, args: Value) -> Value {
    let req = json!({"jsonrpc":"2.0","id":1,"method":"tools/call",
        "params":{"name":tool,"arguments":args}});
    let mut out = Vec::new();
    serve(
        Cursor::new(format!("{req}\n").into_bytes()),
        &mut out,
        ServeOptions {
            root: root.to_path_buf(),
            allow_outside_root: false,
        },
    )
    .unwrap();
    serde_json::from_slice(&out).unwrap()
}

fn text(reply: &Value) -> String {
    reply["result"]["content"][0]["text"]
        .as_str()
        .unwrap_or_else(|| panic!("no text: {reply}"))
        .to_string()
}

#[test]
fn ctx_profiles_lists_each_profile_with_how_to_call_it() {
    let root = repo("list");
    let listed = text(&call(&root, "ctx_profiles", json!({})));
    assert!(listed.starts_with("2 pack profile(s):"), "{listed}");
    assert!(
        listed.contains("- wip: Uncommitted work with its tests"),
        "{listed}"
    );
    assert!(listed.contains("changed=true"), "{listed}");
    assert!(listed.contains("tests=5"), "{listed}");
    assert!(
        listed.contains(r#"use: ctx_pack {"profile":"wip"}"#),
        "{listed}"
    );

    let empty = temp("empty");
    let none = text(&call(&empty, "ctx_profiles", json!({})));
    assert!(none.starts_with("No pack profiles"), "{none}");
}

#[test]
fn ctx_pack_profile_applies_selection_budget_and_goal() {
    let root = repo("apply");
    let packed = text(&call(
        &root,
        "ctx_pack",
        json!({"path": ".", "profile": "wip", "explain": true}),
    ));
    assert!(packed.contains("src/store.py"), "{packed}");
    assert!(
        packed.contains("src/test_store.py"),
        "tests=5 adds the test: {packed}"
    );
    assert!(!packed.contains("src/other.py"), "changed=true: {packed}");
    assert!(packed.contains("store load"), "goal from profile: {packed}");
    assert!(packed.contains("7000"), "budget from profile: {packed}");
}

#[test]
fn explicit_arguments_override_the_profile() {
    let root = repo("override");
    let packed = text(&call(
        &root,
        "ctx_pack",
        json!({"path": ".", "profile": "wip", "budget": 9000, "goal": "store mine", "tests": 0}),
    ));
    assert!(packed.contains("9000"), "{packed}");
    assert!(packed.contains("store mine"), "{packed}");
    // tests: 0 means "unset" over JSON, so the profile's value still applies.
    assert!(packed.contains("src/test_store.py"), "{packed}");

    let docs = text(&call(
        &root,
        "ctx_pack",
        json!({"path": ".", "profile": "docs"}),
    ));
    assert!(docs.contains("docs/guide.md"), "{docs}");
    assert!(
        !docs.contains("src/store.py"),
        "include from profile: {docs}"
    );
}

#[test]
fn unknown_or_conflicting_profiles_are_reported() {
    let root = repo("errors");
    let reply = call(&root, "ctx_pack", json!({"path": ".", "profile": "nope"}));
    let err = &reply["error"];
    assert_eq!(err["code"], -32602, "{reply}");
    assert!(
        err["message"]
            .as_str()
            .unwrap()
            .contains("unknown profile \"nope\" (defined: docs, wip)"),
        "{reply}"
    );
    assert!(err["data"]["hint"]
        .as_str()
        .unwrap()
        .contains("ctx_profiles"));

    // wip selects changed files; an explicit file list conflicts with it.
    let reply = call(
        &root,
        "ctx_pack",
        json!({"path": ".", "profile": "wip", "files": ["src/other.py"]}),
    );
    assert!(reply.get("error").is_some(), "{reply}");

    std::fs::write(root.join("ctx.toml"), "[profiles.bad]\nbudget = 0\n").unwrap();
    let reply = call(&root, "ctx_profiles", json!({}));
    assert!(reply.get("error").is_some(), "{reply}");
    assert!(
        reply["error"]["message"]
            .as_str()
            .unwrap()
            .contains("budget must be between"),
        "{reply}"
    );
}
