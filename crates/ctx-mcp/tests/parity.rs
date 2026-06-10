//! MCP stdio differential-parity harness: Go server (oracle) vs Rust server.
//!
//! This is the IMMUTABLE byte-parity ORACLE for the Go→Rust MCP migration
//! (ADR-0005 Wave 2). It is deliberately STRICT and DISCRIMINATING: it boots
//! BOTH the frozen Go server (`ctx mcp serve`) and the incomplete Rust server
//! (`ctx-mcp`) as stdio subprocesses, sends the SAME JSON-RPC request frame(s)
//! to each, reads the response frame(s) back, and asserts the responses are
//! BYTE-IDENTICAL after a narrow, documented normalization for genuinely
//! non-deterministic fields only.
//!
//! ## Wire framing (replicated from internal/mcp/server.go)
//! The Go server reads requests with `bufio.Scanner` (newline-delimited) and
//! writes each response with `json.Encoder.Encode`, which appends a single
//! `\n`. The transport is therefore NDJSON: one compact JSON object per line,
//! request AND response. There is NO LSP-style `Content-Length` framing. This
//! harness replicates that exactly — it writes each request object followed by
//! `\n` to the child's stdin, and reads exactly one `\n`-terminated line per
//! expected response from the child's stdout.
//!
//! Two further framing facts matter for byte-parity:
//!   * Go's `encoding/json` marshals `map[string]any` with keys sorted
//!     alphabetically. serde_json (as resolved in this crate's dependency
//!     graph, with `preserve_order` enabled transitively) does NOT sort —
//!     it preserves insertion order. That difference is a REAL parity defect
//!     the migration loop must fix; the oracle surfaces it rather than hiding
//!     it (see e.g. the `initialize` case below).
//!   * Notifications (no `id`) get no reply on either side; the corpus only
//!     sends requests WITH an `id`, so the response count is deterministic.
//!
//! ## Determinism pinning
//! `ctx_roots_list` reads `~/.ctx/roots.toml`; both servers are pointed at a
//! pinned `tests/roots-registry/roots.toml` via `CTX_ROOTS_FILE` so its output
//! is machine-independent. The served `--root` is the pinned `tests/fixtures`
//! tree. A stable JSON-RPC `id` is used per request.
//!
//! ## Normalization carve-outs (documented; see DEFERRED.md)
//!   * `Norm::AbsPath` — replaces the resolved fixture root (and the macOS
//!     `/private` symlink variant) with `<ROOT>`. Used for messages that embed
//!     the absolute fixture path (`ctx_skim` header, path-sandbox error hints).
//!   * `Norm::Timestamp` — strips the volatile `**Generated**: <RFC3339>` line
//!     emitted by `ctx_pack` headers.
//!   * `Norm::Version` — replaces ONLY the `serverInfo.version` string value
//!     with `<VERSION>` on both sides. Go's `serverVersion()` returns the
//!     12-char `vcs.revision` commit hash under `go build`, which changes every
//!     commit and is byte-unreproducible by Rust. The substitution is anchored
//!     to `"version":"…"` and deliberately does NOT touch `protocolVersion`
//!     (the deterministic constant `"2024-11-05"`, which stays byte-exact).
//!   * Everything else is `Norm::Exact` — compared byte-for-byte.
//!
//! ## Per-case tests (incremental-progress friendly)
//! Each corpus case is its own `#[test] fn parity_<name>()`, so
//! `cargo test --test parity` reports an accurate `N passed; M failed` and a
//! regression surfaces as a NAMED failing test. The migration loop ports one
//! method/tool per iteration and can count exactly one test flipping green,
//! rather than gating on an all-or-nothing aggregate. Expensive setup (Go
//! oracle build, binary discovery, fixture canonicalization) is shared across
//! cases via a `OnceLock`; each test still boots its own isolated server pair.
//!
//! ## What "RED" means here
//! The Rust draft implements only `initialize`, `tools/list` (2 of 9 tools),
//! and `tools/call` for `ctx_where` + `ctx_symbols`. Every other method/tool/
//! error path produces a DIFFERENT response on the Rust side (method-not-found,
//! unknown-tool, or wrong key order). Those tests are EXPECTED to fail until the
//! migration loop ports them. Today exactly 3 pass (parity_tool_ctx_where,
//! parity_tool_ctx_symbols, parity_err_unknown_method); the rest are red.

use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::time::Duration;

// ----------------------------------------------------------------------------
// Case matrix
// ----------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Norm {
    /// Compare bytes exactly.
    Exact,
    /// Replace the absolute fixture root with `<ROOT>` on both sides first.
    AbsPath,
    /// Strip the volatile `**Generated**: <RFC3339>` line (ctx_pack header).
    Timestamp,
    /// Replace ONLY the `serverInfo.version` string value with `<VERSION>`.
    /// Go emits the build's VCS commit hash here (changes every commit);
    /// `protocolVersion` is left untouched.
    Version,
}

struct Case {
    /// Case name (also selects the `expect_contains` guard).
    name: &'static str,
    /// One or more JSON-RPC request objects (each sent as one NDJSON line).
    /// The harness reads exactly `requests.len()` response lines back and
    /// compares the CONCATENATION, so multi-frame cases stay byte-exact too.
    requests: &'static [&'static str],
    norm: Norm,
}

/// Construct a [`Case`] from its name, one-or-more request frames, and norm.
const fn single(name: &'static str, request: &'static [&'static str], norm: Norm) -> Case {
    Case { name, requests: request, norm }
}

fn cases() -> Vec<Case> {
    vec![
        // ----- lifecycle / discovery methods -----
        // initialize: implemented on BOTH sides, but Go sorts result keys
        // (capabilities, protocolVersion, serverInfo) while Rust preserves
        // insertion order (protocolVersion, capabilities, serverInfo). That
        // key-ordering divergence is a REAL, SATISFIABLE defect: the loop fixes
        // it by emitting alphabetically-sorted keys like Go's encoding/json.
        //
        // serverInfo.version, by contrast, is Go's build VCS commit hash — it
        // changes every commit and Rust can never byte-reproduce it. Without a
        // Norm this case would be UNSATISFIABLE (the loop could fix key order
        // and structure but still never pass). Norm::Version replaces only the
        // version VALUE with <VERSION> on both sides; protocolVersion is left
        // byte-exact. So once the loop emits sorted keys + the correct
        // capabilities/serverInfo shape + ANY version, this case goes GREEN.
        single(
            "initialize",
            &[r#"{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2024-11-05"}}"#],
            Norm::Version,
        ),
        // tools/list: Rust advertises only 2 of 9 tools, in a different order.
        single(
            "tools_list",
            &[r#"{"jsonrpc":"2.0","id":2,"method":"tools/list"}"#],
            Norm::Exact,
        ),
        // prompts/list: not implemented in Rust → method_not_found.
        single(
            "prompts_list",
            &[r#"{"jsonrpc":"2.0","id":3,"method":"prompts/list"}"#],
            Norm::Exact,
        ),
        // prompts/get: not implemented in Rust → method_not_found.
        single(
            "prompts_get_find_code",
            &[r#"{"jsonrpc":"2.0","id":4,"method":"prompts/get","params":{"name":"find-code-for","arguments":{"goal":"rate limiter"}}}"#],
            Norm::Exact,
        ),
        // resources/list: not implemented in Rust → method_not_found.
        single(
            "resources_list",
            &[r#"{"jsonrpc":"2.0","id":5,"method":"resources/list"}"#],
            Norm::Exact,
        ),
        // resources/templates/list: not implemented in Rust → method_not_found.
        single(
            "resources_templates_list",
            &[r#"{"jsonrpc":"2.0","id":6,"method":"resources/templates/list"}"#],
            Norm::Exact,
        ),
        // resources/read (ctx://file/{path} template): not implemented in Rust.
        single(
            "resources_read_file",
            &[r#"{"jsonrpc":"2.0","id":7,"method":"resources/read","params":{"uri":"ctx://file/notes.txt"}}"#],
            Norm::Exact,
        ),
        // ----- tools/call: each ctx_* tool -----
        // ctx_where: IMPLEMENTED on both sides (the one byte-identical case;
        // it proves the harness can detect AGREEMENT, so a RED elsewhere is a
        // real signal, not a harness bug).
        single(
            "tool_ctx_where",
            &[r#"{"jsonrpc":"2.0","id":10,"method":"tools/call","params":{"name":"ctx_where","arguments":{"path":".","query":"rate"}}}"#],
            Norm::Exact,
        ),
        // ctx_symbols: implemented in Rust, but symbol extraction differs from
        // the Go tree-sitter path → byte mismatch in the JSON body.
        single(
            "tool_ctx_symbols",
            &[r#"{"jsonrpc":"2.0","id":11,"method":"tools/call","params":{"name":"ctx_symbols","arguments":{"path":"."}}}"#],
            Norm::Exact,
        ),
        // ctx_pack: unknown tool in Rust. Timestamp norm strips the Generated line.
        single(
            "tool_ctx_pack",
            &[r#"{"jsonrpc":"2.0","id":12,"method":"tools/call","params":{"name":"ctx_pack","arguments":{"path":".","budget":50000,"format":"markdown"}}}"#],
            Norm::Timestamp,
        ),
        // ctx_budget: unknown tool in Rust.
        single(
            "tool_ctx_budget",
            &[r#"{"jsonrpc":"2.0","id":13,"method":"tools/call","params":{"name":"ctx_budget","arguments":{"path":".","budget":10000}}}"#],
            Norm::Exact,
        ),
        // ctx_skim: unknown tool in Rust. AbsPath norm: header embeds the abs path.
        single(
            "tool_ctx_skim",
            &[r#"{"jsonrpc":"2.0","id":14,"method":"tools/call","params":{"name":"ctx_skim","arguments":{"path":"main.go","budget":500}}}"#],
            Norm::AbsPath,
        ),
        // ctx_tree: unknown tool in Rust. Determinism note: Go enriches each
        // entry with per-file git_status, but the served --root is the fixtures
        // SUBDIR, not a repo root. `git status --porcelain` keys are
        // repo-root-relative (`crates/ctx-mcp/tests/fixtures/main.go`) while the
        // walk emits subdir-relative paths (`main.go`), so applyGitStatus never
        // matches → git_status is ALWAYS empty (omitempty drops it) regardless
        // of whether the fixtures are untracked / staged / committed / dirty.
        // Tokens come from tiktoken on fixed bytes → content-deterministic. So
        // the body is stable across checkouts; no Norm needed.
        single(
            "tool_ctx_tree",
            &[r#"{"jsonrpc":"2.0","id":15,"method":"tools/call","params":{"name":"ctx_tree","arguments":{"path":"."}}}"#],
            Norm::Exact,
        ),
        // ctx_focus: unknown tool in Rust.
        single(
            "tool_ctx_focus",
            &[r#"{"jsonrpc":"2.0","id":16,"method":"tools/call","params":{"name":"ctx_focus","arguments":{"anchor":"RateLimit"}}}"#],
            Norm::Exact,
        ),
        // ctx_digest: unknown tool in Rust. (Go requires a git repo; on the
        // non-repo fixture it returns a tool error envelope — still a concrete,
        // deterministic body that Rust's unknown-tool error must NOT match.)
        single(
            "tool_ctx_digest",
            &[r#"{"jsonrpc":"2.0","id":17,"method":"tools/call","params":{"name":"ctx_digest","arguments":{"path":".","since":"7d"}}}"#],
            Norm::Exact,
        ),
        // ctx_roots_list: unknown tool in Rust today. The body is deterministic
        // because BOTH servers read the SAME registry via the CTX_ROOTS_FILE
        // env var (Go: internal/config/roots.go:29; the Rust ecosystem already
        // honors it — ctx-cli/src/main.rs:2860, ctx-web/handlers/roots.rs:142 —
        // so the ported tool will too). The fixture roots.toml bakes absolute
        // /tmp literals with no last_opened, so LAST_OPENED renders "-" and the
        // CURRENT marker is empty (no registered path equals the served root).
        // Fully satisfiable; currently RED only because the tool is unported.
        single(
            "tool_ctx_roots_list",
            &[r#"{"jsonrpc":"2.0","id":18,"method":"tools/call","params":{"name":"ctx_roots_list"}}"#],
            Norm::Exact,
        ),
        // ----- error paths -----
        // Unknown method: BOTH return method_not_found, but with different id
        // bookkeeping? No — both echo the id and the same message shape. This
        // case should be byte-identical (genuine parity on the dispatcher's
        // default branch). The guard asserts a real error body so a both-empty
        // false PASS is impossible.
        single(
            "err_unknown_method",
            &[r#"{"jsonrpc":"2.0","id":20,"method":"nonexistent/method"}"#],
            Norm::Exact,
        ),
        // Parse error: malformed JSON → null-id parse-error response.
        single(
            "err_parse_error",
            &[r#"{not valid json"#],
            Norm::Exact,
        ),
        // Path outside root: Go returns Invalid Params with a data.hint that
        // names the server root. The fixture root is the served root, so it is
        // normalized. Rust's resolve_path returns a terser invalid_params with
        // NO hint → byte mismatch.
        single(
            "err_path_outside_root",
            &[r#"{"jsonrpc":"2.0","id":21,"method":"tools/call","params":{"name":"ctx_where","arguments":{"path":"/etc","query":"x"}}}"#],
            Norm::AbsPath,
        ),
        // Budget exceeded: budget > maxBudget → Invalid Params + hint.
        single(
            "err_budget_too_large",
            &[r#"{"jsonrpc":"2.0","id":22,"method":"tools/call","params":{"name":"ctx_pack","arguments":{"path":".","budget":99999999}}}"#],
            Norm::Exact,
        ),
        // Missing required arg: ctx_focus without anchor → tool-error envelope
        // (isError:true). Rust returns unknown-tool error instead.
        single(
            "err_focus_missing_anchor",
            &[r#"{"jsonrpc":"2.0","id":23,"method":"tools/call","params":{"name":"ctx_focus","arguments":{}}}"#],
            Norm::Exact,
        ),
        // Unknown tool name: Go returns a tool-error ENVELOPE (isError:true);
        // Rust returns a JSON-RPC -32000 error → genuine mismatch the loop
        // must reconcile. Guard asserts Go's envelope shape.
        single(
            "err_unknown_tool",
            &[r#"{"jsonrpc":"2.0","id":24,"method":"tools/call","params":{"name":"ctx_nonexistent","arguments":{}}}"#],
            Norm::Exact,
        ),
    ]
}

/// Substrings the GO response body MUST contain for the named case. This is the
/// anti-escape guard: byte-equality alone would PASS even if BOTH servers
/// returned the same empty/error body, so for cases that should produce
/// meaningful, case-specific data we additionally assert the Go body literally
/// contains a real field/value. This makes a "both-empty / both-error" false
/// PASS impossible.
fn expect_contains(name: &str) -> &'static [&'static str] {
    match name {
        "initialize" => &[
            r#""protocolVersion":"2024-11-05""#,
            r#""serverInfo":"#,
            r#""name":"ctx""#,
            r#""capabilities":"#,
        ],
        "tools_list" => &[
            r#""name":"ctx_pack""#,
            r#""name":"ctx_where""#,
            r#""name":"ctx_budget""#,
            r#""name":"ctx_symbols""#,
            r#""name":"ctx_skim""#,
            r#""name":"ctx_digest""#,
            r#""name":"ctx_focus""#,
            r#""name":"ctx_roots_list""#,
            r#""name":"ctx_tree""#,
        ],
        "prompts_list" => &[
            r#""name":"onboard-codebase""#,
            r#""name":"summarize-recent-activity""#,
            r#""name":"find-code-for""#,
        ],
        "prompts_get_find_code" => &[
            r#""role":"user""#,
            r#"Find the code in this repository that implements: rate limiter"#,
            r#""description":"Locate the code responsible for a goal"#,
        ],
        "resources_list" => &[
            r#""uri":"ctx://docs/readme""#,
            r#""mimeType":"text/markdown""#,
            r#""name":"ctx README""#,
        ],
        "resources_templates_list" => &[
            r#""uriTemplate":"ctx://file/{path}""#,
            r#""name":"Repository file""#,
        ],
        "resources_read_file" => &[
            r#""uri":"ctx://file/notes.txt""#,
            r#""mimeType":"text/plain""#,
            r#"plain notes about config loader"#,
        ],
        // ctx_where: ranked match on RateLimit in main.go (line 13).
        "tool_ctx_where" => &[
            r#"Best matches"#,
            r#"1. main.go:13 RateLimit"#,
            r#"symbol match: RateLimit"#,
            r#"anchor: RateLimit@main.go"#,
        ],
        // ctx_symbols: JSON map keyed by main.go with the 3 symbols.
        "tool_ctx_symbols" => &[
            r#"\"main.go\""#,
            r#"\"name\": \"Greeter\""#,
            r#"\"kind\": \"type\""#,
            r#"\"name\": \"RateLimit\""#,
            r#"\"name\": \"main\""#,
        ],
        // ctx_pack: markdown context-pack header + body for main.go.
        "tool_ctx_pack" => &[
            r#"# Context Pack"#,
            r#"## Included files"#,
            r#"### main.go"#,
            r#"func RateLimit() {}"#,
        ],
        // ctx_budget: budget plan JSON with included files + token totals.
        "tool_ctx_budget" => &[
            r#"\"budget\": 10000"#,
            r#"\"included\":"#,
            r#"\"Path\": \"main.go\""#,
            r#"\"used\": 124"#,
        ],
        // ctx_skim: tier header + go source body. The header embeds the
        // machine-specific abs path (normalized in the body comparison), so the
        // guard — which runs against the RAW Go body — asserts only the
        // path-independent shape (tier/budget header tail + the source body).
        "tool_ctx_skim" => &[
            r#"# tier=full tokens=44/500 path="#,
            r#"main.go lang=go"#,
            r#"package main"#,
            r#"func RateLimit() {}"#,
        ],
        // ctx_tree: flat JSON array of fixture entries with tokens.
        "tool_ctx_tree" => &[
            r#"\"path\": \"main.go\""#,
            r#"\"is_dir\": false"#,
            r#"\"tokens\": 44"#,
            r#"\"path\": \"notes.txt\""#,
        ],
        // ctx_focus: anchor header + main.go body.
        "tool_ctx_focus" => &[
            r#"# anchor=RateLimit origin=main.go hops=1"#,
            r#"### main.go"#,
            r#"func RateLimit() {}"#,
        ],
        // ctx_digest: on the non-repo fixture Go produces a deterministic tool
        // error envelope (isError:true) whose text names a git failure. We
        // assert the envelope shape so a both-unknown-tool false PASS cannot
        // sneak through (Rust returns a JSON-RPC error, not this envelope).
        "tool_ctx_digest" => &[r#""isError":true"#, r#""content":"#],
        // ctx_roots_list: pinned registry → alpha/beta rows, tab-aligned table.
        "tool_ctx_roots_list" => &[
            r#"NAME\tPATH\tLAST_OPENED\tCURRENT"#,
            r#"alpha\t/tmp/ctx-mcp-parity-alpha"#,
            r#"beta\t/tmp/ctx-mcp-parity-beta"#,
        ],
        // err_unknown_method: JSON-RPC -32601 with the method name echoed.
        "err_unknown_method" => &[
            r#""code":-32601"#,
            r#""method not found: nonexistent/method""#,
        ],
        // err_parse_error: -32700 parse error, null id (id field omitted).
        "err_parse_error" => &[r#""code":-32700"#],
        // err_path_outside_root: -32602 + a hint naming the server root. The
        // hint embeds the machine-specific abs path (normalized in the body
        // comparison), so the guard — run against the RAW Go body — asserts the
        // path-independent shape only.
        "err_path_outside_root" => &[
            r#""code":-32602"#,
            r#""message":"path outside server root""#,
            r#""hint":"server root is "#,
        ],
        // err_budget_too_large: -32602 with the budget-bound message + hint.
        "err_budget_too_large" => &[
            r#""code":-32602"#,
            r#""budget must be between 0 and 1000000""#,
            r#""hint":"suggested: 50000""#,
        ],
        // err_focus_missing_anchor: tool-error envelope, NOT a JSON-RPC error.
        "err_focus_missing_anchor" => &[
            r#""isError":true"#,
            r#"ctx_focus: 'anchor' is required"#,
        ],
        // err_unknown_tool: Go wraps an unknown tool name in the MCP tool-error
        // ENVELOPE (result.isError:true + content text), NOT a JSON-RPC error.
        // Rust returns a JSON-RPC -32000 error instead → genuine byte mismatch.
        // The guard asserts Go's envelope so the divergence is real, not a
        // both-error false PASS.
        "err_unknown_tool" => &[
            r#""isError":true"#,
            r#"unknown tool: ctx_nonexistent"#,
        ],
        _ => &[],
    }
}

// ----------------------------------------------------------------------------
// Per-case test driver
// ----------------------------------------------------------------------------
//
// Each corpus case is its OWN `#[test] fn parity_<name>()` (see the block at the
// bottom of this file) so `cargo test --test parity` reports an accurate
// `N passed; M failed` and a regressed case shows as a NAMED failing test. This
// is what lets the migration loop count incremental progress: porting one tool
// flips exactly one test green while the rest stay red, instead of an
// all-or-nothing aggregate that can only pass on the final iteration.
//
// The expensive setup (locating/building the Go oracle binary, locating the
// Rust binary, canonicalizing the fixture root) is shared across all cases via
// a process-global `OnceLock`. Each test still boots its OWN pair of server
// subprocesses for its own request(s) — that per-test isolation is cheap and
// desirable (no cross-case state leakage).

use std::sync::OnceLock;

/// Shared, lazily-initialized harness context: binary paths + abs fixture root.
/// Computed once per test-binary process (the Go `go build` happens at most
/// once even though 22 tests reference it).
struct Harness {
    go_bin: Option<PathBuf>,
    rust_bin: PathBuf,
    fixture: PathBuf,
    roots_file: PathBuf,
    abs_root: String,
}

fn harness() -> &'static Harness {
    static HARNESS: OnceLock<Harness> = OnceLock::new();
    HARNESS.get_or_init(|| {
        let fixture = fixture_dir();
        let roots_file = manifest_dir().join("tests/roots-registry/roots.toml");
        let rust_bin = locate_rust_binary().expect(
            "Rust ctx-mcp binary not found. Run \
             `cargo build --manifest-path crates/ctx-mcp/Cargo.toml --bin ctx-mcp`.",
        );
        let abs_root = std::fs::canonicalize(&fixture)
            .unwrap_or_else(|_| fixture.clone())
            .to_string_lossy()
            .to_string();
        Harness {
            go_bin: locate_go_binary(),
            rust_bin,
            fixture,
            roots_file,
            abs_root,
        }
    })
}

/// Look up a single corpus case by name. Panics if the name is unknown (a typo
/// in a `parity_*` test would otherwise silently skip a case).
fn case_by_name(name: &'static str) -> Case {
    cases()
        .into_iter()
        .find(|c| c.name == name)
        .unwrap_or_else(|| panic!("no corpus case named {name:?}"))
}

/// Run a single case end-to-end and assert byte-parity + anti-escape guards.
/// Boots a fresh Go+Rust server pair, exchanges the case's request frame(s),
/// normalizes per the case's `Norm`, and panics (failing THIS test) on any
/// body mismatch or guard miss. If the Go oracle is unavailable the case is
/// skipped (returns without asserting), mirroring the prior aggregate behavior.
fn run_case(name: &'static str) {
    let h = harness();
    let Some(go_bin) = h.go_bin.as_ref() else {
        eprintln!(
            "SKIP {name}: Go oracle binary not found and `go build` unavailable. \
             Set CTX_GO_BIN or run `go build -o /tmp/ctx-go ./cmd/ctx`."
        );
        return;
    };
    let case = case_by_name(name);

    // Boot a FRESH pair of servers so no request leaks state across cases.
    let mut go = Server::spawn(go_bin, &["mcp", "serve", "--root"], &h.fixture, &h.roots_file)
        .expect("spawn go oracle");
    let mut rust = Server::spawn(&h.rust_bin, &["--root"], &h.fixture, &h.roots_file)
        .expect("spawn rust ctx-mcp");

    let go_resp = go.exchange(case.requests);
    let rust_resp = rust.exchange(case.requests);
    go.shutdown();
    rust.shutdown();

    let go_body = go_resp.expect("go oracle exchange");
    let rust_body = rust_resp.expect("rust ctx-mcp exchange");

    let gn = normalize(&go_body, case.norm, &h.abs_root);
    let rn = normalize(&rust_body, case.norm, &h.abs_root);

    let mut why = Vec::new();
    if gn != rn {
        why.push(format!("body mismatch\n  go  ={gn}\n  rust={rn}"));
    }
    // Anti-escape guard: the (Go) body MUST contain the expected shape so a
    // both-empty / both-error response can never false-PASS.
    for needle in expect_contains(case.name) {
        if !go_body.contains(needle) {
            why.push(format!("guard: go body missing {needle:?} (got {go_body:?})"));
        }
    }

    assert!(why.is_empty(), "{}: {}", case.name, why.join("; "));
}

// ----------------------------------------------------------------------------
// Normalization
// ----------------------------------------------------------------------------

fn normalize(body: &str, norm: Norm, abs_root: &str) -> String {
    match norm {
        Norm::Exact => body.to_string(),
        Norm::AbsPath => {
            let s = body.replace(abs_root, "<ROOT>");
            let private = format!("/private{abs_root}");
            s.replace(&private, "<ROOT>")
        }
        Norm::Timestamp => {
            // Strip the volatile `**Generated**: <RFC3339>` token wherever it
            // appears (it is embedded inside an escaped JSON string, so match
            // the marker and drop to the next escaped newline `\n`).
            strip_generated(body)
        }
        Norm::Version => strip_version(body),
    }
}

/// Replace the `serverInfo.version` string VALUE with `<VERSION>`. Anchors on
/// the literal key token `"version":"` and rewrites up to the next `"`.
///
/// This is safe against `protocolVersion`: that key serializes as
/// `"protocolVersion":"…"`, where `version` is preceded by the letters
/// `protocol` — NOT by a `"`. The anchor `"version":"` (quote + lowercase
/// `version` + `":"`) therefore never matches inside `protocolVersion`, so the
/// deterministic `protocolVersion` constant stays byte-exact.
fn strip_version(body: &str) -> String {
    let anchor = "\"version\":\"";
    let Some(key_at) = body.find(anchor) else {
        return body.to_string();
    };
    let val_start = key_at + anchor.len();
    // The version value is a JSON string; find its closing quote. None of the
    // values Go/Rust emit here contain an escaped quote, so a plain scan for the
    // next `"` is correct.
    let Some(rel_end) = body[val_start..].find('"') else {
        return body.to_string();
    };
    let val_end = val_start + rel_end;
    let mut out = String::with_capacity(body.len());
    out.push_str(&body[..val_start]);
    out.push_str("<VERSION>");
    out.push_str(&body[val_end..]);
    out
}

/// Remove the `**Generated**: …` line from a ctx_pack header. The pack text is
/// JSON-string-escaped, so newlines appear as the two characters `\` `n`.
fn strip_generated(body: &str) -> String {
    let marker = "**Generated**: ";
    let Some(start) = body.find(marker) else {
        return body.to_string();
    };
    // Find the next escaped newline (`\n` => backslash + 'n') after the marker.
    let tail = &body[start..];
    if let Some(nl) = tail.find("\\n") {
        let mut out = String::with_capacity(body.len());
        out.push_str(&body[..start]);
        out.push_str("**Generated**: <TS>");
        out.push_str(&tail[nl..]);
        out
    } else {
        body.to_string()
    }
}

// ----------------------------------------------------------------------------
// Server lifecycle: stdio subprocess speaking NDJSON JSON-RPC
// ----------------------------------------------------------------------------

struct Server {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl Server {
    /// Spawn `bin <args...> <fixture>` with CTX_ROOTS_FILE pinned. `args` is the
    /// flag list up to (but not including) the root path value, e.g.
    /// `["mcp","serve","--root"]` for Go or `["--root"]` for Rust.
    fn spawn(
        bin: &Path,
        args: &[&str],
        fixture: &Path,
        roots_file: &Path,
    ) -> std::io::Result<Self> {
        let mut cmd = Command::new(bin);
        cmd.args(args)
            .arg(fixture)
            .env("CTX_ROOTS_FILE", roots_file)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        let mut child = cmd.spawn()?;
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = BufReader::new(child.stdout.take().expect("child stdout"));
        Ok(Server { child, stdin, stdout })
    }

    /// Write each request as one NDJSON line, then read exactly `requests.len()`
    /// response lines and return their concatenation (each kept verbatim minus
    /// the trailing `\n`, joined by `\n`). A short read (server replied to
    /// fewer frames than sent — e.g. a notification or an early exit) returns an
    /// Err so the case is reported as a SETUP failure rather than a silent pass.
    fn exchange(&mut self, requests: &[&str]) -> std::io::Result<String> {
        for req in requests {
            self.stdin.write_all(req.as_bytes())?;
            self.stdin.write_all(b"\n")?;
        }
        self.stdin.flush()?;

        let mut lines = Vec::with_capacity(requests.len());
        for _ in 0..requests.len() {
            let mut line = String::new();
            let n = self.stdout.read_line(&mut line)?;
            if n == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::UnexpectedEof,
                    format!(
                        "server closed stdout after {}/{} responses",
                        lines.len(),
                        requests.len()
                    ),
                ));
            }
            lines.push(line.trim_end_matches(['\n', '\r']).to_string());
        }
        Ok(lines.join("\n"))
    }

    fn shutdown(mut self) {
        // Dropping stdin sends EOF; the server's read loop then exits cleanly.
        drop(self.stdin);
        // Give it a brief chance to exit, then force-kill so the test never hangs.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        loop {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) if std::time::Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(10));
                }
                _ => {
                    let _ = self.child.kill();
                    let _ = self.child.wait();
                    break;
                }
            }
        }
    }
}

// ----------------------------------------------------------------------------
// Binary discovery
// ----------------------------------------------------------------------------

fn manifest_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn fixture_dir() -> PathBuf {
    manifest_dir().join("tests/fixtures")
}

/// Locate the compiled Rust `ctx-mcp` binary next to this test's target dir.
fn locate_rust_binary() -> Option<PathBuf> {
    // `CARGO_BIN_EXE_ctx-mcp` is injected by cargo for integration tests that
    // depend on a `[[bin]]`/`src/bin/*` target of the SAME crate.
    if let Some(p) = option_env!("CARGO_BIN_EXE_ctx-mcp") {
        let p = PathBuf::from(p);
        if p.exists() {
            return Some(p);
        }
    }
    // Fallback: search the target dir relative to the manifest.
    for profile in ["debug", "release"] {
        let p = manifest_dir().join(format!("target/{profile}/ctx-mcp"));
        if p.exists() {
            return Some(p);
        }
    }
    None
}

/// Locate the Go oracle binary: `$CTX_GO_BIN`, else `/tmp/ctx-go`, else build it
/// via `go build` into the repo's target dir (mirrors ctx-web/tests/parity.rs).
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
    // Wave 4 (ADR-0005): build the FROZEN oracle from the `go-oracle/v1` tag (not
    // the working tree) via ci/build-go-oracle.sh, so the parity gate survives Go
    // deletion. Its cmd/ctx is byte-identical to the pre-deletion tree.
    let repo_root = manifest_dir().ancestors().nth(2)?.to_path_buf();
    let script_out = Command::new("bash")
        .arg(repo_root.join("ci/build-go-oracle.sh"))
        .output()
        .ok()?;
    let out = PathBuf::from(String::from_utf8_lossy(&script_out.stdout).trim().to_string());
    if script_out.status.success() && out.exists() {
        Some(out)
    } else {
        None
    }
}

// ----------------------------------------------------------------------------
// Per-case tests — one #[test] per corpus case so partial progress is countable
// ----------------------------------------------------------------------------
//
// Each test name matches its corpus case name (prefixed `parity_`). None are
// `#[ignore]`d: the currently-red cases RUN and FAIL now, and flip to passing
// as the migration loop ports each method/tool. The 3 cases that already match
// (parity_tool_ctx_where, parity_tool_ctx_symbols, parity_err_unknown_method)
// pass today.

#[test]
fn parity_initialize() {
    run_case("initialize");
}

#[test]
fn parity_tools_list() {
    run_case("tools_list");
}

#[test]
fn parity_prompts_list() {
    run_case("prompts_list");
}

#[test]
fn parity_prompts_get_find_code() {
    run_case("prompts_get_find_code");
}

#[test]
fn parity_resources_list() {
    run_case("resources_list");
}

#[test]
fn parity_resources_templates_list() {
    run_case("resources_templates_list");
}

#[test]
fn parity_resources_read_file() {
    run_case("resources_read_file");
}

#[test]
fn parity_tool_ctx_where() {
    run_case("tool_ctx_where");
}

#[test]
fn parity_tool_ctx_symbols() {
    run_case("tool_ctx_symbols");
}

#[test]
fn parity_tool_ctx_pack() {
    run_case("tool_ctx_pack");
}

#[test]
fn parity_tool_ctx_budget() {
    run_case("tool_ctx_budget");
}

#[test]
fn parity_tool_ctx_skim() {
    run_case("tool_ctx_skim");
}

#[test]
fn parity_tool_ctx_tree() {
    run_case("tool_ctx_tree");
}

#[test]
fn parity_tool_ctx_focus() {
    run_case("tool_ctx_focus");
}

#[test]
fn parity_tool_ctx_digest() {
    run_case("tool_ctx_digest");
}

#[test]
fn parity_tool_ctx_roots_list() {
    run_case("tool_ctx_roots_list");
}

#[test]
fn parity_err_unknown_method() {
    run_case("err_unknown_method");
}

#[test]
fn parity_err_parse_error() {
    run_case("err_parse_error");
}

#[test]
fn parity_err_path_outside_root() {
    run_case("err_path_outside_root");
}

#[test]
fn parity_err_budget_too_large() {
    run_case("err_budget_too_large");
}

#[test]
fn parity_err_focus_missing_anchor() {
    run_case("err_focus_missing_anchor");
}

#[test]
fn parity_err_unknown_tool() {
    run_case("err_unknown_tool");
}
