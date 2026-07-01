// crates/ctx-focus/src/expand.rs
//
// Port of internal/focus.Expand. Ordering:
//
//   1. anchor-origin     (1 file: the anchor itself)
//   2. same-dir          (other supported files in the same directory)
//   3. basename-prefix   (files whose stem == origin stem or stem startsWith origin_stem+"_")
//   4. name-match        (files containing the anchor name at identifier boundaries)
//
// hops=2 collects symbol names from the hop-1 result set and runs a
// second name-match round against those.

use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::{HashMap, HashSet};

use crate::resolve::{basename, dirname, stem, supported_ext};
use crate::types::{Anchor, AnchorKind, ExpandOptions, FileInfo, FileInput};

/// Pre-compiled cache of identifier-boundary regexes. We expect a single
/// expand() call to use one regex per query plus N regexes for hop2's
/// per-symbol scans; caching avoids rebuilding the same regex for queries
/// like a "Pack" symbol that shows up across hop2's many seeds.
static IDENTIFIER_RX_CACHE: Lazy<std::sync::Mutex<HashMap<String, Regex>>> =
    Lazy::new(|| std::sync::Mutex::new(HashMap::new()));

fn identifier_pattern(name: &str) -> Regex {
    let mut cache = IDENTIFIER_RX_CACHE.lock().expect("cache mutex");
    if let Some(re) = cache.get(name) {
        return re.clone();
    }
    let quoted = regex::escape(name);
    let pattern = format!(r"(?:^|[^A-Za-z0-9_]){}(?:[^A-Za-z0-9_]|$)", quoted);
    let re = Regex::new(&pattern).expect("identifier pattern always valid");
    cache.insert(name.to_string(), re.clone());
    re
}

/// expand runs the BFS expansion. Output is anchor-origin first, then
/// same-dir, basename-prefix, name-match — deduped by path (first wins).
pub fn expand(files: &[FileInput], anchor: &Anchor, opts: &ExpandOptions) -> Vec<FileInfo> {
    let hops = if opts.hops > 2 {
        2
    } else if opts.hops < 1 {
        1
    } else {
        opts.hops
    };

    let hop1 = expand_once(files, anchor);
    if hops >= 2 {
        expand_hop2(files, hop1)
    } else {
        hop1
    }
}

fn expand_once(files: &[FileInput], anchor: &Anchor) -> Vec<FileInfo> {
    let mut seen: HashSet<String> = HashSet::new();
    let mut out: Vec<FileInfo> = Vec::new();

    let add = |fi: FileInfo, seen: &mut HashSet<String>, out: &mut Vec<FileInfo>| {
        if seen.insert(fi.path.clone()) {
            out.push(fi);
        }
    };

    // Priority 1: anchor-origin
    add(
        FileInfo {
            path: anchor.origin_path.clone(),
            reason: "anchor-origin".into(),
            tokens: 0,
        },
        &mut seen,
        &mut out,
    );

    let origin_dir = dirname(&anchor.origin_path).to_string();
    let origin_base = basename(&anchor.origin_path).to_string();
    let origin_prefix = strip_ext(&origin_base).to_string();

    // Priority 2: same-dir
    for fi in files {
        if fi.is_dir || !supported_ext(&fi.path) {
            continue;
        }
        let rel = to_slash(&fi.path);
        if rel == anchor.origin_path {
            continue;
        }
        if dirname(&rel) == origin_dir {
            add(
                FileInfo {
                    path: rel,
                    reason: "same-dir".into(),
                    tokens: 0,
                },
                &mut seen,
                &mut out,
            );
        }
    }

    // Priority 3: basename-prefix
    for fi in files {
        if fi.is_dir || !supported_ext(&fi.path) {
            continue;
        }
        let rel = to_slash(&fi.path);
        let s = stem(&rel);
        let matches_prefix = s == origin_prefix || s.starts_with(&format!("{}_", origin_prefix));
        if matches_prefix {
            add(
                FileInfo {
                    path: rel,
                    reason: "basename-prefix".into(),
                    tokens: 0,
                },
                &mut seen,
                &mut out,
            );
        }
    }

    // Priority 4: name-match
    let anchor_name = anchor_name_for(anchor);
    let pattern = identifier_pattern(&anchor_name);
    for fi in files {
        if fi.is_dir || !supported_ext(&fi.path) {
            continue;
        }
        let rel = to_slash(&fi.path);
        if contains_identifier(&fi.lines, &pattern) {
            add(
                FileInfo {
                    path: rel,
                    reason: "name-match".into(),
                    tokens: 0,
                },
                &mut seen,
                &mut out,
            );
        }
    }

    out
}

fn expand_hop2(files: &[FileInput], hop1: Vec<FileInfo>) -> Vec<FileInfo> {
    // Index files by repo-relative path for the hop1 symbol pickup.
    let mut idx: HashMap<&str, &FileInput> = HashMap::with_capacity(files.len());
    for fi in files {
        idx.insert(fi.path.as_str(), fi);
    }

    let mut seen: HashSet<String> = HashSet::with_capacity(hop1.len() * 2);
    for fi in &hop1 {
        seen.insert(fi.path.clone());
    }

    // Collect symbol names from hop-1 files.
    let mut new_names: HashSet<String> = HashSet::new();
    for fi in &hop1 {
        if let Some(input) = idx.get(fi.path.as_str()) {
            for sym in &input.symbols {
                new_names.insert(sym.name.clone());
            }
        }
    }

    // Iteration order across HashSet is non-deterministic — to match
    // Go's map iteration semantics (also non-deterministic but the result
    // set is invariant under any iteration order because the inner loop
    // dedups against `seen`), we deterministically sort here so goldens
    // can compare exactly.
    let mut names_sorted: Vec<String> = new_names.into_iter().collect();
    names_sorted.sort();

    let mut out = hop1;
    for name in names_sorted {
        let pat = identifier_pattern(&name);
        for fi in files {
            if fi.is_dir || !supported_ext(&fi.path) {
                continue;
            }
            let rel = to_slash(&fi.path);
            if seen.contains(&rel) {
                continue;
            }
            if contains_identifier(&fi.lines, &pat) {
                seen.insert(rel.clone());
                out.push(FileInfo {
                    path: rel,
                    reason: "name-match".into(),
                    tokens: 0,
                });
            }
        }
    }
    out
}

fn anchor_name_for(anchor: &Anchor) -> String {
    match anchor.kind {
        AnchorKind::Symbol => anchor.name.clone(),
        _ => stem(&anchor.origin_path).to_string(),
    }
}

fn strip_ext(name: &str) -> &str {
    match name.rfind('.') {
        Some(i) if i > 0 => &name[..i],
        _ => name,
    }
}

fn to_slash(s: &str) -> String {
    s.replace('\\', "/")
}

fn contains_identifier(lines: &[String], pattern: &Regex) -> bool {
    for line in lines {
        if pattern.is_match(line) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::SymbolInfo;

    fn mkfile(path: &str, lines: Vec<&str>, syms: Vec<(&str, i64)>) -> FileInput {
        FileInput {
            path: path.into(),
            is_dir: false,
            symbols: syms
                .into_iter()
                .map(|(n, l)| SymbolInfo {
                    name: n.into(),
                    kind: "function".into(),
                    line: l,
                })
                .collect(),
            lines: lines.into_iter().map(String::from).collect(),
        }
    }

    fn anchor(origin: &str, name: &str, kind: AnchorKind) -> Anchor {
        Anchor {
            kind,
            raw: name.into(),
            name: name.into(),
            origin_path: origin.into(),
        }
    }

    #[test]
    fn expand_hops1_basic() {
        let files = vec![
            mkfile(
                "internal/pack/pack.go",
                vec!["package pack", "func Pack() {}"],
                vec![("Pack", 2)],
            ),
            mkfile(
                "internal/pack/pack_test.go",
                vec!["package pack", "func TestPack() {}"],
                vec![("TestPack", 2)],
            ),
            mkfile(
                "internal/pack/helper.go",
                vec!["package pack", "func helper() {}"],
                vec![("helper", 2)],
            ),
            mkfile(
                "internal/render/render.go",
                vec!["// uses Pack here"],
                vec![],
            ),
        ];
        let a = anchor("internal/pack/pack.go", "Pack", AnchorKind::Symbol);
        let out = expand(&files, &a, &ExpandOptions { hops: 1 });
        let paths: Vec<&str> = out.iter().map(|f| f.path.as_str()).collect();
        assert!(paths.contains(&"internal/pack/pack.go"));
        assert!(paths.contains(&"internal/pack/helper.go"));
        assert!(paths.contains(&"internal/pack/pack_test.go"));
        assert!(paths.contains(&"internal/render/render.go"));
        assert_eq!(out[0].reason, "anchor-origin");
    }

    #[test]
    fn expand_hops2_is_superset() {
        let files = vec![
            mkfile(
                "a/anchor.go",
                vec!["package a", "func Pack() {}"],
                vec![("Pack", 2)],
            ),
            mkfile(
                "a/sibling.go",
                vec!["package a", "func Helper() {}"],
                vec![("Helper", 2)],
            ),
            mkfile(
                "b/distant.go",
                vec!["// references Helper somewhere"],
                vec![],
            ),
        ];
        let a = anchor("a/anchor.go", "Pack", AnchorKind::Symbol);
        let h1 = expand(&files, &a, &ExpandOptions { hops: 1 });
        let h2 = expand(&files, &a, &ExpandOptions { hops: 2 });
        assert!(h2.len() >= h1.len());
        // b/distant.go should appear at hops=2 because Helper is a hop-1 file's symbol.
        assert!(h2.iter().any(|f| f.path == "b/distant.go"));
    }

    #[test]
    fn expand_hops_clamped() {
        let files = vec![mkfile("a/x.go", vec!["package a"], vec![])];
        let a = anchor("a/x.go", "x", AnchorKind::Basename);
        let r1 = expand(&files, &a, &ExpandOptions { hops: 0 });
        let r2 = expand(&files, &a, &ExpandOptions { hops: 99 });
        assert_eq!(r1.len(), 1);
        assert!(!r2.is_empty());
    }
}
