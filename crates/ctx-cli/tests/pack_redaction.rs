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
    let output = Command::new(env!("CARGO_BIN_EXE_ctx"))
        .arg("pack")
        .arg(".")
        .arg("--no-contract")
        .args(args)
        .current_dir(root)
        .env("CTX_GO_BIN", "/definitely/missing/ctx-go")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "ctx pack failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
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
