// crates/ctx-scan/src/scan.rs
//
// Port of internal/scan/secret.go's scanning core. Function names use
// Rust snake_case; the public functions documented below preserve a
// 1:1 mapping to the Go API so the parity oracle is straightforward.
//
//   Go                                   Rust
//   ---------------------------------    ---------------------------------
//   ScanFile(path)                       scan_file(path)
//   ScanFileWithOptions(path, opts)      scan_file_with_options(path, opts)
//   ScanFiles(paths)                     scan_files(paths)
//   ScanFilesWithOptions(paths, opts)    scan_files_with_options(paths, opts)
//
// LINE-OF-MATCH SEMANTICS (must match Go exactly)
// ===============================================
// Go's `bufio.Scanner` strips the trailing newline. `BufRead::lines`
// does the same — both yield logical lines without `\n`. The line
// number reported is 1-based and increments per record returned by
// the iterator (matching `lineNo++` in the Go loop).
//
// EARLY-EXIT SEMANTICS
// ====================
// Go's loop breaks out of the inner `for _, pattern := range
// secretPatterns` on the first match (after the allowlist check). We
// preserve that: at most one regex warning per line; ordering decides
// which kind wins.
//
// PATH NORMALISATION FOR allowlist_files
// ======================================
// Go's `filepath.ToSlash` converts `\\` to `/` on Windows. We mirror
// with `.replace('\\', "/")`. On unix this is a no-op; on windows it
// matches Go's normalisation.

use std::fs;
use std::io::{BufRead, BufReader};

use crate::patterns::secret_patterns;
use crate::types::{Options, Warning};

/// Default helper mirroring Go's `ScanFile(path string)`.
pub fn scan_file(path: &str) -> std::io::Result<Vec<Warning>> {
    scan_file_with_options(path, &Options::default())
}

/// Main entry point. Mirrors `ScanFileWithOptions(path, opts)`.
pub fn scan_file_with_options(path: &str, opts: &Options) -> std::io::Result<Vec<Warning>> {
    if allowlisted_file(path, &opts.allowlist_files) {
        return Ok(Vec::new());
    }
    let file = fs::File::open(path)?;
    let mut reader = BufReader::new(file);

    let mut warnings: Vec<Warning> = Vec::new();
    let mut line_no: i64 = 0;
    // read_until + from_utf8_lossy instead of BufRead::lines(): lines()
    // errors on the first non-UTF8 line, aborting the whole scan and losing
    // already-found warnings. Binary-ish lines are scanned lossily instead.
    let mut raw = Vec::new();
    loop {
        raw.clear();
        if reader.read_until(b'\n', &mut raw)? == 0 {
            break;
        }
        // Strip the trailing newline / CRLF like BufRead::lines (and Go's
        // bufio.Scanner) does.
        if raw.last() == Some(&b'\n') {
            raw.pop();
            if raw.last() == Some(&b'\r') {
                raw.pop();
            }
        }
        let line = String::from_utf8_lossy(&raw);
        let line = line.as_ref();
        line_no += 1;

        // First match wins per the Go `break`.
        for pattern in secret_patterns() {
            if let Some(m) = pattern.re.find(&line) {
                let matched = m.as_str();
                if allowlisted(matched, &line, &opts.allowlist) {
                    continue;
                }
                warnings.push(Warning {
                    path: path.to_string(),
                    line: line_no,
                    kind: pattern.kind.to_string(),
                    severity: pattern.severity.to_string(),
                    message: "secret-like pattern detected".to_string(),
                    preview: preview(matched),
                });
                break;
            }
        }

        if opts.enable_entropy {
            for token in entropy_candidates(&line) {
                if allowlisted(&token, &line, &opts.allowlist) {
                    continue;
                }
                if token.chars().count() >= 20 && crate::entropy::shannon_entropy(&token) >= 4.0 {
                    warnings.push(Warning {
                        path: path.to_string(),
                        line: line_no,
                        kind: "high_entropy".to_string(),
                        severity: "low".to_string(),
                        message: "high-entropy string detected".to_string(),
                        preview: preview(&token),
                    });
                    break;
                }
            }
        }
    }
    Ok(warnings)
}

/// Mirrors `ScanFiles(paths)`.
pub fn scan_files(paths: &[String]) -> Vec<Warning> {
    scan_files_with_options(paths, &Options::default())
}

/// Mirrors `ScanFilesWithOptions(paths, opts)`. Go swallows per-file
/// errors with `continue`; we mirror that policy (a missing file does
/// not abort the batch).
pub fn scan_files_with_options(paths: &[String], opts: &Options) -> Vec<Warning> {
    let mut all = Vec::new();
    for p in paths {
        if let Ok(w) = scan_file_with_options(p, opts) {
            all.extend(w);
        }
    }
    all
}

/// Mirrors `allowlisted(match, line, allowlist)` — empty entries are
/// skipped (matching Go's `if allowed == "" { continue }`).
fn allowlisted(matched: &str, line: &str, allowlist: &[String]) -> bool {
    for allowed in allowlist {
        if allowed.is_empty() {
            continue;
        }
        if matched.contains(allowed.as_str()) || line.contains(allowed.as_str()) {
            return true;
        }
    }
    false
}

/// Mirrors `allowlistedFile(path, patterns)`. The two trailing `/**`
/// branches in the Go source are preserved here as `prefix-style` and
/// `contains-style` glob shortcuts.
fn allowlisted_file(path: &str, patterns: &[String]) -> bool {
    let slash = path.replace('\\', "/");
    for pattern in patterns {
        if pattern.is_empty() {
            continue;
        }
        let pat = pattern.replace('\\', "/");
        if glob_match(&pat, &slash) {
            return true;
        }
        if let Some(prefix) = pat.strip_suffix("/**") {
            if slash.starts_with(&format!("{prefix}/")) {
                return true;
            }
            if slash.contains(&format!("/{prefix}/")) {
                return true;
            }
        }
    }
    false
}

/// Minimal `filepath.Match`-style glob matcher used by allowlist_files.
/// Supports `*`, `?`, literal segments, and a tiny `[abc]` class form.
/// `*` does NOT cross the `/` separator, matching Go's semantics.
fn glob_match(pattern: &str, name: &str) -> bool {
    glob_match_inner(pattern.as_bytes(), name.as_bytes())
}

fn glob_match_inner(pattern: &[u8], name: &[u8]) -> bool {
    let mut p = 0;
    let mut n = 0;
    let mut star_p: Option<usize> = None;
    let mut star_n: usize = 0;

    while n < name.len() {
        if p < pattern.len() {
            let pc = pattern[p];
            if pc == b'*' {
                star_p = Some(p);
                star_n = n;
                p += 1;
                continue;
            }
            if pc == b'?' && name[n] != b'/' {
                p += 1;
                n += 1;
                continue;
            }
            if pc == b'[' {
                if let Some(off) = pattern[p + 1..].iter().position(|&b| b == b']') {
                    let class = &pattern[p + 1..p + 1 + off];
                    if class.contains(&name[n]) && name[n] != b'/' {
                        p += off + 2;
                        n += 1;
                        continue;
                    }
                }
            } else if pc == name[n] {
                p += 1;
                n += 1;
                continue;
            }
        }
        if let Some(sp) = star_p {
            if name[star_n] == b'/' {
                return false;
            }
            p = sp + 1;
            star_n += 1;
            n = star_n;
            continue;
        }
        return false;
    }
    while p < pattern.len() && pattern[p] == b'*' {
        p += 1;
    }
    p == pattern.len()
}

/// Mirrors `preview(s string) string` — the redaction-friendly snippet
/// stored in `Warning.preview`. Note: Go indexes by bytes (`s[:4]`),
/// not runes. We do the same with `as_bytes()` to preserve byte-exact
/// output. If a non-ASCII rune straddles offset 4 we'd produce
/// invalid UTF-8 here, so we step back to the nearest UTF-8 boundary.
fn preview(s: &str) -> String {
    if s.len() <= 12 {
        return s.to_string();
    }
    let bytes = s.as_bytes();
    let head = take_utf8_prefix(bytes, 4);
    let tail = take_utf8_suffix(bytes, 4);
    format!("{head}[...]{tail}")
}

fn take_utf8_prefix(bytes: &[u8], n: usize) -> &str {
    let mut end = n.min(bytes.len());
    while end > 0 && !is_char_boundary(bytes, end) {
        end -= 1;
    }
    std::str::from_utf8(&bytes[..end]).unwrap_or("")
}

fn take_utf8_suffix(bytes: &[u8], n: usize) -> &str {
    let len = bytes.len();
    let mut start = if len >= n { len - n } else { 0 };
    while start < len && !is_char_boundary(bytes, start) {
        start += 1;
    }
    std::str::from_utf8(&bytes[start..]).unwrap_or("")
}

fn is_char_boundary(bytes: &[u8], pos: usize) -> bool {
    if pos == 0 || pos == bytes.len() {
        return true;
    }
    let b = bytes[pos];
    !(b & 0xC0 == 0x80)
}

/// Mirrors `entropyCandidates(line string) []string`.
///
/// Go: `strings.FieldsFunc` splits on every rune that's not a
/// letter / digit / one of `_-+/=`. We perform the same partition.
fn entropy_candidates(line: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for r in line.chars() {
        if r.is_alphabetic() || r.is_numeric() || matches!(r, '_' | '-' | '+' | '/' | '=') {
            current.push(r);
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn sample_aws_access_key() -> String {
        ["AKIA", "IOSFODNN7EXAMPLE"].concat()
    }

    fn sample_gcp_api_key() -> String {
        ["AIza", "12345678901234567890123456789012345"].concat()
    }

    fn sample_github_pat() -> String {
        ["ghp_", "123456789012345678901234567890123456"].concat()
    }

    fn sample_slack_token() -> String {
        ["xoxb-", "1234567890-abcdefghijklmnop"].concat()
    }

    fn sample_private_key_header() -> String {
        ["-----BEGIN ", "PRIVATE KEY-----"].concat()
    }

    fn write_temp(content: &str) -> String {
        let mut dir = std::env::temp_dir();
        let pid = std::process::id();
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        dir.push(format!("ctx-scan-test-{pid}-{nanos}"));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("secrets.txt");
        let mut f = std::fs::File::create(&path).unwrap();
        f.write_all(content.as_bytes()).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn detects_all_canonical_kinds() {
        // Mirror of TestScanFileDetectsSecretPatterns in Go's
        // internal/scan/secret_test.go.
        let content = format!(
            "aws=\"{}\"\ngcp=\"{}\"\ngithub=\"{}\"\nslack=\"{}\"\n{}",
            sample_aws_access_key(),
            sample_gcp_api_key(),
            sample_github_pat(),
            sample_slack_token(),
            sample_private_key_header(),
        );
        let path = write_temp(&content);
        let warnings = scan_file(&path).unwrap();
        let kinds: std::collections::HashSet<&str> =
            warnings.iter().map(|w| w.kind.as_str()).collect();
        for want in [
            "aws_access_key",
            "gcp_api_key",
            "github_pat",
            "slack_token",
            "private_key",
        ] {
            assert!(kinds.contains(want), "missing {want} in {warnings:?}");
        }
        for w in &warnings {
            assert!(!w.severity.is_empty());
            assert!(w.line >= 1);
        }
    }

    #[test]
    fn allowlist_skips_known_value() {
        // Mirror of TestScanFileAllowlist.
        let key = sample_aws_access_key();
        let path = write_temp(&format!("aws=\"{key}\""));
        let opts = Options {
            allowlist: vec![key],
            ..Default::default()
        };
        let w = scan_file_with_options(&path, &opts).unwrap();
        assert!(w.is_empty(), "{w:?}");
    }

    #[test]
    fn entropy_opt_in_fires_on_random_string() {
        // Mirror of TestScanFileEntropyOptIn.
        let path = write_temp(r#"token="abcdefghijklmnopqrstuvwxyzABCDEF1234567890""#);
        let opts = Options {
            enable_entropy: true,
            ..Default::default()
        };
        let w = scan_file_with_options(&path, &opts).unwrap();
        assert!(!w.is_empty(), "expected entropy warning");
    }

    #[test]
    fn non_utf8_line_does_not_abort_scan() {
        // A binary-ish (invalid UTF-8) line between two secrets must not
        // abort the scan or drop the warnings around it.
        let key = sample_aws_access_key();
        let pat = sample_github_pat();
        let mut content = format!("aws=\"{key}\"\n").into_bytes();
        content.extend_from_slice(&[0xff, 0xfe, b'x', 0xff, b'\n']);
        content.extend_from_slice(format!("github=\"{pat}\"\n").as_bytes());

        let dir = std::env::temp_dir().join(format!(
            "ctx-scan-nonutf8-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("mixed.txt");
        std::fs::write(&path, &content).unwrap();

        let warnings = scan_file(&path.to_string_lossy()).unwrap();
        let lines: Vec<i64> = warnings.iter().map(|w| w.line).collect();
        assert!(lines.contains(&1), "{warnings:?}");
        assert!(lines.contains(&3), "{warnings:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn preview_short_input_passes_through() {
        assert_eq!(preview("short"), "short");
        assert_eq!(preview("twelvecharss"), "twelvecharss");
    }

    #[test]
    fn preview_long_input_redacts_middle() {
        // 16-char input -> first 4 + [...] + last 4
        assert_eq!(preview("0123456789ABCDEF"), "0123[...]CDEF");
    }

    #[test]
    fn allowlist_file_glob_matches() {
        // The Go `allowlistedFile` is order-insensitive; we just verify
        // a simple `*` glob fires.
        assert!(allowlisted_file("src/foo.go", &["src/*.go".to_string()]));
        assert!(!allowlisted_file(
            "src/sub/foo.go",
            &["src/*.go".to_string()]
        ));
        assert!(allowlisted_file("src/sub/foo.go", &["src/**".to_string()]));
    }
}
