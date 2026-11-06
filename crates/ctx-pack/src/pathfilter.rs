// crates/ctx-pack/src/pathfilter.rs
//
// `--include` / `--exclude` path globs for `ctx pack` and the MCP
// `ctx_pack` tool. Shared so both entry points scope a pack the same way.
//
// Pattern rules (gitignore-flavoured, matched against the root-relative
// path with `/` separators):
//
//   * `*` matches within one path segment, `?` one character,
//     `**` any number of segments (including none), `{a,b}` alternatives.
//   * A pattern without `/` matches the file name at any depth
//     (`*.rs`, `Cargo.toml`).
//   * A pattern with `/` is anchored at the root (`src/**/*.ts`); a leading
//     `/` or `./` is ignored.
//   * A pattern that matches a directory selects everything below it
//     (`crates/ctx-web`, `docs/`).
//
// A file is kept when it matches at least one include (or there are no
// includes) and no exclude.

/// Compiled include / exclude globs.
#[derive(Debug, Clone, Default)]
pub struct PathFilter {
    include: Vec<Glob>,
    exclude: Vec<Glob>,
}

#[derive(Debug, Clone)]
struct Glob {
    /// Brace-expanded alternatives, each split into `/` segments.
    alternatives: Vec<Vec<String>>,
    /// No `/` in the pattern: match the basename or any single segment.
    basename: bool,
}

impl PathFilter {
    /// Build a filter. Each entry may hold several comma-separated
    /// patterns (`"src/**,*.md"`); commas inside `{...}` are kept.
    pub fn new<S: AsRef<str>>(include: &[S], exclude: &[S]) -> Result<Self, String> {
        Ok(PathFilter {
            include: compile_all(include)?,
            exclude: compile_all(exclude)?,
        })
    }

    /// True when the filter has no patterns and keeps every path.
    pub fn is_empty(&self) -> bool {
        self.include.is_empty() && self.exclude.is_empty()
    }

    /// Whether the root-relative `path` passes the filter.
    pub fn allows(&self, path: &str) -> bool {
        let path = path.trim_start_matches("./").replace('\\', "/");
        if !self.include.is_empty() && !self.include.iter().any(|g| g.matches(&path)) {
            return false;
        }
        !self.exclude.iter().any(|g| g.matches(&path))
    }
}

fn compile_all<S: AsRef<str>>(raw: &[S]) -> Result<Vec<Glob>, String> {
    let mut out = Vec::new();
    for entry in raw {
        for pattern in split_top_level_commas(entry.as_ref()) {
            let pattern = pattern.trim();
            if pattern.is_empty() {
                continue;
            }
            out.push(Glob::compile(pattern)?);
        }
    }
    Ok(out)
}

fn split_top_level_commas(s: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '{' => depth += 1,
            '}' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&s[start..i]);
                start = i + 1;
            }
            _ => {}
        }
    }
    out.push(&s[start..]);
    out
}

/// Expand `{a,b}` groups (nested groups included) into plain patterns.
fn expand_braces(pattern: &str) -> Result<Vec<String>, String> {
    let Some(open) = pattern.find('{') else {
        if pattern.contains('}') {
            return Err(format!("unbalanced '}}' in pattern {pattern:?}"));
        }
        return Ok(vec![pattern.to_string()]);
    };
    let mut depth = 0usize;
    let mut close = None;
    for (i, c) in pattern[open..].char_indices() {
        match c {
            '{' => depth += 1,
            '}' => {
                depth -= 1;
                if depth == 0 {
                    close = Some(open + i);
                    break;
                }
            }
            _ => {}
        }
    }
    let close = close.ok_or_else(|| format!("unbalanced '{{' in pattern {pattern:?}"))?;
    let (head, body, tail) = (
        &pattern[..open],
        &pattern[open + 1..close],
        &pattern[close + 1..],
    );
    let mut out = Vec::new();
    for alt in split_top_level_commas(body) {
        out.extend(expand_braces(&format!("{head}{alt}{tail}"))?);
    }
    if out.len() > 256 {
        return Err(format!(
            "pattern {pattern:?} expands to too many alternatives"
        ));
    }
    Ok(out)
}

impl Glob {
    fn compile(pattern: &str) -> Result<Self, String> {
        let trimmed = pattern.trim_start_matches("./").trim_start_matches('/');
        let trimmed = trimmed.trim_end_matches('/');
        if trimmed.is_empty() {
            return Err(format!("empty path pattern {pattern:?}"));
        }
        let basename = !trimmed.contains('/');
        let alternatives = expand_braces(trimmed)?
            .into_iter()
            .map(|alt| alt.split('/').map(str::to_string).collect())
            .collect();
        Ok(Glob {
            alternatives,
            basename,
        })
    }

    fn matches(&self, path: &str) -> bool {
        let segments: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        self.alternatives.iter().any(|alt| {
            if self.basename {
                // `*.rs` / `docs`: any segment (a matching directory selects
                // what is below it, a matching last segment is the file).
                return segments.iter().any(|seg| segment_match(&alt[0], seg));
            }
            // Anchored: the pattern must match the path or one of its
            // leading directories.
            (1..=segments.len()).any(|n| segments_match(alt, &segments[..n]))
        })
    }
}

fn segments_match(pattern: &[String], path: &[&str]) -> bool {
    match pattern.split_first() {
        None => path.is_empty(),
        Some((first, rest)) if first == "**" => {
            (0..=path.len()).any(|skip| segments_match(rest, &path[skip..]))
        }
        Some((first, rest)) => match path.split_first() {
            Some((seg, path_rest)) => segment_match(first, seg) && segments_match(rest, path_rest),
            None => false,
        },
    }
}

/// `*` / `?` wildcard match within a single segment.
fn segment_match(pattern: &str, text: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let t: Vec<char> = text.chars().collect();
    let (mut pi, mut ti) = (0usize, 0usize);
    let mut star: Option<(usize, usize)> = None;
    while ti < t.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == t[ti]) {
            pi += 1;
            ti += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ti));
            pi += 1;
        } else if let Some((sp, st)) = star {
            pi = sp + 1;
            ti = st + 1;
            star = Some((sp, st + 1));
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn filter(include: &[&str], exclude: &[&str]) -> PathFilter {
        PathFilter::new(include, exclude).unwrap()
    }

    #[test]
    fn empty_filter_keeps_everything() {
        let f = filter(&[], &[]);
        assert!(f.is_empty());
        assert!(f.allows("src/main.rs"));
    }

    #[test]
    fn basename_patterns_match_at_any_depth() {
        let f = filter(&["*.rs"], &[]);
        assert!(f.allows("main.rs"));
        assert!(f.allows("crates/a/src/lib.rs"));
        assert!(!f.allows("README.md"));
        assert!(!f.allows("src/rs/notes.md"));
    }

    #[test]
    fn anchored_patterns_and_double_star() {
        let f = filter(&["src/**/*.ts"], &[]);
        assert!(f.allows("src/a.ts"));
        assert!(f.allows("src/x/y/a.ts"));
        assert!(!f.allows("lib/src/a.ts"));
        assert!(!f.allows("src/a.tsx"));
        let f = filter(&["**/tests/*.rs"], &[]);
        assert!(f.allows("tests/a.rs"));
        assert!(f.allows("crates/x/tests/a.rs"));
        assert!(!f.allows("crates/x/tests/deep/a.rs"));
    }

    #[test]
    fn directory_patterns_select_their_subtree() {
        let f = filter(&["crates/ctx-web", "docs/"], &[]);
        assert!(f.allows("crates/ctx-web/src/lib.rs"));
        assert!(f.allows("docs/adr/README.md"));
        assert!(!f.allows("crates/ctx-web-extra/src/lib.rs"));
        assert!(!f.allows("crates/ctx-mcp/src/lib.rs"));
    }

    #[test]
    fn excludes_win_over_includes() {
        let f = filter(&["crates/**"], &["tests", "*_test.rs"]);
        assert!(f.allows("crates/a/src/lib.rs"));
        assert!(!f.allows("crates/a/tests/it.rs"));
        assert!(!f.allows("crates/a/src/lib_test.rs"));
        assert!(!f.allows("README.md"));
    }

    #[test]
    fn braces_and_comma_lists() {
        let f = filter(&["*.{rs,toml}"], &[]);
        assert!(f.allows("Cargo.toml"));
        assert!(f.allows("src/lib.rs"));
        assert!(!f.allows("src/lib.ts"));
        let f = filter(&["src/**,*.md"], &[]);
        assert!(f.allows("src/a/b.go"));
        assert!(f.allows("docs/x.md"));
        assert!(!f.allows("lib/x.go"));
    }

    #[test]
    fn question_mark_and_leading_dot_slash() {
        let f = filter(&["./src/v?.rs"], &[]);
        assert!(f.allows("src/v1.rs"));
        assert!(!f.allows("src/v10.rs"));
    }

    #[test]
    fn rejects_unbalanced_braces() {
        assert!(PathFilter::new(&["*.{rs"], &[] as &[&str]).is_err());
        assert!(PathFilter::new(&["*.rs}"], &[] as &[&str]).is_err());
    }
}
