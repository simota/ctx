// crates/ctx-contract/src/parse_refs.rs
//
// Port of internal/contract/parse_refs.go.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::io::{BufRead, BufReader, Cursor};

use crate::types::Reference;

/// Closed set of file extensions recognised as path references.
/// Order matches `supportedExts` in parse_refs.go verbatim — change the
/// Go side and this list together.
pub const SUPPORTED_EXTS: &[&str] = &[
    ".go", ".ts", ".tsx", ".js", ".jsx", ".mjs",
    ".py", ".rs", ".java", ".kt", ".rb", ".swift",
    ".md", ".toml", ".yaml", ".yml", ".json", ".sh", ".sql",
];

fn ext_alternation() -> String {
    let parts: Vec<String> = SUPPORTED_EXTS
        .iter()
        .map(|e| regex::escape(e.trim_start_matches('.')))
        .collect();
    format!("(?i:{})", parts.join("|"))
}

static EXT_ALTERNATION: Lazy<String> = Lazy::new(ext_alternation);

static PATH_REF_RE: Lazy<Regex> = Lazy::new(|| {
    // (?P<path>...) optional (?::L?<start>(?:-L?<end>)?)?
    let pat = format!(
        r"(?P<path>[A-Za-z_][A-Za-z0-9._/-]*\.{ext})(?::L?(?P<start>\d+)(?:-L?(?P<end>\d+))?)?",
        ext = *EXT_ALTERNATION
    );
    Regex::new(&pat).expect("compile pathRefRe")
});

static SYMBOL_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"`([A-Za-z_][A-Za-z0-9_.]*)`").expect("compile symbolRe"));

static DIFF_HEADER_RE: Lazy<Regex> = Lazy::new(|| {
    // PARITY (Phase 1 C-01): Go's regexp treats `\s`/`\S` as ASCII-only
    // (`[\t\n\f\r ]`); Rust's `regex` defaults to Unicode-aware classes
    // that match NBSP / ideographic space. We can't use `(?-u:\S)`
    // because the `regex::Regex` (string) API forbids patterns that
    // could match invalid UTF-8 — so we spell the ASCII classes out
    // by literal char ranges, which is functionally equivalent to
    // Go's RE2 behaviour and stays UTF-8-safe.
    Regex::new(r"(?m)^\+\+\+[\t\n\x0C\r ]+b/([^\t\n\x0C\r ]+)").expect("compile diffHeaderRe")
});

fn looks_like_path(s: &str) -> bool {
    if s.contains('/') {
        return true;
    }
    for ext in SUPPORTED_EXTS {
        if s.ends_with(ext) {
            return true;
        }
    }
    false
}

/// Scan `response` and return the references found, preserving source
/// line numbers. Duplicates (same kind/path/range/symbol/line) collapse.
///
/// PARITY: matches bufio.Scanner 1MB cap (Phase 1 L-01). Lines longer
/// than 1 MiB are silently dropped, mirroring the Go scanner's default
/// behaviour when `Buffer(..., 1024*1024)` is exceeded.
pub fn extract_references(response: &[u8]) -> Vec<Reference> {
    const MAX_LINE: usize = 1024 * 1024;

    let mut refs: Vec<Reference> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    let add = |refs: &mut Vec<Reference>, seen: &mut HashSet<String>, r: Reference| {
        let key = format!(
            "{}|{}|{}-{}|{}|{}",
            r.kind, r.path, r.line_start, r.line_end, r.symbol, r.source_line
        );
        if seen.contains(&key) {
            return;
        }
        seen.insert(key);
        refs.push(r);
    };

    let reader = BufReader::new(Cursor::new(response));
    let mut line_no: i32 = 0;
    for line_res in reader.lines() {
        // PARITY (Phase 1 L-01): mirror bufio.Scanner. We increment
        // `line_no` at the top of the iteration *before* any early
        // continue/break so the numbering of accepted lines matches
        // Go's `for scanner.Scan() { lineNo++; ... }`.
        line_no += 1;
        // BufReader::lines yields io::Result<String>. For UTF-8 errors
        // or other IO failures we silently skip the line, the closest
        // analogue to bufio.Scanner's drop-on-error behaviour.
        let line = match line_res {
            Ok(s) => s,
            Err(_) => continue,
        };
        // PARITY (Phase 1 L-01): bufio.Scanner with `Buffer(..., 1MiB)`
        // returns false the *first* time a line exceeds the cap and
        // terminates Scan() entirely — subsequent lines are silently
        // dropped. We mirror that with `break`, not `continue`.
        if line.len() > MAX_LINE {
            break;
        }

        // Diff headers — must run before generic path matcher.
        if let Some(caps) = DIFF_HEADER_RE.captures(&line) {
            if let Some(m) = caps.get(1) {
                add(
                    &mut refs,
                    &mut seen,
                    Reference {
                        kind: "diff-header".to_string(),
                        path: m.as_str().to_string(),
                        line_start: 0,
                        line_end: 0,
                        symbol: String::new(),
                        source_line: line_no,
                    },
                );
                continue;
            }
        }
        if line.starts_with("--- a/") || line.starts_with("+++ b/") {
            // Companion side of the diff header pair.
            continue;
        }

        // Path / line-range references.
        for caps in PATH_REF_RE.captures_iter(&line) {
            let path = caps.name("path").map(|m| m.as_str()).unwrap_or("");
            let start_opt = caps
                .name("start")
                .and_then(|m| m.as_str().parse::<i32>().ok());
            let end_opt = caps
                .name("end")
                .and_then(|m| m.as_str().parse::<i32>().ok());

            match start_opt {
                None => {
                    add(
                        &mut refs,
                        &mut seen,
                        Reference {
                            kind: "file".to_string(),
                            path: path.to_string(),
                            line_start: 0,
                            line_end: 0,
                            symbol: String::new(),
                            source_line: line_no,
                        },
                    );
                }
                Some(start) => {
                    let end = end_opt.unwrap_or(start);
                    add(
                        &mut refs,
                        &mut seen,
                        Reference {
                            kind: "line-range".to_string(),
                            path: path.to_string(),
                            line_start: start,
                            line_end: end,
                            symbol: String::new(),
                            source_line: line_no,
                        },
                    );
                }
            }
        }

        // Symbol references inside backticks.
        for caps in SYMBOL_RE.captures_iter(&line) {
            let sym = caps.get(1).map(|m| m.as_str()).unwrap_or("");
            if looks_like_path(sym) {
                continue;
            }
            add(
                &mut refs,
                &mut seen,
                Reference {
                    kind: "symbol".to_string(),
                    path: String::new(),
                    line_start: 0,
                    line_end: 0,
                    symbol: sym.to_string(),
                    source_line: line_no,
                },
            );
        }
    }
    refs
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_plain_file_reference() {
        let refs = extract_references(b"see internal/limit/limit.go for context\n");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "file");
        assert_eq!(refs[0].path, "internal/limit/limit.go");
        assert_eq!(refs[0].source_line, 1);
    }

    #[test]
    fn extracts_line_range_with_github_style() {
        let refs = extract_references(b"check foo.go:L10-L20 please");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "line-range");
        assert_eq!(refs[0].line_start, 10);
        assert_eq!(refs[0].line_end, 20);
    }

    #[test]
    fn extracts_symbol_in_backticks() {
        let refs = extract_references(b"call `DoThing` first");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "symbol");
        assert_eq!(refs[0].symbol, "DoThing");
    }

    #[test]
    fn skips_backtick_path_as_symbol() {
        let refs = extract_references(b"see `foo.go` here");
        // Treated as a file reference (from the unquoted regex hit
        // inside the backticks) — not a separate symbol.
        assert!(refs.iter().all(|r| r.kind != "symbol"));
    }

    #[test]
    fn diff_header_emits_diff_header_kind() {
        let refs = extract_references(b"+++ b/internal/foo.go\n");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].kind, "diff-header");
        assert_eq!(refs[0].path, "internal/foo.go");
    }
}
