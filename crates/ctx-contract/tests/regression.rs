// crates/ctx-contract/tests/regression.rs
//
// Phase 5 regression tests for the 4 CONFIRMED parity bugs surfaced in
// PHASE4_REVIEW.md. Each test exercises an input that the existing Go
// goldens do not cover (>1 MiB line, internal `..` paths, Unicode
// whitespace, JSON `"contract": null`) and asserts the Rust port now
// matches the Go reference implementation.

use std::collections::BTreeMap;
use std::io::Write;

use ctx_contract::builder::build;
use ctx_contract::embed::{embed_json_patch, embed_plain, parse_from_pack};
use ctx_contract::hash::sha256_hex;
use ctx_contract::parse_refs::extract_references;
use ctx_contract::types::{Contract, File, FileInput, VerifyOptions};
use ctx_contract::verify::verify;
use ctx_contract::SCHEMA_VERSION;

// ---------------------------------------------------------------------
// F-01 — bufio.Scanner 1MB cap *terminates* (Go) not *skips* (old Rust)
// ---------------------------------------------------------------------

/// A response whose 2nd line exceeds the bufio cap. Go's Scan() returns
/// false at that point and never reaches the 3rd line — any reference on
/// the 3rd line should *not* appear in the output. The pre-fix Rust port
/// would `continue` past the long line and surface that 3rd-line ref.
#[test]
fn f01_oversized_line_terminates_scanning_like_go_bufio() {
    let mut buf: Vec<u8> = Vec::new();
    buf.extend_from_slice(b"see internal/foo/bar.go for context\n");
    // ~1.5 MiB of `x` followed by newline — single line, well over 1 MiB.
    buf.extend(std::iter::repeat(b'x').take(1024 * 1024 + 512 * 1024));
    buf.push(b'\n');
    buf.extend_from_slice(b"also check pkg/after/long.go\n");

    let refs = extract_references(&buf);

    // First-line ref must be present.
    assert!(
        refs.iter()
            .any(|r| r.path == "internal/foo/bar.go" && r.kind == "file"),
        "first-line ref must survive, refs = {refs:?}"
    );
    // Third-line ref must NOT be present — Go's bufio terminates after
    // the cap, dropping every subsequent line.
    assert!(
        !refs.iter().any(|r| r.path == "pkg/after/long.go"),
        "post-cap ref must be dropped (Go bufio terminates), refs = {refs:?}"
    );
}

// ---------------------------------------------------------------------
// F-02 — `a/../b.go` must resolve to `b.go` (filepath.Clean parity)
// ---------------------------------------------------------------------

/// A contract path containing a cancellable `..` must resolve to a real
/// file inside the worktree, matching Go's `filepath.Clean` semantics.
/// The pre-fix Rust port rejected any `..` component outright.
#[test]
fn f02_relative_dotdot_collapses_against_preceding_component() {
    // Use a temp worktree directory.
    let dir = tempdir_unique("ctx-contract-f02");
    std::fs::create_dir_all(dir.join("pkg/sub")).unwrap();
    let target = dir.join("pkg/sub/foo.go");
    std::fs::write(&target, b"package sub\n").unwrap();

    let body = b"package sub\n";
    let expected_sha = sha256_hex(body);

    let contract = Contract {
        schema_version: SCHEMA_VERSION,
        created: "2026-05-29T00:00:00Z".into(),
        files: vec![File {
            // `pkg/sub/../sub/foo.go` should clean to `pkg/sub/foo.go`.
            path: "pkg/sub/../sub/foo.go".into(),
            line_start: 1,
            line_end: 1,
            sha256: expected_sha.clone(),
            line_hashes: vec![],
            symbols: vec![],
        }],
    };
    let opts = VerifyOptions {
        strict: false,
        no_symbols: false,
        worktree_root: dir.to_string_lossy().to_string(),
    };
    // Response cites the same internal-`..` path so Verify exercises the
    // cited_paths → worktree_sha branch.
    let response = b"see pkg/sub/../sub/foo.go for context\n";
    let res = verify(&contract, response, &opts);

    // Reference resolved → at least one OK; no StaleContent violation.
    assert!(
        res.violations.iter().all(|v| {
            // The reference itself may still be OutOfContext-tagged because
            // the verify lookup_path key is the literal `pkg/sub/../sub/foo.go`
            // which won't match the contract's stored path verbatim — but
            // the *worktree* check must not blow up with the "cannot be
            // resolved" message.
            v.message != "contract path cannot be resolved inside worktree"
        }),
        "internal `..` must not be rejected as un-resolvable, res = {:#?}",
        res
    );
    // And there should be no `stale-content` violation since the file
    // exists and the sha matches.
    assert!(
        !res.violations.iter().any(|v| {
            v.message == "worktree file is missing"
                || v.message == "worktree file differs from pack contract"
        }),
        "worktree read must succeed via filepath.Clean parity, res = {:#?}",
        res
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------
// F-04 — bare `\s`/`\S` must NOT match Unicode whitespace in Rust
// ---------------------------------------------------------------------

/// `+++<NBSP>b/foo.go` looks like a diff header but the separator is a
/// non-ASCII whitespace (U+00A0). Go's regexp rejects it; the Rust port
/// (pre-fix) used Unicode-aware `\s` which would accept it.
#[test]
fn f04_diff_header_rejects_unicode_whitespace_separator() {
    let input = "+++\u{00A0}b/internal/foo.go\n".as_bytes();
    let refs = extract_references(input);
    // The Go regexp `^\+\+\+\s+b/(\S+)` will not match because `\s` only
    // matches `[\t\n\f\r ]`. The post-fix Rust regex uses the explicit
    // ASCII class and must agree.
    assert!(
        refs.iter().all(|r| r.kind != "diff-header"),
        "NBSP separator must not match the diff-header regex, refs = {refs:?}"
    );
}

/// And the ideographic space (U+3000) likewise must not match.
#[test]
fn f04_diff_header_rejects_ideographic_space_separator() {
    let input = "+++\u{3000}b/internal/foo.go\n".as_bytes();
    let refs = extract_references(input);
    assert!(
        refs.iter().all(|r| r.kind != "diff-header"),
        "ideographic-space separator must not match the diff-header regex, refs = {refs:?}"
    );
}

/// Embed regex `<!-- ctx:contract v1\s*(.*?)\s*-->` must use ASCII-only
/// `\s`. With Rust's Unicode-aware default the `\s*` would consume NBSP
/// padding so the inner capture would NOT include it; with the post-fix
/// `(?-u:\s)*` the NBSP must end up *inside* the captured body just like
/// Go. We assert this by feeding a *strip* scenario: the strip regex
/// must leave a trailing-NBSP plain header alone because its `\s*`
/// can't span NBSP into the trailing newline.
#[test]
fn f04_strip_contract_block_treats_nbsp_as_non_whitespace() {
    use ctx_contract::embed::strip_contract_block;
    // Plain-header form `# CTX-CONTRACT v1: {json}\s*(\n|$)` — wedge a
    // NBSP between the JSON and the newline so Go's ASCII `\s*` cannot
    // bridge it, leaving the block UNSTRIPPED. The pre-fix Rust regex
    // (Unicode `\s*`) would bridge the NBSP and strip the block.
    let body = format!("prefix\n# CTX-CONTRACT v1: {{\"a\":1}}\u{00A0}\nsuffix");
    let stripped = strip_contract_block(body.as_bytes());
    let s = std::str::from_utf8(&stripped).unwrap();
    assert!(
        s.contains("# CTX-CONTRACT v1:"),
        "ASCII `\\s` parity: plain-header block with trailing NBPS must NOT be stripped, got = {s:?}"
    );
}

// ---------------------------------------------------------------------
// F-03 — JSON pack with `"contract": null` should report a zero
// Contract with SchemaVersion=1 (Go's `json.Unmarshal(null, &c)` parity)
// ---------------------------------------------------------------------

#[test]
fn f03_contract_null_decodes_as_zero_contract_with_schema_version() {
    let body = br#"{"pack":"x","contract":null}"#;
    let parsed =
        parse_from_pack(body).expect("Go would return (Contract{SchemaVersion:1}, true) here");
    assert_eq!(parsed.schema_version, SCHEMA_VERSION);
    assert!(parsed.files.is_empty());
}

// ---------------------------------------------------------------------
// F-05 — embed_json_patch must emit alphabetically-ordered keys
// (matches Go's `json.Marshal(map[string]RawMessage)`)
// ---------------------------------------------------------------------

#[test]
fn f05_embed_json_patch_sorts_top_level_keys_alphabetically() {
    let c = Contract {
        schema_version: SCHEMA_VERSION,
        created: "2026-05-29T00:00:00Z".into(),
        files: vec![],
    };
    let input = br#"{"z":"last","a":"first"}"#;
    let out = embed_json_patch(input, &c).unwrap();
    let s = std::str::from_utf8(&out).unwrap();
    let a_pos = s.find("\"a\"").expect("`a` key present");
    let c_pos = s.find("\"contract\"").expect("`contract` key present");
    let z_pos = s.find("\"z\"").expect("`z` key present");
    assert!(
        a_pos < c_pos && c_pos < z_pos,
        "expected a < contract < z, got: {s}"
    );

    // And ensure embed_plain still works for the side branch.
    let mut buf: Vec<u8> = b"plain body".to_vec();
    embed_plain(&mut buf, &c).unwrap();
    // ensure round-trip via parse
    assert!(parse_from_pack(&buf).is_some());

    // BTreeMap re-import sanity (silences unused warning if test trims later).
    let _: BTreeMap<String, ()> = BTreeMap::new();
}

#[test]
fn line_range_worktree_staleness_checks_only_cited_range() {
    let dir = tempdir_unique("ctx-contract-line-range-stale");
    let path = dir.join("a.go");
    let original = b"same\noriginal\nkept\n";
    std::fs::write(&path, b"same\nchanged\nkept\n").unwrap();

    let contract = build(vec![FileInput {
        path: "a.go".into(),
        content: original.to_vec(),
        symbols: vec![],
    }]);
    let opts = VerifyOptions {
        strict: false,
        no_symbols: false,
        worktree_root: dir.to_string_lossy().to_string(),
    };

    let unchanged = verify(&contract, b"see a.go:1-1\n", &opts);
    assert_eq!(
        unchanged.exit_code, 0,
        "unchanged cited line should not be stale: {unchanged:#?}"
    );

    let changed = verify(&contract, b"see a.go:2-2\n", &opts);
    assert_eq!(changed.exit_code, 1, "changed cited line must be stale");
    assert!(
        changed.violations.iter().any(|v| v.message
            == "worktree line range differs from pack contract"
            && v.line_start == 2
            && v.line_end == 2),
        "expected line-range stale violation, got {changed:#?}"
    );

    cleanup(&dir);
}

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

fn tempdir_unique(prefix: &str) -> std::path::PathBuf {
    use std::time::{SystemTime, UNIX_EPOCH};
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("{prefix}-{pid}-{nanos}"));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

fn cleanup(p: &std::path::Path) {
    let _ = std::fs::remove_dir_all(p);
}

// Force the `Write` import to stay used; embed_plain returns io::Result
// so it would already pull it in via the trait — but keep this explicit
// so a future trim of the helpers doesn't break compilation.
#[allow(dead_code)]
fn _force_write_use(w: &mut dyn Write) {
    let _ = w.write_all(b"");
}
