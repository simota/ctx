//! HTTP differential-parity harness: Go server (oracle) vs Rust server.
//!
//! Both servers are started on ephemeral loopback ports against the SAME
//! fixture directory, polled for readiness, then hit with identical requests.
//! Each case asserts BYTE-equality of (status, body, Content-Type), with
//! documented carve-outs for non-deterministic fields.
//!
//! ## Readiness wait
//! Each server is polled with `GET /api/file?path=hello.txt` (Rust) or its
//! own readiness probe until it returns 200, up to a 10 s timeout (50 ms
//! interval). The Go server additionally prints its bound URL on stdout,
//! which we parse to discover its ephemeral port.
//!
//! ## Comparison
//! For every [`Case`] we compare HTTP status, response body bytes, and the
//! `Content-Type` header. The `Date` header (Go-only, wall-clock) and any
//! transport framing (`Content-Length`/`Connection`, which both stacks set
//! correctly but format independently) are excluded by construction — we
//! only assert the headers listed in `compare_headers`.
//!
//! ## Carve-outs (documented per ADR-0005)
//!   * `not_found` error messages embed the resolved ABSOLUTE path + an OS
//!     errno string, which differ by machine/sandbox. Such cases set
//!     `Norm::AbsPath`, which replaces the volatile path with a placeholder
//!     on BOTH sides before comparison (the code/status/shape still match
//!     byte-for-byte).
//!   * Replay list/show responses embed absolute store paths; those cases
//!     use `Norm::AbsPathAndStore` which also replaces the store_path.
//!   * No raw-float fields are emitted by the ported routes, so the
//!     ADR-0005 float tolerance is not needed here.
//!
//! ## Adding a route to the matrix
//! Append one [`Case`] to `cases()`. That is the only change required.

use std::io::{BufRead, BufReader, Read};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ----------------------------------------------------------------------------
// Route matrix
// ----------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Norm {
    /// Compare bytes exactly.
    Exact,
    /// Replace the absolute fixture path with `<ROOT>` on both sides first
    /// (handles `not_found` messages that embed a machine-specific path).
    AbsPath,
    /// Replace both the fixture root AND the replay store path with stable
    /// placeholders. Used for replay responses that embed `store_path`.
    AbsPathAndStore,
}

struct Case {
    name: &'static str,
    method: &'static str,
    path: &'static str,
    norm: Norm,
    /// Optional JSON request body (for POST routes). `None` = no body.
    body: Option<&'static str>,
}

impl Case {
    const fn get(name: &'static str, path: &'static str, norm: Norm) -> Self {
        Case {
            name,
            method: "GET",
            path,
            norm,
            body: None,
        }
    }
    const fn head(name: &'static str, path: &'static str, norm: Norm) -> Self {
        Case {
            name,
            method: "HEAD",
            path,
            norm,
            body: None,
        }
    }
    const fn post(name: &'static str, path: &'static str, norm: Norm, body: &'static str) -> Self {
        Case {
            name,
            method: "POST",
            path,
            norm,
            body: Some(body),
        }
    }
    /// A PUT case (no body). Used to exercise genuine default-405 paths where
    /// Go does NOT route the method to a handler (so both servers 405 alike).
    const fn put(name: &'static str, path: &'static str, norm: Norm) -> Self {
        Case {
            name,
            method: "PUT",
            path,
            norm,
            body: None,
        }
    }
}

fn cases() -> Vec<Case> {
    vec![
        // --- /api/tree ---
        // Root tree: all fixture files (no tokens). abs_root is machine-specific
        // so we normalize it. tree.root path is relative (e.g. ".").
        Case::get("tree_root", "/api/tree", Norm::AbsPath),
        // With tokens: both servers call the same token estimator.
        Case::get("tree_root_tokens", "/api/tree?tokens=true", Norm::AbsPath),
        // Traversal rejected.
        Case::get("tree_traversal", "/api/tree?path=../etc", Norm::Exact),
        // Depth-limited subtree.
        Case::get("tree_depth1", "/api/tree?depth=1", Norm::AbsPath),
        // --- /api/dir ---
        // Root dir listing (sorted children + README detection).
        Case::get("dir_root", "/api/dir", Norm::Exact),
        // Explicit root path.
        Case::get("dir_root_dot", "/api/dir?path=.", Norm::Exact),
        // Non-dir path → not_a_dir error.
        Case::get("dir_not_dir", "/api/dir?path=hello.txt", Norm::Exact),
        // Not-found path → 404.
        Case::get("dir_notfound", "/api/dir?path=nosuchdir", Norm::AbsPath),
        // Traversal rejected.
        Case::get("dir_traversal", "/api/dir?path=../etc", Norm::Exact),
        // --- /api/roots ---
        // Both servers read the same pinned registry (CTX_ROOTS_FILE is set
        // before the servers start). Sorted alphabetically (alpha before beta).
        Case::get("roots_list", "/api/roots", Norm::Exact),
        // --- /api/evidence ---
        // Missing path param → bad_request.
        Case::get("evidence_missing_path", "/api/evidence", Norm::Exact),
        // Not-found file → 404.
        Case::get(
            "evidence_notfound",
            "/api/evidence?path=nope.txt",
            Norm::AbsPath,
        ),
        // Directory path → not_a_file.
        Case::get("evidence_isdir", "/api/evidence?path=.", Norm::Exact),
        // Traversal rejected.
        Case::get(
            "evidence_traversal",
            "/api/evidence?path=../etc/x",
            Norm::Exact,
        ),
        // Happy path: hello.txt exists in snap-alpha (sha matches → fresh).
        // store_path is machine-specific → normalize with AbsPathAndStore.
        Case::get(
            "evidence_hello",
            "/api/evidence?path=hello.txt",
            Norm::AbsPathAndStore,
        ),
        // Limit clamping: limit=1 shows only 1 snapshot of 4.
        Case::get(
            "evidence_hello_limit1",
            "/api/evidence?path=hello.txt&limit=1",
            Norm::AbsPathAndStore,
        ),
        // main.go is only in snap-verify, has wrong SHA → stale.
        Case::get(
            "evidence_main_go",
            "/api/evidence?path=main.go",
            Norm::AbsPathAndStore,
        ),
        // --- /api/evidence/verify ---
        // GET rejected (POST only).
        Case::get(
            "evidence_verify_get_rejected",
            "/api/evidence/verify",
            Norm::Exact,
        ),
        // Missing pack field → bad_request.
        Case::post(
            "evidence_verify_missing_pack",
            "/api/evidence/verify",
            Norm::Exact,
            r#"{"pack":"","response":"hello"}"#,
        ),
        // Missing response field → bad_request.
        Case::post(
            "evidence_verify_missing_response",
            "/api/evidence/verify",
            Norm::Exact,
            "{\"pack\":\"# ctx pack\",\"response\":\"\"}",
        ),
        // Pack with no embedded contract → no_contract.
        Case::post(
            "evidence_verify_no_contract",
            "/api/evidence/verify",
            Norm::Exact,
            r#"{"pack":"no contract here","response":"hello"}"#,
        ),
        // --- /api/file: happy paths on non-symbol-bearing files ---
        Case::get("file_txt", "/api/file?path=hello.txt", Norm::Exact),
        Case::get("file_json", "/api/file?path=data.json", Norm::Exact),
        Case::get("file_md", "/api/file?path=notes.md", Norm::Exact),
        Case::get("file_bin", "/api/file?path=raw.bin", Norm::Exact),
        // --- /api/file: error envelopes ---
        Case::get("file_missing", "/api/file", Norm::Exact),
        Case::get("file_notfound", "/api/file?path=nope.txt", Norm::AbsPath),
        Case::get("file_traversal", "/api/file?path=../etc/x", Norm::Exact),
        Case::get("file_isdir", "/api/file?path=.", Norm::Exact),
        // --- /raw/*: static byte serving + headers ---
        Case::get("raw_txt", "/raw/hello.txt", Norm::Exact),
        Case::get("raw_json", "/raw/data.json", Norm::Exact),
        Case::get("raw_bin", "/raw/raw.bin", Norm::Exact),
        Case::head("raw_head", "/raw/hello.txt", Norm::Exact),
        Case::get("raw_missing", "/raw/", Norm::Exact),
        Case::get("raw_notfound", "/raw/nope.txt", Norm::AbsPath),
        Case::get("raw_secret", "/raw/.env", Norm::Exact),
        // --- SPA shell (embedded index.html) ---
        // RETIRED from Go-vs-Rust parity: the Go oracle embeds the SPA frozen
        // at the go-oracle/v1 tag, while the vendored crates/ctx-web/dist
        // evolves with the frontend — GET / stopped being byte-comparable the
        // first time `make web` ran post-tag (same accepted
        // divergence-by-design class as native clap --help). The surviving
        // native invariant is covered by `spa_root_serves_vendored_index_html`
        // below: the compiled-in embed must serve EXACTLY the vendored bytes.
        // --- /api/where ---
        Case::get("where_missing_q", "/api/where", Norm::Exact),
        Case::get(
            "where_no_match",
            "/api/where?q=xyzzy_no_match_at_all",
            Norm::Exact,
        ),
        Case::get(
            "where_traversal",
            "/api/where?q=hello&path=../etc",
            Norm::Exact,
        ),
        // Both Go and Rust search the same fixture dir (non-code files only,
        // no symbols). Scoring is basename + content match for "hello" on
        // hello.txt; both produce score=15, same reason, same match details.
        Case::get("where_hello", "/api/where?q=hello", Norm::Exact),
        // --- /api/relations ---
        Case::get("relations_missing_path", "/api/relations", Norm::Exact),
        Case::get(
            "relations_unsupported",
            "/api/relations?path=hello.txt",
            Norm::Exact,
        ),
        Case::get(
            "relations_traversal",
            "/api/relations?path=../etc/x",
            Norm::Exact,
        ),
        // --- /api/replay/list (store_path is machine-specific → normalized) ---
        Case::get(
            "replay_list_empty",
            "/api/replay/list",
            Norm::AbsPathAndStore,
        ),
        Case::get(
            "replay_list_populated",
            "/api/replay/list",
            Norm::AbsPathAndStore,
        ),
        // --- /api/replay/show ---
        Case::get("replay_show_missing_id", "/api/replay/show", Norm::Exact),
        Case::get(
            "replay_show_invalid_id",
            "/api/replay/show?id=../evil",
            Norm::Exact,
        ),
        Case::get(
            "replay_show_not_found",
            "/api/replay/show?id=does-not-exist",
            Norm::Exact,
        ),
        Case::get(
            "replay_show_snap_alpha",
            "/api/replay/show?id=snap-alpha",
            Norm::Exact,
        ),
        // --- /api/replay/diff ---
        Case::get("replay_diff_missing_id", "/api/replay/diff", Norm::Exact),
        Case::get(
            "replay_diff_not_found",
            "/api/replay/diff?id=does-not-exist",
            Norm::Exact,
        ),
        // Diff against snap-diff: worktree walk + SHA256 + tiktoken counts.
        // snap-diff pins hello.txt (current sha → unchanged), notes.md (wrong
        // sha → modified), ghost.txt (absent → removed); data.json + raw.bin
        // are worktree-only → added. Both servers hash the same files and use
        // the same tiktoken counter (ctx_tokens::count_file == Go CountFile),
        // so the kind/sort/token deltas are byte-identical. No raw floats are
        // emitted by this route, so no FloatTol slot is needed.
        Case::get(
            "replay_diff_snap",
            "/api/replay/diff?id=snap-diff",
            Norm::Exact,
        ),
        Case::get(
            "replay_diff_snap_strict",
            "/api/replay/diff?id=snap-diff&strict=true",
            Norm::Exact,
        ),
        Case::get(
            "replay_diff_snap_limit",
            "/api/replay/diff?id=snap-diff&limit=1",
            Norm::Exact,
        ),
        // --- /api/budget ---
        // Missing budget param → bad_request (budget defaults to 0 which is <= 0).
        Case::get("budget_missing_budget", "/api/budget", Norm::Exact),
        // Explicitly zero budget → bad_request.
        Case::get("budget_zero", "/api/budget?budget=0", Norm::Exact),
        // Traversal → bad_path (before the budget check in Go; in Rust the order
        // is: check budget first, then resolve path — we match Go's order).
        Case::get(
            "budget_traversal",
            "/api/budget?budget=100&path=../etc",
            Norm::Exact,
        ),
        // Large budget: all fixture files (non-binary) fit.
        // Fixture has 5 files; all are text so they all have non-zero tokens.
        Case::get("budget_all_fit", "/api/budget?budget=10000", Norm::Exact),
        // Tiny budget (1 token): only the smallest file may fit; the rest are
        // "budget exceeded". We use budget=1 so even 1-token files hit rule 3
        // (tokens > budget/2 → budget/2=0 → tokens > 0 → "too large"). So
        // ALL files are excluded as "too large". Included is empty [].
        Case::get("budget_none_fit", "/api/budget?budget=1", Norm::Exact),
        // --- /api/tests ---
        // Missing path param → bad_request.
        Case::get("tests_missing_path", "/api/tests", Norm::Exact),
        // Non-existent file → 404 (machine path in message → AbsPath norm).
        Case::get("tests_notfound", "/api/tests?path=nope.go", Norm::AbsPath),
        // Directory path → not_a_file.
        Case::get("tests_isdir", "/api/tests?path=.", Norm::Exact),
        // Traversal rejected.
        Case::get("tests_traversal", "/api/tests?path=../etc/x", Norm::Exact),
        // Non-Go file → kind="" empty tests array.
        Case::get("tests_non_go", "/api/tests?path=hello.txt", Norm::Exact),
        // Source file WITH a real test → TotalTests>0, Tests populated.
        // Anti-escape guard: body must contain the test file path and test count.
        Case::get(
            "tests_source_with_tests",
            "/api/tests?path=gocode/add.go",
            Norm::Exact,
        ),
        // Test file → Sources populated; tests array empty.
        // Anti-escape guard: body must contain the source file path.
        Case::get(
            "tests_test_file_sources",
            "/api/tests?path=gocode/add_test.go",
            Norm::Exact,
        ),
        // limit=1 on a source file that has exactly 1 test (already satisfies limit,
        // confirms limit clamping works without truncating the single result).
        Case::get(
            "tests_limit1",
            "/api/tests?path=gocode/add.go&limit=1",
            Norm::Exact,
        ),
        // REGRESSION LOCK: a real complex file (internal/symbols/extractor.go,
        // copied verbatim into the fixture as symbols/extractor.go). Its
        // matching test (symbols/extractor_test.go) uses a LOCAL variable
        // named `out` 7 times. The previous symbol extractor over-extracted
        // `out` (a func PARAMETER `walkNode(..., out *[]model.Symbol, ...)`),
        // polluting matched_symbols → ["Extract","New","out"] and inflating
        // scores/total_tests. Go's go/ast only surfaces TOP-LEVEL decls, so
        // matched_symbols must be exactly ["Extract","New"] (NO "out").
        // expect_contains guards the real symbols + asserts test_count=5.
        Case::get(
            "tests_extractor_complex",
            "/api/tests?path=symbols/extractor.go",
            Norm::Exact,
        ),
        // REGRESSION LOCK #2 (under-extraction): a real file with a grouped
        // top-level `var ( … )` block (internal/tokens/counter.go, copied
        // verbatim as tokens/counter.go). In tree-sitter-go the grouped var
        // specs are wrapped in a `var_spec_list`, NOT direct children of
        // `var_declaration` — so the top-level walk previously DROPPED
        // `sharedEncoder`/`sharedEncoderErr`/`sharedEncoderOnce`. Go's go/ast
        // surfaces all grouped-block names, so `sharedEncoder` MUST appear in
        // matched_symbols and the perf_test.go cross-file match (on
        // `sharedEncoder`) MUST be present. expect_contains guards both.
        Case::get(
            "tests_counter_grouped_var",
            "/api/tests?path=tokens/counter.go",
            Norm::Exact,
        ),
        // --- /api/replay/verify (POST) ---
        Case::get(
            "replay_verify_get_rejected",
            "/api/replay/verify",
            Norm::Exact,
        ),
        // Validation error envelopes (handler-owned messages, not decoder text).
        Case::post(
            "replay_verify_missing_id",
            "/api/replay/verify",
            Norm::Exact,
            r#"{"id":"","response":"see main.go"}"#,
        ),
        Case::post(
            "replay_verify_missing_response",
            "/api/replay/verify",
            Norm::Exact,
            r#"{"id":"snap-verify","response":""}"#,
        ),
        Case::post(
            "replay_verify_not_found",
            "/api/replay/verify",
            Norm::Exact,
            r#"{"id":"nope-missing","response":"see main.go"}"#,
        ),
        // Happy path: response cites main.go (a recognized code-file ref) which
        // IS in snap-verify's manifest → OK reference, exit_code 0. Reuses
        // ctx_contract::verify byte-for-byte (same logic as CLI contract verify).
        Case::post(
            "replay_verify_ok",
            "/api/replay/verify",
            Norm::Exact,
            r#"{"id":"snap-verify","response":"See main.go for the entrypoint."}"#,
        ),
        // Violation: response cites a code path NOT in the manifest →
        // out-of-context violation, exit_code 1.
        Case::post(
            "replay_verify_violation",
            "/api/replay/verify",
            Norm::Exact,
            r#"{"id":"snap-verify","response":"See missing/phantom.go for the bug."}"#,
        ),
        // Worktree check: cite main.go with check_worktree; snap-verify's pinned
        // sha (all-zeros) differs from the current worktree bytes → stale-content.
        Case::post(
            "replay_verify_stale",
            "/api/replay/verify",
            Norm::Exact,
            r#"{"id":"snap-verify","response":"See main.go.","check_worktree":true}"#,
        ),
        // --- /api/mix (READ side — ported & byte-parity) ---
        // mix list/get are exercised against the pinned fixture in
        // replay-store/ctx/mixes/. Both servers share XDG_STATE_HOME so they
        // read the same store.
        Case::get("mix_list_populated", "/api/mix", Norm::Exact),
        Case::get("mix_get_alpha", "/api/mix/aabbccdd11223344", Norm::Exact),
        Case::get("mix_get_beta", "/api/mix/1234567890abcdef", Norm::Exact),
        Case::get("mix_get_missing", "/api/mix/does-not-exist", Norm::Exact),
        // --- /api/mix unsupported methods (GENUINE default-405 — byte-parity) ---
        // PUT is NOT routed to a mutation in Go: handleMixCollection sends
        // GET→list, POST→create, default→405; handleMixRoute sends GET→get,
        // DELETE→delete, default→405. So PUT hits the default-405 branch on
        // BOTH servers with the SAME envelope + Allow header. This is true
        // byte-parity. (POST-create and DELETE-delete are NOT tested here:
        // Go performs them (201/204) while Rust returns a deliberate 405
        // sentinel — a KNOWN DIVERGENCE, see DEFERRED_ROUTES.md. They are not
        // byte-parity-able because GenerateID is crypto/rand + writes mutate
        // the shared fixture store.)
        Case::put("mix_collection_put_rejected", "/api/mix", Norm::Exact),
        Case::put(
            "mix_item_put_rejected",
            "/api/mix/aabbccdd11223344",
            Norm::Exact,
        ),
        // --- /api/symbols ---
        // Single code file (main.go): extracts the `main` function symbol.
        // Non-trivial guard: body must contain the symbol shape.
        Case::get("symbols_file_go", "/api/symbols?path=main.go", Norm::Exact),
        // Non-code file (hello.txt): files map has key with null value.
        Case::get(
            "symbols_file_txt",
            "/api/symbols?path=hello.txt",
            Norm::Exact,
        ),
        // Directory root: only main.go has symbols; all other fixture files
        // (hello.txt, notes.md, data.json, raw.bin) are non-code → skipped.
        Case::get("symbols_dir_root", "/api/symbols", Norm::Exact),
        // Not-found path → 404 (machine path in message → AbsPath norm).
        Case::get(
            "symbols_notfound",
            "/api/symbols?path=nope.go",
            Norm::AbsPath,
        ),
        // Traversal rejected.
        Case::get(
            "symbols_traversal",
            "/api/symbols?path=../etc/x",
            Norm::Exact,
        ),
        // --- /api/definition ---
        // Missing name param → bad_request.
        Case::get("definition_missing_name", "/api/definition", Norm::Exact),
        // Known symbol `main` in main.go — entry file, enriched with role+tokens.
        Case::get("definition_main", "/api/definition?name=main", Norm::Exact),
        // Same symbol with from hint (same dir → same ranking, deterministic).
        Case::get(
            "definition_main_from",
            "/api/definition?name=main&from=main.go",
            Norm::Exact,
        ),
        // Unknown symbol name → 200 with empty candidates array.
        Case::get(
            "definition_no_match",
            "/api/definition?name=Xyzzy_NoSuchSymbol",
            Norm::Exact,
        ),
        // --- /api/file: symbols field now populated for code files ---
        // main.go is a Go file with one function → symbols list present.
        Case::get("file_go_symbols", "/api/file?path=main.go", Norm::Exact),
    ]
}

/// Substrings the Go response body MUST contain for the named case. This is an
/// anti-escape guard: byte-equality alone would pass even if BOTH servers
/// returned an empty/error body, so for cases that should produce meaningful
/// data we additionally assert the shape is non-trivial.
fn expect_contains(name: &str) -> &'static [&'static str] {
    match name {
        // tree_root: must include the root node and at least one known file.
        "tree_root" => &[r#""root":"."#, r#""total":"#, r#""name":"hello.txt""#],
        "tree_root_tokens" => &[r#""tokens":"#, r#""name":"hello.txt""#],
        "tree_depth1" => &[r#""root":"."#, r#""total":"#],
        // dir_root: must list the known children sorted dirs-first.
        "dir_root" => &[
            r#""path":"."#,
            r#""file_count":"#,
            r#""name":"hello.txt""#,
            r#""children":"#,
        ],
        "dir_root_dot" => &[r#""path":"."#, r#""children":"#],
        // roots_list: alpha < beta (sorted); both entries present.
        "roots_list" => &[r#""roots":"#, r#""name":"alpha""#, r#""name":"beta""#],
        // evidence_hello: snap-alpha includes hello.txt.
        "evidence_hello" => &[
            r#""path":"hello.txt""#,
            r#""total_snapshots":"#,
            r#""snapshots":"#,
            r#""id":"snap-alpha""#,
        ],
        // limit=1 returns only the NEWEST snapshot with hello.txt (snap-diff, 2026-01-03).
        "evidence_hello_limit1" => &[
            r#""path":"hello.txt""#,
            r#""total_snapshots":3"#,
            r#""id":"snap-diff""#,
        ],
        // evidence_main_go: snap-verify has main.go with wrong sha → stale.
        "evidence_main_go" => &[
            r#""path":"main.go""#,
            r#""status":"stale""#,
            r#""id":"snap-verify""#,
        ],
        // budget_all_fit: all fixture files have non-zero tokens, so included is non-empty.
        // main.go (entry) is highest-priority so it appears in included.
        "budget_all_fit" => &[r#""budget":10000"#, r#""included":"#, r#""path":"main.go""#],
        // budget_none_fit: budget=1, all files excluded as "too large" (tokens > budget/2=0).
        "budget_none_fit" => &[r#""budget":1"#, r#""included":[]"#, r#""excluded":"#],
        // where_hello must rank hello.txt with a content+basename match.
        "where_hello" => &[
            r#""path":"hello.txt""#,
            r#""score":15"#,
            r#""type":"content""#,
        ],
        // diff against snap-diff: notes.md modified, ghost.txt removed,
        // data.json/raw.bin added, hello.txt unchanged (counted, not listed).
        "replay_diff_snap" => &[
            r#""snapshot_id":"snap-diff""#,
            r#""kind":"modified""#,
            r#""kind":"removed""#,
            r#""kind":"added""#,
            r#""unchanged_count":1"#,
        ],
        "replay_diff_snap_strict" => &[r#""strict":true"#, r#""kind":"modified""#],
        "replay_diff_snap_limit" => &[r#""truncated":true"#],
        // verify happy path: an OK reference to main.go, exit 0.
        "replay_verify_ok" => &[
            r#""pack_file":"replay:snap-verify""#,
            r#""ok":[{"#,
            r#""path":"main.go""#,
            r#""exit_code":0"#,
        ],
        // verify violation: out-of-context phantom path, exit 1.
        "replay_verify_violation" => &[
            r#""violations":[{"#,
            r#""out-of-context""#,
            r#""exit_code":1"#,
        ],
        // verify stale: worktree bytes differ from pinned sha.
        "replay_verify_stale" => &[r#""stale_files":[{"#, r#""exit_code":1"#],
        // mix list populated: both pinned entries present, newest-first (beta > alpha).
        "mix_list_populated" => &[
            r#""mixes":"#,
            r#""id":"1234567890abcdef""#,
            r#""id":"aabbccdd11223344""#,
            r#""file_count":1"#,
            r#""file_count":2"#,
        ],
        // mix get alpha: full record including goal + 2 files.
        "mix_get_alpha" => &[
            r#""id":"aabbccdd11223344""#,
            r#""goal":"parity test alpha mix""#,
            r#""files":["hello.txt","notes.md"]"#,
            r#""budget":{}"#,
        ],
        // mix get beta: full record, no goal (omitted), 1 file.
        "mix_get_beta" => &[
            r#""id":"1234567890abcdef""#,
            r#""name":"Beta Mix""#,
            r#""files":["data.json"]"#,
            r#""budget":{}"#,
        ],
        // mix unsupported-method 405 envelopes: genuine default-405 on BOTH
        // servers (PUT is not routed to any mutation in Go).
        "mix_collection_put_rejected" => {
            &[r#""code":"method_not_allowed""#, r#""GET or POST only""#]
        }
        "mix_item_put_rejected" => &[r#""code":"method_not_allowed""#, r#""GET or DELETE only""#],
        // symbols: file case must include real symbol shape.
        "symbols_file_go" => &[
            r#""path":"main.go""#,
            r#""files":{"main.go""#,
            r#""name":"main""#,
            r#""kind":"function""#,
            r#""line":3"#,
        ],
        // symbols: non-code file must have null value for its key.
        "symbols_file_txt" => &[r#""path":"hello.txt""#, r#""files":{"hello.txt":null}"#],
        // symbols: directory walk must find main.go's symbol.
        "symbols_dir_root" => &[
            r#""path":".""#,
            r#""main.go""#,
            r#""name":"main""#,
            r#""kind":"function""#,
        ],
        // definition: named symbol with enrichment fields.
        "definition_main" => &[
            r#""name":"main""#,
            r#""candidates":"#,
            r#""path":"main.go""#,
            r#""kind":"function""#,
            r#""symbol_name":"main""#,
            r#""file_role":"entry""#,
        ],
        "definition_main_from" => &[
            r#""name":"main""#,
            r#""path":"main.go""#,
            r#""file_role":"entry""#,
        ],
        // definition: unknown symbol → empty candidates array (not null).
        "definition_no_match" => &[r#""name":"Xyzzy_NoSuchSymbol""#, r#""candidates":[]"#],
        // file: main.go symbols list populated by tree-sitter.
        "file_go_symbols" => &[
            r#""path":"main.go""#,
            r#""symbols":"#,
            r#""name":"main""#,
            r#""kind":"function""#,
        ],
        // tests: source file with real tests — TotalTests>0, populated Tests array.
        // This guard prevents the "both-empty false PASS" trap. The symbol name
        // is unique across the fixture so only gocode/add_test.go matches.
        "tests_source_with_tests" => &[
            r#""path":"gocode/add.go""#,
            r#""kind":"go""#,
            r#""tests":"#,
            r#""path":"gocode/add_test.go""#,
            r#""test_count":1"#,
            r#""matched_symbols":["GocodeUniqueSum"]"#,
            r#""total_tests":1"#,
        ],
        // tests: test file → Sources populated with add.go.
        "tests_test_file_sources" => &[
            r#""path":"gocode/add_test.go""#,
            r#""kind":"go""#,
            r#""sources":"#,
            r#""path":"gocode/add.go""#,
        ],
        // tests_limit1: same as source_with_tests but with limit=1.
        "tests_limit1" => &[
            r#""path":"gocode/add.go""#,
            r#""tests":"#,
            r#""total_tests":1"#,
        ],
        // tests_extractor_complex: REGRESSION LOCK for the `out` over-extraction.
        // matched_symbols MUST be exactly ["Extract","New"] (top-level only) —
        // NOT include "out" (a func parameter / test local). Guards real data
        // (test_count=5 from extractor_test.go's 5 Test funcs) so a both-empty
        // false PASS is impossible.
        "tests_extractor_complex" => &[
            r#""path":"symbols/extractor.go""#,
            r#""kind":"go""#,
            r#""path":"symbols/extractor_test.go""#,
            r#""test_count":5"#,
            r#""matched_symbols":["Extract","New"]"#,
            r#""total_tests":1"#,
        ],
        // tests_counter_grouped_var: REGRESSION LOCK for grouped `var ( … )`
        // under-extraction. `sharedEncoder` (a grouped-block top-level var)
        // MUST appear in counter_test.go's matched_symbols (sorted byte-order:
        // uppercase C/N before lowercase s) AND drive the perf_test.go
        // cross-file match (matched on "sharedEncoder" alone). With the bug,
        // sharedEncoder was dropped → perf_test.go would not match and
        // counter_test.go's matched_symbols would miss it.
        "tests_counter_grouped_var" => &[
            r#""path":"tokens/counter.go""#,
            r#""kind":"go""#,
            r#""path":"tokens/counter_test.go""#,
            r#""matched_symbols":["CountString","NewTiktokenCounter","sharedEncoder"]"#,
            r#""path":"tokens/perf_test.go""#,
            r#""matched_symbols":["sharedEncoder"]"#,
            r#""total_tests":2"#,
        ],
        _ => &[],
    }
}

/// Headers compared for byte-equality. Excludes `Date` (wall-clock) and
/// transport framing the two stacks format independently.
const COMPARE_HEADERS: &[&str] = &[
    "content-type",
    "content-security-policy",
    "x-content-type-options",
    "x-frame-options",
    "referrer-policy",
    "cache-control",
    "allow",
];

// ----------------------------------------------------------------------------
// Test entry point
// ----------------------------------------------------------------------------

#[test]
fn http_byte_parity_go_vs_rust() {
    let fixture = fixture_dir();
    // Replay store lives OUTSIDE the served fixture root (sibling dir) so the
    // /api/replay/diff worktree walk does not pick up the snapshot JSON files.
    // XDG_STATE_HOME is set to this dir so both servers read the same manifests.
    let replay_store = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/replay-store");

    // Pinned roots registry — both servers read the same file via CTX_ROOTS_FILE
    // so /api/roots output is deterministic and machine-independent.
    let roots_file = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/roots-registry/roots.toml");

    // Set env vars before spawning any threads (process-global, safe here since
    // no other threads are running at this point in the test).
    // SAFETY: single-threaded at this point; no other threads running.
    #[allow(deprecated)]
    unsafe {
        std::env::set_var("XDG_STATE_HOME", &replay_store);
        std::env::set_var("CTX_ROOTS_FILE", &roots_file);
    }

    let go_bin = match locate_go_binary() {
        Some(b) => b,
        None => {
            eprintln!(
                "SKIP: Go oracle binary not found and `go build` unavailable. \
                 Set CTX_GO_BIN or run `go build -o /tmp/ctx-go ./cmd/ctx`."
            );
            return;
        }
    };

    let mut go = GoServer::start(&go_bin, &fixture, &replay_store, &roots_file);
    let rust = RustServer::start(&fixture);

    go.wait_ready();
    rust.wait_ready();

    let go_base = format!("http://{}", go.addr);
    let rust_base = format!("http://127.0.0.1:{}", rust.port);

    let abs_root = std::fs::canonicalize(&fixture).unwrap();
    let abs_root = abs_root.to_string_lossy().to_string();

    let abs_store = std::fs::canonicalize(&replay_store).unwrap_or_else(|_| replay_store.clone());
    let abs_store = abs_store.to_string_lossy().to_string();
    // Store path used by both servers: XDG_STATE_HOME/ctx/replay
    let store_path = format!("{abs_store}/ctx/replay");

    let mut failures = Vec::new();
    let all = cases();
    for case in &all {
        let g = http_request(&go_base, case.method, case.path, case.body);
        let r = http_request(&rust_base, case.method, case.path, case.body);

        let gb = normalize(&g.body, case.norm, &abs_root, &store_path);
        let rb = normalize(&r.body, case.norm, &abs_root, &store_path);

        let mut why = Vec::new();
        if g.status != r.status {
            why.push(format!("status go={} rust={}", g.status, r.status));
        }
        if gb != rb {
            why.push(format!(
                "body go={:?} rust={:?}",
                String::from_utf8_lossy(&gb),
                String::from_utf8_lossy(&rb)
            ));
        }
        for h in COMPARE_HEADERS {
            let gv = g.header(h);
            let rv = r.header(h);
            if gv != rv {
                why.push(format!("header {h} go={gv:?} rust={rv:?}"));
            }
        }
        // Anti-escape guard: for cases that MUST produce non-trivial output,
        // assert the (already-equal) Go body contains the expected substrings.
        // This prevents a "both servers return the same error/empty" false PASS.
        for needle in expect_contains(case.name) {
            let body = String::from_utf8_lossy(&g.body);
            if !body.contains(needle) {
                why.push(format!("guard: body missing {needle:?} (got {body:?})"));
            }
        }
        if why.is_empty() {
            eprintln!("PASS {}", case.name);
        } else {
            eprintln!("FAIL {}: {}", case.name, why.join("; "));
            failures.push(case.name);
        }
    }

    go.kill();
    rust.shutdown();

    assert!(
        failures.is_empty(),
        "{}/{} parity cases failed: {:?}",
        failures.len(),
        all.len(),
        failures
    );
    eprintln!("ALL {} parity cases byte-identical", all.len());
}

// ----------------------------------------------------------------------------
// Normalization
// ----------------------------------------------------------------------------

fn normalize(body: &[u8], norm: Norm, abs_root: &str, store_path: &str) -> Vec<u8> {
    match norm {
        Norm::Exact => body.to_vec(),
        Norm::AbsPath => {
            // Replace the resolved fixture root (and the macOS /private prefix
            // variant) with a stable placeholder so the message shape is
            // compared without the machine-specific absolute path.
            let s = String::from_utf8_lossy(body);
            let s = s.replace(abs_root, "<ROOT>");
            let private = format!("/private{abs_root}");
            let s = s.replace(&private, "<ROOT>");
            s.into_bytes()
        }
        Norm::AbsPathAndStore => {
            // Replace both the fixture root and the replay store path so that
            // machine-specific absolute paths don't break comparison.
            let s = String::from_utf8_lossy(body);
            let s = s.replace(store_path, "<STORE>");
            let private_store = format!("/private{store_path}");
            let s = s.replace(&private_store, "<STORE>");
            let s = s.replace(abs_root, "<ROOT>");
            let private = format!("/private{abs_root}");
            let s = s.replace(&private, "<ROOT>");
            s.into_bytes()
        }
    }
}

// ----------------------------------------------------------------------------
// Minimal blocking HTTP client (no external dep needed for these requests)
// ----------------------------------------------------------------------------

struct HttpResponse {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
}

impl HttpResponse {
    fn header(&self, name: &str) -> Option<String> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    }
}

/// Tiny HTTP/1.1 client: connects, sends the request, reads the full response.
/// Sufficient for loopback GET/HEAD/POST against both servers. An optional
/// `body` (JSON) is sent with a `Content-Type: application/json` header and a
/// `Content-Length`.
fn http_request(base: &str, method: &str, path: &str, body: Option<&str>) -> HttpResponse {
    let hostport = base.trim_start_matches("http://");
    let mut stream = TcpStream::connect(hostport).expect("connect");
    stream
        .set_read_timeout(Some(Duration::from_secs(5)))
        .unwrap();
    let req = match body {
        Some(b) => format!(
            "{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\
             Content-Type: application/json\r\nContent-Length: {}\r\n\r\n{b}",
            b.len()
        ),
        None => {
            format!("{method} {path} HTTP/1.1\r\nHost: {hostport}\r\nConnection: close\r\n\r\n")
        }
    };
    use std::io::Write;
    stream.write_all(req.as_bytes()).unwrap();

    let mut raw = Vec::new();
    stream.read_to_end(&mut raw).unwrap();
    parse_response(&raw, method == "HEAD")
}

fn parse_response(raw: &[u8], head: bool) -> HttpResponse {
    let split = find_subslice(raw, b"\r\n\r\n").expect("header/body boundary");
    let header_block = &raw[..split];
    let raw_body = &raw[split + 4..];
    let mut lines = header_block.split(|&b| b == b'\n');
    let status_line = String::from_utf8_lossy(lines.next().unwrap_or(b""));
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse::<u16>().ok())
        .unwrap_or(0);
    let mut headers = Vec::new();
    let mut chunked = false;
    for line in lines {
        let line = String::from_utf8_lossy(line);
        let line = line.trim_end_matches('\r');
        if let Some((k, v)) = line.split_once(':') {
            let (k, v) = (k.trim().to_string(), v.trim().to_string());
            if k.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked") {
                chunked = true;
            }
            headers.push((k, v));
        }
    }
    // Go switches large responses (no Content-Length) to HTTP/1.1 chunked
    // transfer encoding; Rust/axum sends them with Content-Length. Chunked
    // framing is transport detail this harness explicitly ignores (see module
    // docs), so de-chunk before comparing bodies — otherwise the chunk-size
    // hex prefixes (`833\r\n…\r\n0\r\n\r\n`) would make a byte-identical body
    // compare unequal.
    let body = if chunked {
        dechunk(raw_body)
    } else {
        raw_body.to_vec()
    };
    HttpResponse {
        status,
        headers,
        body: if head { Vec::new() } else { body },
    }
}

/// Decode an HTTP/1.1 chunked transfer-encoded body into the raw bytes.
/// Each chunk is `<hex-size>\r\n<data>\r\n`, terminated by a `0\r\n\r\n`.
/// Chunk extensions (`;name=val`) are ignored (split on `;`).
fn dechunk(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < body.len() {
        // Read the chunk-size line up to CRLF.
        let line_end = match find_subslice(&body[pos..], b"\r\n") {
            Some(i) => pos + i,
            None => break,
        };
        let size_line = &body[pos..line_end];
        // Strip any chunk extension after ';'.
        let size_hex = size_line.split(|&b| b == b';').next().unwrap_or(size_line);
        let size_str = String::from_utf8_lossy(size_hex);
        let size = match usize::from_str_radix(size_str.trim(), 16) {
            Ok(n) => n,
            Err(_) => break,
        };
        pos = line_end + 2; // skip CRLF after size line
        if size == 0 {
            break; // last chunk
        }
        if pos + size > body.len() {
            out.extend_from_slice(&body[pos..]);
            break;
        }
        out.extend_from_slice(&body[pos..pos + size]);
        pos += size + 2; // skip chunk data + trailing CRLF
    }
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ----------------------------------------------------------------------------
// Go server lifecycle
// ----------------------------------------------------------------------------

struct GoServer {
    child: Child,
    addr: String,
}

impl GoServer {
    fn start(bin: &Path, fixture: &Path, replay_store: &Path, roots_file: &Path) -> Self {
        let mut child = Command::new(bin)
            .arg("browse")
            .arg(fixture)
            .arg("--no-open")
            .arg("--port")
            .arg("0")
            .arg("--bind")
            .arg("127.0.0.1")
            .arg("--no-register")
            // Set XDG_STATE_HOME so the Go server reads from the same
            // replay store as the Rust server.
            .env("XDG_STATE_HOME", replay_store)
            // Set CTX_ROOTS_FILE so the Go server reads from the same
            // pinned roots registry as the Rust server.
            .env("CTX_ROOTS_FILE", roots_file)
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn go server");

        // Parse the bound URL from the child's stdout line:
        //   "ctx browse: serving <dir> at http://127.0.0.1:PORT/"
        let stdout = child.stdout.take().expect("go stdout");
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines().map_while(Result::ok) {
                if let Some(idx) = line.find("http://") {
                    let url = line[idx..].trim_end_matches(['.', ',', ';', ':']);
                    let addr = url
                        .trim_start_matches("http://")
                        .trim_end_matches('/')
                        .to_string();
                    let _ = tx.send(addr);
                    break;
                }
            }
        });
        let addr = rx
            .recv_timeout(Duration::from_secs(10))
            .expect("go server emit URL");
        GoServer { child, addr }
    }

    fn wait_ready(&self) {
        wait_port(&self.addr);
    }

    fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

// ----------------------------------------------------------------------------
// Rust server lifecycle (in-process, background tokio runtime)
// ----------------------------------------------------------------------------

struct RustServer {
    port: u16,
    shutdown_tx: Option<tokio::sync::oneshot::Sender<()>>,
    handle: Option<thread::JoinHandle<()>>,
}

impl RustServer {
    fn start(fixture: &Path) -> Self {
        let root = fixture.to_string_lossy().to_string();
        let (port_tx, port_rx) = mpsc::channel();
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();

        let handle = thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(async move {
                let mut server = ctx_web::Server::new(root, "127.0.0.1:0", false);
                server.listen().await.unwrap();
                let port = server.addr().unwrap().port();
                port_tx.send(port).unwrap();
                let _ = server
                    .serve(async move {
                        let _ = shutdown_rx.await;
                    })
                    .await;
            });
        });

        let port = port_rx
            .recv_timeout(Duration::from_secs(10))
            .expect("rust server bind");
        RustServer {
            port,
            shutdown_tx: Some(shutdown_tx),
            handle: Some(handle),
        }
    }

    fn wait_ready(&self) {
        wait_port(&format!("127.0.0.1:{}", self.port));
    }

    fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

// ----------------------------------------------------------------------------
// Shared helpers
// ----------------------------------------------------------------------------

/// Poll a TCP port until it accepts a connection, up to 10 s (50 ms interval).
fn wait_port(hostport: &str) {
    let deadline = Instant::now() + Duration::from_secs(10);
    loop {
        if TcpStream::connect(hostport).is_ok() {
            return;
        }
        if Instant::now() >= deadline {
            panic!("server at {hostport} not ready within timeout");
        }
        thread::sleep(Duration::from_millis(50));
    }
}

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

/// Locate the Go oracle binary: `$CTX_GO_BIN`, else `/tmp/ctx-go`, else build
/// it via `go build` into the crate's target dir.
fn locate_go_binary() -> Option<PathBuf> {
    if let Ok(p) = std::env::var("CTX_GO_BIN") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    let tmp = PathBuf::from("/tmp/ctx-go");
    if tmp.exists() {
        return Some(tmp);
    }
    // Wave 4 (ADR-0005): build the FROZEN oracle from the `go-oracle/v1` tag
    // (not the working tree) via ci/build-go-oracle.sh, so the parity gate
    // survives Go deletion. Its cmd/ctx is byte-identical to the pre-deletion tree.
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)?
        .to_path_buf();
    let out = Command::new("bash")
        .arg(repo_root.join("ci/build-go-oracle.sh"))
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let bin = PathBuf::from(String::from_utf8_lossy(&out.stdout).trim().to_string());
    if bin.exists() {
        Some(bin)
    } else {
        None
    }
}

// Native replacement for the retired `spa_root` Go-parity case (see the
// comment in `cases()`): the SPA shell can no longer be byte-compared against
// the frozen Go oracle once the frontend evolves, but the embed must always
// serve EXACTLY the vendored crates/ctx-web/dist/index.html bytes — a stale
// or partial `make web` would otherwise ship silently.
#[test]
fn spa_root_serves_vendored_index_html() {
    let fixture = fixture_dir();
    let rust = RustServer::start(&fixture);
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);
    let resp = http_request(&base, "GET", "/", None);
    let expected = std::fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join("dist/index.html"))
        .expect("read vendored dist/index.html");
    assert_eq!(resp.status, 200, "GET / status");
    assert_eq!(
        resp.body, expected,
        "GET / must serve the vendored dist/index.html bytes"
    );
    let ct = resp.header("content-type").unwrap_or_default();
    assert!(ct.starts_with("text/html"), "content-type: {ct:?}");
    rust.shutdown();
}

#[test]
fn native_where_all_requires_every_query_term() {
    let fixture = fixture_dir();
    let rust = RustServer::start(&fixture);
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);

    let broad = http_request(&base, "GET", "/api/where?q=hello%20markdown", None);
    assert_eq!(broad.status, 200, "broad search status");
    let broad_body = String::from_utf8_lossy(&broad.body);
    assert!(
        broad_body.contains(r#""path":"hello.txt""#) && broad_body.contains(r#""path":"notes.md""#),
        "default OR search should include one-term matches; body:\n{broad_body}"
    );

    let strict = http_request(&base, "GET", "/api/where?q=hello%20markdown&all=true", None);
    assert_eq!(strict.status, 200, "strict search status");
    let strict_body = String::from_utf8_lossy(&strict.body);
    assert!(
        strict_body.contains(r#""results":[]"#),
        "all=true should exclude files that only match one query term; body:\n{strict_body}"
    );
    assert!(
        !strict_body.contains(r#""path":"hello.txt""#),
        "hello.txt must be excluded"
    );
    assert!(
        !strict_body.contains(r#""path":"notes.md""#),
        "notes.md must be excluded"
    );

    rust.shutdown();
}

#[test]
fn native_where_literal_filters_mixed_ascii_cjk() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let fixture = std::env::temp_dir().join(format!("ctx-web-where-literal-{unique}"));
    std::fs::create_dir_all(fixture.join("docs")).expect("create fixture");
    std::fs::write(fixture.join("docs/normalized.md"), "ab テスト\n").expect("write fixture");
    std::fs::write(fixture.join("docs/exact.md"), "ABテスト rollout\n").expect("write fixture");

    let rust = RustServer::start(&fixture);
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);

    let resp = http_request(
        &base,
        "GET",
        "/api/where?q=AB%E3%83%86%E3%82%B9%E3%83%88&literal=AB%E3%83%86%E3%82%B9%E3%83%88",
        None,
    );
    assert_eq!(resp.status, 200, "literal search status");
    let body = String::from_utf8_lossy(&resp.body);
    assert!(
        body.contains(r#""path":"docs/exact.md""#),
        "literal search should include exact content match; body:\n{body}"
    );
    assert!(
        !body.contains(r#""path":"docs/normalized.md""#),
        "literal search should exclude normalized-only token matches; body:\n{body}"
    );

    rust.shutdown();
    let _ = std::fs::remove_dir_all(&fixture);
}

#[test]
fn native_where_uses_tree_sitter_symbols_for_symbol_matches() {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("clock after epoch")
        .as_nanos();
    let fixture = std::env::temp_dir().join(format!("ctx-web-symbols-{unique}"));
    std::fs::create_dir_all(fixture.join("src")).expect("create fixture");
    std::fs::write(
        fixture.join("src/lib.rs"),
        "const CacheLimit: usize = 128;\n\nfn helper() {}\n",
    )
    .expect("write fixture");

    let rust = RustServer::start(&fixture);
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);

    let resp = http_request(&base, "GET", "/api/where?q=CacheLimit", None);
    assert_eq!(resp.status, 200, "symbol search status");
    let body = String::from_utf8_lossy(&resp.body);
    assert!(
        body.contains(r#""path":"src/lib.rs""#),
        "symbol search should include the Rust source file; body:\n{body}"
    );
    assert!(
        body.contains(r#""reason":"symbol match: CacheLimit"#),
        "where search should classify the const declaration as a symbol match, not just content; body:\n{body}"
    );
    assert!(
        body.contains(r#""type":"symbol""#),
        "symbol match should include a symbol-typed match entry; body:\n{body}"
    );

    rust.shutdown();
    let _ = std::fs::remove_dir_all(&fixture);
}
