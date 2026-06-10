use std::fs;

mod common;
use common::*;

#[test]
fn delegated_root_execution_matches_go_for_json_tree() {
    assert_delegated_parity(&["--git=false", "--json", "tests/where-fixtures/small_repo"]);
}

// Wave 4 blocker #2b slice 2: the DEFAULT text tree (`ctx [path]`, no --json) +
// the renderPlanFit footer must run native byte-identical to Go.
#[test]
fn native_root_text_tree_matches_go() {
    assert_delegated_parity(&["--git=false", "tests/where-fixtures/small_repo"]);
}

// Wave 4 blocker #2b slice 3: the root FLAG-VARIANTS (--depth/--tokens/--unit/
// --budget/--plan, text + JSON) must run native byte-identical to Go. The native
// data layer already handles these for `map`; root must reuse it. (Deterministic
// flags only; --since/--until/--use-mtime are mtime-dependent on a non-git
// fixture, so they are wired/routed but not byte-parity-tested here.)
#[test]
fn native_root_depth_matches_go() {
    assert_delegated_parity(&[
        "--git=false",
        "--depth",
        "1",
        "tests/where-fixtures/small_repo",
    ]);
    assert_delegated_parity(&[
        "--git=false",
        "--json",
        "--depth",
        "1",
        "tests/where-fixtures/small_repo",
    ]);
}

#[test]
fn native_root_tokens_unit_matches_go() {
    assert_delegated_parity(&["--git=false", "--tokens", "tests/where-fixtures/small_repo"]);
    assert_delegated_parity(&[
        "--git=false",
        "--unit",
        "chars",
        "tests/where-fixtures/small_repo",
    ]);
}

#[test]
fn native_root_budget_matches_go() {
    assert_delegated_parity(&[
        "--git=false",
        "--budget",
        "200",
        "tests/where-fixtures/small_repo",
    ]);
    assert_delegated_parity(&[
        "--git=false",
        "--budget",
        "200",
        "--json",
        "tests/where-fixtures/small_repo",
    ]);
}

#[test]
fn native_root_plan_matches_go() {
    assert_delegated_parity(&[
        "--git=false",
        "--plan",
        "gpt-4o",
        "tests/where-fixtures/small_repo",
    ]);
}

// ── Wave-2 byte-parity suite for native `--symbols --json` ───────────────────
//
// Goal: prove `ctx [PATH] --symbols --json` runs NATIVELY (tree-sitter
// extraction in ctx-symbols) and is byte-identical to Go's
// render.JSONSymbols across all 5 supported languages (Go / JS / Python /
// TS / TSX, 6 extensions incl. .jsx). The lang_corpus fixture exercises
// generics, classes, interfaces, type aliases, and nested defs per
// language; the small/medium/large corpora add scale + mixed languages.

#[test]
fn native_roots_add_list_remove_with_env_registry() {
    let root = test_dir("roots");
    let project = root.join("project");
    fs::create_dir_all(&project).unwrap();
    let registry = root.join("registry").join("roots.toml");
    let registry_arg = registry.to_string_lossy().to_string();
    let project_arg = project.to_string_lossy().to_string();

    let add = run_rust_in_with_env(
        &root,
        &["roots", "add", &project_arg, "--name", "alpha"],
        &[("CTX_ROOTS_FILE", &registry_arg)],
    );
    assert!(
        add.status.success(),
        "roots add failed: {}",
        String::from_utf8_lossy(&add.stderr)
    );
    assert!(String::from_utf8_lossy(&add.stdout).contains("registered alpha"));

    let list = run_rust_in_with_env(
        &root,
        &["roots", "list"],
        &[("CTX_ROOTS_FILE", &registry_arg)],
    );
    assert!(
        list.status.success(),
        "roots list failed: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let list_stdout = String::from_utf8_lossy(&list.stdout);
    // Go uses tabwriter (space-aligned columns); list output contains "NAME" header
    // followed by space padding (not raw tabs). Check for the header word and entry.
    assert!(
        list_stdout.contains("NAME"),
        "expected NAME header in: {list_stdout}"
    );
    assert!(
        list_stdout.contains("PATH"),
        "expected PATH header in: {list_stdout}"
    );
    assert!(
        list_stdout.contains("LAST OPENED"),
        "expected LAST OPENED header in: {list_stdout}"
    );
    assert!(
        list_stdout.contains("alpha"),
        "expected 'alpha' entry in: {list_stdout}"
    );
    assert!(
        list_stdout.contains(&project_arg),
        "expected path in: {list_stdout}"
    );

    let remove = run_rust_in_with_env(
        &root,
        &["roots", "remove", "alpha"],
        &[("CTX_ROOTS_FILE", &registry_arg)],
    );
    assert!(
        remove.status.success(),
        "roots remove failed: {}",
        String::from_utf8_lossy(&remove.stderr)
    );
    assert!(String::from_utf8_lossy(&remove.stdout).contains("removed alpha"));

    let empty = run_rust_in_with_env(
        &root,
        &["roots", "ls"],
        &[("CTX_ROOTS_FILE", &registry_arg)],
    );
    assert!(String::from_utf8_lossy(&empty.stdout).contains("no roots registered"));
}

#[test]
fn native_roots_open_missing_uses_env_registry() {
    let root = test_dir("roots-open-missing");
    fs::create_dir_all(&root).unwrap();
    let registry = root.join("registry").join("roots.toml");
    let registry_arg = registry.to_string_lossy().to_string();

    let output = run_rust_in_with_env(
        &root,
        &[
            "roots",
            "open",
            "missing",
            "--no-open",
            "--timeout",
            "100ms",
        ],
        &[("CTX_ROOTS_FILE", &registry_arg)],
    );
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr)
        .contains("ctx roots open: no registered root matches \"missing\""));
}
