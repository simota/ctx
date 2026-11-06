// crates/ctx-pack/src/excerpt.rs
//
// Line-range excerpts: a pack selection entry may name part of a file —
// `src/store.rs:120-180`, `src/store.rs:42`, or the GitHub form
// `src/store.rs#L120-L180` — instead of the whole file. This is how a model
// that saw a `--overflow signatures` digest (which numbers each declaration
// with its line in the full file) gets the body it asks for, without the
// budget paying for the whole file again.
//
// An excerpt is packed as the requested source lines, verbatim, under a
// one-line `[ctx: ...]` header that names the span; several ranges of one
// file are merged into a single packed entry with one header per segment.
// The pack contract records the original line numbers and line hashes of
// exactly the lines shown (see `ctx_contract::FileInput::partial`), so
// `ctx contract verify` accepts citations of those lines and flags
// citations of lines the model never saw.

use ctx_contract::types::LineHash;

/// A 1-based, inclusive line range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct LineRange {
    pub start: usize,
    pub end: usize,
}

impl std::fmt::Display for LineRange {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.start == self.end {
            write!(f, "{}", self.start)
        } else {
            write!(f, "{}-{}", self.start, self.end)
        }
    }
}

/// Split a selection entry into its path and line range.
///
/// Accepts `path:N`, `path:N-M`, `path#LN` and `path#LN-LM` (also
/// `path#LN-M`). Anything else — including a range whose start is 0 or
/// whose end precedes its start — is returned unchanged as a whole-file
/// path, so a file whose name really ends in `:12` still resolves. Callers
/// that can see the filesystem should prefer the literal path when it
/// exists (see [`split_spec_in`]).
pub fn split_spec(raw: &str) -> (&str, Option<LineRange>) {
    if let Some((path, frag)) = raw.rsplit_once("#L") {
        if let Some(range) = parse_range(frag, true) {
            if !path.is_empty() {
                return (path, Some(range));
            }
        }
    }
    if let Some((path, suffix)) = raw.rsplit_once(':') {
        if let Some(range) = parse_range(suffix, false) {
            if !path.is_empty() {
                return (path, Some(range));
            }
        }
    }
    (raw, None)
}

/// [`split_spec`], except that an entry naming an existing file under
/// `root` verbatim is always a whole-file selection.
pub fn split_spec_in<'a>(root: &std::path::Path, raw: &'a str) -> (&'a str, Option<LineRange>) {
    let (path, range) = split_spec(raw);
    if range.is_some() && root.join(raw.trim()).is_file() {
        return (raw, None);
    }
    (path, range)
}

fn parse_range(text: &str, github: bool) -> Option<LineRange> {
    let number = |s: &str| -> Option<usize> {
        let s = if github {
            s.strip_prefix('L').unwrap_or(s)
        } else {
            s
        };
        if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) || s.len() > 9 {
            return None;
        }
        s.parse().ok()
    };
    let (start, end) = match text.split_once('-') {
        Some((a, b)) => (number(a)?, number(b)?),
        None => {
            let n = number(text)?;
            (n, n)
        }
    };
    (start >= 1 && end >= start).then_some(LineRange { start, end })
}

/// What a selection asks for of one file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Wanted {
    Whole,
    Lines(Vec<LineRange>),
}

/// Group selection entries by path, in first-appearance order. A
/// whole-file entry for a path wins over any range of it; ranges of the
/// same path are merged (overlapping or adjacent ones coalesce).
/// `clean` normalises the path part (and may reject it with `None`, in
/// which case the raw entry is returned in the error list).
pub fn group_specs<F>(
    root: &std::path::Path,
    entries: &[String],
    mut clean: F,
) -> (Vec<(String, Wanted)>, Vec<String>)
where
    F: FnMut(&str) -> Option<String>,
{
    let mut order: Vec<(String, Wanted)> = Vec::new();
    let mut rejected = Vec::new();
    for raw in entries {
        if raw.trim().is_empty() {
            continue;
        }
        let (path, range) = split_spec_in(root, raw.trim());
        let Some(path) = clean(path) else {
            rejected.push(raw.clone());
            continue;
        };
        match order.iter_mut().find(|(p, _)| *p == path) {
            Some((_, wanted)) => match (wanted.clone(), range) {
                (Wanted::Whole, _) => {}
                (Wanted::Lines(_), None) => *wanted = Wanted::Whole,
                (Wanted::Lines(mut ranges), Some(range)) => {
                    ranges.push(range);
                    *wanted = Wanted::Lines(merge_ranges(ranges));
                }
            },
            None => order.push((
                path,
                match range {
                    Some(range) => Wanted::Lines(vec![range]),
                    None => Wanted::Whole,
                },
            )),
        }
    }
    (order, rejected)
}

/// Sort and coalesce overlapping or adjacent ranges.
pub fn merge_ranges(mut ranges: Vec<LineRange>) -> Vec<LineRange> {
    ranges.sort();
    let mut out: Vec<LineRange> = Vec::with_capacity(ranges.len());
    for range in ranges {
        match out.last_mut() {
            Some(last) if range.start <= last.end.saturating_add(1) => {
                last.end = last.end.max(range.end);
            }
            _ => out.push(range),
        }
    }
    out
}

/// `10-20, 40` — how a set of ranges is shown in listings.
pub fn label(ranges: &[LineRange]) -> String {
    ranges
        .iter()
        .map(LineRange::to_string)
        .collect::<Vec<_>>()
        .join(", ")
}

/// A packed excerpt of a file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Excerpt {
    /// The text to pack: each segment's `[ctx: ...]` header and its lines.
    pub content: String,
    /// The ranges actually shown, clamped to the file's length.
    pub ranges: Vec<LineRange>,
    /// Number of lines in the whole file.
    pub total_lines: usize,
    /// Original line number and SHA-256 of every line shown, for the
    /// pack contract.
    pub line_hashes: Vec<LineHash>,
    /// Requested ranges that start after the last line (not shown).
    pub out_of_range: Vec<LineRange>,
}

impl Excerpt {
    /// `lines 10-20, 40 of 412` — the note shown next to the path.
    pub fn note(&self) -> String {
        format!("lines {} of {}", label(&self.ranges), self.total_lines)
    }

    /// The skip reason for the requested ranges that were not shown, if
    /// any (`path:900-950` is past the end of a 412-line file).
    pub fn out_of_range_reason(&self) -> Option<String> {
        (!self.out_of_range.is_empty()).then(|| past_end(&self.out_of_range, self.total_lines))
    }
}

fn past_end(ranges: &[LineRange], total: usize) -> String {
    format!(
        "line range {} is past the end of the file ({total} lines)",
        label(ranges)
    )
}

/// Cut `ranges` out of `text` (the file's full, already-redacted content).
/// Ranges running past the end are clamped; ranges starting after the
/// last line are left out and listed in `out_of_range`. An error means
/// no requested line exists.
pub fn excerpt(path: &str, text: &str, ranges: &[LineRange]) -> Result<Excerpt, String> {
    let lines = ctx_contract::builder::split_logical_lines(text.as_bytes());
    let total = lines.len();
    let mut shown = Vec::new();
    let mut out_of_range = Vec::new();
    for range in merge_ranges(ranges.to_vec()) {
        if range.start > total {
            out_of_range.push(range);
            continue;
        }
        shown.push(LineRange {
            start: range.start,
            end: range.end.min(total),
        });
    }
    if shown.is_empty() {
        return Err(past_end(&out_of_range, total));
    }
    let shown = merge_ranges(shown);
    let mut content = String::new();
    let mut line_hashes = Vec::new();
    for (i, range) in shown.iter().enumerate() {
        if i == 0 {
            content.push_str(&format!(
                "[ctx: excerpt - {path} lines {range} of {total}; the full file is not in this pack]\n"
            ));
        } else {
            content.push_str(&format!("[ctx: {path} lines {range}]\n"));
        }
        for n in range.start..=range.end {
            let line = lines[n - 1];
            content.push_str(&String::from_utf8_lossy(line));
            line_hashes.push(LineHash {
                line: n as i32,
                sha256: ctx_contract::hash::sha256_hex(line),
            });
        }
        if !content.ends_with('\n') {
            content.push('\n');
        }
    }
    Ok(Excerpt {
        content,
        ranges: shown,
        total_lines: total,
        line_hashes,
        out_of_range,
    })
}

/// Contract line hashes for the source lines `numbers` (1-based) of
/// `text` — the declaration lines a signature digest shows.
pub fn line_hashes_of(text: &str, numbers: &[usize]) -> Vec<LineHash> {
    let lines = ctx_contract::builder::split_logical_lines(text.as_bytes());
    let mut numbers: Vec<usize> = numbers
        .iter()
        .copied()
        .filter(|n| *n >= 1 && *n <= lines.len())
        .collect();
    numbers.sort_unstable();
    numbers.dedup();
    numbers
        .into_iter()
        .map(|n| LineHash {
            line: n as i32,
            sha256: ctx_contract::hash::sha256_hex(lines[n - 1]),
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn r(start: usize, end: usize) -> LineRange {
        LineRange { start, end }
    }

    #[test]
    fn splits_colon_and_github_specs() {
        assert_eq!(split_spec("src/a.rs:10-20"), ("src/a.rs", Some(r(10, 20))));
        assert_eq!(split_spec("src/a.rs:42"), ("src/a.rs", Some(r(42, 42))));
        assert_eq!(
            split_spec("src/a.rs#L10-L20"),
            ("src/a.rs", Some(r(10, 20)))
        );
        assert_eq!(split_spec("src/a.rs#L10-20"), ("src/a.rs", Some(r(10, 20))));
        assert_eq!(split_spec("src/a.rs#L7"), ("src/a.rs", Some(r(7, 7))));
        assert_eq!(split_spec("C:/x/a.rs:3"), ("C:/x/a.rs", Some(r(3, 3))));
    }

    #[test]
    fn leaves_non_ranges_alone() {
        for raw in [
            "src/a.rs",
            "src/a.rs:",
            "src/a.rs:0",
            "src/a.rs:20-10",
            "src/a.rs:abc",
            "src/a.rs:1-",
            "C:/x/a.rs",
            ":12",
        ] {
            assert_eq!(split_spec(raw), (raw, None), "{raw}");
        }
    }

    #[test]
    fn a_literal_file_name_wins_over_a_range() {
        let dir = std::env::temp_dir().join(format!("ctx-excerpt-literal-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("notes:12"), "x\n").unwrap();
        assert_eq!(split_spec_in(&dir, "notes:12"), ("notes:12", None));
        assert_eq!(split_spec_in(&dir, "other:12"), ("other", Some(r(12, 12))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn groups_by_path_merging_ranges_and_preferring_whole_files() {
        let root = std::path::Path::new("/nonexistent-ctx-root");
        let entries: Vec<String> = [
            "b.rs:30-40",
            "a.rs:1-5",
            "b.rs:10-12",
            "b.rs:41-50",
            "a.rs",
            "a.rs:7",
            "../x.rs:1",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        let (grouped, rejected) = group_specs(root, &entries, |p| {
            (!p.starts_with("..")).then(|| p.to_string())
        });
        assert_eq!(
            grouped,
            vec![
                (
                    "b.rs".to_string(),
                    Wanted::Lines(vec![r(10, 12), r(30, 50)])
                ),
                ("a.rs".to_string(), Wanted::Whole),
            ]
        );
        assert_eq!(rejected, vec!["../x.rs:1".to_string()]);
    }

    #[test]
    fn cuts_ranges_with_headers_and_hashes_the_shown_lines() {
        let text = "one\ntwo\nthree\nfour\nfive\n";
        let ex = excerpt("a.txt", text, &[r(4, 99), r(2, 2)]).unwrap();
        assert_eq!(
            ex.content,
            "[ctx: excerpt - a.txt lines 2 of 5; the full file is not in this pack]\ntwo\n\
             [ctx: a.txt lines 4-5]\nfour\nfive\n"
        );
        assert_eq!(ex.ranges, vec![r(2, 2), r(4, 5)]);
        assert_eq!(ex.note(), "lines 2, 4-5 of 5");
        let lines: Vec<i32> = ex.line_hashes.iter().map(|h| h.line).collect();
        assert_eq!(lines, vec![2, 4, 5]);
        assert_eq!(
            ex.line_hashes[0].sha256,
            ctx_contract::hash::sha256_hex(b"two\n")
        );
    }

    #[test]
    fn ranges_past_the_end_are_reported_and_a_missing_final_newline_is_added() {
        assert!(excerpt("a", "x\ny", &[r(3, 4)])
            .unwrap_err()
            .contains("line range 3-4 is past the end of the file (2 lines)"));
        let ex = excerpt("a", "x\ny", &[r(1, 1), r(5, 5)]).unwrap();
        assert_eq!(ex.ranges, vec![r(1, 1)]);
        assert_eq!(
            ex.out_of_range_reason().unwrap(),
            "line range 5 is past the end of the file (2 lines)"
        );
        let ex = excerpt("a", "x\ny", &[r(2, 2)]).unwrap();
        assert!(ex.content.ends_with("y\n"));
        assert_eq!(
            ex.line_hashes[0].sha256,
            ctx_contract::hash::sha256_hex(b"y")
        );
    }

    #[test]
    fn hashes_selected_source_lines() {
        let got = line_hashes_of("a\nb\nc\n", &[3, 1, 3, 0, 9]);
        let lines: Vec<i32> = got.iter().map(|h| h.line).collect();
        assert_eq!(lines, vec![1, 3]);
    }
}
