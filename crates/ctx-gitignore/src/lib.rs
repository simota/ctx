//! Faithful Rust port of `github.com/sabhiram/go-gitignore`
//! @ v0.0.0-20210923224102-525f6e181f06 (`ignore.go`) — the exact library the
//! frozen Go oracle's `internal/walk` used for `.gitignore` / `.ctxignore`
//! matching. Byte-parity with the oracle requires reproducing this library's
//! behaviour, including its deliberate quirks:
//!
//! - `?` is escaped and treated as a LITERAL character (git treats it as a
//!   single-char wildcard; this library does not).
//! - The `([^/+])/.*\*\.` heuristic prepends a leading `/` to patterns like
//!   `foo/*.blah`, anchoring them to the root.
//! - The regex-transform replacement ORDER matters and is preserved exactly
//!   (dot-escape → `/**/` → `**/` → `/**` → `\*` → `*` → `?` → magic-star).
//! - `regexp.Compile` errors are silently ignored in Go (the line is
//!   dropped); represented here as `Option` and dropped likewise.
//! - A bare `!` line compiles to a negation pattern that matches everything
//!   (same as Go).

use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::OnceLock;

use regex::{Regex, RegexSet};

/// One compiled ignore pattern (mirrors Go's `IgnorePattern`, minus the
/// line-number metadata which the walker never uses).
struct IgnorePattern {
    pattern: Regex,
    negate: bool,
}

/// Mirrors Go's `GitIgnore`: an ordered list of compiled patterns.
pub struct GitIgnore {
    patterns: Vec<IgnorePattern>,
    /// Number of `matches_path` calls so far (drives the lazy `set` build).
    calls: AtomicUsize,
    /// All patterns compiled into one automaton, so a path is matched against
    /// every pattern in a single pass (~10x faster than one regex run per
    /// pattern). Index `i` of the set is `patterns[i]`. Compiling it costs
    /// about as much as a few hundred per-pattern lookups, so it is built only
    /// once a walk has made `SET_AFTER_CALLS` calls; `None` inside means the
    /// combined set could not be built and the per-pattern loop is kept.
    set: OnceLock<Option<RegexSet>>,
}

/// Calls answered by the per-pattern loop before the `RegexSet` is built.
const SET_AFTER_CALLS: usize = 256;

/// Port of Go's `getPatternFromLine`. Returns `None` when the line is a
/// comment, blank, or fails to compile (Go drops those lines silently).
fn get_pattern_from_line(line: &str) -> Option<(Regex, bool)> {
    // Trim OS-specific carriage returns. [Go: strings.TrimRight(line, "\r")]
    let mut line = line.trim_end_matches('\r').to_string();

    // Strip comments [Rule 2]
    if line.starts_with('#') {
        return None;
    }

    // Trim string [Rule 3] (spaces only, both ends — matches strings.Trim(line, " "))
    line = line.trim_matches(' ').to_string();

    if line.is_empty() {
        return None;
    }

    // [Rule 4]: leading "!" negates the pattern.
    let mut negate = false;
    if line.starts_with('!') {
        negate = true;
        line.remove(0);
    }

    // Handle [Rule 2, 4] when # or ! is escaped with a \.
    // [Go: regexp.MustCompile(`^(\#|\!)`) then strip first char]
    if line.starts_with('#') || line.starts_with('!') {
        line.remove(0);
    }

    // If we encounter a foo/*.blah in a folder, prepend the / char.
    // [Go: regexp `([^\/+])/.*\*\.` — char class is "not / and not +"]
    static LEADING_SLASH_HEURISTIC: OnceLock<Regex> = OnceLock::new();
    let heuristic = LEADING_SLASH_HEURISTIC.get_or_init(|| Regex::new(r"([^/+])/.*\*\.").unwrap());
    if heuristic.is_match(&line) && !line.starts_with('/') {
        line.insert(0, '/');
    }

    // Handle escaping the "." char.
    line = line.replace('.', r"\.");

    const MAGIC_STAR: &str = "#$~";

    // Handle "/**/" usage.
    if line.starts_with("/**/") {
        line.remove(0);
    }
    line = line.replace("/**/", "(/|/.+/)");
    line = line.replace("**/", &format!("(|.{MAGIC_STAR}/)"));
    line = line.replace("/**", &format!("(|/.{MAGIC_STAR})"));

    // Handle escaping the "*" char (a user-written `\*` literal star).
    line = line.replace(r"\*", &format!("\\{MAGIC_STAR}"));
    line = line.replace('*', "([^/]*)");

    // Handle escaping the "?" char (quirk: treated as a literal).
    line = line.replace('?', r"\?");

    line = line.replace(MAGIC_STAR, "*");

    // Temporary regex.
    let expr = if line.ends_with('/') {
        format!("{line}(|.*)$")
    } else {
        format!("{line}(|/.*)$")
    };
    let expr = if let Some(rest) = expr.strip_prefix('/') {
        format!("^(|/){rest}")
    } else {
        format!("^(|.*/){expr}")
    };

    // [Go: pattern, _ := regexp.Compile(expr) — compile errors drop the line]
    Regex::new(&expr).ok().map(|re| (re, negate))
}

/// Reads an ignore file as raw bytes. Go reads it with `ioutil.ReadFile` and
/// never validates the encoding, so a file that is not valid UTF-8 (e.g. a
/// Shift_JIS or Latin-1 comment) must not fail the whole walk.
fn read_ignore_file(path: &Path) -> Result<Vec<u8>, String> {
    std::fs::read(path).map_err(|err| format!("read {}: {err}", path.display()))
}

/// Splits on `\n` like Go's `strings.Split`, dropping any line that is not
/// valid UTF-8: Go's `regexp.Compile` rejects such a pattern and the library
/// drops the line, so the remaining lines still apply.
fn utf8_lines(body: &[u8]) -> impl Iterator<Item = &str> {
    body.split(|&b| b == b'\n')
        .filter_map(|line| std::str::from_utf8(line).ok())
}

impl GitIgnore {
    /// Port of Go's `CompileIgnoreLines`.
    pub fn from_lines<I, S>(lines: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<str>,
    {
        let mut patterns = Vec::new();
        for line in lines {
            if let Some((pattern, negate)) = get_pattern_from_line(line.as_ref()) {
                patterns.push(IgnorePattern { pattern, negate });
            }
        }
        Self {
            patterns,
            calls: AtomicUsize::new(0),
            set: OnceLock::new(),
        }
    }

    /// Port of Go's `CompileIgnoreFile`.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let body = read_ignore_file(path)?;
        Ok(Self::from_lines(utf8_lines(&body)))
    }

    /// Port of Go's `CompileIgnoreFileAndLines`: file content first, then the
    /// extra lines (order matters for negation precedence).
    pub fn from_file_and_lines(path: &Path, lines: &[String]) -> Result<Self, String> {
        let body = read_ignore_file(path)?;
        Ok(Self::from_lines(
            utf8_lines(&body).chain(lines.iter().map(String::as_str)),
        ))
    }

    /// Port of Go's `MatchesPath`: later patterns win; a negated pattern only
    /// clears a match already established by an earlier pattern.
    pub fn matches_path(&self, f: &str) -> bool {
        // Go replaces the OS path separator with '/'; on Unix this is a no-op.
        let f = if std::path::MAIN_SEPARATOR == '/' {
            std::borrow::Cow::Borrowed(f)
        } else {
            std::borrow::Cow::Owned(f.replace(std::path::MAIN_SEPARATOR, "/"))
        };
        match self.regex_set() {
            // `SetMatches::iter` yields the matching indices in ascending
            // (= pattern) order, and a non-matching pattern never changes
            // the state, so this is the same fold as the per-pattern loop.
            Some(set) => fold_matches(set.matches(&f).iter().map(|idx| &self.patterns[idx])),
            None => self.matches_path_per_pattern(&f),
        }
    }

    fn matches_path_per_pattern(&self, f: &str) -> bool {
        fold_matches(self.patterns.iter().filter(|ip| ip.pattern.is_match(f)))
    }
}

/// Folds the patterns that match a path, in pattern order: a pattern sets the
/// match, a negated one clears a match an earlier pattern established.
fn fold_matches<'a>(matching: impl Iterator<Item = &'a IgnorePattern>) -> bool {
    let mut matches = false;
    for ip in matching {
        if !ip.negate {
            matches = true;
        } else if matches {
            matches = false;
        }
    }
    matches
}

impl GitIgnore {
    fn regex_set(&self) -> Option<&RegexSet> {
        if let Some(set) = self.set.get() {
            return set.as_ref();
        }
        if self.calls.fetch_add(1, Ordering::Relaxed) < SET_AFTER_CALLS {
            return None;
        }
        self.set
            .get_or_init(|| RegexSet::new(self.patterns.iter().map(|ip| ip.pattern.as_str())).ok())
            .as_ref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn compile(lines: &[&str]) -> GitIgnore {
        GitIgnore::from_lines(lines.iter().copied())
    }

    // Port of TestCompileIgnoreLines.
    #[test]
    fn basic_lines() {
        let object = compile(&["abc/def", "a/b/c", "b"]);
        assert!(object.matches_path("abc/def/child"));
        assert!(object.matches_path("a/b/c/d"));
        assert!(!object.matches_path("abc"));
        assert!(!object.matches_path("def"));
        assert!(!object.matches_path("bd"));
    }

    // Port of TestCompileIgnoreLines_HandleIncludePattern (negation).
    #[test]
    fn negation() {
        let object = compile(&[
            "",
            "# exclude everything except directory foo/bar",
            "/*",
            "!/foo",
            "/foo/*",
            "!/foo/bar",
            "",
        ]);
        assert!(object.matches_path("a"));
        assert!(object.matches_path("foo/baz"));
        assert!(!object.matches_path("foo"));
        assert!(!object.matches_path("/foo/bar"));
    }

    // Port of TestCompileIgnoreLines_HandleSpaces (comments / blanks dropped).
    #[test]
    fn comments_and_spaces() {
        let object = compile(&["#", "# A comment", "", "    # Invalid Comment", "abc/def"]);
        // Quirk: "    # Invalid Comment" is NOT a comment — the '#'-prefix
        // check runs before space-trimming, so it compiles as a pattern
        // (with the leading '#' then stripped). Mirror Go: 2 patterns total.
        assert_eq!(object.patterns.len(), 2);
        assert!(!object.matches_path("abc/abc"));
        assert!(object.matches_path("abc/def"));
    }

    // Port of TestCompileIgnoreLines_HandleLeadingSlash.
    #[test]
    fn leading_slash() {
        let object = compile(&["/a/b/c", "d/e/f", "/g"]);
        assert!(object.matches_path("a/b/c"));
        assert!(object.matches_path("a/b/c/d"));
        assert!(object.matches_path("d/e/f"));
        assert!(object.matches_path("g"));
    }

    // Port of TestCompileIgnoreLines_HandleLeadingSpecialChars.
    #[test]
    fn leading_special_chars() {
        let object = compile(&["# Comment", r"\#file.txt", r"\!file.txt", "file.txt"]);
        assert!(object.matches_path("#file.txt"));
        assert!(object.matches_path("!file.txt"));
        assert!(object.matches_path("a/!file.txt"));
        assert!(object.matches_path("file.txt"));
        assert!(object.matches_path("a/file.txt"));
        assert!(!object.matches_path("file2.txt"));
    }

    // Port of TestCompileIgnoreLines_HandleAllFilesInDir (the leading-slash
    // heuristic anchors "Documentation/*.html" to the root).
    #[test]
    fn all_files_in_dir() {
        let object = compile(&["Documentation/*.html"]);
        assert!(object.matches_path("Documentation/git.html"));
        assert!(!object.matches_path("Documentation/ppc/ppc.html"));
        assert!(!object.matches_path("tools/perf/Documentation/perf.html"));
    }

    // Port of TestCompileIgnoreLines_HandleDoubleStar.
    #[test]
    fn double_star() {
        let object = compile(&["**/foo", "bar"]);
        assert!(object.matches_path("foo"));
        assert!(object.matches_path("baz/foo"));
        assert!(object.matches_path("bar"));
        assert!(object.matches_path("baz/bar"));
    }

    // Port of TestCompileIgnoreLines_HandleLeadingSlashPath.
    #[test]
    fn anchored_glob() {
        let object = compile(&["/*.c"]);
        assert!(object.matches_path("hello.c"));
        assert!(!object.matches_path("foo/hello.c"));
    }

    // Port of ExampleCompileIgnoreLines.
    #[test]
    fn example_lines() {
        let object = compile(&["node_modules", "*.out", "foo/*.c"]);
        assert!(object.matches_path("node_modules/test/foo.js"));
        assert!(object.matches_path("node_modules2/test.out"));
        assert!(!object.matches_path("test/foo.js"));
    }

    // Port of TestCompileIgnoreLines_CheckNestedDotFiles.
    #[test]
    fn nested_dot_files() {
        let object = compile(&[
            "**/external/**/*.md",
            "**/external/**/*.json",
            "**/external/**/*.gzip",
            "**/external/**/.*ignore",
            "**/external/foobar/*.css",
            "**/external/barfoo/less",
            "**/external/barfoo/scss",
        ]);
        assert!(object.matches_path("external/foobar/angular.foo.css"));
        assert!(object.matches_path("external/barfoo/.gitignore"));
        assert!(object.matches_path("external/barfoo/.bower.json"));
    }

    // Port of TestCompileIgnoreLines_CarriageReturn.
    #[test]
    fn carriage_return() {
        let object = compile(&["abc/def\r", "a/b/c\r", "b\r"]);
        assert!(object.matches_path("abc/def/child"));
        assert!(object.matches_path("a/b/c/d"));
        assert!(!object.matches_path("abc"));
        assert!(!object.matches_path("def"));
        assert!(!object.matches_path("bd"));
    }

    // Port of TestWildCardFiles.
    #[test]
    fn wildcard_files() {
        let object = compile(&["*.swp", "/foo/*.wat", "bar/*.txt"]);
        assert!(object.matches_path("yo.swp"));
        assert!(object.matches_path("something/else/but/it/hasyo.swp"));
        assert!(object.matches_path("foo/bar.wat"));
        assert!(object.matches_path("/foo/something.wat"));
        assert!(object.matches_path("bar/something.txt"));
        assert!(object.matches_path("/bar/somethingelse.txt"));
        assert!(!object.matches_path("something/not/infoo/wat.wat"));
        assert!(!object.matches_path("something/not/infoo/wat.txt"));
    }

    // Port of TestPrecedingSlash.
    #[test]
    fn preceding_slash() {
        let object = compile(&["/foo", "bar/"]);
        assert!(object.matches_path("foo/bar.wat"));
        assert!(object.matches_path("/foo/something.txt"));
        assert!(object.matches_path("bar/something.txt"));
        assert!(object.matches_path("/bar/somethingelse.go"));
        assert!(object.matches_path("/boo/something/bar/boo.txt"));
        assert!(!object.matches_path("something/foo/something.txt"));
    }

    // Quirk: `?` is escaped and matches only a literal '?', unlike git.
    #[test]
    fn question_mark_is_literal() {
        let object = compile(&["fo?bar"]);
        assert!(object.matches_path("fo?bar"));
        assert!(!object.matches_path("foobar"));
        assert!(!object.matches_path("foxbar"));
    }

    // Dir-only patterns ("dist/") do NOT match a plain file named "dist",
    // but do match the directory check-path "dist/" and anything below it.
    #[test]
    fn dir_only_pattern() {
        let object = compile(&["dist/"]);
        assert!(!object.matches_path("dist"));
        assert!(object.matches_path("dist/"));
        assert!(object.matches_path("dist/app.js"));
    }

    // Go reads ignore files as bytes: a line that is not valid UTF-8 is
    // dropped (regexp.Compile fails) and every other line still applies.
    #[test]
    fn non_utf8_lines_are_dropped_not_fatal() {
        let dir = std::env::temp_dir().join(format!("ctx-gitignore-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join(".gitignore");
        std::fs::write(
            &path,
            b"# \x83R\x83\x81\x83\x93\x83g\nsecret.txt\nbuild/\n\x83t\x83@.txt\n",
        )
        .unwrap();
        let object = GitIgnore::from_file(&path).expect("non-UTF-8 ignore file must load");
        assert_eq!(object.patterns.len(), 2);
        assert!(object.matches_path("secret.txt"));
        assert!(object.matches_path("build/"));
        assert!(!object.matches_path("keep.txt"));
        let object = GitIgnore::from_file_and_lines(&path, &["*.log".to_string()]).unwrap();
        assert!(object.matches_path("secret.txt"));
        assert!(object.matches_path("a.log"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    // Escaped star matches a literal '*'.
    #[test]
    fn escaped_star() {
        let object = compile(&[r"a\*b"]);
        assert!(object.matches_path("a*b"));
        assert!(!object.matches_path("axb"));
    }

    #[test]
    fn regex_set_agrees_with_per_pattern_matching() {
        let lines = [
            "*.log",
            "!keep.log",
            "build/",
            "/root-only",
            "**/deep",
            "a/**/z",
            "docs/*.md",
            "!docs/README.md",
            "target/",
            "!",
            "?ab",
            "\\#hash",
            "node_modules/",
            "!node_modules/keep/",
        ];
        let paths = [
            "x.log",
            "keep.log",
            "src/keep.log",
            "build",
            "build/",
            "src/build/out.o",
            "root-only",
            "src/root-only",
            "deep",
            "x/y/deep/z",
            "a/z",
            "a/b/c/z",
            "docs/guide.md",
            "docs/README.md",
            "docs/sub/x.md",
            "target/",
            "crates/x/target/",
            "?ab",
            "xab",
            "#hash",
            "node_modules/keep/",
            "node_modules/pkg/index.js",
            "src/main.rs",
            "",
            "/",
        ];
        let ignore = compile(&lines);
        // Past the threshold every call is answered by the RegexSet.
        for _ in 0..=SET_AFTER_CALLS {
            ignore.matches_path("warm-up");
        }
        assert!(matches!(ignore.set.get(), Some(Some(_))));
        for path in paths {
            assert_eq!(
                ignore.matches_path(path),
                ignore.matches_path_per_pattern(path),
                "{path:?}"
            );
        }
    }
}
