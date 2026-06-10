use serde_json::Value;
use std::fs;

mod common;
use common::*;

#[test]
fn native_noise_json_reports_candidates() {
    let root = test_dir("noise");
    fs::create_dir_all(&root).unwrap();
    fs::write(root.join("go.sum"), "module fake\n").unwrap();
    fs::write(root.join("app.min.js"), "function minified(){}\n").unwrap();
    fs::write(root.join("keep.go"), "package main\n\nfunc main() {}\n").unwrap();

    let output = run_rust_in(&root, &["noise", "--format", "json", "--top", "2"]);
    assert!(
        output.status.success(),
        "noise failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let payload: Value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "parse noise JSON: {err}\n{}",
            String::from_utf8_lossy(&output.stdout)
        )
    });
    let candidates = payload["candidates"]
        .as_array()
        .expect("noise candidates should be an array");
    assert!(candidates.iter().any(|item| item["path"] == "go.sum"));
    assert!(candidates.iter().any(|item| item["reason"] == "lockfile"));
    assert_eq!(payload["total_files"], 2);
}

/// Byte-parity for `noise --format text` (default).
#[test]
fn noise_parity_text_format() {
    let root = write_noise_fixture();
    assert_delegated_parity_in(&root, &["noise", "--format", "text"]);
}

/// Byte-parity for `noise` with no explicit --format (defaults to text).
#[test]
fn noise_parity_default_format() {
    let root = write_noise_fixture();
    assert_delegated_parity_in(&root, &["noise"]);
}

/// Byte-parity for `noise --format json`.
#[test]
fn noise_parity_json_format() {
    let root = write_noise_fixture();
    assert_delegated_parity_in(&root, &["noise", "--format", "json"]);
}

/// Byte-parity for `noise --top 1` (limits displayed candidates).
#[test]
fn noise_parity_top_flag() {
    let root = write_noise_fixture();
    assert_delegated_parity_in(&root, &["noise", "--top", "1"]);
    assert_delegated_parity_in(&root, &["noise", "--top", "1", "--format", "json"]);
}

/// Byte-parity for `noise --apply` (gitignore-syntax proposal to stdout).
/// Exercises: em dash comment line, reason grouping, individual file listing.
#[test]
fn noise_parity_apply() {
    let root = write_noise_fixture();
    assert_delegated_parity_in(&root, &["noise", "--apply"]);
}

/// Byte-parity for `noise --apply` with 3 same-dir+ext files → glob aggregation.
/// Go collapses 3+ same-(dir,ext) into `dir/*.ext`; Rust must match.
#[test]
fn noise_parity_apply_glob_aggregation() {
    let root = write_noise_glob_fixture();
    assert_delegated_parity_in(&root, &["noise", "--apply"]);
}

/// Full flag matrix: text/json × top/no-top × apply.
#[test]
fn noise_parity_full_flag_matrix() {
    let root = write_noise_fixture();

    // format × top
    for format in &["text", "json"] {
        assert_delegated_parity_in(&root, &["noise", "--format", format]);
        assert_delegated_parity_in(&root, &["noise", "--format", format, "--top", "1"]);
        assert_delegated_parity_in(&root, &["noise", "--format", format, "--top", "100"]);
    }

    // --apply (top is ignored)
    assert_delegated_parity_in(&root, &["noise", "--apply"]);

    // explicit root path (same dir)
    let root_str = root.to_string_lossy().into_owned();
    assert_delegated_parity_in(&root, &["noise", "--format", "text", &root_str]);
    assert_delegated_parity_in(&root, &["noise", "--format", "json", &root_str]);
}

// ── Wave-1 byte-parity suite for `digest` ─────────────────────────────────────
//
// Goal: prove `digest` is byte-identical to Go across its full flag/value surface.
//
// Go surface (internal/cli/digest.go):
//   Flags: --since <str> (default "7d"), --top N (default 10),
//          --out <file> (write to file), --format markdown|json|plain
//   Output includes: commit count, author count, hot files sorted by commits desc
//
// Determinism approach:
//   Git objects (commit hashes, dates, authors) are pinned by using fixed env vars
//   GIT_AUTHOR_DATE / GIT_COMMITTER_DATE plus fixed user config. This makes
//   commit hashes reproducible, so `since_ref` and `head_ref` in JSON match.
//
// Non-deterministic fields (documented exclusions in JSON tests):
//   - `period.since`: Go stores RFC3339 with nanoseconds (e.g. 2024-06-01T11:15:41.385642Z).
//     Rust outputs midnight UTC (2024-06-01T00:00:00Z). The DATE portion matches but
//     time-of-day differs. Excluded from JSON byte-parity; date is asserted separately.
//   - `period.until`: Go stores RFC3339 with nanoseconds of process start.
//     Rust stores RFC3339 with seconds. Both have today's date but differ in
//     time-of-day and sub-seconds. Excluded from JSON byte-parity.
//   Text/plain/markdown formats render both as YYYY-MM-DD (date only), which IS
//   deterministic given the same UTC date — byte-exact via assert_delegated_parity_in.
//
// Root-commit handling:
//   Go uses go-git: root commit files are enumerated via c.Files() WITHOUT marking
//   them as "added". Only non-root commits mark from==nil as added.
//   Rust uses `git log --name-status` with a %P (parents) field to detect root
//   commits and skip marking their A-status files as added. Matches Go exactly.
//
// Token/symbol deltas:
//   Both binaries use tiktoken cl100k_base + regex-based symbol extraction.
//   Deltas are computed from `git show {since_ref}:{path}` vs `git show HEAD:{path}`.
//   Results are byte-identical when both run against the same pinned repo.
//
// No `return None` in run_digest_command reachable by valid invocation:
//   parse_digest_args returns None only for unknown flags or unexpected positionals —
//   both are error paths. The happy path (valid flags, no positionals) never returns None.
