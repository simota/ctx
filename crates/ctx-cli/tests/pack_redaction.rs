//! `ctx pack` must not emit secrets that its scanner can see: the whole
//! private-key block, `--api-only` output, and keys written next to non-ASCII
//! text are all redacted.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

mod common;
use common::test_dir;

fn fixture(name: &str, files: &[(&str, &str)]) -> PathBuf {
    let root = test_dir(name);
    fs::create_dir_all(&root).unwrap();
    for (rel, body) in files {
        let path = root.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    let status = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success());
    root
}

fn pack(root: &Path, args: &[&str]) -> String {
    let mut full = vec!["pack", ".", "--no-contract"];
    full.extend_from_slice(args);
    ctx(root, &full, None)
}

fn ctx(root: &Path, args: &[&str], stdin: Option<&str>) -> String {
    use std::io::Write;
    use std::process::Stdio;

    let mut child = Command::new(env!("CARGO_BIN_EXE_ctx"))
        .args(args)
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    let mut pipe = child.stdin.take().unwrap();
    pipe.write_all(stdin.unwrap_or_default().as_bytes())
        .unwrap();
    drop(pipe);
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "ctx {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn git(root: &Path, args: &[&str]) {
    let status = Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
        .status()
        .unwrap();
    assert!(status.success(), "git {args:?} failed");
}

fn sample_token() -> String {
    ["ghp_", "123456789012345678901234567890123456"].concat()
}

// Split so this file does not itself look like a key to scanners.
fn pem(label: &str, body: &str) -> String {
    format!("-----BEGIN {label}-----\n{body}\n-----END {label}-----\n")
}

#[test]
fn pack_redacts_private_key_body_and_end_line() {
    let key = pem(
        "RSA PRIVATE KEY",
        "MIIEpAIBAAKCAQEAkeybodyline1\nZZZkeybodyline2ZZZ",
    );
    let root = fixture(
        "pack-redact-pem",
        &[("key.go", &format!("package key\n\nconst k = `\n{key}`\n"))],
    );
    let out = pack(&root, &[]);
    assert!(out.contains("kind=private_key"), "{out}");
    assert!(!out.contains("keybodyline"), "key body leaked:\n{out}");
    assert!(!out.contains("-----END RSA"), "END line leaked:\n{out}");
    // The line after the block is kept.
    assert!(out.contains("`\n"), "{out}");
}

#[test]
fn pack_detects_pkcs8_encrypted_and_pgp_private_keys() {
    let enc = pem("ENCRYPTED PRIVATE KEY", "MIIFencryptedbody");
    let pgp = pem("PGP PRIVATE KEY BLOCK", "lQOYBpgpbody");
    let root = fixture(
        "pack-redact-pem-kinds",
        &[
            ("enc.go", &format!("package a\n\nconst e = `\n{enc}`\n")),
            ("pgp.go", &format!("package a\n\nconst p = `\n{pgp}`\n")),
        ],
    );
    let out = pack(&root, &[]);
    assert!(!out.contains("encryptedbody"), "PKCS#8 key leaked:\n{out}");
    assert!(!out.contains("pgpbody"), "PGP key leaked:\n{out}");
}

#[test]
fn pack_api_only_redacts_exported_secrets() {
    let token = ["ghp_", "123456789012345678901234567890123456"].concat();
    let root = fixture(
        "pack-redact-api-only",
        &[(
            "a.ts",
            &format!("export const GITHUB_TOKEN = \"{token}\";\nexport function f() {{}}\n"),
        )],
    );
    let out = pack(&root, &["--api-only"]);
    assert!(!out.contains(&token), "token leaked:\n{out}");
    assert!(out.contains("kind=github_pat"), "{out}");
    assert!(out.contains("export function f()"), "{out}");
}

#[test]
fn pack_redacts_keys_adjacent_to_cjk_text() {
    let aws = ["AKIA", "IOSFODNN7EXAMPLE"].concat();
    let root = fixture(
        "pack-redact-cjk",
        &[(
            "a.ts",
            &format!("// キーは{aws}です\nexport const x = 1;\n"),
        )],
    );
    let out = pack(&root, &[]);
    assert!(!out.contains(&aws), "key leaked:\n{out}");
    assert!(out.contains("kind=aws_access_key"), "{out}");
}

#[test]
fn pack_keeps_code_after_a_header_mentioned_in_source() {
    let root = fixture(
        "pack-redact-header-mention",
        &[(
            "parse.rs",
            "fn check(l: &str) -> bool {\n    l.starts_with(\"-----BEGIN RSA PRIVATE KEY-----\")\n}\n\nfn still_visible() {}\n",
        )],
    );
    let out = pack(&root, &[]);
    assert!(out.contains("kind=private_key"), "{out}");
    assert!(
        out.contains("fn still_visible() {}"),
        "code after the header lost:\n{out}"
    );
}

#[test]
fn pack_from_where_redacts_secrets() {
    let token = sample_token();
    let root = fixture(
        "pack-redact-from-where",
        &[("lib.rs", &format!("const T: &str = \"{token}\";\n"))],
    );
    let out = ctx(
        &root,
        &["pack", "--from-where", "--no-contract"],
        Some("lib.rs\n"),
    );
    assert!(out.contains("lib.rs"), "{out}");
    assert!(!out.contains(&token), "token leaked:\n{out}");
    assert!(out.contains("kind=github_pat"), "{out}");
}

#[test]
fn pack_diff_redacts_secrets_in_contents_and_patch() {
    let token = sample_token();
    let root = fixture("pack-redact-diff", &[("lib.rs", "fn a() {}\n")]);
    git(&root, &["add", "."]);
    git(&root, &["commit", "-qm", "one"]);
    fs::write(
        root.join("lib.rs"),
        format!("fn a() {{}}\nconst T: &str = \"{token}\";\n"),
    )
    .unwrap();
    git(&root, &["commit", "-qam", "two"]);
    for layout in ["", "--layout=unified", "--layout=side-by-side"] {
        let mut args = vec!["pack", "--diff", "HEAD~1..HEAD"];
        if !layout.is_empty() {
            args.push(layout);
        }
        let out = ctx(&root, &args, None);
        assert!(out.contains("lib.rs"), "{out}");
        assert!(!out.contains(&token), "token leaked ({layout}):\n{out}");
    }
}

#[test]
fn focus_redacts_secrets() {
    let token = sample_token();
    let root = fixture(
        "focus-redact",
        &[(
            "lib.rs",
            &format!("pub fn anchor_fn() {{}}\nconst T: &str = \"{token}\";\n"),
        )],
    );
    let out = ctx(&root, &["focus", "lib.rs"], None);
    assert!(out.contains("anchor_fn"), "{out}");
    assert!(!out.contains(&token), "token leaked:\n{out}");
}
