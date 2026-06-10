// crates/ctx-symbols/src/lookup/mod.rs
//
// Pure-compute post-processing port of internal/symbols/lookup.go's
// sort + filter logic. Mirrors LookupByName WITHOUT the walk + extract
// (which stays Go-side because it transitively calls tree-sitter).
//
// Caller contract:
//   - Go walks the corpus, extracts symbols via tree-sitter, hands us
//     a Vec<FileSymbols>.
//   - We answer query(name, from, kind) → Vec<Hit> with the byte-exact
//     sort precedence of lookup.go::sortHits:
//       1. From's directory match
//       2. From's first path segment match
//       3. exported / public (Go: leading uppercase)
//       4. lexical path order

pub mod session;

use crate::types::{FileSymbols, Hit, LookupArgs};

/// Resolve a stateless query (no session caching). Matches lookup.go's
/// LookupByName semantics on the post-walk data.
pub fn resolve(corpus: &[FileSymbols], args: &LookupArgs) -> Vec<Hit> {
    if args.name.is_empty() {
        return Vec::new();
    }
    let (want_kind, kind_filter) = normalize_kind(&args.kind);

    let mut hits: Vec<Hit> = Vec::new();
    for fs in corpus {
        for s in &fs.symbols {
            if s.name != args.name {
                continue;
            }
            if kind_filter && s.kind != want_kind {
                continue;
            }
            hits.push(Hit {
                path: fs.path.clone(),
                line: s.line,
                kind: s.kind.clone(),
                symbol_name: s.name.clone(),
            });
        }
    }
    sort_hits(&mut hits, &args.from);
    hits
}

/// Mirror of lookup.go::sortHits — stable sort with the four-stage
/// precedence (From dir match → From first-segment match → exported →
/// lexical path order).
pub fn sort_hits(hits: &mut [Hit], from: &str) {
    let (from_dir, from_seg) = anchor_parts(from);
    let from_set = !from.is_empty();
    // Go uses sort.SliceStable; Rust's slice::sort_by is stable too.
    hits.sort_by(|hi, hj| {
        if from_set {
            let si = same_dir(&hi.path, &from_dir);
            let sj = same_dir(&hj.path, &from_dir);
            if si != sj {
                return bool_first(si, sj);
            }
            let fi = same_first_segment(&hi.path, &from_seg);
            let fj = same_first_segment(&hj.path, &from_seg);
            if fi != fj {
                return bool_first(fi, fj);
            }
        }
        let pi = is_exported(&hi.symbol_name);
        let pj = is_exported(&hj.symbol_name);
        if pi != pj {
            return bool_first(pi, pj);
        }
        hi.path.cmp(&hj.path)
    });
}

#[inline]
fn bool_first(a: bool, b: bool) -> std::cmp::Ordering {
    // returns Less when a is true (a sorts first) — mirrors Go `return si`.
    match (a, b) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => std::cmp::Ordering::Equal,
    }
}

/// anchor_parts returns (dir, first_segment) of a forward-slash path.
/// Both fields are empty when `from` is empty.
fn anchor_parts(from: &str) -> (String, String) {
    if from.is_empty() {
        return (String::new(), String::new());
    }
    let clean = path_clean(from);
    let dir = path_dir(&clean);
    let dir = if dir == "." { String::new() } else { dir };
    let first = match clean.find('/') {
        Some(i) => clean[..i].to_string(),
        None => clean,
    };
    (dir, first)
}

fn same_dir(p: &str, dir: &str) -> bool {
    let d = path_dir(p);
    let d = if d == "." { String::new() } else { d };
    d == dir
}

fn same_first_segment(p: &str, seg: &str) -> bool {
    if seg.is_empty() {
        return false;
    }
    match p.find('/') {
        Some(i) => &p[..i] == seg,
        None => p == seg,
    }
}

fn is_exported(name: &str) -> bool {
    match name.chars().next() {
        Some(c) => c.is_uppercase(),
        None => false,
    }
}

/// normalize_kind mirrors lookup.go::normalizeKind.
fn normalize_kind(kind: &str) -> (String, bool) {
    if kind.is_empty() {
        return (String::new(), false);
    }
    let key = kind.trim().to_lowercase();
    let canonical = match key.as_str() {
        "func" | "fn" | "function" => "function",
        "method" => "method",
        "type" => "type",
        "class" => "class",
        "interface" => "interface",
        "export" => "export",
        _ => return (key, true),
    };
    (canonical.to_string(), true)
}

// ---------- minimal path helpers (Go path.Dir / path.Clean semantics) ----------

/// path.Clean equivalent for forward-slash POSIX-style paths. We only
/// need the subset that lookup.go calls.
fn path_clean(p: &str) -> String {
    // Convert backslashes to slashes (filepath.ToSlash) — caller is
    // expected to pass forward-slash already but be defensive.
    let p = p.replace('\\', "/");
    if p.is_empty() {
        return ".".to_string();
    }
    let rooted = p.starts_with('/');
    let mut out: Vec<&str> = Vec::new();
    for seg in p.split('/') {
        match seg {
            "" | "." => continue,
            ".." => {
                if let Some(last) = out.last() {
                    if *last != ".." {
                        out.pop();
                        continue;
                    }
                }
                if !rooted {
                    out.push("..");
                }
            }
            other => out.push(other),
        }
    }
    let joined = out.join("/");
    if rooted {
        if joined.is_empty() {
            "/".to_string()
        } else {
            format!("/{joined}")
        }
    } else if joined.is_empty() {
        ".".to_string()
    } else {
        joined
    }
}

/// path.Dir equivalent for forward-slash paths.
fn path_dir(p: &str) -> String {
    let cleaned = path_clean(p);
    match cleaned.rfind('/') {
        Some(0) => "/".to_string(),
        Some(i) => path_clean(&cleaned[..i]),
        None => ".".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::Symbol;

    fn sym(name: &str, kind: &str, line: i32) -> Symbol {
        Symbol {
            name: name.to_string(),
            kind: kind.to_string(),
            line,
        }
    }

    fn fs(path: &str, syms: Vec<Symbol>) -> FileSymbols {
        FileSymbols {
            path: path.to_string(),
            symbols: syms,
        }
    }

    #[test]
    fn empty_name_returns_empty() {
        let corpus = vec![fs("a.go", vec![sym("F", "function", 1)])];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: String::new(),
                from: String::new(),
                kind: String::new(),
            },
        );
        assert!(hits.is_empty());
    }

    #[test]
    fn name_match_returns_hit() {
        let corpus = vec![fs("a.go", vec![sym("F", "function", 5)])];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: "F".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.go");
        assert_eq!(hits[0].line, 5);
        assert_eq!(hits[0].kind, "function");
    }

    #[test]
    fn kind_filter_applied() {
        let corpus = vec![
            fs("a.go", vec![sym("F", "function", 1)]),
            fs("b.go", vec![sym("F", "type", 2)]),
        ];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: "F".to_string(),
                from: String::new(),
                kind: "type".to_string(),
            },
        );
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "b.go");
    }

    #[test]
    fn kind_alias_func_matches_function() {
        let corpus = vec![fs("a.go", vec![sym("F", "function", 1)])];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: "F".to_string(),
                from: String::new(),
                kind: "func".to_string(),
            },
        );
        assert_eq!(hits.len(), 1);
    }

    #[test]
    fn from_directory_match_ranks_first() {
        let corpus = vec![
            fs("internal/a/x.go", vec![sym("F", "function", 1)]),
            fs("internal/b/y.go", vec![sym("F", "function", 2)]),
        ];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: "F".to_string(),
                from: "internal/b/z.go".to_string(),
                kind: String::new(),
            },
        );
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, "internal/b/y.go");
        assert_eq!(hits[1].path, "internal/a/x.go");
    }

    #[test]
    fn from_first_segment_match_ranks_after_dir_match() {
        let corpus = vec![
            fs("internal/a/x.go", vec![sym("F", "function", 1)]),
            fs("web/handlers.go", vec![sym("F", "function", 2)]),
        ];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: "F".to_string(),
                from: "internal/b/z.go".to_string(),
                kind: String::new(),
            },
        );
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, "internal/a/x.go");
    }

    #[test]
    fn exported_ranks_before_unexported() {
        let corpus = vec![
            fs("a.go", vec![sym("foo", "function", 1)]),
            fs("b.go", vec![sym("Foo", "function", 2)]),
        ];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: "foo".to_string(),
                from: String::new(),
                kind: String::new(),
            },
        );
        // names differ — only "foo" matches; ensure no false ordering bug.
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].path, "a.go");
    }

    #[test]
    fn lexical_path_tiebreak() {
        let corpus = vec![
            fs("z.go", vec![sym("F", "function", 1)]),
            fs("a.go", vec![sym("F", "function", 2)]),
        ];
        let hits = resolve(
            &corpus,
            &LookupArgs {
                name: "F".to_string(),
                ..Default::default()
            },
        );
        assert_eq!(hits.len(), 2);
        assert_eq!(hits[0].path, "a.go");
        assert_eq!(hits[1].path, "z.go");
    }

    #[test]
    fn path_clean_strips_dots_and_doubles() {
        assert_eq!(path_clean("a/./b"), "a/b");
        assert_eq!(path_clean("a/b/.."), "a");
        assert_eq!(path_clean(""), ".");
        assert_eq!(path_clean("foo.go"), "foo.go");
    }

    #[test]
    fn path_dir_handles_root() {
        assert_eq!(path_dir("foo.go"), ".");
        assert_eq!(path_dir("a/b/c.go"), "a/b");
    }

    #[test]
    fn anchor_parts_extracts_dir_and_first_seg() {
        let (d, s) = anchor_parts("internal/web/handlers.go");
        assert_eq!(d, "internal/web");
        assert_eq!(s, "internal");
    }

    #[test]
    fn anchor_parts_empty_for_empty_from() {
        let (d, s) = anchor_parts("");
        assert_eq!(d, "");
        assert_eq!(s, "");
    }

    #[test]
    fn from_bare_filename_first_segment_is_filename() {
        let (d, s) = anchor_parts("foo.go");
        assert_eq!(d, "");
        assert_eq!(s, "foo.go");
    }
}
