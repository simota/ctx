//! `ctx tests` and `ctx pack --tests`: pairing source files with the tests
//! that exercise them, by naming convention and by imports.

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

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .status()
        .expect("run git");
    assert!(status.success(), "git {args:?} failed");
}

/// A polyglot repository with tests in each language's usual place.
fn polyglot(name: &str) -> PathBuf {
    let root = test_dir(name);
    for (path, body) in [
        ("go/store.go", "package store\nfunc Open() {}\n"),
        (
            "go/store_test.go",
            "package store\nfunc TestOpen(t *testing.T) {}\n",
        ),
        ("go/other.go", "package store\n"),
        ("app/db.py", "def connect():\n    pass\n"),
        ("tests/test_db.py", "from app.db import connect\n"),
        ("web/src/session.ts", "export const ttl = 1;\n"),
        (
            "web/src/__tests__/session.test.ts",
            "import { ttl } from '../session';\n",
        ),
        ("Cargo.toml", "[workspace]\nmembers = [\"core\"]\n"),
        ("core/Cargo.toml", "[package]\nname = \"my-core\"\n"),
        ("core/src/lib.rs", "pub mod codec;\n"),
        ("core/src/codec.rs", "pub fn decode() {}\n"),
        ("core/tests/roundtrip.rs", "use my_core::codec::decode;\n"),
        ("core/tests/fixtures/codec.rs", "// data, not a test\n"),
        ("README.md", "# demo\n"),
    ] {
        let full = root.join(path);
        fs::create_dir_all(full.parent().unwrap()).unwrap();
        fs::write(full, body).unwrap();
    }
    root
}

#[test]
fn tests_lists_tests_by_name_and_by_import() {
    let root = polyglot("tests-cmd-text");
    let text = stdout(&ctx(
        &root,
        &[
            "tests",
            "go/store.go",
            "app/db.py",
            "web/src/session.ts",
            "core/src/codec.rs",
            "go/other.go",
        ],
        "",
    ));
    assert!(text.starts_with("Tests for 5 source file(s):\n"), "{text}");
    for (test, why) in [
        ("go/store_test.go", "named after go/store.go"),
        ("tests/test_db.py", "named after app/db.py"),
        (
            "web/src/__tests__/session.test.ts",
            "named after web/src/session.ts",
        ),
        ("core/tests/roundtrip.rs", "imports core/src/codec.rs"),
    ] {
        let line = text
            .lines()
            .find(|line| line.contains(test))
            .unwrap_or_else(|| panic!("{test} missing:\n{text}"));
        assert!(line.contains(why), "{line}");
    }
    assert!(!text.contains("fixtures"), "{text}");
    assert!(
        text.contains("No test found for:\n  - go/other.go\n"),
        "{text}"
    );
}

#[test]
fn tests_json_paths_and_directory_anchor() {
    let root = polyglot("tests-cmd-json");
    let json: Value =
        serde_json::from_str(&stdout(&ctx(&root, &["tests", "go/", "--json"], ""))).unwrap();
    assert_eq!(json["sources"].as_array().unwrap().len(), 2, "{json}");
    assert_eq!(json["tests"][0]["path"], "go/store_test.go");
    assert_eq!(json["tests"][0]["link"], "name");
    assert_eq!(json["untested"][0], "go/other.go");

    let paths = stdout(&ctx(
        &root,
        &["tests", "core/src/codec.rs", "--format", "paths"],
        "",
    ));
    assert_eq!(paths, "core/tests/roundtrip.rs\n");
}

#[test]
fn tests_without_a_selection_is_an_error() {
    let root = polyglot("tests-cmd-empty");
    let out = ctx(&root, &["tests"], "");
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("--changed"));
}

#[test]
fn tests_changed_lists_changed_tests_and_tests_of_changed_sources() {
    let root = polyglot("tests-cmd-changed");
    git(&root, &["init", "-q", "-b", "main"]);
    git(&root, &["add", "-A"]);
    git(
        &root,
        &[
            "-c",
            "user.email=dev@example.com",
            "-c",
            "user.name=Dev",
            "commit",
            "-q",
            "-m",
            "init",
        ],
    );
    fs::write(
        root.join("go/store.go"),
        "package store\nfunc Open() int { return 1 }\n",
    )
    .unwrap();
    fs::write(root.join("tests/test_db.py"), "# edited\n").unwrap();
    let paths = stdout(&ctx(
        &root,
        &["tests", "--changed", "--format", "paths"],
        "",
    ));
    assert_eq!(paths, "tests/test_db.py\ngo/store_test.go\n");

    // `ctx pack --changed --tests` packs the change, then its tests.
    let pack: Value = serde_json::from_str(&stdout(&ctx(
        &root,
        &["pack", "--changed", "--tests", "--json", "--no-contract"],
        "",
    )))
    .unwrap();
    let files: Vec<(&str, &str)> = pack["included"]
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
    let store_test = files
        .iter()
        .find(|(path, _)| *path == "go/store_test.go")
        .unwrap_or_else(|| panic!("store_test.go not packed: {files:?}"));
    assert_eq!(store_test.1, "test: named after go/store.go");
}

#[test]
fn pack_from_stdin_with_tests_appends_tests_after_the_selection() {
    let root = polyglot("tests-pack-stdin");
    let text = stdout(&ctx(
        &root,
        &[
            "pack",
            "--from-stdin",
            "--tests",
            "--explain",
            "--no-contract",
        ],
        "core/src/codec.rs\n",
    ));
    let selected = text.find("### core/src/codec.rs").expect("codec packed");
    let test = text
        .find("### core/tests/roundtrip.rs")
        .unwrap_or_else(|| panic!("test not packed:\n{text}"));
    assert!(selected < test);
    assert!(text.contains("test: imports core/src/codec.rs"), "{text}");
}

#[test]
fn pack_tests_needs_a_selection() {
    let root = polyglot("tests-pack-noselect");
    let out = ctx(&root, &["pack", "--tests"], "");
    assert!(!out.status.success());
    assert!(
        String::from_utf8_lossy(&out.stderr).contains("--tests needs a file selection"),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn a_command_missing_its_arguments_points_at_its_usage() {
    let root = polyglot("usage-hint");
    let out = ctx(&root, &["deps"], "");
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("Usage: ctx deps <PATH>..."), "{stderr}");
    assert!(stderr.contains("Run `ctx deps --help`"), "{stderr}");
}
