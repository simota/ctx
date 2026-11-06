//! `ctx mix` builds recipes that `ctx pack --from-mix` packs.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

use serde_json::Value;

fn test_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("ctx-mix-cli-{name}-{}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

fn ctx(root: &Path, state: &Path, args: &[&str], stdin: Option<&str>) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(args)
        .current_dir(root)
        .env("XDG_STATE_HOME", state)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    {
        let mut pipe = child.stdin.take().unwrap();
        if let Some(text) = stdin {
            pipe.write_all(text.as_bytes()).unwrap();
        }
    }
    child.wait_with_output().unwrap()
}

fn ok(out: Output) -> String {
    assert!(
        out.status.success(),
        "ctx failed: {}\n{}",
        String::from_utf8_lossy(&out.stderr),
        String::from_utf8_lossy(&out.stdout)
    );
    String::from_utf8(out.stdout).unwrap()
}

fn fixture(name: &str) -> (PathBuf, PathBuf) {
    let root = test_dir(name);
    let state = test_dir(&format!("{name}-state"));
    fs::create_dir_all(root.join("src/auth")).unwrap();
    fs::write(root.join("src/auth/login.rs"), "pub fn login() {}\n").unwrap();
    fs::write(root.join("src/auth/token.rs"), "pub fn refresh() {}\n").unwrap();
    fs::write(root.join("src/main.rs"), "fn main() {}\n").unwrap();
    fs::write(root.join("README.md"), "# demo\n").unwrap();
    (root, state)
}

fn packed_paths(json: &str) -> Vec<String> {
    let payload: Value = serde_json::from_str(json).unwrap();
    payload["included"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["path"].as_str().unwrap().to_string())
        .collect()
}

#[test]
fn saved_mix_packs_its_files_even_when_the_goal_does_not_match_them() {
    let (root, state) = fixture("save-pack");
    let out = ok(ctx(
        &root,
        &state,
        &[
            "mix",
            "save",
            "Auth Review",
            "src/auth",
            "README.md",
            "--goal",
            "unrelated words",
            "--budget",
            "9000",
        ],
        None,
    ));
    assert!(out.contains("Saved mix auth-review (3 files)"), "{out}");

    let shown: Value = serde_json::from_str(&ok(ctx(
        &root,
        &state,
        &["mix", "show", "auth review", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(
        shown["files"],
        serde_json::json!(["src/auth/login.rs", "src/auth/token.rs", "README.md"])
    );
    assert_eq!(shown["budget"]["limit"], 9000);

    let json = ok(ctx(
        &root,
        &state,
        &[
            "pack",
            "--from-mix",
            "Auth Review",
            "--format",
            "json",
            "--no-contract",
        ],
        None,
    ));
    let payload: Value = serde_json::from_str(&json).unwrap();
    assert_eq!(payload["goal"], "unrelated words");
    assert_eq!(payload["budget"], 9000);
    let mut paths = packed_paths(&json);
    paths.sort();
    assert_eq!(
        paths,
        vec!["README.md", "src/auth/login.rs", "src/auth/token.rs"]
    );
}

#[test]
fn add_drop_set_and_rm_edit_the_recipe() {
    let (root, state) = fixture("edit");
    ok(ctx(
        &root,
        &state,
        &["mix", "save", "work", "src/main.rs"],
        None,
    ));
    ok(ctx(
        &root,
        &state,
        &["mix", "add", "work", "src/auth"],
        None,
    ));
    ok(ctx(
        &root,
        &state,
        &["mix", "drop", "work", "src/auth/token.rs"],
        None,
    ));
    ok(ctx(
        &root,
        &state,
        &[
            "mix",
            "set",
            "work",
            "--goal",
            "login flow",
            "--budget",
            "500",
        ],
        None,
    ));
    let paths = ok(ctx(
        &root,
        &state,
        &["mix", "show", "work", "--paths"],
        None,
    ));
    assert_eq!(paths, "src/main.rs\nsrc/auth/login.rs\n");
    let shown: Value = serde_json::from_str(&ok(ctx(
        &root,
        &state,
        &["mix", "show", "work", "--json"],
        None,
    )))
    .unwrap();
    assert_eq!(shown["goal"], "login flow");
    assert!(shown["updated"].as_str().is_some_and(|s| !s.is_empty()));
    // Dropping the last file is refused rather than leaving an empty recipe.
    let out = ctx(&root, &state, &["mix", "drop", "work", "src"], None);
    assert!(!out.status.success());
    ok(ctx(&root, &state, &["mix", "rm", "work"], None));
    let listed = ok(ctx(&root, &state, &["mix", "list", "--json"], None));
    assert_eq!(listed.trim(), "[]");
}

#[test]
fn save_reads_where_json_and_rejects_paths_outside_the_root() {
    let (root, state) = fixture("stdin");
    let where_json = r#"[{"path":"src/auth/login.rs","score":10},{"path":"../outside.rs","score":5},{"path":"gone.rs","score":1}]"#;
    let out = ctx(
        &root,
        &state,
        &["mix", "save", "from where", "--from-where"],
        Some(where_json),
    );
    let stdout = ok(out);
    assert!(stdout.contains("(1 files)"), "{stdout}");

    let outside = test_dir("stdin-outside");
    fs::write(outside.join("x.rs"), "x").unwrap();
    let out = ctx(
        &root,
        &state,
        &["mix", "save", "bad", outside.join("x.rs").to_str().unwrap()],
        None,
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("outside the repository root"));
    let out = ctx(&root, &state, &["mix", "save", "bad", "missing.rs"], None);
    assert!(!out.status.success());
}

#[test]
fn pack_from_mix_warns_about_files_missing_from_the_tree() {
    let (root, state) = fixture("missing");
    ok(ctx(
        &root,
        &state,
        &["mix", "save", "m", "src/main.rs", "README.md"],
        None,
    ));
    fs::remove_file(root.join("README.md")).unwrap();
    let out = ctx(
        &root,
        &state,
        &[
            "pack",
            "--from-mix",
            "m",
            "--format",
            "json",
            "--no-contract",
        ],
        None,
    );
    let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
    assert_eq!(packed_paths(&ok(out)), vec!["src/main.rs"]);
    assert!(stderr.contains("lists README.md"), "{stderr}");
}

#[test]
fn save_from_refs_records_the_files_and_lines_a_reply_mentions() {
    let (root, state) = fixture("refs");
    let reply =
        "The refresh lives in `token.rs`; the entry point is src/main.rs:1.\nlogin.rs is fine.";
    let stdout = ok(ctx(
        &root,
        &state,
        &["mix", "save", "reply", "--from-refs", "--json"],
        Some(reply),
    ));
    let mix: Value = serde_json::from_str(&stdout).unwrap();
    let files: Vec<&str> = mix["files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|f| f.as_str().unwrap())
        .collect();
    assert_eq!(
        files,
        vec!["src/auth/token.rs", "src/main.rs:1-11", "src/auth/login.rs"]
    );
    let out = ctx(
        &root,
        &state,
        &["mix", "add", "reply", "--from-refs", "--from-stdin"],
        Some("x"),
    );
    assert!(!out.status.success());
}
