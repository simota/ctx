use std::fs;

mod common;
use common::*;

/// `roots list` — empty registry emits hint message.
#[test]
fn roots_parity_list_empty() {
    let dir = test_dir("roots-parity-list-empty");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    assert_roots_parity_in_env(&dir, &go_rf, &rs_rf, &[], &["roots", "list"]);
}

/// `roots add` — first registration prints "ctx roots: registered NAME -> PATH".
#[test]
fn roots_parity_add_first() {
    let dir = test_dir("roots-parity-add-first");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[],
        &["roots", "add", &target, "--name", "myroot"],
    );
}

/// `roots list` — single entry.
#[test]
fn roots_parity_list_one_entry() {
    let dir = test_dir("roots-parity-list-one");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[&["roots", "add", &target, "--name", "myroot"]],
        &["roots", "list"],
    );
}

/// `roots ls` (alias) — same as list.
#[test]
fn roots_parity_ls_alias() {
    let dir = test_dir("roots-parity-ls-alias");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[&["roots", "add", &target, "--name", "myroot"]],
        &["roots", "ls"],
    );
}

/// `roots add` — already registered prints "ctx roots: already registered".
#[test]
fn roots_parity_add_already_registered() {
    let dir = test_dir("roots-parity-add-again");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[&["roots", "add", &target, "--name", "myroot"]],
        &["roots", "add", &target, "--name", "myroot"],
    );
}

/// `roots register` (alias for add).
#[test]
fn roots_parity_register_alias() {
    let dir = test_dir("roots-parity-register-alias");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[],
        &["roots", "register", &target, "--name", "regroot"],
    );
}

/// `roots remove` — removal by name prints "ctx roots: removed NAME".
#[test]
fn roots_parity_remove_by_name() {
    let dir = test_dir("roots-parity-remove-by-name");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[&["roots", "add", &target, "--name", "removeroot"]],
        &["roots", "remove", "removeroot"],
    );
}

/// `roots rm` (alias) — same as remove.
#[test]
fn roots_parity_rm_alias() {
    let dir = test_dir("roots-parity-rm-alias");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[&["roots", "add", &target, "--name", "rmroot"]],
        &["roots", "rm", "rmroot"],
    );
}

/// `roots remove` — non-existent name → error "no entry matches", exit 1, cobra double-print.
#[test]
fn roots_parity_remove_nonexistent() {
    let dir = test_dir("roots-parity-remove-nonexistent");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[],
        &["roots", "remove", "nonexistent"],
    );
}

/// `roots list` — multiple entries; verifies alphabetical sort and tabwriter alignment.
#[test]
fn roots_parity_list_multiple_entries() {
    let dir = test_dir("roots-parity-list-multi");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    // Register /tmp (canonical path on macOS = /private/tmp) and the test dir.
    let target = dir.to_string_lossy().into_owned();
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[
            &["roots", "add", &target, "--name", "zroot"],
            &["roots", "add", "/tmp", "--name", "aroot"],
        ],
        &["roots", "list"],
    );
}

/// Full roots sequence: add → list → remove → list.
#[test]
fn roots_parity_full_sequence() {
    let dir = test_dir("roots-parity-sequence");
    fs::create_dir_all(&dir).unwrap_or_else(|err| panic!("create {}: {err}", dir.display()));
    let go_rf = dir.join("go-roots.toml");
    let rs_rf = dir.join("rs-roots.toml");
    let target = dir.to_string_lossy().into_owned();

    // add
    assert_roots_parity_in_env(
        &dir,
        &go_rf,
        &rs_rf,
        &[],
        &["roots", "add", &target, "--name", "seqroot"],
    );
    // list after add
    assert_roots_parity_in_env(&dir, &go_rf, &rs_rf, &[], &["roots", "list"]);
    // remove
    assert_roots_parity_in_env(&dir, &go_rf, &rs_rf, &[], &["roots", "remove", "seqroot"]);
    // list after remove (empty)
    assert_roots_parity_in_env(&dir, &go_rf, &rs_rf, &[], &["roots", "list"]);
}
