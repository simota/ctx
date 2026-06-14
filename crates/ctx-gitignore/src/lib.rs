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
use std::sync::OnceLock;

use regex::Regex;

/// One compiled ignore pattern (mirrors Go's `IgnorePattern`, minus the
/// line-number metadata which the walker never uses).
struct IgnorePattern {
    pattern: Regex,
    negate: bool,
}

/// Mirrors Go's `GitIgnore`: an ordered list of compiled patterns.
pub struct GitIgnore {
    patterns: Vec<IgnorePattern>,
}

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
        Self { patterns }
    }

    /// Port of Go's `CompileIgnoreFile`.
    pub fn from_file(path: &Path) -> Result<Self, String> {
        let body = std::fs::read_to_string(path)
            .map_err(|err| format!("read {}: {err}", path.display()))?;
        Ok(Self::from_lines(body.split('\n')))
    }

    /// Port of Go's `CompileIgnoreFileAndLines`: file content first, then the
    /// extra lines (order matters for negation precedence).
    pub fn from_file_and_lines(path: &Path, lines: &[String]) -> Result<Self, String> {
        let body = std::fs::read_to_string(path)
            .map_err(|err| format!("read {}: {err}", path.display()))?;
        Ok(Self::from_lines(
            body.split('\n').chain(lines.iter().map(String::as_str)),
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
        let mut matches = false;
        for ip in &self.patterns {
            if ip.pattern.is_match(&f) {
                if !ip.negate {
                    matches = true;
                } else if matches {
                    matches = false;
                }
            }
        }
        matches
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

    // Escaped star matches a literal '*'.
    #[test]
    fn escaped_star() {
        let object = compile(&[r"a\*b"]);
        assert!(object.matches_path("a*b"));
        assert!(!object.matches_path("axb"));
    }
}
