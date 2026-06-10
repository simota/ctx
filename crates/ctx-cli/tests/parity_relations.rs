mod common;
use common::*;

#[test]
fn native_relations_commands_match_go() {
    let root = write_relation_fixture();
    assert_delegated_parity_in(&root, &["deps", "cmd/main.go"]);
    assert_delegated_parity_in(&root, &["impact", "lib/lib.go"]);
    assert_delegated_parity_in(&root, &["deps", "cmd/main.go", "--format", "json"]);
    assert_delegated_parity_in(&root, &["--json", "impact", "lib/lib.go"]);
}

// ── Wave-1 byte-parity suite for `deps` and `impact` ─────────────────────────
//
// Goal: prove `deps` and `impact` are provably byte-identical to Go across their
// ENTIRE flag/value surface so both commands are ready for Wave-3 zero-delegation
// cutover.
//
// Flag surface for both commands:
//   - positional <file>            (required)
//   - --format text|json           (text is default)
//   - --json                       (global flag, overrides --format)
//   - --relations-engine go|rust   (go is the default/no-op; rust is a documented
//                                   carve-out: Go exits 1 "requires -tags rust_contract",
//                                   Rust runs native — NOT tested here)
//
// Fixtures used:
//   - write_relation_fixture()     simple 2-file graph (cmd/main.go → lib/lib.go)
//   - write_go_project_fixture()   multi-file graph: main.go → {lib/a.go, lib/b.go, lib/sub/c.go}
//                                  non-trivial: multiple deps, multiple importers, transitive depth
//
// Combinations covered per command:
//   text × non-empty, text × empty, json × non-empty, json × empty,
//   --json global flag × non-empty, --relations-engine go (no-op)
//
// No `return None` is reachable in `run_relations_command` for any valid
// deps/impact invocation: `parse_relation_args` returns `None` only when it
// encounters an unknown flag or no/multiple positional args — which are
// error/usage paths, not valid invocations. The function body has no
// unconditional `return None` on the happy path.

/// `deps` text format — file with dependencies (non-empty result).
#[test]
fn deps_parity_text_non_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["deps", "main.go"]);
}

/// `deps` json format — file with dependencies (non-empty result).
#[test]
fn deps_parity_json_non_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["deps", "main.go", "--format", "json"]);
}

/// `deps` text format — file with NO dependencies (empty result).
/// Verifies text: "No dependencies found for <path>."
#[test]
fn deps_parity_text_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["deps", "lib/a.go"]);
}

/// `deps` json format — file with NO dependencies.
/// Verifies json: items is [] (not null) — Go nil slice → JSON null risk confirmed non-issue.
#[test]
fn deps_parity_json_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["deps", "lib/a.go", "--format", "json"]);
}

/// `deps` with --json global flag (overrides format to json).
#[test]
fn deps_parity_json_global_flag() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["--json", "deps", "main.go"]);
}

/// `deps` with --relations-engine go (documented no-op in both Go and Rust).
/// Note: --relations-engine rust is a documented carve-out — Go exits 1
/// ("requires -tags rust_contract"), Rust runs native (exit 0). NOT tested here.
#[test]
fn deps_parity_relations_engine_go() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["deps", "main.go", "--relations-engine", "go"]);
}

/// `impact` text format — file with dependents (non-empty result).
#[test]
fn impact_parity_text_non_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["impact", "lib/a.go"]);
}

/// `impact` json format — file with dependents (non-empty result).
#[test]
fn impact_parity_json_non_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["impact", "lib/a.go", "--format", "json"]);
}

/// `impact` text format — file with NO dependents (empty result).
/// Verifies text: "No dependents found for <path>."
#[test]
fn impact_parity_text_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["impact", "main.go"]);
}

/// `impact` json format — file with NO dependents.
/// Verifies json: items is [] (not null).
#[test]
fn impact_parity_json_empty() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["impact", "main.go", "--format", "json"]);
}

/// `impact` with --json global flag (overrides format to json).
#[test]
fn impact_parity_json_global_flag() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["--json", "impact", "lib/a.go"]);
}

/// `impact` with --relations-engine go (no-op).
#[test]
fn impact_parity_relations_engine_go() {
    let root = write_go_project_fixture();
    assert_delegated_parity_in(&root, &["impact", "lib/a.go", "--relations-engine", "go"]);
}

/// Full deps × impact × format cross-product on the multi-file go_project fixture.
/// lib/sub/c.go is imported by main.go — tests deeper transitive path resolution.
#[test]
fn deps_impact_full_matrix_go_project() {
    let root = write_go_project_fixture();

    // deps × all format values × representative targets
    for format in &["text", "json"] {
        // non-empty deps
        assert_delegated_parity_in(&root, &["deps", "main.go", "--format", format]);
        // empty deps (leaf node)
        assert_delegated_parity_in(&root, &["deps", "lib/b.go", "--format", format]);
        // deeper sub-package
        assert_delegated_parity_in(&root, &["deps", "lib/sub/c.go", "--format", format]);
    }

    // impact × all format values × representative targets
    for format in &["text", "json"] {
        // non-empty impact (imported by main.go)
        assert_delegated_parity_in(&root, &["impact", "lib/a.go", "--format", format]);
        // non-empty impact via sub-package
        assert_delegated_parity_in(&root, &["impact", "lib/sub/c.go", "--format", format]);
        // empty impact (no one imports main.go)
        assert_delegated_parity_in(&root, &["impact", "main.go", "--format", format]);
    }
}
