// crates/ctx-symbols/src/apionly.rs
//
// Pure post-processing port of internal/symbols/apionly.go's render
// path. The tree-sitter AST walk stays Go-side; the Go caller passes
// us `(lines, ranges)` (post-AST) and we run:
//
//   1. stable sort ranges by (start asc, end asc)
//   2. merge overlapping ranges
//   3. apply per-range end_replacement (if set)
//   4. concatenate lines with blank-line separators between non-
//      adjacent ranges
//   5. trim trailing whitespace on each emitted line, trim final
//      newlines, append a single "\n"
//
// This is the LITERAL behaviour of Go's `renderAPIRanges`. The byte-
// exact contract matters — this function feeds `ctx pack --api-only`.

use crate::types::{APIRange, APIRenderRequest};

pub fn render_api(req: &APIRenderRequest) -> String {
    render_api_ranges(&req.lines, &req.ranges)
}

fn render_api_ranges(lines: &[String], ranges_in: &[APIRange]) -> String {
    if ranges_in.is_empty() {
        return String::new();
    }
    // Apply end_replacement first by cloning the lines we touch.
    let mut effective_lines: Vec<String> = lines.to_vec();
    let mut ranges: Vec<APIRange> = ranges_in.to_vec();
    for r in &ranges {
        if let Some(repl) = &r.end_replacement {
            let idx = r.end as usize;
            if idx < effective_lines.len() {
                effective_lines[idx] = repl.clone();
            }
        }
    }
    // Stable sort by (start asc, end asc) matching `sort.Slice` in Go.
    ranges.sort_by(|a, b| {
        if a.start == b.start {
            a.end.cmp(&b.end)
        } else {
            a.start.cmp(&b.start)
        }
    });

    let mut out: Vec<String> = Vec::new();
    let mut last_end: i32 = -1;
    for r in &ranges {
        if r.start <= last_end {
            if r.end > last_end {
                last_end = r.end;
            }
            continue;
        }
        if !out.is_empty() {
            out.push(String::new());
        }
        let mut i = r.start;
        while i <= r.end && (i as usize) < effective_lines.len() {
            out.push(trim_trailing_ws(&effective_lines[i as usize]));
            i += 1;
        }
        last_end = r.end;
    }
    let joined = out.join("\n");
    let trimmed = trim_trailing_newlines(&joined);
    let mut s = String::with_capacity(trimmed.len() + 1);
    s.push_str(trimmed);
    s.push('\n');
    s
}

fn trim_trailing_ws(s: &str) -> String {
    // Go: strings.TrimRight(s, " \t") — only ASCII space + tab.
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > 0 && (bytes[end - 1] == b' ' || bytes[end - 1] == b'\t') {
        end -= 1;
    }
    s[..end].to_string()
}

fn trim_trailing_newlines(s: &str) -> &str {
    // Go: strings.TrimRight(s, "\n")
    let bytes = s.as_bytes();
    let mut end = bytes.len();
    while end > 0 && bytes[end - 1] == b'\n' {
        end -= 1;
    }
    &s[..end]
}

// is_comment_line and leading_comment_start are exposed too for
// completeness — they're pure helpers used by the Go side when it
// computes ranges. We port them so a future "full apionly" path can
// avoid duplicate logic in Go.

pub fn is_comment_line(s: &str) -> bool {
    // Go: strings.HasPrefix various — port literally.
    let t = s;
    t.starts_with("//")
        || t.starts_with("/*")
        || t.starts_with('*')
        || t.starts_with("*/")
        || t.starts_with('#')
        || t.starts_with("\"\"\"")
        || t.starts_with("'''")
        || t.ends_with("\"\"\"")
        || t.ends_with("'''")
}

pub fn leading_comment_start(lines: &[String], row: i32) -> i32 {
    let mut i = row - 1;
    while i >= 0 && (i as usize) < lines.len() && lines[i as usize].trim().is_empty() {
        i -= 1;
    }
    if i < 0 || (i as usize) >= lines.len() {
        return row;
    }
    if !is_comment_line(lines[i as usize].trim()) {
        return row;
    }
    while i >= 0 && (i as usize) < lines.len() && is_comment_line(lines[i as usize].trim()) {
        i -= 1;
    }
    i + 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lines(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn empty_ranges_returns_empty() {
        let req = APIRenderRequest {
            lines: lines(&["a", "b"]),
            ranges: vec![],
        };
        assert_eq!(render_api(&req), "");
    }

    #[test]
    fn single_range_renders_with_trailing_newline() {
        let req = APIRenderRequest {
            lines: lines(&["package x", "", "func F() {}"]),
            ranges: vec![APIRange {
                start: 0,
                end: 0,
                end_replacement: None,
            }],
        };
        assert_eq!(render_api(&req), "package x\n");
    }

    #[test]
    fn two_ranges_separated_by_blank_line() {
        let req = APIRenderRequest {
            lines: lines(&["A", "B", "C", "D"]),
            ranges: vec![
                APIRange {
                    start: 0,
                    end: 0,
                    end_replacement: None,
                },
                APIRange {
                    start: 2,
                    end: 3,
                    end_replacement: None,
                },
            ],
        };
        assert_eq!(render_api(&req), "A\n\nC\nD\n");
    }

    #[test]
    fn overlapping_ranges_are_merged() {
        let req = APIRenderRequest {
            lines: lines(&["A", "B", "C", "D"]),
            ranges: vec![
                APIRange {
                    start: 0,
                    end: 2,
                    end_replacement: None,
                },
                APIRange {
                    start: 1,
                    end: 3,
                    end_replacement: None,
                },
            ],
        };
        assert_eq!(render_api(&req), "A\nB\nC\n");
    }

    #[test]
    fn end_replacement_applied_to_emitted_line() {
        let req = APIRenderRequest {
            lines: lines(&["func F() {", "  return", "}"]),
            ranges: vec![APIRange {
                start: 0,
                end: 0,
                end_replacement: Some("func F()".to_string()),
            }],
        };
        assert_eq!(render_api(&req), "func F()\n");
    }

    #[test]
    fn trailing_whitespace_is_trimmed_per_line() {
        let req = APIRenderRequest {
            lines: lines(&["A   ", "B\t"]),
            ranges: vec![APIRange {
                start: 0,
                end: 1,
                end_replacement: None,
            }],
        };
        assert_eq!(render_api(&req), "A\nB\n");
    }

    #[test]
    fn is_comment_line_recognises_all_prefixes() {
        for s in &["// x", "/* x", "* x", "*/", "# x", "\"\"\"x", "'''x"] {
            assert!(is_comment_line(s), "{s}");
        }
        for s in &["\"\"\"", "'''"] {
            // These start AND end with the triple-quote, so both branches hit.
            assert!(is_comment_line(s), "{s}");
        }
        assert!(!is_comment_line("func F()"));
        assert!(!is_comment_line(""));
    }

    #[test]
    fn leading_comment_start_walks_back_over_comments() {
        let ls = lines(&[
            "// LoginUser",
            "// returns a session",
            "func LoginUser() {}",
        ]);
        assert_eq!(leading_comment_start(&ls, 2), 0);
    }

    #[test]
    fn leading_comment_start_returns_row_when_no_comments() {
        let ls = lines(&["func F() {}"]);
        assert_eq!(leading_comment_start(&ls, 0), 0);
    }

    #[test]
    fn leading_comment_start_skips_blank_lines() {
        let ls = lines(&["// hi", "", "", "func F() {}"]);
        assert_eq!(leading_comment_start(&ls, 3), 0);
    }
}
