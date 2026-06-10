// crates/ctx-scan/tests/regression.rs
//
// Pin edge cases discovered during the Phase 1 port. Pioneer learned
// that surface-level parity tests miss several classes of bug
// (Unicode boundaries, NUL bytes, very long lines); we proactively
// pin them here so a regression cannot land silently.

use ctx_scan::scan::{scan_file_with_options, scan_files_with_options};
use ctx_scan::types::Options;

use std::io::Write;
use std::path::PathBuf;

fn write_temp(name: &str, content: &[u8]) -> PathBuf {
    let mut dir = std::env::temp_dir();
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    dir.push(format!("ctx-scan-regression-{nanos}-{name}"));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("input.txt");
    let mut f = std::fs::File::create(&path).unwrap();
    f.write_all(content).unwrap();
    path
}

fn sample_aws_access_key() -> String {
    ["AKIA", "IOSFODNN7EXAMPLE"].concat()
}

fn sample_gcp_api_key() -> String {
    ["AIza", "12345678901234567890123456789012345"].concat()
}

/// R-01: Empty file produces no warnings and no error.
#[test]
fn empty_file_emits_no_warnings() {
    let path = write_temp("r01", b"");
    let warnings = scan_file_with_options(
        &path.to_string_lossy(),
        &Options::default(),
    )
    .unwrap();
    assert!(warnings.is_empty());
}

/// R-02: A line with multiple secrets emits at most one warning
/// (mirrors the Go `break` after first match).
#[test]
fn first_match_wins_per_line() {
    let content = format!(
        "aws=\"{}\" gcp=\"{}\"",
        sample_aws_access_key(),
        sample_gcp_api_key()
    );
    let path = write_temp("r02", content.as_bytes());
    let w = scan_file_with_options(&path.to_string_lossy(), &Options::default())
        .unwrap();
    assert_eq!(w.len(), 1, "expected 1, got {w:?}");
    assert_eq!(w[0].kind, "aws_access_key");
}

/// R-03: A very long line (>1 MiB) must not crash or truncate
/// behaviour. We don't expect a match in junk filler; the test pins
/// the absence of an exception.
#[test]
fn very_long_line_does_not_panic() {
    let mut body = b"prefix=".to_vec();
    body.extend(std::iter::repeat(b'a').take(2 * 1024 * 1024));
    body.extend_from_slice(b"\n");
    let path = write_temp("r03", &body);
    let w = scan_file_with_options(&path.to_string_lossy(), &Options::default())
        .unwrap();
    // No regex in our set fires on a million 'a's, so this must come
    // back empty. The important assertion is the absence of a panic.
    assert!(w.is_empty());
}

/// R-04: NUL bytes embedded in the file MUST NOT terminate scanning
/// early. BufRead::lines splits on `\n` only.
#[test]
fn embedded_nul_byte_does_not_terminate_scan() {
    let mut body = Vec::new();
    body.extend_from_slice(b"prefix\x00middle\n");
    body.extend_from_slice(format!("aws=\"{}\"\n", sample_aws_access_key()).as_bytes());
    let path = write_temp("r04", &body);
    let w = scan_file_with_options(&path.to_string_lossy(), &Options::default())
        .unwrap();
    assert_eq!(w.len(), 1, "{w:?}");
    assert_eq!(w[0].kind, "aws_access_key");
    assert_eq!(w[0].line, 2);
}

/// R-05: Unicode content (non-ASCII multi-byte runes) must not
/// destabilise preview() or entropy candidates.
#[test]
fn unicode_line_with_secret_emits_clean_preview() {
    // The Japanese characters before the assignment ensure the regex
    // anchor (^? \b?) sees a non-ASCII left boundary.
    let line = format!("コメント aws=\"{}\"\n", sample_aws_access_key());
    let path = write_temp("r05", line.as_bytes());
    let w = scan_file_with_options(&path.to_string_lossy(), &Options::default())
        .unwrap();
    assert_eq!(w.len(), 1, "{w:?}");
    // Preview must be valid UTF-8 and shaped like `AKIA[...]MPLE`.
    let p = &w[0].preview;
    assert!(p.starts_with("AKIA"), "{p}");
    assert!(p.ends_with("MPLE"), "{p}");
}

/// R-06: A path under an allowlist_files glob short-circuits the
/// scan, returning an empty result without opening the file.
#[test]
fn allowlist_files_short_circuits_scan() {
    // Don't even create a file; the function must early-return before
    // touching the disk when the glob matches the (string) path.
    let opts = Options {
        allowlist_files: vec!["tests/fixtures/**".to_string()],
        ..Default::default()
    };
    let w = scan_file_with_options(
        "tests/fixtures/never-exists.txt",
        &opts,
    )
    .unwrap();
    assert!(w.is_empty());
}

/// R-07: scan_files swallows per-file errors (Go's `continue`
/// semantics). A non-existent path must NOT abort the whole batch.
#[test]
fn scan_files_skips_missing_paths() {
    let real = write_temp(
        "r07",
        format!("aws=\"{}\"", sample_aws_access_key()).as_bytes(),
    );
    let paths = vec![
        "definitely-not-a-real-path.txt".to_string(),
        real.to_string_lossy().into_owned(),
    ];
    let w = scan_files_with_options(&paths, &Options::default());
    assert_eq!(w.len(), 1);
    assert_eq!(w[0].kind, "aws_access_key");
}
