//! `ctx pack` line-range excerpts: a selection entry `path:START-END` (or
//! `path#LSTART-LEND`) packs just those lines, and the pack contract records
//! them under their real line numbers — so the body of a function an
//! `--overflow signatures` digest pointed at can be packed on its own and
//! `ctx contract verify` checks citations against exactly what was shown.

use serde_json::Value;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};

mod common;
use common::test_dir;

fn ctx(root: &Path, args: &[&str], stdin: Option<&str>, envs: &[(&str, &str)]) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_ctx"));
    cmd.args(args)
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in envs {
        cmd.env(key, value);
    }
    let mut child = cmd.spawn().expect("spawn ctx");
    {
        let mut pipe = child.stdin.take().unwrap();
        if let Some(input) = stdin {
            pipe.write_all(input.as_bytes()).unwrap();
        }
    }
    child.wait_with_output().expect("wait ctx")
}

fn ok(out: Output) -> String {
    assert!(
        out.status.success(),
        "ctx failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// `src/auth/session.rs` holds 120 six-line functions: `session_fn_i`
/// starts on line `6 * i + 1`.
fn fixture(name: &str) -> PathBuf {
    let root = test_dir(name);
    fs::create_dir_all(root.join("src/auth")).unwrap();
    let big: String = (0..120)
        .map(|i| {
            format!(
                "pub fn session_fn_{i}(user: &str,\n    ttl: u64) -> bool {{\n    let x = {i};\n    x > 0 && user.len() > {i} && ttl > 3\n}}\n\n"
            )
        })
        .collect();
    fs::write(root.join("src/auth/session.rs"), big).unwrap();
    fs::write(
        root.join("src/auth/login.rs"),
        "pub fn login() -> bool { true }\n",
    )
    .unwrap();
    root
}

fn verify(root: &Path, pack: &Path, response: &str) -> (bool, String) {
    let resp = root.join("response.txt");
    fs::write(&resp, response).unwrap();
    let out = ctx(
        root,
        &[
            "contract",
            "verify",
            pack.to_str().unwrap(),
            "--response",
            resp.to_str().unwrap(),
            "--check-worktree",
            "--root",
            root.to_str().unwrap(),
        ],
        None,
        &[],
    );
    (
        out.status.success(),
        String::from_utf8_lossy(&out.stdout).into_owned(),
    )
}

#[test]
fn from_stdin_line_ranges_pack_only_those_lines() {
    let root = fixture("pack-excerpt-stdin");
    let md = ok(ctx(
        &root,
        &["pack", "--from-stdin", "--no-contract", "--budget", "400"],
        Some("src/auth/session.rs:13-17\nsrc/auth/login.rs\nsrc/auth/session.rs#L601-L602\n"),
        &[],
    ));
    // The whole file is ~4k tokens: only the excerpt is budgeted.
    assert!(
        md.contains("- src/auth/session.rs (") && md.contains("lines 13-17, 601-602 of 720)"),
        "{md}"
    );
    assert!(md.contains(
        "[ctx: excerpt - src/auth/session.rs lines 13-17 of 720; the full file is not in this pack]\npub fn session_fn_2(user: &str,\n"
    ));
    assert!(md.contains("[ctx: src/auth/session.rs lines 601-602]\npub fn session_fn_100(user: &str,\n    ttl: u64) -> bool {\n"));
    assert!(!md.contains("session_fn_3("), "{md}");
    assert!(md.contains("pub fn login()"), "{md}");

    let v: Value = serde_json::from_str(&ok(ctx(
        &root,
        &["pack", "--from-stdin", "--json", "--no-warnings"],
        Some("src/auth/session.rs:13-17\nsrc/auth/session.rs:900-910\n"),
        &[],
    )))
    .unwrap();
    assert_eq!(v["included"][0]["lines"], "lines 13-17 of 720", "{v}");
    assert_eq!(
        v["skipped"][0]["path"], "src/auth/session.rs:900-910",
        "{v}"
    );
    assert!(v["skipped"][0]["reason"]
        .as_str()
        .unwrap()
        .contains("past the end of the file (720 lines)"));
}

#[test]
fn a_whole_file_entry_wins_and_api_only_is_rejected() {
    let root = fixture("pack-excerpt-whole");
    let md = ok(ctx(
        &root,
        &["pack", "--from-stdin", "--no-contract"],
        Some("src/auth/login.rs:1\nsrc/auth/login.rs\n"),
        &[],
    ));
    assert!(
        md.contains("- src/auth/login.rs (") && !md.contains("[ctx: excerpt"),
        "{md}"
    );

    let out = ctx(
        &root,
        &["pack", "--from-stdin", "--api-only"],
        Some("src/auth/session.rs:1-5\n"),
        &[],
    );
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("do not combine with --api-only"));
}

#[test]
fn signature_digest_then_excerpt_round_trips_through_contract_verify() {
    let root = fixture("pack-excerpt-contract");

    // 1. The digest is too small for the whole file; its contract records
    //    the declaration lines under their real numbers.
    let digest = root.join("digest.md");
    ok(ctx(
        &root,
        &[
            "pack",
            "--goal",
            "session",
            "--budget",
            "3000",
            "--overflow",
            "signatures",
            "--no-warnings",
            "--out",
            digest.to_str().unwrap(),
        ],
        None,
        &[],
    ));
    let text = fs::read_to_string(&digest).unwrap();
    assert!(text.contains("13: pub fn session_fn_2(user: &str,\n14:     ttl: u64) -> bool {\n"));
    let (passed, report) = verify(
        &root,
        &digest,
        "session_fn_2 is declared at src/auth/session.rs:L13-L14.\n",
    );
    assert!(passed, "{report}");
    let (passed, report) = verify(
        &root,
        &digest,
        "Its body, src/auth/session.rs:L15-L16, checks ttl.\n",
    );
    assert!(!passed, "the digest never showed the body: {report}");
    assert!(
        report.contains("referenced lines were not in the pack"),
        "{report}"
    );

    // 2. Pack the body the model asked for; citing it now verifies.
    let excerpt = root.join("excerpt.md");
    ok(ctx(
        &root,
        &["pack", "--from-stdin", "--out", excerpt.to_str().unwrap()],
        Some("src/auth/session.rs:13-17\n"),
        &[],
    ));
    let cite = "src/auth/session.rs:L15-L16 checks ttl; see src/auth/session.rs.\n";
    let (passed, report) = verify(&root, &excerpt, cite);
    assert!(passed, "{report}");

    // 3. An edit outside the excerpt keeps it current; one inside is stale.
    let path = root.join("src/auth/session.rs");
    let original = fs::read_to_string(&path).unwrap();
    fs::write(&path, original.replace("let x = 50;", "let x = 51;")).unwrap();
    assert!(verify(&root, &excerpt, cite).0);
    fs::write(&path, original.replace("let x = 2;", "let x = 3;")).unwrap();
    let (passed, report) = verify(&root, &excerpt, cite);
    assert!(!passed, "{report}");
    assert!(report.contains("stale"), "{report}");
}

#[test]
fn mixes_keep_line_ranges() {
    let root = fixture("pack-excerpt-mix");
    let state = root.join("state");
    fs::create_dir_all(&state).unwrap();
    let env = [("XDG_STATE_HOME", state.to_str().unwrap())];
    ok(ctx(
        &root,
        &[
            "mix",
            "save",
            "body",
            "src/auth/session.rs#L13-L17",
            "src/auth/login.rs",
        ],
        None,
        &env,
    ));
    let show = ok(ctx(&root, &["mix", "show", "body"], None, &env));
    assert!(show.contains("  src/auth/session.rs:13-17\n"), "{show}");
    assert!(!show.contains("missing here"), "{show}");

    let md = ok(ctx(
        &root,
        &["pack", "--from-mix", "body", "--no-contract"],
        None,
        &env,
    ));
    assert!(md.contains("lines 13-17 of 720)"), "{md}");
    assert!(!md.contains("session_fn_3("), "{md}");

    let out = ctx(&root, &["mix", "add", "body", "src/auth:1-3"], None, &env);
    assert!(!out.status.success());
    assert!(String::from_utf8_lossy(&out.stderr).contains("a line range needs a file"));

    // Dropping the file removes its excerpts too.
    ok(ctx(
        &root,
        &["mix", "drop", "body", "src/auth/session.rs"],
        None,
        &env,
    ));
    let show = ok(ctx(&root, &["mix", "show", "body"], None, &env));
    assert!(!show.contains("session.rs"), "{show}");
}
