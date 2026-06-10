mod common;
use common::*;

#[test]
fn native_where_regex_vimgrep_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(
        &root,
        &["where", "--regex", "Helper", "--format", "vimgrep"],
    );
    assert_delegated_parity_in(&root, &["where", "--regex=Run", "--format=vimgrep"]);
}

#[test]
fn native_where_symbol_query_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "Run", "--format", "vimgrep"]);
    assert_delegated_parity_in(&root, &["where", "Helper", "--format=json"]);
}

/// Byte-parity for `where` default format (includes score + breakdown line).
#[test]
fn native_where_default_format_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "Run"]);
    assert_delegated_parity_in(&root, &["where", "Helper"]);
    // format=default explicit
    assert_delegated_parity_in(&root, &["where", "Run", "--format", "default"]);
}

/// Byte-parity for `where --plain` format across symbol and text queries.
#[test]
fn native_where_plain_format_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "Run", "--plain"]);
    assert_delegated_parity_in(&root, &["where", "Helper", "--plain"]);
}

/// Byte-parity for `where --limit N` across all formats.
#[test]
fn native_where_limit_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "Run", "--limit", "1"]);
    assert_delegated_parity_in(&root, &["where", "Run", "--limit", "1", "--format", "json"]);
    assert_delegated_parity_in(
        &root,
        &["where", "Run", "--limit", "1", "--format", "vimgrep"],
    );
    assert_delegated_parity_in(&root, &["where", "Run", "--limit=1", "--plain"]);
}

/// Byte-parity for `where --all` (AND semantics) across formats.
#[test]
fn native_where_all_flag_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "Run", "--all"]);
    assert_delegated_parity_in(&root, &["where", "Run", "--all", "--format", "json"]);
    assert_delegated_parity_in(
        &root,
        &["where", "Run Helper", "--all", "--format", "vimgrep"],
    );
}

/// Byte-parity for `where --context N` (context lines in default format).
#[test]
fn native_where_context_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "Run", "--context", "1"]);
    assert_delegated_parity_in(&root, &["where", "Helper", "--context", "2"]);
    // context with json (context lines are embedded in match objects)
    assert_delegated_parity_in(
        &root,
        &["where", "Run", "--context", "1", "--format", "json"],
    );
    // context with vimgrep (vimgrep format ignores context)
    assert_delegated_parity_in(
        &root,
        &["where", "Run", "--context", "1", "--format", "vimgrep"],
    );
    // context with plain (plain format ignores context)
    assert_delegated_parity_in(&root, &["where", "Run", "--context", "1", "--plain"]);
}

/// Byte-parity for `where --no-suggest` suppressing did-you-mean on zero results.
/// Also validates zero-result JSON emits `null` (Go nil-slice → null) and
/// zero-result explain JSON emits `{"results":null}`.
#[test]
fn native_where_no_suggest_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "nonexistentxyzzy", "--no-suggest"]);
    assert_delegated_parity_in(&root, &["where", "nonexistentxyzzy"]);
    // Go emits `null` for empty JSON result (nil slice), not `[]`
    assert_delegated_parity_in(&root, &["where", "nonexistentxyzzy", "--format", "json"]);
    // Go emits `{"results":null}` for empty JSON+explain result
    assert_delegated_parity_in(
        &root,
        &["where", "nonexistentxyzzy", "--format", "json", "--explain"],
    );
    assert_delegated_parity_in(&root, &["where", "nonexistentxyzzy", "--plain"]);
}

/// Byte-parity for `where --explain` across default, json, vimgrep, and plain formats.
/// The explain path emits an ExpandedKeywords header in default format and wraps
/// the json output in a { expanded_keywords, results } envelope.
#[test]
fn native_where_explain_matches_go() {
    let root = write_where_fixture();
    // default format with explain header
    assert_delegated_parity_in(&root, &["where", "Run", "--explain"]);
    // json wraps in envelope
    assert_delegated_parity_in(&root, &["where", "Run", "--explain", "--format", "json"]);
    // vimgrep ignores explain
    assert_delegated_parity_in(&root, &["where", "Run", "--explain", "--format", "vimgrep"]);
    // plain ignores explain (no synonyms in fixture)
    assert_delegated_parity_in(&root, &["where", "Run", "--explain", "--plain"]);
}

/// Byte-parity for `where --regex` (regex search mode) across all formats.
#[test]
fn native_where_regex_all_formats_matches_go() {
    let root = write_where_fixture();
    // default format with regex
    assert_delegated_parity_in(&root, &["where", "--regex", "Helper"]);
    assert_delegated_parity_in(&root, &["where", "--regex", "Run"]);
    // json
    assert_delegated_parity_in(&root, &["where", "--regex", "Helper", "--format", "json"]);
    // plain
    assert_delegated_parity_in(&root, &["where", "--regex", "Run", "--plain"]);
    // with limit
    assert_delegated_parity_in(&root, &["where", "--regex", "func", "--limit", "1"]);
}

/// Byte-parity for `where --where-engine go` (the go engine selector is a no-op in Rust).
/// Note: --where-engine rust is a documented carve-out (Go exits 1 "requires -tags
/// rust_contract", Rust runs native exits 0) — NOT tested here.
#[test]
fn native_where_engine_go_matches_go() {
    let root = write_where_fixture();
    assert_delegated_parity_in(&root, &["where", "Run", "--where-engine", "go"]);
    assert_delegated_parity_in(
        &root,
        &["where", "Run", "--where-engine", "go", "--format", "json"],
    );
}

/// Byte-parity for `where` positional query × format cross-product
/// covering symbol search, text search, and regex search.
#[test]
fn native_where_full_flag_matrix_matches_go() {
    let root = write_where_fixture();

    // symbol search × all formats
    for format in &["default", "json", "vimgrep"] {
        assert_delegated_parity_in(&root, &["where", "Run", "--format", format]);
        assert_delegated_parity_in(&root, &["where", "Helper", "--format", format]);
    }
    assert_delegated_parity_in(&root, &["where", "Run", "--plain"]);

    // regex × all formats
    for format in &["default", "json", "vimgrep"] {
        assert_delegated_parity_in(&root, &["where", "--regex", "func", "--format", format]);
    }
    assert_delegated_parity_in(&root, &["where", "--regex", "func", "--plain"]);

    // explain × default and json (two paths with structural differences)
    assert_delegated_parity_in(&root, &["where", "Run", "--explain"]);
    assert_delegated_parity_in(&root, &["where", "Run", "--explain", "--format", "json"]);

    // limit + all + context combinations
    assert_delegated_parity_in(&root, &["where", "Run", "--limit", "1", "--all"]);
    assert_delegated_parity_in(&root, &["where", "Run", "--context", "1", "--limit", "1"]);
    assert_delegated_parity_in(&root, &["where", "Run", "--no-suggest", "--format", "json"]);
}
