//! HTTP differential-parity ORACLE for the git×3 routes (ADR-0005 Wave 2):
//!   * GET /api/git/diff?path=          (handleGitDiff   — worktree vs HEAD)
//!   * GET /api/git/file-log?path=&limit=  (handleFileLog — file-scoped log)
//!   * GET /api/git/commit-diff?path=&from=&to=  (handleCommitDiff)
//!
//! This is the IMMUTABLE byte-parity oracle. The Go web server (`internal/web`)
//! is the frozen reference; the Rust `ctx-web` server does NOT yet serve these
//! routes, so EVERY case here is RED by construction: Go returns real git JSON
//! (`application/json`) while Rust falls through its SPA catch-all and returns
//! `index.html` (`text/html`). A later migration loop turns each case GREEN by
//! implementing the Rust handler. DO NOT implement the handlers here — and if
//! a case ever passes before the Rust route exists, the oracle is wrong.
//!
//! ## Per-case structure (deliberate)
//! Each case is its OWN `#[test] fn gitparity_<case>()` so a migration loop can
//! count progress (N passing / M total) instead of an all-or-nothing monolith.
//! Every test boots BOTH servers against the SAME deterministic git fixture,
//! issues one HTTP request, and asserts:
//!   1. byte-identical (status, body, content-type) between Go and Rust, AND
//!   2. an `expect_contains` guard that the GO body carries the meaningful
//!      shape (real diff line text / commit hash / Binary / Truncated flag).
//! Guard (2) makes a both-empty / both-error false-PASS impossible.
//!
//! ## Deterministic git fixture (the crux)
//! Commit hashes depend on content + author/committer identity + dates. The
//! fixture (see `build_git_fixture`) `git init`s a fresh temp dir and creates a
//! fixed commit sequence with FROZEN `GIT_AUTHOR_*` / `GIT_COMMITTER_*` name,
//! email and date. Identical inputs ⇒ identical SHA-1s on every run and every
//! machine, so the hashes baked into `expect_contains` (e.g. `e494c66`) stay
//! stable. Verified byte-identical across two independent builds.
//!
//! ## Mirrors `parity.rs`
//! The Go/Rust boot + minimal HTTP client + de-chunk + header-compare machinery
//! is copied from `tests/parity.rs`. The ONLY material differences: (a) the
//! served root is a freshly-built git repo (not the static `tests/fixtures`),
//! and (b) each case is a standalone `#[test]`.

use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

// ----------------------------------------------------------------------------
// Normalization (mirrors parity.rs::Norm)
// ----------------------------------------------------------------------------

#[derive(Clone, Copy)]
enum Norm {
    /// Compare bytes exactly.
    Exact,
    /// Replace the absolute fixture root (and macOS `/private` variant) with
    /// `<ROOT>` on both sides — for error messages that embed a machine path.
    AbsPath,
}

fn normalize(body: &[u8], norm: Norm, abs_root: &str) -> Vec<u8> {
    match norm {
        Norm::Exact => body.to_vec(),
        Norm::AbsPath => {
            let s = String::from_utf8_lossy(body);
            let s = s.replace(abs_root, "<ROOT>");
            let private = format!("/private{abs_root}");
            let s = s.replace(&private, "<ROOT>");
            s.into_bytes()
        }
    }
}

// ----------------------------------------------------------------------------
// The single shared parity assertion used by every per-case test.
// ----------------------------------------------------------------------------

/// Boot Go+Rust against the deterministic git fixture, issue one GET, and
/// assert byte-parity + the `expect` substring guard against the Go body.
///
/// Because the Rust server does not serve `/api/git/*`, this is RED today: the
/// assertion fails on a body/content-type mismatch (Go JSON vs Rust SPA HTML).
/// That is the intended state of the oracle until the Rust route is ported.
fn assert_parity(path: &str, norm: Norm, expect: &[&str]) {
    let fx = GitFixture::build();
    let go_bin = match locate_go_binary() {
        Some(b) => b,
        None => {
            // Mirror parity.rs: if the Go oracle can't be built, skip rather
            // than fail (keeps CI green on toolchain-less runners). A real run
            // has Go available, so the RED assertions below execute.
            eprintln!(
                "SKIP: Go oracle binary not found and `go build` unavailable. \
                 Set CTX_GO_BIN or run `go build -o target/ctx-go-oracle ./cmd/ctx`."
            );
            return;
        }
    };

    let mut go = GoServer::start(&go_bin, fx.root());
    let rust = RustServer::start(fx.root());
    go.wait_ready();
    rust.wait_ready();

    let go_base = format!("http://{}", go.addr);
    let rust_base = format!("http://127.0.0.1:{}", rust.port);

    let abs_root = std::fs::canonicalize(fx.root()).unwrap();
    let abs_root = abs_root.to_string_lossy().to_string();

    let g = http_request(&go_base, "GET", path, None);
    let r = http_request(&rust_base, "GET", path, None);

    go.kill();
    rust.shutdown();

    let gb = normalize(&g.body, norm, &abs_root);
    let rb = normalize(&r.body, norm, &abs_root);

    // Anti-escape guard FIRST: prove the Go (oracle) body carries the real
    // git shape, so this can never become a both-empty false PASS.
    let go_body = String::from_utf8_lossy(&g.body);
    for needle in expect {
        assert!(
            go_body.contains(needle),
            "GUARD FAILED: Go body for {path:?} missing {needle:?}\nGo body: {go_body}"
        );
    }

    // Byte-parity assertions. RED today (Rust has no git route → SPA HTML).
    let gct = g.header("content-type");
    let rct = r.header("content-type");
    assert!(
        g.status == r.status && gb == rb && gct == rct,
        "PARITY MISMATCH for {path:?}\n  status: go={} rust={}\n  content-type: go={:?} rust={:?}\n  go body:   {}\n  rust body: {}",
        g.status,
        r.status,
        gct,
        rct,
        String::from_utf8_lossy(&gb),
        String::from_utf8_lossy(&rb),
    );
}

// ----------------------------------------------------------------------------
// Per-case tests — one #[test] per (route × case).
// ----------------------------------------------------------------------------

// --- /api/git/diff (worktree vs HEAD) ---

// (a) Worktree-modified file vs HEAD: greeting.txt was committed then edited in
// the worktree. Expect add/del/eq lines with old_num/new_num bookkeeping.
#[test]
fn gitparity_diff_worktree_modified() {
    assert_parity(
        "/api/git/diff?path=greeting.txt",
        Norm::Exact,
        &[
            r#""path":"greeting.txt""#,
            r#"{"type":"eq","text":"alpha","old_num":1,"new_num":1}"#,
            r#"{"type":"del","text":"BETA","old_num":2}"#,
            r#"{"type":"add","text":"BETA-worktree","new_num":2}"#,
            r#"{"type":"add","text":"zeta","new_num":6}"#,
        ],
    );
}

// (d) Binary file (NUL byte in first 8000 bytes) → Binary:true, empty lines.
#[test]
fn gitparity_diff_binary() {
    assert_parity(
        "/api/git/diff?path=image.bin",
        Norm::Exact,
        &[r#""path":"image.bin""#, r#""binary":true"#, r#""lines":[]"#],
    );
}

// (e) Large file exceeding the 5000-line diff cap → Truncated:true, exactly
// 5000 emitted lines.
#[test]
fn gitparity_diff_truncated() {
    assert_parity(
        "/api/git/diff?path=big.txt",
        Norm::Exact,
        &[
            r#""path":"big.txt""#,
            r#""truncated":true"#,
            r#"{"type":"del","text":"line 0","old_num":1}"#,
        ],
    );
}

// No-change: notes.txt is committed and unmodified in the worktree →
// NoChange:true, empty lines. Guards that an identical file is NOT a false
// "diff" and the flag is emitted.
#[test]
fn gitparity_diff_no_change() {
    assert_parity(
        "/api/git/diff?path=notes.txt",
        Norm::Exact,
        &[
            r#""path":"notes.txt""#,
            r#""no_change":true"#,
            r#""lines":[]"#,
        ],
    );
}

// (f) Missing required path param → 400 bad_request.
#[test]
fn gitparity_diff_missing_path() {
    assert_parity(
        "/api/git/diff",
        Norm::Exact,
        &[r#""code":"bad_request""#, r#""path is required""#],
    );
}

// (f) Path traversal rejected → 400 path_traversal (no machine path in body).
#[test]
fn gitparity_diff_traversal() {
    assert_parity(
        "/api/git/diff?path=../etc/passwd",
        Norm::Exact,
        &[
            r#""code":"path_traversal""#,
            r#""path traversal not allowed""#,
        ],
    );
}

// --- /api/git/file-log ---

// (c) File-log with multiple commits: greeting.txt touched by 3 commits,
// newest-first. Guards real short+full hashes, author identity, subjects, and
// Unix dates baked from the frozen fixture.
#[test]
fn gitparity_file_log_multi() {
    assert_parity(
        "/api/git/file-log?path=greeting.txt",
        Norm::Exact,
        &[
            r#""path":"greeting.txt""#,
            r#""hash":"5623208","hash_full":"5623208c15831d8ebd5593d5bf189425e5d18165""#,
            r#""subject":"Append epsilon""#,
            r#""hash":"e494c66","hash_full":"e494c667fa44faadf3b928887f209801b189de7a""#,
            r#""subject":"Add greeting""#,
            r#""author":"Parity Bot","author_email":"parity@example.com""#,
            r#""date":1577934245"#,
            r#""truncated":false"#,
        ],
    );
}

// (c) File-log limit clamping: limit=1 returns only the newest commit and sets
// Truncated:true (more commits exist).
#[test]
fn gitparity_file_log_limit() {
    assert_parity(
        "/api/git/file-log?path=greeting.txt&limit=1",
        Norm::Exact,
        &[
            r#""hash":"5623208""#,
            r#""subject":"Append epsilon""#,
            r#""truncated":true"#,
        ],
    );
}

// (f) File with no commit history → empty commits array, Truncated:false (the
// path resolves inside the repo but was never committed).
#[test]
fn gitparity_file_log_no_history() {
    assert_parity(
        "/api/git/file-log?path=nope.txt",
        Norm::Exact,
        &[
            r#""path":"nope.txt""#,
            r#""commits":[]"#,
            r#""truncated":false"#,
        ],
    );
}

// (f) Missing required path param → 400 bad_request.
#[test]
fn gitparity_file_log_missing_path() {
    assert_parity(
        "/api/git/file-log",
        Norm::Exact,
        &[r#""code":"bad_request""#, r#""path is required""#],
    );
}

// --- /api/git/commit-diff ---

// (b) Commit-to-commit diff: greeting.txt between commit 1 (e494c66) and
// commit 2 (1e6958a). Guards the exact add/del/eq line sequence the
// diffmatchpatch pipeline produces between the two committed blobs.
#[test]
fn gitparity_commit_diff() {
    assert_parity(
        "/api/git/commit-diff?path=greeting.txt&from=e494c66&to=1e6958a",
        Norm::Exact,
        &[
            r#""path":"greeting.txt""#,
            r#"{"type":"eq","text":"alpha","old_num":1,"new_num":1}"#,
            r#"{"type":"del","text":"beta","old_num":2}"#,
            r#"{"type":"add","text":"BETA","new_num":2}"#,
            r#"{"type":"add","text":"delta","new_num":4}"#,
        ],
    );
}

// (f) Missing from/to params → 400 bad_request.
#[test]
fn gitparity_commit_diff_missing_revs() {
    assert_parity(
        "/api/git/commit-diff?path=greeting.txt",
        Norm::Exact,
        &[r#""code":"bad_request""#, r#""from and to are required""#],
    );
}

// Product behavior intentionally diverges from the frozen Go oracle here:
// history UI can request stale or shallow-clone revisions, which is client
// input state rather than a server fault. Keep that as 400 to avoid surfacing
// Internal Server Error during ordinary history navigation.
#[test]
fn rust_commit_diff_bad_rev_returns_bad_request() {
    let fx = GitFixture::build();
    let rust = RustServer::start(fx.root());
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);
    let response = http_request(
        &base,
        "GET",
        "/api/git/commit-diff?path=greeting.txt&from=deadbeef&to=e494c66",
        None,
    );
    rust.shutdown();

    let body = String::from_utf8_lossy(&response.body);
    assert_eq!(response.status, 400, "body: {body}");
    assert_eq!(
        response.header("content-type").as_deref(),
        Some("application/json; charset=utf-8"),
    );
    assert!(
        body.contains(r#""code":"invalid_revision""#),
        "body should classify bad revisions as client input errors: {body}",
    );
    assert!(
        body.contains(r#"cannot resolve from=\"deadbeef\": reference not found"#),
        "body should keep the resolver detail: {body}",
    );
}

#[test]
fn rust_changed_files_returns_range_manifest() {
    let fx = GitFixture::build();
    fx.git(&["reset", "--hard", "HEAD"], None);
    fx.git(&["checkout", "-q", "-b", "feature"], None);
    fx.write("review.txt", "one\ntwo\n");
    fx.git(&["add", "review.txt"], None);
    fx.commit("Add review file", "2020-05-06T07:08:09+00:00");
    fx.git(&["checkout", "-q", "main"], None);

    let rust = RustServer::start(fx.root());
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);
    let response = http_request(
        &base,
        "GET",
        "/api/git/changed-files?base=main&head=feature&mode=merge-base",
        None,
    );
    rust.shutdown();

    let body = String::from_utf8_lossy(&response.body);
    assert_eq!(response.status, 200, "body: {body}");
    assert_eq!(
        response.header("content-type").as_deref(),
        Some("application/json; charset=utf-8"),
    );
    assert!(body.contains(r#""requested_base":"main""#), "body: {body}");
    assert!(
        body.contains(r#""requested_head":"feature""#),
        "body: {body}"
    );
    assert!(body.contains(r#""mode":"merge-base""#), "body: {body}");
    assert!(body.contains(r#""effective_base":"#), "body: {body}");
    assert!(body.contains(r#""effective_head":"#), "body: {body}");
    assert!(body.contains(r#""merge_base":"#), "body: {body}");
    assert!(body.contains(r#""limit":1000"#), "body: {body}");
    assert!(body.contains(r#""truncated":false"#), "body: {body}");
    assert!(body.contains(r#""status":"added""#), "body: {body}");
    assert!(body.contains(r#""path":"review.txt""#), "body: {body}");
    assert!(body.contains(r#""additions":2"#), "body: {body}");
    assert!(
        !body.contains(r#""lines":"#) && !body.contains("@@"),
        "manifest must stay metadata-only: {body}",
    );
}

#[test]
fn rust_changed_files_rejects_invalid_inputs_without_mutating_repo() {
    let fx = GitFixture::build();
    fx.git(&["reset", "--hard", "HEAD"], None);
    let before_head = fx.git_capture(&["rev-parse", "HEAD"]);
    let before_status = fx.git_capture(&["status", "--porcelain"]);

    let rust = RustServer::start(fx.root());
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);
    let cases = [
        (
            "/api/git/changed-files?head=feature&mode=merge-base",
            "bad_request",
        ),
        (
            "/api/git/changed-files?base=main&mode=merge-base",
            "bad_request",
        ),
        (
            "/api/git/changed-files?base=main&head=feature&mode=sideways",
            "bad_request",
        ),
        (
            "/api/git/changed-files?base=-c.config&head=feature&mode=merge-base",
            "invalid_ref",
        ),
        (
            "/api/git/changed-files?base=main..evil&head=feature&mode=merge-base",
            "invalid_ref",
        ),
    ];
    for (path, code) in cases {
        let response = http_request(&base, "GET", path, None);
        let body = String::from_utf8_lossy(&response.body);
        assert_eq!(response.status, 400, "path: {path}, body: {body}");
        assert!(
            body.contains(&format!(r#""code":"{code}""#)),
            "path: {path}, body: {body}",
        );
    }
    rust.shutdown();

    assert_eq!(fx.git_capture(&["rev-parse", "HEAD"]), before_head);
    assert_eq!(fx.git_capture(&["status", "--porcelain"]), before_status);
}

#[test]
fn rust_git_diff_from_served_subdirectory() {
    let fx = GitFixture::build();
    let project = fx.root().join("project");
    std::fs::create_dir_all(project.join("src")).expect("create nested fixture dir");
    std::fs::write(project.join("src/nested.txt"), "one\ntwo\n").expect("write nested file");
    fx.git(&["add", "project/src/nested.txt"], None);
    fx.commit("Add nested project file", "2020-05-06T07:08:09+00:00");
    std::fs::write(project.join("src/nested.txt"), "one\nTWO\nthree\n").expect("dirty nested file");

    let rust = RustServer::start(&project);
    rust.wait_ready();
    let base = format!("http://127.0.0.1:{}", rust.port);
    let response = http_request(&base, "GET", "/api/git/diff?path=src/nested.txt", None);
    rust.shutdown();

    let body = String::from_utf8_lossy(&response.body);
    assert_eq!(response.status, 200, "body: {body}");
    assert!(
        body.contains(r#""path":"src/nested.txt""#),
        "body should keep served-root-relative path: {body}",
    );
    assert!(
        body.contains(r#"{"type":"del","text":"two","old_num":2}"#),
        "body should include HEAD-side line from nested file: {body}",
    );
    assert!(
        body.contains(r#"{"type":"add","text":"TWO","new_num":2}"#),
        "body should include worktree-side line from nested file: {body}",
    );
    assert!(
        body.contains(r#"{"type":"add","text":"three","new_num":3}"#),
        "body should include added worktree line from nested file: {body}",
    );
}

// ----------------------------------------------------------------------------
// Deterministic git fixture
// ----------------------------------------------------------------------------

/// A freshly-built deterministic git repository served as the root for both
/// servers. Lives under the system temp dir, uniquely named per test so the
/// per-case tests run in parallel without clobbering each other. Dropped (and
/// the directory removed) when the test ends.
struct GitFixture {
    dir: PathBuf,
}

impl GitFixture {
    fn root(&self) -> &Path {
        &self.dir
    }

    /// `git init` a unique temp dir and create the frozen commit sequence.
    ///
    /// Commit graph (oldest → newest), all with frozen identity `Parity Bot
    /// <parity@example.com>` and per-commit frozen dates:
    ///   1. e494c66  Add greeting              greeting.txt = alpha/beta/gamma
    ///   2. 1e6958a  Modify greeting and add…  greeting.txt edited + notes.txt
    ///   3. 5623208  Append epsilon            greeting.txt += epsilon
    ///   4. 70170c7  Add binary                image.bin (NUL byte → binary)
    ///   5. be1e194  Add big file              big.txt (6000 lines)
    /// Then the WORKTREE is dirtied (uncommitted): greeting.txt is edited
    /// (drives the worktree-vs-HEAD diff) and big.txt is rewritten so every
    /// line differs (drives the >5000-line truncation cap).
    fn build() -> Self {
        // Unique dir per fixture so parallel #[test]s don't collide.
        static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let pid = std::process::id();
        let dir = std::env::temp_dir().join(format!("ctx_gitparity_{pid}_{n}"));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("create fixture dir");

        let fx = GitFixture { dir };
        fx.init_repo();
        fx
    }

    fn init_repo(&self) {
        // git init + pin identity in repo config (belt-and-suspenders with the
        // per-command env so the fixture is identity-stable regardless of the
        // machine's global git config).
        self.git(&["init", "-q", "-b", "main", "."], None);
        self.git(&["config", "user.name", "Parity Bot"], None);
        self.git(&["config", "user.email", "parity@example.com"], None);

        // Commit 1.
        self.write("greeting.txt", "alpha\nbeta\ngamma\n");
        self.git(&["add", "greeting.txt"], None);
        self.commit("Add greeting", "2020-01-02T03:04:05+00:00");

        // Commit 2.
        self.write("greeting.txt", "alpha\nBETA\ngamma\ndelta\n");
        self.write("notes.txt", "second file\n");
        self.git(&["add", "greeting.txt", "notes.txt"], None);
        self.commit("Modify greeting and add notes", "2020-02-03T04:05:06+00:00");

        // Commit 3.
        self.write("greeting.txt", "alpha\nBETA\ngamma\ndelta\nepsilon\n");
        self.git(&["add", "greeting.txt"], None);
        self.commit("Append epsilon", "2020-03-04T05:06:07+00:00");

        // Commit 4: binary (NUL byte within first 8000 bytes → binary sniff).
        std::fs::write(
            self.dir.join("image.bin"),
            b"PNG\x00\x01\x02binarydata\n".as_slice(),
        )
        .expect("write image.bin");
        self.git(&["add", "image.bin"], None);
        self.commit("Add binary", "2020-03-15T01:02:03+00:00");

        // Commit 5: large file (6000 lines) so the diff later exceeds 5000.
        let mut big = String::new();
        for i in 0..6000 {
            big.push_str(&format!("line {i}\n"));
        }
        self.write("big.txt", &big);
        self.git(&["add", "big.txt"], None);
        self.commit("Add big file", "2020-04-05T06:07:08+00:00");

        // Dirty the worktree (uncommitted) — drives diff & truncation cases.
        self.write(
            "greeting.txt",
            "alpha\nBETA-worktree\ngamma\ndelta\nepsilon\nzeta\n",
        );
        let mut big2 = String::new();
        for i in 0..6000 {
            big2.push_str(&format!("LINE {i}\n"));
        }
        self.write("big.txt", &big2);
    }

    fn write(&self, rel: &str, contents: &str) {
        std::fs::write(self.dir.join(rel), contents).unwrap_or_else(|e| panic!("write {rel}: {e}"));
    }

    /// A commit with frozen author+committer name/email/date so the resulting
    /// SHA-1 is reproducible.
    fn commit(&self, msg: &str, date: &str) {
        let env: &[(&str, &str)] = &[
            ("GIT_AUTHOR_NAME", "Parity Bot"),
            ("GIT_AUTHOR_EMAIL", "parity@example.com"),
            ("GIT_COMMITTER_NAME", "Parity Bot"),
            ("GIT_COMMITTER_EMAIL", "parity@example.com"),
            ("GIT_AUTHOR_DATE", date),
            ("GIT_COMMITTER_DATE", date),
        ];
        self.git(&["commit", "-q", "-m", msg], Some(env));
    }

    fn git(&self, args: &[&str], env: Option<&[(&str, &str)]>) {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(&self.dir);
        // Neutralize ambient git config that could perturb hashes (signing,
        // template hooks, autocrlf). Identity is pinned per-commit above.
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0");
        if let Some(pairs) = env {
            for (k, v) in pairs {
                cmd.env(k, v);
            }
        }
        let status = cmd
            .status()
            .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
        assert!(status.success(), "git {args:?} exited with {status}");
    }

    fn git_capture(&self, args: &[&str]) -> String {
        let mut cmd = Command::new("git");
        cmd.args(args).current_dir(&self.dir);
        cmd.env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_SYSTEM", "/dev/null")
            .env("GIT_TERMINAL_PROMPT", "0");
        let output = cmd
            .output()
            .unwrap_or_else(|e| panic!("git {args:?} failed to spawn: {e}"));
        assert!(
            output.status.success(),
            "git {args:?} exited with {}",
            output.status
        );
        String::from_utf8(output.stdout).expect("git stdout utf8")
    }
}

impl Drop for GitFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}

// ----------------------------------------------------------------------------
// Minimal blocking HTTP client (copied from parity.rs)
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

fn dechunk(body: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut pos = 0usize;
    while pos < body.len() {
        let line_end = match find_subslice(&body[pos..], b"\r\n") {
            Some(i) => pos + i,
            None => break,
        };
        let size_line = &body[pos..line_end];
        let size_hex = size_line.split(|&b| b == b';').next().unwrap_or(size_line);
        let size_str = String::from_utf8_lossy(size_hex);
        let size = match usize::from_str_radix(size_str.trim(), 16) {
            Ok(n) => n,
            Err(_) => break,
        };
        pos = line_end + 2;
        if size == 0 {
            break;
        }
        if pos + size > body.len() {
            out.extend_from_slice(&body[pos..]);
            break;
        }
        out.extend_from_slice(&body[pos..pos + size]);
        pos += size + 2;
    }
    out
}

fn find_subslice(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack.windows(needle.len()).position(|w| w == needle)
}

// ----------------------------------------------------------------------------
// Go server lifecycle (copied from parity.rs)
// ----------------------------------------------------------------------------

struct GoServer {
    child: Child,
    addr: String,
}

impl GoServer {
    fn start(bin: &Path, fixture: &Path) -> Self {
        let mut child = Command::new(bin)
            .arg("browse")
            .arg(fixture)
            .arg("--no-open")
            .arg("--port")
            .arg("0")
            .arg("--bind")
            .arg("127.0.0.1")
            .arg("--no-register")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn go server");

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
// Rust server lifecycle (in-process tokio runtime, copied from parity.rs)
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
// Shared helpers (copied from parity.rs)
// ----------------------------------------------------------------------------

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

/// Locate the Go oracle binary: `$CTX_GO_BIN`, else `/tmp/ctx-go`, else build
/// it via `go build` into the repo's target dir.
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
