use std::process::Command;

mod common;
use common::*;

#[test]
fn native_symbols_json_matches_go_lang_corpus() {
    // All 5 languages / 6 extensions in one corpus.
    assert_delegated_parity(&[
        "--git=false",
        "--symbols",
        "--json",
        "tests/symbols-fixtures/lang_corpus",
    ]);
}

#[test]
fn native_symbols_json_matches_go_all_corpora() {
    for fx in ["small_corpus", "medium_corpus", "large_corpus"] {
        let dir = format!("tests/symbols-fixtures/{fx}");
        assert_delegated_parity(&["--git=false", "--symbols", "--json", &dir]);
    }
}

/// Prove the symbols-JSON path is NATIVE (not delegated): run the Rust
/// binary with CTX_GO_BIN pointed at a non-existent path. If it delegated,
/// the spawn would fail; native execution must still produce Go-identical
/// output. We compare against the Go oracle output captured separately.
#[test]
fn native_symbols_json_runs_without_go_binary() {
    let root = repo_root();
    let args = [
        "--git=false",
        "--symbols",
        "--json",
        "tests/symbols-fixtures/lang_corpus",
    ];
    let go = run_go_in(&root, &args);

    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args(args)
        .env(
            "CTX_GO_BIN",
            "/nonexistent/ctx-go-binary-should-never-spawn",
        )
        .current_dir(&root);
    let rust = cmd
        .output()
        .expect("run Rust ctx symbols-json without a real Go binary");

    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit mismatch; native path should not delegate.\nRust stderr:\n{}",
        String::from_utf8_lossy(&rust.stderr),
    );
    assert_eq!(
        rust.stdout,
        go.stdout,
        "native symbols-JSON stdout diverges from Go oracle.\nGo:\n{}\nRust:\n{}",
        String::from_utf8_lossy(&go.stdout),
        String::from_utf8_lossy(&rust.stdout),
    );
}

/// Wave 4 (#2b slice 4): `--symbols --json --depth=N` now runs NATIVELY,
/// applying the depth limit to the walk (reusing `build_root_tree`) so deep
/// files are excluded — byte-identical to Go's `render.JSONSymbols`. On
/// `medium_corpus` at `--depth=1` every file lives one level below the root, so
/// the walk halts before reaching them → no symbol files → `{"files": null}`.
#[test]
fn native_symbols_json_with_depth_matches_go() {
    assert_delegated_parity(&[
        "--git=false",
        "--symbols",
        "--json",
        "--depth=1",
        "tests/symbols-fixtures/medium_corpus",
    ]);
}
