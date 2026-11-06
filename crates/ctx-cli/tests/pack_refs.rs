//! `ctx pack --from-refs`: free text on stdin — a model reply, a stack
//! trace, a review comment — becomes a pack of the files and lines it
//! refers to, with unresolved mentions recorded as omitted.

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

fn json(out: Output) -> Value {
    assert!(
        out.status.success(),
        "ctx failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).expect("pack json")
}

fn fixture(name: &str) -> PathBuf {
    let root = test_dir(name);
    fs::create_dir_all(root.join("src/api")).unwrap();
    fs::create_dir_all(root.join("lib/api")).unwrap();
    let long: String = (1..=100).map(|i| format!("line {i}\n")).collect();
    fs::write(root.join("src/api/user.ts"), &long).unwrap();
    fs::write(root.join("src/store.rs"), &long).unwrap();
    fs::write(root.join("src/util.ts"), "export const a = 1;\n").unwrap();
    fs::write(root.join("lib/api/util.ts"), "export const b = 2;\n").unwrap();
    fs::write(root.join(".env"), "TOKEN=secret\n").unwrap();
    root
}

fn included(pack: &Value) -> Vec<(String, String)> {
    pack["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| {
            (
                f["path"].as_str().unwrap().to_string(),
                f["lines"].as_str().unwrap_or("").to_string(),
            )
        })
        .collect()
}

#[test]
fn packs_files_and_lines_named_in_a_stack_trace() {
    let root = fixture("pack_refs_stack");
    let trace = "TypeError: x is undefined\n    at load (/ci/build/src/api/user.ts:42:7)\nthread 'main' panicked at src/store.rs:90:5:\n";
    let pack = json(ctx(&root, &["pack", "--from-refs", "--json"], trace));
    let files = included(&pack);
    assert_eq!(files.len(), 2, "{files:?}");
    assert!(
        files.contains(&("src/api/user.ts".into(), "lines 32-52 of 100".into())),
        "{files:?}"
    );
    assert!(
        files.contains(&("src/store.rs".into(), "lines 80-100 of 100".into())),
        "{files:?}"
    );
    let reason = pack["included"][0]["reason"].as_str().unwrap();
    assert!(reason.starts_with("referenced as "), "{reason}");
}

#[test]
fn around_controls_context_and_bare_mentions_pack_whole_files() {
    let root = fixture("pack_refs_around");
    let reply = "Change `src/store.rs:10-12` (see src/store.rs#L20) and `src/util.ts`.";
    let pack = json(ctx(
        &root,
        &["pack", "--from-refs", "--around", "0", "--json"],
        reply,
    ));
    let files = included(&pack);
    assert!(
        files.contains(&("src/store.rs".into(), "lines 10-12, 20 of 100".into())),
        "{files:?}"
    );
    assert!(
        files.contains(&("src/util.ts".into(), String::new())),
        "{files:?}"
    );
}

#[test]
fn ambiguous_and_missing_mentions_are_omitted_and_secrets_never_resolve() {
    let root = fixture("pack_refs_unresolved");
    let reply = "See util.ts, src/gone.rs, .env and src/store.rs.";
    let out = ctx(&root, &["pack", "--from-refs", "--json"], reply);
    let stderr = String::from_utf8_lossy(&out.stderr).to_string();
    assert!(stderr.contains("util.ts"), "{stderr}");
    let pack = json(out);
    assert_eq!(
        included(&pack),
        vec![("src/store.rs".to_string(), String::new())]
    );
    let skipped: Vec<String> = pack["skipped"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| {
            format!(
                "{} {}",
                s["path"].as_str().unwrap(),
                s["reason"].as_str().unwrap()
            )
        })
        .collect();
    assert!(
        skipped.iter().any(|s| s.starts_with("util.ts ambiguous")),
        "{skipped:?}"
    );
    assert!(
        skipped.iter().any(|s| s.starts_with("src/gone.rs no file")),
        "{skipped:?}"
    );
    assert!(!skipped.iter().any(|s| s.contains(".env")), "{skipped:?}");
}

#[test]
fn input_without_references_is_an_error() {
    let root = fixture("pack_refs_none");
    let out = ctx(
        &root,
        &["pack", "--from-refs"],
        "no paths in here, e.g. v1.2",
    );
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("names no file"), "{stderr}");
}

#[test]
fn from_refs_rejects_other_selections() {
    let root = fixture("pack_refs_conflict");
    let out = ctx(
        &root,
        &["pack", "--from-refs", "--from-stdin"],
        "src/store.rs",
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("does not combine"));
}
