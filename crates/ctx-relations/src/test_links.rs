// crates/ctx-relations/src/test_links.rs
//
// Pairing source files with the tests that exercise them.
//
// Two kinds of evidence are used, strongest first:
//
//   1. Naming conventions — `foo_test.go`, `test_foo.py`, `foo.test.ts`,
//      `foo.spec.js`, `__tests__/foo.ts`, `FooTest.java`, `foo_spec.rb`,
//      `tests/foo.rs`, a Rust `foo/tests.rs` module, … — restricted to tests
//      that live in the source's directory, one of its ancestors' test
//      directories (`tests/`, `src/test/…`) or a mirror of it.
//   2. Imports — a test file whose resolved imports include the source
//      (`crates/x/tests/api.rs` → `use x::…` → `crates/x/src/lib.rs`).
//
// The result answers "which tests should I read (or run) with this change"
// without any git history, so it also works for new files and young
// repositories where co-change companions have nothing to say.

use std::collections::BTreeMap;

use serde::Serialize;

use crate::types::Index;

/// Extensions of files that can hold executable tests.
const CODE_EXTS: &[&str] = &[
    "rs", "go", "py", "js", "jsx", "ts", "tsx", "mjs", "cjs", "mts", "cts", "svelte", "vue",
    "java", "kt", "kts", "scala", "php", "swift", "rb", "cs", "dart", "ex", "exs", "c", "cc",
    "cpp", "h", "hpp",
];

/// Directory names whose contents are tests.
const TEST_DIRS: &[&str] = &["test", "tests", "__tests__", "__test__", "spec", "specs"];

/// Directory names that hold test *support* (data, snapshots, mocks), not
/// tests. Files under them are never reported.
const SUPPORT_DIRS: &[&str] = &[
    "fixtures",
    "fixture",
    "testdata",
    "test-data",
    "test_data",
    "__snapshots__",
    "snapshots",
    "__mocks__",
    "mocks",
    "golden",
    "goldens",
];

/// File stems that name their directory rather than themselves.
const GENERIC_STEMS: &[&str] = &["mod", "lib", "main", "index", "__init__", "init"];

/// Directory names skipped when a generic stem borrows its directory's name.
const LAYOUT_DIRS: &[&str] = &["src", "source", "sources", "lib", "app", "pkg", "internal"];

/// Extensions where a `test_foo` stem marks a test (pytest, C/C++ suites).
/// Elsewhere `test_` is an ordinary word (`test_links.rs`).
const PREFIX_TEST_EXTS: &[&str] = &["py", "c", "cc", "cpp", "rb"];

/// Extensions where a CamelCase `FooTest` / `FooTests` / `FooSpec` stem marks
/// a test (JVM, Swift, PHP, C#, Scala).
const CAMEL_TEST_EXTS: &[&str] = &["java", "kt", "kts", "scala", "swift", "php", "cs"];

/// How a test was linked to a source file, strongest first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TestLinkKind {
    /// Named after the source and placed next to it (or in a mirrored test
    /// directory): `foo.rs` ↔ `tests/foo.rs`, `foo.go` ↔ `foo_test.go`.
    Name,
    /// Imports the source.
    Imports,
    /// Named after the source with a suffix: `pack.rs` ↔ `tests/pack_cli.rs`.
    NamePrefix,
    /// Named after the source but elsewhere in the tree (only reported when
    /// nothing closer matches and the name is unambiguous).
    NameElsewhere,
}

/// A test file paired with one of the seeds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TestLink {
    /// The test file, repo-relative.
    pub path: String,
    /// The seed it was paired with (its strongest link).
    pub subject: String,
    pub kind: TestLinkKind,
    /// Every seed it was paired with, `subject` first.
    pub subjects: Vec<String>,
}

impl TestLink {
    /// A one-line human reason, e.g. `test: named after src/a.rs`.
    pub fn reason(&self) -> String {
        match self.kind {
            TestLinkKind::Name => format!("test: named after {}", self.subject),
            TestLinkKind::Imports => format!("test: imports {}", self.subject),
            TestLinkKind::NamePrefix => format!("test: named like {}", self.subject),
            TestLinkKind::NameElsewhere => {
                format!("test: named after {} (elsewhere in the tree)", self.subject)
            }
        }
    }
}

fn split_name(path: &str) -> (Vec<&str>, &str) {
    let mut parts: Vec<&str> = path.split('/').filter(|p| !p.is_empty()).collect();
    let name = parts.pop().unwrap_or("");
    (parts, name)
}

fn split_ext(name: &str) -> (&str, &str) {
    match name.rfind('.') {
        Some(0) | None => (name, ""),
        Some(i) => (&name[..i], &name[i + 1..]),
    }
}

fn is_test_dir(dir: &str) -> bool {
    TEST_DIRS.iter().any(|d| dir.eq_ignore_ascii_case(d))
}

fn is_support_dir(dir: &str) -> bool {
    SUPPORT_DIRS.iter().any(|d| dir.eq_ignore_ascii_case(d))
}

fn is_code_ext(ext: &str) -> bool {
    CODE_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e))
}

/// `FooTest`, `FooTests`, `FooSpec` → `Foo` (the char before the suffix must
/// not be uppercase, so `ABTest` stays whole and `Latest` never matches).
fn strip_camel_test(stem: &str) -> Option<&str> {
    for suffix in ["Tests", "Test", "Spec"] {
        if let Some(head) = stem.strip_suffix(suffix) {
            let last = head.chars().last()?;
            if !last.is_uppercase() && last.is_alphanumeric() {
                return Some(head);
            }
        }
    }
    None
}

/// The stem a test file names, with its test marker removed, or `None` when
/// the file name carries no test marker.
fn strip_test_marker<'a>(stem: &'a str, ext: &str) -> Option<&'a str> {
    let lower = stem.to_ascii_lowercase();
    for suffix in [
        ".test", ".spec", "_test", "_tests", "_spec", "-test", "-spec",
    ] {
        if lower.len() > suffix.len() && lower.ends_with(suffix) {
            return Some(&stem[..stem.len() - suffix.len()]);
        }
    }
    if lower.len() > 5
        && lower.starts_with("test_")
        && PREFIX_TEST_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e))
    {
        return Some(&stem[5..]);
    }
    if CAMEL_TEST_EXTS.iter().any(|e| ext.eq_ignore_ascii_case(e)) {
        return strip_camel_test(stem);
    }
    None
}

/// Whether `path` (repo-relative, `/`-separated) is a test file: a code file
/// whose name carries a test marker or that lives under a test directory —
/// but not test data under `fixtures/`, `testdata/`, `__snapshots__/`, ….
pub fn is_test_path(path: &str) -> bool {
    let (dirs, name) = split_name(path);
    let (stem, ext) = split_ext(name);
    if !is_code_ext(ext) || dirs.iter().any(|d| is_support_dir(d)) {
        return false;
    }
    if dirs.iter().any(|d| is_test_dir(d)) {
        return true;
    }
    if stem.eq_ignore_ascii_case("tests") || stem.eq_ignore_ascii_case("test") {
        // Rust `foo/tests.rs`, Go/Python `test.py`-style modules.
        return ext.eq_ignore_ascii_case("rs");
    }
    strip_test_marker(stem, ext).is_some()
}

fn normalize(stem: &str) -> String {
    stem.to_ascii_lowercase().replace('-', "_")
}

/// The nearest directory in `dirs` that names a unit (not a test or layout
/// directory such as `src/`), used for `mod.rs`, `index.ts`, `__init__.py`.
fn unit_dir_name(dirs: &[&str]) -> Option<String> {
    dirs.iter()
        .rev()
        .find(|d| !is_test_dir(d) && !LAYOUT_DIRS.iter().any(|l| d.eq_ignore_ascii_case(l)))
        .map(|d| normalize(d))
}

fn named_stem(stem: &str, dirs: &[&str]) -> Option<String> {
    let lower = stem.to_ascii_lowercase();
    if GENERIC_STEMS.contains(&lower.as_str()) || lower == "tests" || lower == "test" {
        unit_dir_name(dirs)
    } else {
        Some(normalize(stem))
    }
}

/// The unit a source file is named for: `src/auth/session.ts` → `session`,
/// `src/auth/index.ts` → `auth`, `crates/x/src/lib.rs` → `x`.
fn source_subject(path: &str) -> Option<String> {
    let (dirs, name) = split_name(path);
    let (stem, ext) = split_ext(name);
    if !is_code_ext(ext) {
        return None;
    }
    named_stem(stem, &dirs)
}

/// The unit a test file is named for: `foo_test.go` → `foo`,
/// `__tests__/session.test.ts` → `session`, `src/pack/tests.rs` → `pack`.
fn test_subject(path: &str) -> Option<String> {
    let (dirs, name) = split_name(path);
    let (stem, ext) = split_ext(name);
    let stem = strip_test_marker(stem, ext).unwrap_or(stem);
    named_stem(stem, &dirs)
}

/// The directory a file belongs to once test and source-set layout
/// directories are removed, so `src/main/java/x` and `src/test/java/x`, or
/// `crates/a/src` and `crates/a/tests`, can be compared.
fn location_key(path: &str) -> Vec<String> {
    let (dirs, _) = split_name(path);
    dirs.iter()
        .filter(|d| !is_test_dir(d) && !d.eq_ignore_ascii_case("main"))
        .map(|d| d.to_ascii_lowercase())
        .collect()
}

fn common_prefix(a: &[String], b: &[String]) -> usize {
    a.iter().zip(b).take_while(|(x, y)| x == y).count()
}

/// Test files in `files` that exercise `seeds` (all repo-relative), best
/// first: by link strength, then in seed order, then path. Each test is
/// reported once, for its strongest (then nearest) link. Seeds that are themselves tests
/// are skipped, and no seed is reported as a test of another.
pub fn find_tests(files: &[String], index: Option<&Index>, seeds: &[String]) -> Vec<TestLink> {
    let seed_set: std::collections::BTreeSet<&str> = seeds.iter().map(String::as_str).collect();
    // Test files with what they are named for and where they live.
    let tests: Vec<(&str, Option<String>, Vec<String>)> = files
        .iter()
        .filter(|f| !seed_set.contains(f.as_str()) && is_test_path(f))
        .map(|f| (f.as_str(), test_subject(f), location_key(f)))
        .collect();

    // path → (kind, -nearness, subject)
    // path → (kind, distance, seed position, seed)
    let mut best: BTreeMap<&str, (TestLinkKind, usize, usize, &str)> = BTreeMap::new();
    let mut found: Vec<(&str, TestLinkKind, usize, usize)> = Vec::new();
    for (at, seed) in seeds.iter().enumerate() {
        if is_test_path(seed) {
            continue;
        }
        if let Some(subject) = source_subject(seed) {
            let key = location_key(seed);
            // Candidates whose directory is the seed's, an ancestor's or a
            // mirror of it: (path, kind, nearness).
            let mut near: Vec<(&str, TestLinkKind, usize)> = Vec::new();
            let mut elsewhere: Vec<&str> = Vec::new();
            for (path, test_subject, test_key) in &tests {
                let Some(test_subject) = test_subject else {
                    continue;
                };
                let exact = *test_subject == subject;
                let prefixed = !exact
                    && subject.len() >= 3
                    && test_subject.len() > subject.len() + 1
                    && test_subject.starts_with(&subject)
                    && test_subject.as_bytes()[subject.len()] == b'_';
                if !exact && !prefixed {
                    continue;
                }
                let shared = common_prefix(&key, test_key);
                let related = shared == test_key.len() || shared == key.len();
                if related {
                    let kind = if exact {
                        TestLinkKind::Name
                    } else {
                        TestLinkKind::NamePrefix
                    };
                    near.push((path, kind, shared));
                } else if exact {
                    elsewhere.push(path);
                }
            }
            // Keep only the nearest matches of the strongest kind found.
            if let Some(top) = near
                .iter()
                .map(|(_, k, n)| (*k, std::cmp::Reverse(*n)))
                .min()
            {
                for (path, kind, shared) in &near {
                    if *kind == TestLinkKind::NamePrefix
                        || (*kind, std::cmp::Reverse(*shared)) == top
                    {
                        found.push((path, *kind, *shared, at));
                    }
                }
            } else if !elsewhere.is_empty() && elsewhere.len() <= 2 {
                for path in elsewhere {
                    found.push((path, TestLinkKind::NameElsewhere, 0, at));
                }
            }
        }
        if let Some(index) = index {
            for importer in index.importers.get(seed).into_iter().flatten() {
                if seed_set.contains(importer.as_str()) || !is_test_path(importer) {
                    continue;
                }
                let shared = common_prefix(&location_key(seed), &location_key(importer));
                found.push((importer.as_str(), TestLinkKind::Imports, shared, at));
            }
        }
    }
    let mut all_subjects: BTreeMap<&str, Vec<usize>> = BTreeMap::new();
    for (path, kind, shared, at) in found {
        let linked = all_subjects.entry(path).or_default();
        if !linked.contains(&at) {
            linked.push(at);
        }
        let candidate = (kind, usize::MAX - shared, at, seeds[at].as_str());
        match best.get(path) {
            Some(current) if *current <= candidate => {}
            _ => {
                best.insert(path, candidate);
            }
        }
    }
    // Strongest links first; within a kind, in the order the seeds were given.
    let mut out: Vec<(TestLinkKind, usize, TestLink)> = best
        .into_iter()
        .map(|(path, (kind, _, at, subject))| {
            let mut subjects = vec![subject.to_string()];
            for other in all_subjects.get(path).into_iter().flatten() {
                if *other != at {
                    subjects.push(seeds[*other].clone());
                }
            }
            (
                kind,
                at,
                TestLink {
                    path: path.to_string(),
                    subject: subject.to_string(),
                    kind,
                    subjects,
                },
            )
        })
        .collect();
    out.sort_by(|a, b| {
        a.0.cmp(&b.0)
            .then(a.1.cmp(&b.1))
            .then_with(|| a.2.path.cmp(&b.2.path))
    });
    out.into_iter().map(|(_, _, link)| link).collect()
}

/// The source files `test` exercises — the reverse of [`find_tests`]: each
/// returned link's `path` is a source and its `subject` is `test`. Uses the
/// same rules, so a source is listed exactly when `find_tests` on it would
/// report `test`. Empty when `test` is not a test file.
pub fn find_subjects(files: &[String], index: Option<&Index>, test: &str) -> Vec<TestLink> {
    if !is_test_path(test) {
        return Vec::new();
    }
    let wanted = test_subject(test);
    let mut candidates: Vec<&String> = Vec::new();
    if let Some(index) = index {
        candidates.extend(index.imports.get(test).into_iter().flatten());
    }
    if let Some(wanted) = wanted.as_deref() {
        candidates.extend(files.iter().filter(|file| {
            !is_test_path(file)
                && source_subject(file).is_some_and(|subject| {
                    wanted == subject
                        || (subject.len() >= 3
                            && wanted.len() > subject.len() + 1
                            && wanted.starts_with(&subject)
                            && wanted.as_bytes()[subject.len()] == b'_')
                })
        }));
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut out: Vec<TestLink> = Vec::new();
    for candidate in candidates {
        if !seen.insert(candidate.as_str()) || is_test_path(candidate) {
            continue;
        }
        let seed = [candidate.clone()];
        if let Some(link) = find_tests(files, index, &seed)
            .into_iter()
            .find(|link| link.path == test)
        {
            out.push(TestLink {
                path: candidate.clone(),
                subject: test.to_string(),
                kind: link.kind,
                subjects: vec![test.to_string()],
            });
        }
    }
    // A name match elsewhere in the tree is only a guess; drop it when the
    // test has a closer subject.
    if out
        .iter()
        .any(|link| link.kind != TestLinkKind::NameElsewhere)
    {
        out.retain(|link| link.kind != TestLinkKind::NameElsewhere);
    }
    out.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.path.cmp(&b.path)));
    out
}

/// Walks `root`, resolves its imports and pairs `seeds` with their tests.
/// See [`find_tests`].
pub fn tests_for(root: &str, seeds: &[String]) -> std::io::Result<Vec<TestLink>> {
    let files: Vec<String> = crate::walk::walk(root)?
        .into_iter()
        .filter(|f| !f.is_dir && f.path != ".")
        .map(|f| f.path)
        .collect();
    let index = crate::build::build(root).ok();
    Ok(find_tests(&files, index.as_ref(), seeds))
}

#[cfg(test)]
mod unit_tests {
    use super::*;

    fn files(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    fn paths(links: &[TestLink]) -> Vec<&str> {
        links.iter().map(|l| l.path.as_str()).collect()
    }

    #[test]
    fn recognizes_test_files_across_languages() {
        for path in [
            "pkg/store_test.go",
            "app/test_db.py",
            "app/db_test.py",
            "src/a.test.ts",
            "src/a.spec.tsx",
            "src/__tests__/a.ts",
            "src/test/java/x/FooTest.java",
            "Tests/AppTests.swift",
            "spec/user_spec.rb",
            "crates/x/tests/api.rs",
            "src/pack/tests.rs",
        ] {
            assert!(is_test_path(path), "{path}");
        }
        for path in [
            "src/latest.rs",
            "src/contest.go",
            "src/ABTest.java",
            "tests/fixtures/sample.rs",
            "pkg/testdata/a.go",
            "tests/README.md",
            "src/testing.rs",
            "src/attest.py",
            "crates/x/src/test_links.rs",
        ] {
            assert!(!is_test_path(path), "{path}");
        }
    }

    #[test]
    fn pairs_by_name_next_to_source_and_in_mirrored_dirs() {
        let all = files(&[
            "pkg/store.go",
            "pkg/store_test.go",
            "other/store_test.go",
            "src/auth/session.ts",
            "src/auth/__tests__/session.test.ts",
            "src/main/java/x/Foo.java",
            "src/test/java/x/FooTest.java",
            "crates/a/src/excerpt.rs",
            "crates/a/tests/excerpt.rs",
            "crates/b/tests/excerpt.rs",
        ]);
        let seeds = files(&[
            "pkg/store.go",
            "src/auth/session.ts",
            "src/main/java/x/Foo.java",
            "crates/a/src/excerpt.rs",
        ]);
        let found = find_tests(&all, None, &seeds);
        assert_eq!(
            paths(&found),
            [
                "pkg/store_test.go",
                "src/auth/__tests__/session.test.ts",
                "src/test/java/x/FooTest.java",
                "crates/a/tests/excerpt.rs",
            ]
        );
        assert!(found.iter().all(|l| l.kind == TestLinkKind::Name));
        assert_eq!(found[0].reason(), "test: named after pkg/store.go");
    }

    #[test]
    fn generic_stems_use_their_directory_and_python_tests_dir_matches() {
        let all = files(&[
            "src/pack/mod.rs",
            "src/pack/tests.rs",
            "app/db.py",
            "tests/test_db.py",
            "tests/test_dbx.py",
        ]);
        let found = find_tests(&all, None, &files(&["src/pack/mod.rs", "app/db.py"]));
        assert_eq!(paths(&found), ["src/pack/tests.rs", "tests/test_db.py"]);
    }

    #[test]
    fn prefix_names_and_elsewhere_names_are_weaker() {
        let all = files(&[
            "crates/cli/src/pack.rs",
            "crates/cli/tests/pack_cli.rs",
            "lib/parser.rb",
            "spec/unit/parser_spec.rb",
        ]);
        let found = find_tests(
            &all,
            None,
            &files(&["crates/cli/src/pack.rs", "lib/parser.rb"]),
        );
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].kind, TestLinkKind::NamePrefix);
        assert_eq!(found[0].path, "crates/cli/tests/pack_cli.rs");
        assert_eq!(found[1].kind, TestLinkKind::NameElsewhere);
    }

    #[test]
    fn subjects_are_the_reverse_of_tests() {
        let mut index = Index::default();
        index.imports.insert(
            "crates/x/tests/api.rs".into(),
            vec!["crates/x/src/lib.rs".into()],
        );
        index.importers.insert(
            "crates/x/src/lib.rs".into(),
            vec!["crates/x/tests/api.rs".into()],
        );
        let all = files(&[
            "crates/x/src/lib.rs",
            "crates/x/src/api.rs",
            "crates/x/tests/api.rs",
            "crates/z/src/api.rs",
            "pkg/store.go",
            "pkg/store_test.go",
        ]);
        let found = find_subjects(&all, Some(&index), "crates/x/tests/api.rs");
        assert_eq!(
            paths(&found),
            ["crates/x/src/api.rs", "crates/x/src/lib.rs"]
        );
        assert_eq!(found[0].kind, TestLinkKind::Name);
        assert_eq!(found[1].kind, TestLinkKind::Imports);
        assert_eq!(
            paths(&find_subjects(&all, None, "pkg/store_test.go")),
            ["pkg/store.go"]
        );
        assert!(find_subjects(&all, None, "pkg/store.go").is_empty());
    }

    #[test]
    fn importing_tests_are_found_and_seeds_are_never_reported() {
        let mut index = Index::default();
        index.importers.insert(
            "crates/x/src/lib.rs".into(),
            vec![
                "crates/x/tests/api.rs".into(),
                "crates/y/src/main.rs".into(),
            ],
        );
        let all = files(&[
            "crates/x/src/lib.rs",
            "crates/x/tests/api.rs",
            "crates/y/src/main.rs",
            "crates/x/tests/x.rs",
        ]);
        let found = find_tests(&all, Some(&index), &files(&["crates/x/src/lib.rs"]));
        // `tests/x.rs` is named after the crate (`lib.rs` → `x`).
        assert_eq!(
            paths(&found),
            ["crates/x/tests/x.rs", "crates/x/tests/api.rs"]
        );
        assert_eq!(found[1].kind, TestLinkKind::Imports);
        let found = find_tests(
            &all,
            Some(&index),
            &files(&["crates/x/src/lib.rs", "crates/y/src/main.rs"]),
        );
        assert_eq!(found[0].subjects, ["crates/x/src/lib.rs"]);
        let seeds = files(&["crates/x/src/lib.rs", "crates/x/tests/api.rs"]);
        let found = find_tests(&all, Some(&index), &seeds);
        assert_eq!(paths(&found), ["crates/x/tests/x.rs"]);
    }
}
