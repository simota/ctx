// crates/ctx-braid/src/shellquote.rs
//
// Port of internal/braid/shellquote.go — POSIX-subset shell tokenisation.
//
//   - Whitespace ( \t\r\n ) outside quotes separates tokens.
//   - Single quotes ('...') quote a literal run; no escape processing,
//     no nested quotes.
//   - Double quotes ("...") quote a literal run with two backslash
//     escapes only: \" and \\. Other backslashes are kept verbatim so
//     regex tokens like "a\d" inside double quotes survive intact.
//   - Quotes may be adjacent or interleaved with bare runs to build a
//     single token (e.g. foo"bar baz" -> one token "foobar baz").
//
// An unclosed single or double quote returns an error rather than a
// best-effort split, matching shell behaviour.
//
// The Go reference operates on bytes (the source is the strand source
// string, ASCII-only in practice). We mirror that here using a byte
// loop so the tokenisation is byte-for-byte identical.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ShellSplitError {
    UnclosedSingleQuote,
    UnclosedDoubleQuote,
}

impl fmt::Display for ShellSplitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ShellSplitError::UnclosedSingleQuote => write!(f, "unclosed single quote"),
            ShellSplitError::UnclosedDoubleQuote => write!(f, "unclosed double quote"),
        }
    }
}

impl std::error::Error for ShellSplitError {}

/// shellSplit tokenises a command-line source string per the POSIX
/// subset documented in the module preamble. Mirrors Go's `shellSplit`.
pub fn shell_split(s: &str) -> Result<Vec<String>, ShellSplitError> {
    let bytes = s.as_bytes();
    let mut out: Vec<String> = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    let mut in_single = false;
    let mut in_double = false;
    let mut token_open = false;

    let mut i = 0usize;
    while i < bytes.len() {
        let ch = bytes[i];
        if in_single {
            if ch == b'\'' {
                in_single = false;
                i += 1;
                continue;
            }
            cur.push(ch);
        } else if in_double {
            if ch == b'\\' && i + 1 < bytes.len() {
                let next = bytes[i + 1];
                if next == b'"' || next == b'\\' {
                    cur.push(next);
                    i += 2;
                    continue;
                }
                // Unrecognised escape inside double quotes: keep the
                // backslash literally.
                cur.push(ch);
                i += 1;
                continue;
            }
            if ch == b'"' {
                in_double = false;
                i += 1;
                continue;
            }
            cur.push(ch);
        } else {
            match ch {
                b' ' | b'\t' | b'\r' | b'\n' => {
                    if token_open {
                        out.push(String::from_utf8_lossy(&cur).into_owned());
                        cur.clear();
                        token_open = false;
                    }
                }
                b'\'' => {
                    in_single = true;
                    token_open = true;
                }
                b'"' => {
                    in_double = true;
                    token_open = true;
                }
                _ => {
                    cur.push(ch);
                    token_open = true;
                }
            }
        }
        i += 1;
    }
    if in_single {
        return Err(ShellSplitError::UnclosedSingleQuote);
    }
    if in_double {
        return Err(ShellSplitError::UnclosedDoubleQuote);
    }
    if token_open {
        out.push(String::from_utf8_lossy(&cur).into_owned());
    }
    Ok(out)
}

/// stripCtxAndSub returns the argv tail after a leading optional `ctx`
/// and the subcommand name. Mirrors Go's `stripCtxAndSub` verbatim.
pub fn strip_ctx_and_sub(source: &str) -> Result<Vec<String>, ShellSplitError> {
    let mut out = shell_split(source)?;
    if out.is_empty() {
        return Ok(out);
    }
    if out[0] == "ctx" {
        out.remove(0);
    }
    if out.is_empty() {
        return Ok(out);
    }
    // Drop the subcommand itself.
    out.remove(0);
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_single_quote_preserves_run() {
        let got = shell_split("where 'multi word' --limit 5").unwrap();
        assert_eq!(got, vec!["where", "multi word", "--limit", "5"]);
    }

    #[test]
    fn split_double_quote_preserves_run() {
        let got = shell_split(r#"where "multi word" --regex "a|b""#).unwrap();
        assert_eq!(got, vec!["where", "multi word", "--regex", "a|b"]);
    }

    #[test]
    fn double_quote_keeps_unknown_escape_verbatim() {
        let got = shell_split(r#"where "a\d""#).unwrap();
        assert_eq!(got, vec!["where", r"a\d"]);
    }

    #[test]
    fn double_quote_processes_known_escapes() {
        let got = shell_split(r#"echo "say \"hi\" and \\""#).unwrap();
        assert_eq!(got, vec!["echo", "say \"hi\" and \\"]);
    }

    #[test]
    fn unclosed_single_quote_errors() {
        assert_eq!(
            shell_split("where 'unclosed"),
            Err(ShellSplitError::UnclosedSingleQuote)
        );
    }

    #[test]
    fn unclosed_double_quote_errors() {
        assert_eq!(
            shell_split(r#"where "unclosed"#),
            Err(ShellSplitError::UnclosedDoubleQuote)
        );
    }

    #[test]
    fn strip_ctx_and_sub_drops_leading_ctx_and_subcommand() {
        let tokens = strip_ctx_and_sub("where 'handler' --regex 'router|Handler'").unwrap();
        assert_eq!(tokens, vec!["handler", "--regex", "router|Handler"]);

        let tokens = strip_ctx_and_sub("ctx focus Bar --hops 2").unwrap();
        assert_eq!(tokens, vec!["Bar", "--hops", "2"]);
    }

    #[test]
    fn strip_ctx_and_sub_empty_for_empty_source() {
        let tokens = strip_ctx_and_sub("").unwrap();
        assert!(tokens.is_empty());
    }
}
