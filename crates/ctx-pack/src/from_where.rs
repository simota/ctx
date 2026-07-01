// crates/ctx-pack/src/from_where.rs
//
// Port of internal/pack/from_where.go. The Go ParseFromWhere reader
// auto-detects between two formats:
//
//   1. JSON array — top-level []WhereResult, sorted descending by
//      Score (stable). An empty array is an error.
//   2. Newline-delimited paths — one per line, blank + '#' prefix
//      skipped, dedup preserving first-seen order.
//
// Both formats funnel through cleanInputPath:
//   * trim whitespace, strip surrounding double quotes
//   * empty or "/dev/null" → drop
//   * filepath.Clean(filepath.ToSlash(p))
//   * "." → drop

use crate::types::WhereResult;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FromWhereError {
    Empty,
    BadJson,
}

impl std::fmt::Display for FromWhereError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FromWhereError::Empty => {
                write!(f, "--from-where: requires non-empty stdin (got 0 paths)")
            }
            FromWhereError::BadJson => write!(f, "--from-where: JSON parse failed"),
        }
    }
}

impl std::error::Error for FromWhereError {}

pub fn parse(data: &[u8]) -> Result<Vec<String>, FromWhereError> {
    // Skip leading whitespace to detect format.
    let mut first_non_ws: Option<u8> = None;
    for b in data {
        match *b {
            b' ' | b'\t' | b'\r' | b'\n' => continue,
            other => {
                first_non_ws = Some(other);
                break;
            }
        }
    }
    let Some(first) = first_non_ws else {
        return Err(FromWhereError::Empty);
    };
    if first == b'[' {
        parse_json(data)
    } else {
        parse_lines(data)
    }
}

fn parse_json(data: &[u8]) -> Result<Vec<String>, FromWhereError> {
    let mut results: Vec<WhereResult> = match serde_json::from_slice(data) {
        Ok(v) => v,
        Err(_) => return Err(FromWhereError::BadJson),
    };
    if results.is_empty() {
        return Err(FromWhereError::Empty);
    }
    // Stable sort descending by score — matches sort.SliceStable.
    results.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    let mut paths: Vec<String> = Vec::with_capacity(results.len());
    for r in results {
        let p = clean_input_path(&r.path);
        if !p.is_empty() {
            paths.push(p);
        }
    }
    if paths.is_empty() {
        return Err(FromWhereError::Empty);
    }
    Ok(paths)
}

fn parse_lines(data: &[u8]) -> Result<Vec<String>, FromWhereError> {
    let s = match std::str::from_utf8(data) {
        Ok(s) => s,
        Err(_) => return Err(FromWhereError::Empty),
    };
    let mut seen = std::collections::HashSet::new();
    let mut paths: Vec<String> = Vec::new();
    for line in s.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('#') {
            continue;
        }
        let p = clean_input_path(line);
        if p.is_empty() {
            continue;
        }
        if seen.contains(&p) {
            continue;
        }
        seen.insert(p.clone());
        paths.push(p);
    }
    if paths.is_empty() {
        return Err(FromWhereError::Empty);
    }
    Ok(paths)
}

/// Mirror of internal/pack/stdin.go::cleanInputPath:
///   1. trim spaces, strip surrounding double quotes
///   2. drop "" or "/dev/null"
///   3. filepath.ToSlash(filepath.Clean(p))
///   4. drop "."
pub fn clean_input_path(raw: &str) -> String {
    let trimmed = raw.trim();
    let trimmed = trimmed.trim_matches('"');
    if trimmed.is_empty() || trimmed == "/dev/null" {
        return String::new();
    }
    let cleaned = filepath_clean(trimmed);
    let cleaned = cleaned.replace('\\', "/");
    if cleaned == "." {
        return String::new();
    }
    cleaned
}

/// Minimal port of Go's path/filepath.Clean for the subset we need
/// (forward-slash + simple dot/double-dot resolution). Matches the
/// "FromSlash → Clean → ToSlash" path the Go code takes.
fn filepath_clean(input: &str) -> String {
    let p = input.replace('\\', "/");
    if p.is_empty() {
        return ".".to_string();
    }
    let rooted = p.starts_with('/');
    let mut parts: Vec<&str> = Vec::new();
    for segment in p.split('/') {
        if segment.is_empty() || segment == "." {
            continue;
        }
        if segment == ".." {
            if let Some(last) = parts.last() {
                if *last != ".." {
                    parts.pop();
                    continue;
                }
            }
            if rooted {
                continue;
            }
            parts.push("..");
            continue;
        }
        parts.push(segment);
    }
    let body = parts.join("/");
    if rooted {
        if body.is_empty() {
            "/".to_string()
        } else {
            format!("/{body}")
        }
    } else if body.is_empty() {
        ".".to_string()
    } else {
        body
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_stdin_errors() {
        let r = parse(b"");
        assert!(matches!(r, Err(FromWhereError::Empty)));
    }

    #[test]
    fn json_array_sorted_by_score_desc() {
        let body = br#"[{"path":"b.go","score":0.5},{"path":"a.go","score":0.9}]"#;
        let r = parse(body).unwrap();
        assert_eq!(r, vec!["a.go".to_string(), "b.go".to_string()]);
    }

    #[test]
    fn newline_paths_dedup_preserve_order() {
        let body = b"a.go\nb.go\na.go\n# comment\nc.go\n";
        let r = parse(body).unwrap();
        assert_eq!(r, vec!["a.go", "b.go", "c.go"]);
    }

    #[test]
    fn drops_dev_null_and_dots() {
        let body = b"/dev/null\n.\nx.go\n";
        let r = parse(body).unwrap();
        assert_eq!(r, vec!["x.go".to_string()]);
    }

    #[test]
    fn clean_input_path_strips_quotes() {
        assert_eq!(clean_input_path("\"foo.go\""), "foo.go");
    }

    #[test]
    fn json_only_dev_null_errors() {
        let body = br#"[{"path":"/dev/null","score":0.5}]"#;
        let r = parse(body);
        assert!(matches!(r, Err(FromWhereError::Empty)));
    }

    #[test]
    fn json_parse_failure() {
        let r = parse(b"[ not json ");
        assert!(matches!(r, Err(FromWhereError::BadJson)));
    }
}
