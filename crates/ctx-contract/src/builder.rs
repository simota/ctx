// crates/ctx-contract/src/builder.rs
//
// Port of internal/contract/build.go. Named `builder` rather than `build`
// to dodge Cargo's reserved name (`build.rs` is the package's optional
// build script). The Go file's identifiers (Build, BuildFromFixture …)
// remain as functions named `build`, `dedup_symbols`, etc.

use chrono::{SecondsFormat, Utc};
use once_cell::sync::Lazy;
use std::sync::Mutex;

use crate::hash::sha256_hex;
use crate::types::{Contract, File, FileInput, LineHash};
use crate::SCHEMA_VERSION;

/// Function pointer matching Go's `nowFn`. Returns an RFC3339 UTC
/// second-precision timestamp identical to Go's
/// `time.Now().UTC().Format(time.RFC3339)`.
pub type NowFn = fn() -> String;

fn real_now_rfc3339() -> String {
    Utc::now().to_rfc3339_opts(SecondsFormat::Secs, true)
}

static NOW_FN: Lazy<Mutex<NowFn>> = Lazy::new(|| Mutex::new(real_now_rfc3339 as NowFn));

/// Swaps the clock used by [`build`]. Mirrors `contract.SetNowFunc` in
/// build.go: passing `None` resets to the wall clock, and the previous
/// function pointer is returned so callers can restore it.
pub fn set_now_fn(new: Option<NowFn>) -> NowFn {
    let mut guard = NOW_FN.lock().expect("contract::NOW_FN poisoned");
    let prev = *guard;
    *guard = new.unwrap_or(real_now_rfc3339 as NowFn);
    prev
}

fn current_now() -> String {
    let guard = NOW_FN.lock().expect("contract::NOW_FN poisoned");
    (*guard)()
}

/// Returns the number of logical lines in `b`. Matches Go's
/// `lineCount`:
///
/// * empty input → 0
/// * trailing `\n` → count of `\n` bytes
/// * otherwise    → count of `\n` bytes + 1 (for the dangling line)
pub fn line_count(b: &[u8]) -> i32 {
    if b.is_empty() {
        return 0;
    }
    let mut n = 0i32;
    for &c in b {
        if c == b'\n' {
            n += 1;
        }
    }
    if *b.last().unwrap() != b'\n' {
        n += 1;
    }
    n
}

pub fn split_logical_lines(b: &[u8]) -> Vec<&[u8]> {
    if b.is_empty() {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut start = 0usize;
    for (i, byte) in b.iter().enumerate() {
        if *byte == b'\n' {
            lines.push(&b[start..=i]);
            start = i + 1;
        }
    }
    if start < b.len() {
        lines.push(&b[start..]);
    }
    lines
}

pub fn line_hashes(b: &[u8]) -> Vec<LineHash> {
    split_logical_lines(b)
        .into_iter()
        .enumerate()
        .map(|(i, line)| LineHash {
            line: i as i32 + 1,
            sha256: sha256_hex(line),
        })
        .collect()
}

/// Returns a deduplicated, sorted copy of `in_syms` with empty entries
/// removed. Matches Go's `dedupSymbols`: returns an empty Vec when no
/// non-empty symbols remain (Go returns `nil`, which we expose as an
/// empty Vec — the caller's `skip_serializing_if = "Vec::is_empty"` on
/// `File.symbols` collapses the wire shape identically).
pub fn dedup_symbols(in_syms: &[String]) -> Vec<String> {
    if in_syms.is_empty() {
        return Vec::new();
    }
    let mut seen = std::collections::HashSet::with_capacity(in_syms.len());
    let mut out = Vec::with_capacity(in_syms.len());
    for s in in_syms {
        if s.is_empty() {
            continue;
        }
        if seen.contains(s) {
            continue;
        }
        seen.insert(s.clone());
        out.push(s.clone());
    }
    out.sort();
    out
}

/// Build assembles a `Contract` from packed files. Created timestamp
/// is RFC3339 UTC, second-precision. Files are emitted in
/// path-sorted order. Empty-path inputs are skipped.
pub fn build(files: Vec<FileInput>) -> Contract {
    let mut c = Contract {
        schema_version: SCHEMA_VERSION,
        created: current_now(),
        files: Vec::with_capacity(files.len()),
    };
    for f in files {
        if f.path.is_empty() {
            continue;
        }
        c.files.push(File {
            path: f.path,
            line_start: 1,
            line_end: line_count(&f.content),
            sha256: sha256_hex(&f.content),
            line_hashes: line_hashes(&f.content),
            symbols: dedup_symbols(&f.symbols),
        });
    }
    c.files.sort_by(|a, b| a.path.cmp(&b.path));
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn line_count_empty() {
        assert_eq!(line_count(&[]), 0);
    }

    #[test]
    fn line_count_trailing_newline() {
        assert_eq!(line_count(b"a\nb\n"), 2);
    }

    #[test]
    fn line_count_no_trailing_newline() {
        assert_eq!(line_count(b"a\nb"), 2);
        assert_eq!(line_count(b"a"), 1);
    }

    #[test]
    fn line_hashes_include_trailing_newline_bytes() {
        let got = line_hashes(b"a\nb");
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].line, 1);
        assert_eq!(got[0].sha256, sha256_hex(b"a\n"));
        assert_eq!(got[1].sha256, sha256_hex(b"b"));
    }

    #[test]
    fn dedup_symbols_sorts_and_drops_empty() {
        let got = dedup_symbols(&["B".into(), "A".into(), "B".into(), "".into(), "C".into()]);
        assert_eq!(got, vec!["A", "B", "C"]);
    }

    #[test]
    fn build_skips_empty_path_and_sorts() {
        let prev = set_now_fn(Some(|| "2026-05-29T00:00:00Z".to_string()));
        let c = build(vec![
            FileInput {
                path: "b.go".into(),
                content: b"package b\n".to_vec(),
                symbols: vec!["B".into()],
            },
            FileInput {
                path: "".into(),
                content: b"skipped".to_vec(),
                symbols: vec![],
            },
            FileInput {
                path: "a.go".into(),
                content: b"package a\n".to_vec(),
                symbols: vec!["A".into(), "A".into()],
            },
        ]);
        assert_eq!(c.schema_version, SCHEMA_VERSION);
        assert_eq!(c.created, "2026-05-29T00:00:00Z");
        assert_eq!(c.files.len(), 2);
        assert_eq!(c.files[0].path, "a.go");
        assert_eq!(c.files[1].path, "b.go");
        assert_eq!(c.files[0].line_hashes.len(), 1);
        assert_eq!(c.files[0].symbols, vec!["A".to_string()]);
        set_now_fn(Some(prev));
    }
}
