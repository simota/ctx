//! `ctx where --repos-only` must name the repositories that hold the matches:
//! the root's own repository as `.`, nested repositories by their path, and
//! only formats it can actually render.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

/// Scratch directory outside any git work tree (the crate's `target/` lives
/// inside this repository, which would count as an enclosing repository).
struct Workspace(PathBuf);

impl Workspace {
    fn new(name: &str) -> Self {
        let dir =
            std::env::temp_dir().join(format!("ctx-where-repos-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, body: &str) {
        let path = self.0.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }

    fn git_dir(&self, rel: &str) {
        fs::create_dir_all(self.0.join(rel).join(".git")).unwrap();
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn ctx(cwd: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(args)
        .current_dir(cwd)
        .output()
        .unwrap_or_else(|err| panic!("run ctx {args:?}: {err}"))
}

fn stdout_lines(output: &Output) -> Vec<String> {
    assert!(
        output.status.success(),
        "ctx failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(ToString::to_string)
        .collect()
}

#[test]
fn repos_only_keeps_root_repository_next_to_nested_repository() {
    let ws = Workspace::new("nested");
    ws.git_dir(".");
    ws.git_dir("libs/sub");
    ws.write("src/widget.rs", "fn widget_alpha() {}\n");
    ws.write("libs/sub/src/widget.rs", "fn widget_alpha() {}\n");

    let mut repos = stdout_lines(&ctx(&ws.0, &["where", "widget_alpha", "--repos-only"]));
    repos.sort();
    assert_eq!(repos, vec![".", "libs/sub"]);
}

#[test]
fn repos_only_reports_enclosing_repository_for_subdirectory_root() {
    let ws = Workspace::new("subdir");
    ws.git_dir(".");
    ws.write("src/net/http/w.rs", "fn widget_alpha() {}\n");
    ws.write("src/db/w.rs", "fn widget_alpha() {}\n");

    let repos = stdout_lines(&ctx(
        &ws.0,
        &["where", "widget_alpha", "src", "--repos-only"],
    ));
    assert_eq!(repos, vec!["."]);
}

#[test]
fn repos_only_lists_sibling_repositories_of_a_plain_workspace() {
    let ws = Workspace::new("plain");
    ws.git_dir("repo-a");
    ws.git_dir("repo-b");
    ws.write("repo-a/src/main.rs", "fn widget_alpha() {}\n");
    ws.write("repo-b/README.md", "widget_alpha\n");
    ws.write("notes.md", "widget_alpha\n");

    let mut repos = stdout_lines(&ctx(&ws.0, &["where", "widget_alpha", "--repos-only"]));
    repos.sort();
    assert_eq!(repos, vec!["repo-a", "repo-b"]);

    let json = ctx(&ws.0, &["where", "widget_alpha", "--repos-only", "--json"]);
    let mut parsed: Vec<String> = serde_json::from_slice(&json.stdout).unwrap();
    parsed.sort();
    assert_eq!(parsed, vec!["repo-a", "repo-b"]);
}

#[test]
fn repos_only_rejects_formats_it_cannot_render() {
    let ws = Workspace::new("format");
    ws.git_dir(".");
    ws.write("src/widget.rs", "fn widget_alpha() {}\n");

    for format in ["vimgrep", "bogus"] {
        let output = ctx(
            &ws.0,
            &["where", "widget_alpha", "--repos-only", "--format", format],
        );
        assert!(
            !output.status.success(),
            "--format {format} should fail, got stdout: {}",
            String::from_utf8_lossy(&output.stdout)
        );
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(format),
            "error should name the format: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }
}
