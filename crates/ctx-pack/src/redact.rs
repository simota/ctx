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

const PRIVATE_KEY_KIND: &str = "private_key";
const PEM_END: &[u8] = b"-----END ";

/// The scanner flags only the `-----BEGIN … PRIVATE KEY-----` line, but the
/// key material is on the lines after it. Mark every following line through
/// the matching `-----END ` line as redacted too, so the key body never
/// reaches the output. A block with no END line is redacted to the end of
/// the data: failing closed costs some context, leaking costs the key.
/// Each line still gets its own marker, so line numbers do not shift.
fn extend_private_key_blocks(lines: &[&[u8]], kinds: &mut BTreeMap<i64, String>) {
    let begins: Vec<i64> = kinds
        .iter()
        .filter(|(_, kind)| kind.as_str() == PRIVATE_KEY_KIND)
        .map(|(line, _)| *line)
        .collect();
    for begin in begins {
        let Some(begin_line) = lines.get((begin - 1) as usize) else {
            continue;
        };
        // A whole PEM on one line (e.g. a "\n"-escaped string) is already
        // covered by redacting that line.
        if contains(begin_line, PEM_END) {
            continue;
        }
        // The empty element after a trailing newline is not a line.
        let line_count = if lines.last().is_some_and(|last| last.is_empty()) {
            lines.len() - 1
        } else {
            lines.len()
        };
        for (idx, line) in lines[..line_count].iter().enumerate().skip(begin as usize) {
            kinds
                .entry(idx as i64 + 1)
                .or_insert_with(|| PRIVATE_KEY_KIND.to_string());
            if contains(line, PEM_END) {
                break;
            }
        }
    }
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
    fn unterminated_private_key_block_is_redacted_to_eof() {
        let input = b"-----BEGIN PRIVATE KEY-----\nMIIEbody\nmorebody\n";
        let out = redact_lines(input, &[w(1, "private_key")]);
        let got = String::from_utf8(out).unwrap();
        assert_eq!(got, "[REDACTED — kind=private_key]\n".repeat(3));
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
