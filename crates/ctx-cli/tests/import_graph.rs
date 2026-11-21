//! The import graph on the CLI: `ctx deps` / `ctx impact` with `--depth`,
//! directory anchors and `--format paths`, Rust import resolution, and
//! `ctx pack --deps` / `--dependents` adding the selection's neighbourhood.

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;
use common::test_dir;

fn ctx(root: &Path, args: &[&str], stdin: &str) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(args)
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("spawn ctx");
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stdin.as_bytes())
        .unwrap();
    child.wait_with_output().expect("wait ctx")
}

fn stdout(out: &Output) -> String {
    assert!(
        out.status.success(),
        "ctx failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).into_owned()
}

/// A two-crate Rust workspace:
///   app/src/main.rs  → app/src/cmd.rs → core::store (core/src/store.rs)
///   core/src/lib.rs  declares store; core/src/store.rs uses crate::codec.
fn workspace(name: &str) -> PathBuf {
    let root = test_dir(name);
    for (path, body) in [
        ("Cargo.toml", "[workspace]\nmembers = [\"app\", \"core\"]\n"),
        ("app/Cargo.toml", "[package]\nname = \"app\"\n"),
        ("app/src/main.rs", "mod cmd;\nfn main() { cmd::run(); }\n"),
        (
            "app/src/cmd.rs",
            "use my_core::store::Store;\npub fn run() { Store::open(); }\n",
        ),
        ("core/Cargo.toml", "[package]\nname = \"my-core\"\n"),
        ("core/src/lib.rs", "pub mod codec;\npub mod store;\n"),
        (
            "core/src/store.rs",
            "use crate::codec::decode;\npub struct Store;\n",
        ),
        ("core/src/codec.rs", "pub fn decode() {}\n"),
        ("core/tests/it.rs", "use my_core::store::Store;\n"),
        ("README.md", "# demo\n"),
    ] {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    root
}

#[test]
fn deps_resolves_rust_imports_across_workspace_crates() {
    let root = workspace("graph-deps");
    let text = stdout(&ctx(&root, &["deps", "app/src/cmd.rs"], ""));
    assert_eq!(
        text,
        "Dependencies for app/src/cmd.rs:\n  - core/src/store.rs\n"
    );
    let text = stdout(&ctx(&root, &["impact", "core/src/store.rs"], ""));
    assert_eq!(
        text,
        "Dependents for core/src/store.rs:\n  - app/src/cmd.rs\n  - core/tests/it.rs\n"
    );
}

#[test]
fn depth_follows_transitive_dependents_with_their_route() {
    let root = workspace("graph-depth");
    let text = stdout(&ctx(
        &root,
        &["impact", "core/src/codec.rs", "--depth", "3"],
        "",
    ));
    assert_eq!(
        text,
        "Dependents for core/src/codec.rs (within 3 hops):\n\
         \x20 - core/src/store.rs\n\
         \x20 - app/src/cmd.rs (2 hops, via core/src/store.rs)\n\
         \x20 - core/tests/it.rs (2 hops, via core/src/store.rs)\n\
         \x20 - app/src/main.rs (3 hops, via app/src/cmd.rs)\n"
    );
    let out = ctx(
        &root,
        &["impact", "core/src/codec.rs", "--depth=2", "--json"],
        "",
    );
    let list: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(list["depth"], 2);
    assert_eq!(list["details"][1]["via"], "core/src/store.rs");
    assert_eq!(list["details"][1]["depth"], 2);
}

#[test]
fn directory_anchor_answers_for_the_whole_directory() {
    let root = workspace("graph-dir");
    // What uses anything in core/ from outside it; core's own tests are
    // part of the unit.
    let text = stdout(&ctx(&root, &["impact", "core", "--format", "paths"], ""));
    assert_eq!(text, "app/src/cmd.rs\n");
    // Multiple anchors are one unit too.
    let text = stdout(&ctx(
        &root,
        &[
            "deps",
            "app/src/main.rs",
            "app/src/cmd.rs",
            "--format=paths",
        ],
        "",
    ));
    assert_eq!(text, "core/src/store.rs\n");
}

#[test]
fn invalid_relation_arguments_are_reported() {
    let root = workspace("graph-errors");
    let out = ctx(&root, &["deps", "nope.rs"], "");
    assert_eq!(out.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&out.stderr).contains("nope.rs"));
    let out = ctx(&root, &["deps", "app/src/cmd.rs", "--depth", "0"], "");
    assert!(String::from_utf8_lossy(&out.stderr).contains("--depth must be at least 1"));
    let out = ctx(&root, &["deps", "app/src/cmd.rs", "--format", "xml"], "");
    assert!(String::from_utf8_lossy(&out.stderr).contains("paths"));
}

#[test]
fn pack_deps_and_dependents_add_the_import_neighbourhood() {
    let root = workspace("graph-pack");
    let out = ctx(
        &root,
        &[
            "pack",
            "--from-stdin",
            "--deps",
            "--dependents=1",
            "--json",
            "--no-contract",
        ],
        "core/src/store.rs\n",
    );
    let pack: Value = serde_json::from_str(&stdout(&out)).unwrap();
    let files: Vec<(&str, &str)> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| (f["path"].as_str().unwrap(), f["reason"].as_str().unwrap()))
        .collect();
    assert_eq!(files.len(), 3, "{files:?}");
    assert_eq!(files[0].0, "core/src/store.rs");
    assert_eq!(
        files[1],
        (
            "core/src/codec.rs",
            "dependency: imported by core/src/store.rs"
        )
    );
    // --dependents=1 keeps one of the two importers.
    assert_eq!(files[2].0, "app/src/cmd.rs");
    assert_eq!(files[2].1, "dependent: imports core/src/store.rs");
}

#[test]
fn pack_deps_works_with_from_where_and_needs_a_selection() {
    let root = workspace("graph-pack-where");
    let where_json = r#"[{"path":"app/src/cmd.rs","score":5}]"#;
    let out = ctx(
        &root,
        &["pack", "--from-where", "--deps", "--json", "--no-contract"],
        where_json,
    );
    let pack: Value = serde_json::from_str(&stdout(&out)).unwrap();
    let paths: Vec<&str> = pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f["path"].as_str().unwrap())
        .collect();
    assert_eq!(paths, ["app/src/cmd.rs", "core/src/store.rs"]);

    let out = ctx(&root, &["pack", "--deps"], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--deps needs a file selection"));

    // No import edges: a warning, and the selection is packed alone.
    let out = ctx(
        &root,
        &["pack", "--from-stdin", "--dependents", "--json"],
        "README.md\n",
    );
    assert!(String::from_utf8_lossy(&out.stderr).contains("--dependents: no file here imports"));
    let pack: Value = serde_json::from_str(&stdout(&out)).unwrap();
    assert_eq!(pack["included"].as_array().unwrap().len(), 1);
}
