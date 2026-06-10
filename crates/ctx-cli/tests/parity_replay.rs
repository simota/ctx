use serde_json::Value;

mod common;
use common::*;

#[test]
fn native_replay_diff_json_uses_shared_store() {
    let root = write_replay_fixture();
    let output = run_rust_in(
        &root,
        &[
            "replay", "--shared", "diff", "snap-a", "snap-b", "--format", "json",
        ],
    );
    assert!(
        output.status.success(),
        "replay diff failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse replay diff JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    assert_eq!(payload["a"], "snap-a");
    assert_eq!(payload["b"], "snap-b");
    assert_eq!(payload["summary"]["added"], 1);
    assert_eq!(payload["summary"]["promoted"], 1);
    assert_eq!(payload["changes"]["added"][0]["path"], "src/new.go");
}

#[test]
fn native_replay_list_and_show_use_shared_store() {
    let root = write_replay_fixture();

    let list = run_rust_in(&root, &["--json", "replay", "--shared", "list"]);
    assert!(
        list.status.success(),
        "replay list failed: {}",
        String::from_utf8_lossy(&list.stderr)
    );
    let listed: Value = serde_json::from_slice(&list.stdout).unwrap();
    assert_eq!(listed.as_array().map(Vec::len), Some(2));

    let show = run_rust_in(&root, &["replay", "--shared", "show", "snap-a"]);
    assert!(
        show.status.success(),
        "replay show failed: {}",
        String::from_utf8_lossy(&show.stderr)
    );
    let manifest: Value = serde_json::from_slice(&show.stdout).unwrap();
    assert_eq!(manifest["id"], "snap-a");
    assert_eq!(manifest["entries"][0]["path"], "src/app.go");
}

// ── Wave-1 byte-parity suite for `replay` ────────────────────────────────────
//
// Goal: prove `replay` (and all its subcommands) are provably byte-identical to
// Go across the ENTIRE flag/value surface, making it ready for Wave-3 cutover.
//
// Subcommands: list, show, prune, diff  (no bare `replay` or `verify`).
// Flags per subcommand:
//   - list:  (none beyond --shared, --json global)
//   - show:  <id> positional
//   - prune: --older-than <duration>
//   - diff:  <id-a> <id-b>, --by tier|tokens|score, --format markdown|json,
//             --replay-engine go (no-op; rust is a documented carve-out)
//
// Fixture: `write_replay_parity_fixture()` — 3 deterministic snapshots with
// pinned created_at timestamps so the test is fully reproducible and
// independent of wall-clock time. Snapshot "psnap-a" has no goal (→ "-" in
// text output), "psnap-b" has a goal.
//
// Determinism: all timestamps pinned in JSON files. `prune` uses --older-than
// with a value that makes results predictable relative to fixed timestamps.
//
// `run_replay_command` has no reachable `return None` for valid subcommand
// invocations: `parse_replay_args` returns `None` only on unknown flags or
// when subcommand.as_deref()? is None (no subcommand) or falls through to the
// `_ => return None` arm (unknown subcommand). All four known subcommands
// (list/show/prune/diff) always return Some(ReplayArgs).
//
// --replay-engine rust is a documented carve-out: Go exits 1 ("requires -tags
// rust_contract"), Rust runs native (exit 0). NOT tested here.

/// `replay list` text format — tabwriter-aligned columns, empty goal shows "-".
#[test]
fn replay_parity_list_text() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(&root, &["replay", "--shared", "list"]);
}

/// `replay list` JSON format via --json global flag.
#[test]
fn replay_parity_list_json() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(&root, &["--json", "replay", "--shared", "list"]);
}

// ── replay show ── ────────────────────────────────────────────────────────────

/// `replay show` — JSON manifest output for a snapshot with no goal.
#[test]
fn replay_parity_show_no_goal() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(&root, &["replay", "--shared", "show", "psnap-a"]);
}

/// `replay show` — JSON manifest output for a snapshot with a goal.
#[test]
fn replay_parity_show_with_goal() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(&root, &["replay", "--shared", "show", "psnap-b"]);
}

// ── replay prune ── ───────────────────────────────────────────────────────────

/// `replay prune` text — no snapshots deleted (far-future older-than threshold).
#[test]
fn replay_parity_prune_text_none_deleted() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &["replay", "--shared", "prune", "--older-than", "99999d"],
    );
}

/// `replay prune` JSON — deleted list is null when no snapshots pruned.
/// Verifies Go's nil slice → JSON null vs Rust's empty Vec → JSON null.
#[test]
fn replay_parity_prune_json_none_deleted() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "--json",
            "replay",
            "--shared",
            "prune",
            "--older-than",
            "99999d",
        ],
    );
}

/// `replay prune` text — snapshots ARE deleted (all 3 are from Jan 2026, older than 1d threshold).
/// Uses a 1d threshold which prunes all 3 old snapshots.
/// NOTE: prune is destructive so Go and Rust need SEPARATE fixture copies to avoid
/// one binary deleting the files before the other runs.
#[test]
fn replay_parity_prune_text_with_deletions() {
    let root_go = write_replay_parity_fixture();
    let root_rust = write_replay_parity_fixture();
    let args = &["replay", "--shared", "prune", "--older-than", "1d"];
    let go = run_go_in(&root_go, args);
    let rust = run_rust_in(&root_rust, args);
    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch for args {args:?}"
    );
    assert_eq!(
        rust.stdout,
        go.stdout,
        "stdout mismatch for args {args:?}\nGo: {}\nRust: {}",
        String::from_utf8_lossy(&go.stdout),
        String::from_utf8_lossy(&rust.stdout)
    );
    assert_eq!(rust.stderr, go.stderr, "stderr mismatch for args {args:?}");
}

/// `replay prune` JSON — deleted list populated.
/// NOTE: same destructive-fixture pattern as above.
#[test]
fn replay_parity_prune_json_with_deletions() {
    let root_go = write_replay_parity_fixture();
    let root_rust = write_replay_parity_fixture();
    let args = &[
        "--json",
        "replay",
        "--shared",
        "prune",
        "--older-than",
        "1d",
    ];
    let go = run_go_in(&root_go, args);
    let rust = run_rust_in(&root_rust, args);
    assert_eq!(
        rust.status.code(),
        go.status.code(),
        "exit code mismatch for args {args:?}"
    );
    assert_eq!(
        rust.stdout,
        go.stdout,
        "stdout mismatch for args {args:?}\nGo: {}\nRust: {}",
        String::from_utf8_lossy(&go.stdout),
        String::from_utf8_lossy(&rust.stdout)
    );
    assert_eq!(rust.stderr, go.stderr, "stderr mismatch for args {args:?}");
}

// ── replay diff ── ────────────────────────────────────────────────────────────

/// `replay diff` default format (markdown) — added + promoted.
#[test]
fn replay_parity_diff_markdown_default() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(&root, &["replay", "--shared", "diff", "psnap-a", "psnap-b"]);
}

/// `replay diff` markdown explicit --format.
#[test]
fn replay_parity_diff_markdown_explicit() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "replay", "--shared", "diff", "psnap-a", "psnap-b", "--format", "markdown",
        ],
    );
}

/// `replay diff` JSON format.
#[test]
fn replay_parity_diff_json() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "replay", "--shared", "diff", "psnap-a", "psnap-b", "--format", "json",
        ],
    );
}

/// `replay diff` JSON via --json global flag.
#[test]
fn replay_parity_diff_json_global_flag() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &["--json", "replay", "--shared", "diff", "psnap-a", "psnap-b"],
    );
}

/// `replay diff` with --by tokens sort.
#[test]
fn replay_parity_diff_by_tokens() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "replay", "--shared", "diff", "psnap-a", "psnap-b", "--by", "tokens",
        ],
    );
}

/// `replay diff` with --by score sort.
#[test]
fn replay_parity_diff_by_score() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "replay", "--shared", "diff", "psnap-a", "psnap-b", "--by", "score",
        ],
    );
}

/// `replay diff` with --by tier sort (explicit, same as default).
#[test]
fn replay_parity_diff_by_tier_explicit() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "replay", "--shared", "diff", "psnap-a", "psnap-b", "--by", "tier",
        ],
    );
}

/// `replay diff` b→c: added + removed (psnap-b has auth.go, psnap-c has deploy.go).
#[test]
fn replay_parity_diff_markdown_added_removed() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(&root, &["replay", "--shared", "diff", "psnap-b", "psnap-c"]);
}

/// `replay diff` b→c JSON: added + removed categories.
#[test]
fn replay_parity_diff_json_added_removed() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "replay", "--shared", "diff", "psnap-b", "psnap-c", "--format", "json",
        ],
    );
}

/// `replay diff` with --replay-engine go (no-op; output matches default).
/// NOTE: --replay-engine rust is a documented carve-out — Go exits 1
/// ("requires -tags rust_contract"), Rust runs native (exit 0). NOT tested.
#[test]
fn replay_parity_diff_engine_go() {
    let root = write_replay_parity_fixture();
    assert_delegated_parity_in(
        &root,
        &[
            "replay",
            "--shared",
            "diff",
            "psnap-a",
            "psnap-b",
            "--replay-engine",
            "go",
        ],
    );
}

/// Full matrix: list/show/diff/prune × all format/flag combinations.
/// Aggregates the most critical cross-product for regression detection.
#[test]
fn replay_parity_full_flag_matrix() {
    let root = write_replay_parity_fixture();

    // list × text and JSON
    assert_delegated_parity_in(&root, &["replay", "--shared", "list"]);
    assert_delegated_parity_in(&root, &["--json", "replay", "--shared", "list"]);

    // show × both snapshots
    assert_delegated_parity_in(&root, &["replay", "--shared", "show", "psnap-a"]);
    assert_delegated_parity_in(&root, &["replay", "--shared", "show", "psnap-c"]);

    // diff × format × by
    for format in &["markdown", "json"] {
        for by in &["tier", "tokens", "score"] {
            assert_delegated_parity_in(
                &root,
                &[
                    "replay", "--shared", "diff", "psnap-a", "psnap-b", "--format", format, "--by",
                    by,
                ],
            );
        }
    }

    // prune × json/text × none/some deleted
    assert_delegated_parity_in(
        &root,
        &["replay", "--shared", "prune", "--older-than", "99999d"],
    );
    assert_delegated_parity_in(
        &root,
        &[
            "--json",
            "replay",
            "--shared",
            "prune",
            "--older-than",
            "99999d",
        ],
    );
}
