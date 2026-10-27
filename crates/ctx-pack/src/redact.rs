// crates/ctx-pack/src/redact.rs
//
// Port of internal/pack/redact.go::RedactLines. The gating logic
// (opts.Config.Security.SecretScan + opts.Config.Security.Redact)
// stays on the Go side because it lives on the Config struct. By
// the time the dispatcher hits this crate, the caller has decided
// to redact and supplies the warning list directly.

use std::collections::BTreeMap;

use crate::types::WarningInput;

/// Replace every line listed in `warnings` with a `[REDACTED — kind=K]`
/// marker. Line numbers are 1-based; warnings with line <= 0 are
/// silently dropped (matches Go behaviour).
pub fn redact_lines(data: &[u8], warnings: &[WarningInput]) -> Vec<u8> {
    if warnings.is_empty() {
        return data.to_vec();
    }
    // BTreeMap so iteration is deterministic if we ever expose the
    // kinds map; the actual replacement uses random-access lookup.
    let mut kinds: BTreeMap<i64, String> = BTreeMap::new();
    for w in warnings {
        if w.line <= 0 {
            continue;
        }
        if kinds.contains_key(&w.line) {
            continue;
        }
        let k = if w.kind.is_empty() {
            "secret".to_string()
        } else {
            w.kind.clone()
        };
        kinds.insert(w.line, k);
    }
    if kinds.is_empty() {
        return data.to_vec();
    }

    // bytes.Split on '\n' yields N+1 elements when the input ends
    // with a newline (the last element is empty). We replicate that
    // behaviour with a manual scan to preserve byte parity.
    let mut lines: Vec<&[u8]> = Vec::new();
    let mut last = 0usize;
    for (i, b) in data.iter().enumerate() {
        if *b == b'\n' {
            lines.push(&data[last..i]);
            last = i + 1;
        }
    }
    lines.push(&data[last..]);
    extend_private_key_blocks(&lines, &mut kinds);

    let mut out: Vec<Vec<u8>> = Vec::with_capacity(lines.len());
    for (i, l) in lines.iter().enumerate() {
        let line_no = (i + 1) as i64;
        if let Some(k) = kinds.get(&line_no) {
            out.push(format!("[REDACTED — kind={k}]").into_bytes());
        } else {
            out.push(l.to_vec());
        }
    }
    // bytes.Join with '\n' separator.
    let mut joined: Vec<u8> = Vec::with_capacity(data.len());
    for (i, l) in out.iter().enumerate() {
        if i > 0 {
            joined.push(b'\n');
        }
        joined.extend_from_slice(l);
    }
    joined
}

/// Scan `body` for secrets and redact the lines that hit. The text that is
/// emitted is what gets scanned, so line numbers match even when the caller
/// transformed the file (e.g. `--api-only`); `path` is used only for the
/// warnings and for `allowlist_files` matching.
pub fn redact_secrets(path: &str, body: &str, opts: &ctx_scan::Options) -> String {
    let warnings = ctx_scan::scan_bytes_with_options(path, body.as_bytes(), opts);
    if warnings.is_empty() {
        return body.to_string();
    }
    let inputs: Vec<WarningInput> = warnings
        .into_iter()
        .map(|warning| WarningInput {
            path: path.to_string(),
            line: warning.line,
            kind: warning.kind,
        })
        .collect();
    String::from_utf8_lossy(&redact_lines(body.as_bytes(), &inputs)).into_owned()
}

const PRIVATE_KEY_KIND: &str = "private_key";
const PEM_END: &[u8] = b"-----END ";

/// The scanner flags only the `-----BEGIN … PRIVATE KEY-----` line, but the
/// key material is on the lines after it. Redact the following lines too,
/// through the matching `-----END ` line, as long as they look like key
/// material (base64, RFC 1421 / OpenPGP armor headers, blank lines). A line
/// that does not ends the block unredacted, so code that merely mentions a
/// header (a PEM parser, a test) keeps the rest of its lines; a key with no
/// END line is still redacted up to its last body line. Each line keeps its
/// own marker, so line numbers do not shift.
fn extend_private_key_blocks(lines: &[&[u8]], kinds: &mut BTreeMap<i64, String>) {
    let begins: Vec<i64> = kinds
        .iter()
        .filter(|(_, kind)| kind.as_str() == PRIVATE_KEY_KIND)
        .map(|(line, _)| *line)
        .collect();
    // The empty element after a trailing newline is not a line.
    let line_count = if lines.last().is_some_and(|last| last.is_empty()) {
        lines.len() - 1
    } else {
        lines.len()
    };
    for begin in begins {
        let Some(begin_line) = lines.get((begin - 1) as usize) else {
            continue;
        };
        // A whole PEM on one line (e.g. a "\n"-escaped string) is already
        // covered by redacting that line.
        if contains(begin_line, PEM_END) {
            continue;
        }
        for (idx, line) in lines[..line_count].iter().enumerate().skip(begin as usize) {
            let is_end = contains(line, PEM_END);
            if !is_end && !is_key_material_line(line) {
                break;
            }
            kinds
                .entry(idx as i64 + 1)
                .or_insert_with(|| PRIVATE_KEY_KIND.to_string());
            if is_end {
                break;
            }
        }
    }
}

/// Whether `line` can be part of a PEM / armored key body once the quoting
/// of a string literal (`"…\n" +`, backticks) or a diff marker is removed.
fn is_key_material_line(line: &[u8]) -> bool {
    let mut t = trim_bytes(line, b" \t\r\"'`,;+-");
    t = t.strip_suffix(b"\\n").unwrap_or(t);
    t = t.strip_suffix(b"\\r").unwrap_or(t);
    let t = trim_bytes(t, b" \t\"'`,;+-");
    if t.is_empty() {
        return true;
    }
    if t.iter()
        .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
    {
        return true;
    }
    // `Proc-Type: 4,ENCRYPTED`, `DEK-Info: …`, `Version: GnuPG …`, `Comment: …`
    match t.iter().position(|&b| b == b':') {
        Some(colon) if colon > 0 => {
            t[..colon]
                .iter()
                .all(|b| b.is_ascii_alphanumeric() || *b == b'-')
                && t.get(colon + 1) == Some(&b' ')
        }
        _ => false,
    }
}

fn trim_bytes<'a>(mut s: &'a [u8], set: &[u8]) -> &'a [u8] {
    while let [first, rest @ ..] = s {
        if !set.contains(first) {
            break;
        }
        s = rest;
    }
    while let [rest @ .., last] = s {
        if !set.contains(last) {
            break;
        }
        s = rest;
    }
    s
}

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn w(line: i64, kind: &str) -> WarningInput {
        WarningInput {
            path: String::new(),
            line,
            kind: kind.into(),
        }
    }

    #[test]
    fn empty_warnings_passes_through() {
        let input = b"a\nb\nc";
        let out = redact_lines(input, &[]);
        assert_eq!(out, input);
    }

    #[test]
    fn redacts_marked_line() {
        let input = b"a\nSECRET=hunter2\nc";
        let out = redact_lines(input, &[w(2, "env")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(got, "a\n[REDACTED — kind=env]\nc");
    }

    #[test]
    fn missing_kind_falls_back_to_secret() {
        let input = b"a\nSECRET=x\n";
        let out = redact_lines(input, &[w(2, "")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(got, "a\n[REDACTED — kind=secret]\n");
    }

    #[test]
    fn negative_or_zero_lines_dropped() {
        let input = b"a\nb\n";
        let out = redact_lines(input, &[w(0, "x"), w(-1, "y")]);
        assert_eq!(out, input);
    }

    #[test]
    fn private_key_block_is_redacted_through_end_line() {
        let input = b"a\n-----BEGIN RSA PRIVATE KEY-----\nMIIEbody\nmorebody\n-----END RSA PRIVATE KEY-----\nb\n";
        let out = redact_lines(input, &[w(2, "private_key")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(
            got,
            "a\n[REDACTED — kind=private_key]\n[REDACTED — kind=private_key]\n\
             [REDACTED — kind=private_key]\n[REDACTED — kind=private_key]\nb\n"
        );
    }

    #[test]
    fn unterminated_private_key_block_is_redacted_through_its_body() {
        let input = b"-----BEGIN PRIVATE KEY-----\nMIIEbody\nmorebody\n";
        let out = redact_lines(input, &[w(1, "private_key")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(got, "[REDACTED — kind=private_key]\n".repeat(3));
    }

    #[test]
    fn header_mentioned_in_code_does_not_redact_following_code() {
        let input = b"fn check(l: &str) -> bool { l.starts_with(\"-----BEGIN RSA PRIVATE KEY-----\") }\nfn other() { println!(\"visible\"); }\n";
        let out = redact_lines(input, &[w(1, "private_key")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(
            got,
            "[REDACTED — kind=private_key]\nfn other() { println!(\"visible\"); }\n"
        );
    }

    #[test]
    fn key_in_string_concatenation_and_armor_headers_is_redacted() {
        let input = b"const K = \"-----BEGIN RSA PRIVATE KEY-----\\n\" +\n  \"Proc-Type: 4,ENCRYPTED\\n\" +\n  \"\\n\" +\n  \"MIIEbodyAB+/=\\n\" +\n  \"-----END RSA PRIVATE KEY-----\";\nnext();\n";
        let out = redact_lines(input, &[w(1, "private_key")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(
            got,
            format!("{}next();\n", "[REDACTED — kind=private_key]\n".repeat(5))
        );
    }

    #[test]
    fn key_in_a_diff_patch_is_redacted() {
        let input = b"+-----BEGIN OPENSSH PRIVATE KEY-----\n+b3BlbnNzaC1rZXkt\n+-----END OPENSSH PRIVATE KEY-----\n context\n";
        let out = redact_lines(input, &[w(1, "private_key")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(
            got,
            format!("{} context\n", "[REDACTED — kind=private_key]\n".repeat(3))
        );
    }

    #[test]
    fn single_line_pem_redacts_only_that_line() {
        let input =
            b"k = \"-----BEGIN PRIVATE KEY-----\\nMII\\n-----END PRIVATE KEY-----\"\nnext\n";
        let out = redact_lines(input, &[w(1, "private_key")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(got, "[REDACTED — kind=private_key]\nnext\n");
    }

    #[test]
    fn other_kinds_do_not_extend() {
        let input = b"TOKEN=x\nnext\n";
        let out = redact_lines(input, &[w(1, "generic_secret")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(got, "[REDACTED — kind=generic_secret]\nnext\n");
    }

    #[test]
    fn duplicate_line_first_kind_wins() {
        let input = b"a\nSECRET=x\n";
        let out = redact_lines(input, &[w(2, "first"), w(2, "second")]);
        let got = String::from_utf8(out).unwrap();
        assert!(got.contains("kind=first"));
        assert!(!got.contains("kind=second"));
    }
}
