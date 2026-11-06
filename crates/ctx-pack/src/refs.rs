// crates/ctx-pack/src/refs.rs
//
// File references in free text. A model's reply ("the bug is in
// `src/store.rs:120-180`, see also store_test.rs"), a stack trace
// (`at handler (src/api/user.ts:42:7)`, `File "app/db.py", line 88`), a
// compiler error (`--> src/main.rs:10:5`), a review comment or a GitHub
// permalink names the code that matters next. `ctx pack --from-refs` turns
// that text into a pack of exactly those files and lines.
//
// Scanning is deliberately liberal and resolution deliberately strict: any
// word that could be a path is a candidate, but it is kept only when it
// names a file the caller listed (the pack walk, so ignored and secret
// files never resolve). A mention with directories must match at least its
// last two components (`/usr/lib/python3/json/decoder.py` does not resolve
// to the repository's `tools/decoder.py`); a mention without directories
// must name exactly one file. Leading components the repository does not
// have — an absolute checkout path, `a/` / `b/` diff prefixes, a GitHub
// `https://github.com/o/r/blob/main/` prefix — are dropped.

use std::collections::HashMap;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::excerpt::{merge_ranges, LineRange};

/// One place the text mentions a path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mention {
    /// The mention as written, location included (`src/a.rs:40`).
    pub text: String,
    /// 1-based line of the input text the mention is on.
    pub input_line: usize,
    /// The lines it points at, if it names any.
    pub range: Option<LineRange>,
}

/// A repository file the text refers to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefTarget {
    /// Repository-relative path, as listed by the caller.
    pub path: String,
    /// Lines to pack (widened by `around`, merged); empty = whole file.
    pub ranges: Vec<LineRange>,
    /// Every mention that resolved to this file, in input order.
    pub mentions: Vec<Mention>,
}

impl RefTarget {
    /// Selection entries for this target: `path` for the whole file, or
    /// one `path:START-END` per range — the syntax `--from-stdin` and mixes
    /// accept.
    pub fn specs(&self) -> Vec<String> {
        if self.ranges.is_empty() {
            return vec![self.path.clone()];
        }
        self.ranges
            .iter()
            .map(|range| format!("{}:{}-{}", self.path, range.start, range.end))
            .collect()
    }

    /// Why the file is in the pack: `referenced as "src/a.rs:40" (input
    /// line 3)`, with a count when it was mentioned more than once.
    pub fn reason(&self) -> String {
        let first = &self.mentions[0];
        if self.mentions.len() == 1 {
            format!(
                "referenced as {:?} (input line {})",
                first.text, first.input_line
            )
        } else {
            format!(
                "referenced {} times (first as {:?}, input line {})",
                self.mentions.len(),
                first.text,
                first.input_line
            )
        }
    }
}

/// A path-like mention that named no file, or more than one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unresolved {
    pub mention: String,
    pub input_line: usize,
    pub reason: String,
}

/// Result of [`scan`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RefScan {
    /// Referenced files, in order of first mention.
    pub targets: Vec<RefTarget>,
    /// Mentions with directories that matched no file, and mentions that
    /// matched several (bare names that match nothing are not reported:
    /// prose is full of `e.g.` and `v1.2`).
    pub unresolved: Vec<Unresolved>,
}

impl RefScan {
    /// Selection entries for every target, in order.
    pub fn specs(&self) -> Vec<String> {
        self.targets.iter().flat_map(RefTarget::specs).collect()
    }
}

/// Extensionless file names worth recognising as bare words.
const BARE_NAMES: &[&str] = &[
    "Makefile",
    "Dockerfile",
    "Containerfile",
    "Justfile",
    "justfile",
    "Rakefile",
    "Gemfile",
    "Procfile",
    "Vagrantfile",
    "Jenkinsfile",
    "CODEOWNERS",
    "LICENSE",
];

/// Words: runs of characters that can appear in a path plus its location
/// suffix. Quotes, brackets, backticks and whitespace end a word, so
/// `` `src/a.rs` ``, `(src/a.ts:42:7)` and `"app/db.py"` all yield the path.
static WORD_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r#"[^\s"'`()<>\[\]{},;|*!?]+"#).expect("compile word regex"));

/// `#L10`, `#L10-L20`, `#L10-20` (GitHub permalinks).
static HASH_LOC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^(.+?)#L(\d+)(?:-L?(\d+))?$").expect("compile #L regex"));

/// `:10`, `:10-20`, `:L10-L20`, `:10:5` (column ignored), `:10:5-12:3`.
static COLON_LOC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"^(.+?):L?(\d+)(?::\d+)?(?:[-–]L?(\d+)(?::\d+)?)?$").expect("compile :N regex")
});

/// A location written after the word: `", line 88` (Python tracebacks),
/// `` ` lines 120-180``, ` (line 42)`, `, lines 10 to 20`.
static TRAILING_LOC_RE: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r#"^["'`]?\)?[,:]?\s*\(?\s*(?i:lines?|L)\s*(\d+)(?:\s*(?:-|–|to)\s*L?(\d+))?"#)
        .expect("compile trailing location regex")
});

/// Scan `text` for references to the files in `files` (repository-relative
/// paths with `/` separators). Each referenced range is widened by
/// `around` lines either side. A file mentioned both with and without
/// lines is packed as its lines: in a stack trace or a reply that cites
/// lines, the bare name is a pointer to them, not a request for the file.
pub fn scan(text: &str, files: &[String], around: usize) -> RefScan {
    let index = BasenameIndex::new(files);
    let mut targets: Vec<RefTarget> = Vec::new();
    let mut by_path: HashMap<String, usize> = HashMap::new();
    let mut unresolved: Vec<Unresolved> = Vec::new();
    let mut seen_unresolved = std::collections::HashSet::new();

    for (line_idx, line) in text.lines().enumerate() {
        let input_line = line_idx + 1;
        for word in WORD_RE.find_iter(line) {
            let Some((path, mut range, written)) = parse_word(word.as_str()) else {
                continue;
            };
            if range.is_none() {
                if let Some(caps) = TRAILING_LOC_RE.captures(&line[word.end()..]) {
                    range = location(&caps[1], caps.get(2).map(|m| m.as_str()));
                }
            }
            let text = match range {
                Some(r) if written == path => format!("{path}:{r}"),
                _ => written.to_string(),
            };
            match index.resolve(&path) {
                Resolution::File(file) => {
                    let mention = Mention {
                        text,
                        input_line,
                        range,
                    };
                    match by_path.get(&file) {
                        Some(&i) => targets[i].mentions.push(mention),
                        None => {
                            by_path.insert(file.clone(), targets.len());
                            targets.push(RefTarget {
                                path: file,
                                ranges: Vec::new(),
                                mentions: vec![mention],
                            });
                        }
                    }
                }
                Resolution::Ambiguous(count) => {
                    if seen_unresolved.insert(path.clone()) {
                        unresolved.push(Unresolved {
                            mention: text,
                            input_line,
                            reason: format!(
                                "ambiguous reference: matches {count} files (quote more of the path)"
                            ),
                        });
                    }
                }
                Resolution::Missing => {
                    if path.contains('/')
                        && !written.contains("://")
                        && seen_unresolved.insert(path.clone())
                    {
                        unresolved.push(Unresolved {
                            mention: text,
                            input_line,
                            reason: "no file in this repository matches".to_string(),
                        });
                    }
                }
            }
        }
    }

    for target in &mut targets {
        let ranges: Vec<LineRange> = target
            .mentions
            .iter()
            .filter_map(|mention| mention.range)
            .map(|range| LineRange {
                start: range.start.saturating_sub(around).max(1),
                end: range.end.saturating_add(around),
            })
            .collect();
        target.ranges = merge_ranges(ranges);
    }
    RefScan {
        targets,
        unresolved,
    }
}

/// Split a word into a candidate path (normalised: `\` → `/`, trailing
/// punctuation and `./` dropped), the range it names and the word as
/// written (minus trailing punctuation). `None` when it cannot be a path.
fn parse_word(raw: &str) -> Option<(String, Option<LineRange>, &str)> {
    let written = raw.trim_end_matches(['.', ':', '\'', '"']);
    let (path_part, range) = if let Some(caps) = HASH_LOC_RE.captures(written) {
        (
            caps.get(1)?.as_str(),
            location(&caps[2], caps.get(3).map(|m| m.as_str())),
        )
    } else if let Some(caps) = COLON_LOC_RE.captures(written) {
        (
            caps.get(1)?.as_str(),
            location(&caps[2], caps.get(3).map(|m| m.as_str())),
        )
    } else {
        (written, None)
    };
    let path = path_part.replace('\\', "/");
    let path = path.trim_end_matches(['.', ':']);
    let path = path.strip_prefix("./").unwrap_or(path);
    if !looks_like_path(path) {
        return None;
    }
    Some((path.to_string(), range, written))
}

fn location(start: &str, end: Option<&str>) -> Option<LineRange> {
    let start: usize = start.parse().ok()?;
    let end: usize = match end {
        Some(end) => end.parse().ok()?,
        None => start,
    };
    (start > 0 && end >= start).then_some(LineRange { start, end })
}

fn looks_like_path(path: &str) -> bool {
    let name = path.rsplit('/').next().unwrap_or(path);
    if name.is_empty() || !name.chars().any(|c| c.is_ascii_alphabetic()) {
        return false;
    }
    if path.contains('/') || BARE_NAMES.contains(&name) {
        return true;
    }
    match name.rsplit_once('.') {
        Some((stem, ext)) => {
            !stem.is_empty()
                && !ext.is_empty()
                && ext.len() <= 10
                && ext.chars().all(|c| c.is_ascii_alphanumeric())
                && ext.chars().any(|c| c.is_ascii_alphabetic())
        }
        None => false,
    }
}

enum Resolution {
    File(String),
    Ambiguous(usize),
    Missing,
}

/// The listed files grouped by basename, so a mention is compared only
/// with the files that could end in it.
struct BasenameIndex<'a> {
    by_name: HashMap<&'a str, Vec<&'a str>>,
}

impl<'a> BasenameIndex<'a> {
    fn new(files: &'a [String]) -> Self {
        let mut by_name: HashMap<&str, Vec<&str>> = HashMap::new();
        for file in files {
            let name = file.rsplit('/').next().unwrap_or(file);
            by_name.entry(name).or_default().push(file.as_str());
        }
        Self { by_name }
    }

    fn resolve(&self, path: &str) -> Resolution {
        let comps: Vec<&str> = path
            .split('/')
            .filter(|comp| !comp.is_empty() && *comp != ".")
            .collect();
        let Some(name) = comps.last() else {
            return Resolution::Missing;
        };
        let Some(candidates) = self.by_name.get(name) else {
            return Resolution::Missing;
        };
        if comps.len() == 1 {
            return match candidates.as_slice() {
                [only] => Resolution::File((*only).to_string()),
                many => Resolution::Ambiguous(many.len()),
            };
        }
        // Longest suffix first: an exact path wins, then the fewest
        // leading components dropped. Never below two components.
        for k in (2..=comps.len()).rev() {
            let suffix = comps[comps.len() - k..].join("/");
            if let Some(exact) = candidates.iter().find(|file| **file == suffix) {
                return Resolution::File((*exact).to_string());
            }
            let tail = format!("/{suffix}");
            let matched: Vec<&&str> = candidates
                .iter()
                .filter(|file| file.ends_with(&tail))
                .collect();
            match matched.as_slice() {
                [] => continue,
                [only] => return Resolution::File((**only).to_string()),
                many => return Resolution::Ambiguous(many.len()),
            }
        }
        Resolution::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn files() -> Vec<String> {
        [
            "src/store.rs",
            "src/main.rs",
            "src/api/user.ts",
            "app/db.py",
            "crates/a/src/lib.rs",
            "crates/b/src/lib.rs",
            "tools/decoder.py",
            "Makefile",
            "web/package.json",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect()
    }

    #[test]
    fn resolves_paths_with_ranges_in_prose() {
        let scan = scan(
            "The bug is in `src/store.rs:120-180`; see also src/main.rs.",
            &files(),
            0,
        );
        assert_eq!(scan.specs(), vec!["src/store.rs:120-180", "src/main.rs"]);
        assert_eq!(
            scan.targets[0].reason(),
            r#"referenced as "src/store.rs:120-180" (input line 1)"#
        );
        assert!(scan.unresolved.is_empty());
    }

    #[test]
    fn reads_stack_trace_locations() {
        let text = "Error: boom\n    at handler (/home/ci/build/src/api/user.ts:42:7)\n  File \"app/db.py\", line 88, in query\nerror[E0308]: mismatched types\n --> src/main.rs:10:5\n";
        let scan = scan(text, &files(), 3);
        assert_eq!(
            scan.specs(),
            vec![
                "src/api/user.ts:39-45",
                "app/db.py:85-91",
                "src/main.rs:7-13"
            ]
        );
    }

    #[test]
    fn github_permalinks_and_diff_prefixes_resolve() {
        let text = "https://github.com/o/r/blob/main/src/store.rs#L10-L20\n+++ b/src/main.rs\n";
        let scan = scan(text, &files(), 0);
        assert_eq!(scan.specs(), vec!["src/store.rs:10-20", "src/main.rs"]);
    }

    #[test]
    fn bare_names_resolve_only_when_unique() {
        let scan = scan(
            "Check store.rs, lib.rs and the Makefile. e.g. v1.2",
            &files(),
            0,
        );
        assert_eq!(scan.specs(), vec!["src/store.rs", "Makefile"]);
        assert_eq!(scan.unresolved.len(), 1);
        assert_eq!(scan.unresolved[0].mention, "lib.rs");
        assert!(scan.unresolved[0].reason.contains("matches 2 files"));
    }

    #[test]
    fn partial_paths_resolve_by_suffix_but_need_two_components() {
        let scan = scan(
            "a/src/lib.rs and /usr/lib/python3/json/decoder.py and b/src/lib.rs",
            &files(),
            0,
        );
        assert_eq!(
            scan.specs(),
            vec!["crates/a/src/lib.rs", "crates/b/src/lib.rs"]
        );
        assert_eq!(scan.unresolved.len(), 1);
        assert!(scan.unresolved[0].mention.contains("decoder.py"));
    }

    #[test]
    fn ranges_win_over_bare_mentions_and_merge() {
        let text = "src/store.rs is long.\nLook at src/store.rs:10-12 and src/store.rs#L14.\nAlso `src/store.rs` lines 100-110.";
        let scan = scan(text, &files(), 1);
        assert_eq!(
            scan.specs(),
            vec!["src/store.rs:9-15", "src/store.rs:99-111"]
        );
        assert_eq!(
            scan.targets[0].reason(),
            r#"referenced 4 times (first as "src/store.rs", input line 1)"#
        );
    }

    #[test]
    fn unknown_or_ignored_paths_are_not_packed() {
        let scan = scan(
            "see src/missing.rs and .env and https://example.com/a/b.html",
            &files(),
            0,
        );
        assert!(scan.targets.is_empty());
        assert_eq!(scan.unresolved.len(), 1);
        assert_eq!(scan.unresolved[0].mention, "src/missing.rs");
    }

    #[test]
    fn package_json_is_not_cut_short() {
        let scan = scan("bump web/package.json", &files(), 0);
        assert_eq!(scan.specs(), vec!["web/package.json"]);
    }
}
