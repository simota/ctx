//! `ctx tests`: the test files that exercise a set of source files — named
//! after them (`foo_test.go`, `test_foo.py`, `foo.test.ts`, `tests/foo.rs`,
//! `FooTest.java`, …) or importing them. Answers "which tests do I read or
//! run for this change?" and feeds `ctx pack --from-stdin`.

use std::collections::BTreeSet;
use std::env;
use std::ffi::{OsStr, OsString};
use std::io;
use std::path::Path;
use std::process::ExitCode;

use serde::Serialize;

use crate::commands::pack::{branch_changed_paths, git_changed_paths};
use crate::commands::relations::relation_anchor;
use crate::common::*;

#[derive(Debug, Default)]
pub(crate) struct TestsArgs {
    paths: Vec<OsString>,
    changed: bool,
    base: String,
    format: String,
    top: usize,
}

#[derive(Debug, Serialize)]
struct TestsEntry {
    path: String,
    /// `changed` (a selected test file itself), `name`, `imports`,
    /// `name_prefix`, `name_elsewhere` or `inside` (in a directory anchor).
    link: String,
    /// The source file it was paired with (empty for `changed`).
    #[serde(skip_serializing_if = "String::is_empty")]
    subject: String,
    reason: String,
}

#[derive(Debug, Serialize)]
struct TestsReport {
    /// The source files the tests were looked up for.
    sources: Vec<String>,
    tests: Vec<TestsEntry>,
    /// Sources no test was found for.
    untested: Vec<String>,
}

pub(crate) fn run_tests_command(args: &[OsString]) -> Option<ExitCode> {
    let parsed = parse_tests_args(args)?;
    match tests_report(&parsed).and_then(|report| render_tests(&report, &parsed.format)) {
        Ok(()) => Some(ExitCode::SUCCESS),
        Err(err) => {
            eprintln!("Error: {err}");
            Some(ExitCode::from(1))
        }
    }
}

pub(crate) fn parse_tests_args(args: &[OsString]) -> Option<TestsArgs> {
    let mut parsed = TestsArgs {
        format: "text".to_string(),
        ..Default::default()
    };
    let mut json = false;
    let mut saw_command = false;
    let mut i = 0;
    while i < args.len() {
        let arg = &args[i];
        if !saw_command && arg == OsStr::new("tests") {
            saw_command = true;
        } else if arg == OsStr::new("--json") {
            json = true;
        } else if arg == OsStr::new("--changed") {
            parsed.changed = true;
        } else if let Some(value) = flag_value(arg, "--base") {
            parsed.base = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--base") {
            i += 1;
            parsed.base = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--format") {
            parsed.format = value.to_string_lossy().into_owned();
        } else if arg == OsStr::new("--format") {
            i += 1;
            parsed.format = args.get(i)?.to_string_lossy().into_owned();
        } else if let Some(value) = flag_value(arg, "--top") {
            parsed.top = value.to_string_lossy().parse().ok()?;
        } else if arg == OsStr::new("--top") {
            i += 1;
            parsed.top = args.get(i)?.to_string_lossy().parse().ok()?;
        } else if is_option(arg) || !saw_command {
            return None;
        } else {
            parsed.paths.push(arg.clone());
        }
        i += 1;
    }
    if json && parsed.format == "text" {
        parsed.format = "json".to_string();
    }
    if !parsed.base.is_empty() {
        parsed.changed = true;
    }
    Some(parsed)
}

fn tests_report(args: &TestsArgs) -> Result<TestsReport, String> {
    if !matches!(args.format.as_str(), "text" | "json" | "paths") {
        return Err(format!(
            "unknown --format value {:?} (allowed: text, json, paths)",
            args.format
        ));
    }
    if args.paths.is_empty() && !args.changed {
        return Err(
            "tests: name a file or directory, or pass --changed / --base <REV>".to_string(),
        );
    }
    if args.base.starts_with('-') {
        return Err(format!("tests: --base {:?} is not a revision", args.base));
    }
    let root = env::current_dir()
        .map_err(|err| format!("tests: cannot determine working directory: {err}"))?;
    let root_str = root.to_string_lossy().into_owned();
    let files: Vec<String> = ctx_relations::walk::walk(&root_str)
        .map_err(|err| format!("tests: {err}"))?
        .into_iter()
        .filter(|file| !file.is_dir && file.path != ".")
        .map(|file| file.path)
        .collect();

    // Seeds in the order given; directories expand to the files under them.
    let mut seeds: Vec<String> = Vec::new();
    let mut seen = BTreeSet::new();
    let mut push = |path: String, seeds: &mut Vec<String>| {
        if seen.insert(path.clone()) {
            seeds.push(path);
        }
    };
    // Test files inside a directory anchor are its tests, not sources.
    let mut inside: Vec<(String, String)> = Vec::new();
    for raw in &args.paths {
        let (rel, is_dir) =
            relation_anchor(&root, raw).map_err(|e| e.replacen("relations:", "tests:", 1))?;
        if is_dir {
            let prefix = if rel.is_empty() {
                String::new()
            } else {
                format!("{rel}/")
            };
            let label = if rel.is_empty() {
                ".".to_string()
            } else {
                format!("{rel}/")
            };
            for file in files.iter().filter(|file| file.starts_with(&prefix)) {
                if ctx_relations::is_test_path(file) {
                    inside.push((file.clone(), label.clone()));
                } else {
                    push(file.clone(), &mut seeds);
                }
            }
        } else {
            push(rel, &mut seeds);
        }
    }
    if args.changed {
        for path in changed_files(&root, &args.base)? {
            push(path, &mut seeds);
        }
    }

    let index = ctx_relations::build(&root_str).ok();
    let links = ctx_relations::find_tests(&files, index.as_ref(), &seeds);

    let mut tests = Vec::new();
    // Changed test files are tests to run, too.
    if args.changed {
        for seed in seeds
            .iter()
            .filter(|seed| ctx_relations::is_test_path(seed))
        {
            tests.push(TestsEntry {
                path: seed.clone(),
                link: "changed".to_string(),
                subject: String::new(),
                reason: "test: changed".to_string(),
            });
        }
    }
    let covered: BTreeSet<&str> = links
        .iter()
        .flat_map(|link| link.subjects.iter().map(String::as_str))
        .collect();
    let sources: Vec<String> = seeds
        .iter()
        .filter(|seed| !ctx_relations::is_test_path(seed) && is_source_file(seed))
        .cloned()
        .collect();
    let untested = sources
        .iter()
        .filter(|seed| !covered.contains(seed.as_str()))
        .cloned()
        .collect();
    tests.extend(links.iter().map(|link| {
        TestsEntry {
            path: link.path.clone(),
            link: serde_json::to_value(link.kind)
                .ok()
                .and_then(|v| v.as_str().map(str::to_string))
                .unwrap_or_default(),
            subject: link.subject.clone(),
            reason: link.reason(),
        }
    }));
    for (path, dir) in inside {
        if !tests.iter().any(|test| test.path == path) {
            tests.push(TestsEntry {
                reason: format!("test: inside {dir}"),
                path,
                link: "inside".to_string(),
                subject: String::new(),
            });
        }
    }
    if args.top > 0 {
        tests.truncate(args.top);
    }
    Ok(TestsReport {
        sources,
        tests,
        untested,
    })
}

/// Files a test could exercise: code, not docs or data.
fn is_source_file(path: &str) -> bool {
    ctx_relations::supported(path)
        || matches!(
            Path::new(path).extension().and_then(|e| e.to_str()),
            Some("rb" | "cs" | "scala" | "dart" | "ex" | "exs" | "c" | "cc" | "cpp" | "h" | "hpp")
        )
}

fn changed_files(root: &Path, base: &str) -> Result<Vec<String>, String> {
    if base.is_empty() {
        return Ok(git_changed_paths(root)?.into_iter().collect());
    }
    Ok(branch_changed_paths(root, base)
        .map_err(|err| err.replacen("pack: --base", "tests: --base", 1))?
        .into_iter()
        .filter(|(_, _, deleted)| !deleted)
        .map(|(path, _, _)| path)
        .collect())
}

fn render_tests(report: &TestsReport, format: &str) -> Result<(), String> {
    match format {
        "json" => {
            serde_json::to_writer_pretty(io::stdout(), report).map_err(|err| err.to_string())?;
            println!();
        }
        "paths" => {
            for test in &report.tests {
                println!("{}", test.path);
            }
        }
        _ => {
            if report.sources.is_empty() && report.tests.is_empty() {
                println!("No source files selected.");
                return Ok(());
            }
            if report.tests.is_empty() {
                println!(
                    "No tests found for {} source file(s).",
                    report.sources.len()
                );
            } else {
                match report.sources.len() {
                    0 => println!("Tests:"),
                    n => println!("Tests for {n} source file(s):"),
                }
                let width = report.tests.iter().map(|t| t.path.len()).max().unwrap_or(0);
                for test in &report.tests {
                    let why = test.reason.trim_start_matches("test: ");
                    println!("  - {:width$}  {why}", test.path);
                }
            }
            if !report.untested.is_empty() && !report.tests.is_empty() {
                println!("No test found for:");
                for path in &report.untested {
                    println!("  - {path}");
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(OsString::from).collect()
    }

    #[test]
    fn parses_paths_changed_base_and_format() {
        let parsed = parse_tests_args(&os(&["tests", "src/a.rs", "--format", "paths"])).unwrap();
        assert_eq!(parsed.paths, os(&["src/a.rs"]));
        assert_eq!(parsed.format, "paths");
        let parsed = parse_tests_args(&os(&["tests", "--base=main", "--json"])).unwrap();
        assert!(parsed.changed);
        assert_eq!(parsed.base, "main");
        assert_eq!(parsed.format, "json");
        assert!(parse_tests_args(&os(&["tests"])).unwrap().paths.is_empty());
        assert!(parse_tests_args(&os(&["tests", "--bogus", "a"])).is_none());
    }
}
