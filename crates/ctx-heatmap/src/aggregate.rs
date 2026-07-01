// crates/ctx-heatmap/src/aggregate.rs
//
// Rust port of internal/heatmap/heatmap.go aggregation, TopN, and
// formatting helpers. Behaviour is byte-exact with the Go side:
//
//  - truncate_path: depth=0 collapses to "."; root files fold to "."
//  - aggregate: drops zero-weight buckets, sort by Weight desc + Path asc
//  - format_number: thousand-separator helper used by all renderers

use std::collections::BTreeMap;

use crate::types::{AggregateOptions, Bucket, FileMetric};

/// Aggregate collapses a flat list of FileMetric into one Bucket per
/// directory truncated to `depth` segments.
pub fn aggregate(files: &[FileMetric], opts: &AggregateOptions) -> Vec<Bucket> {
    let depth = if opts.depth < 0 { 0 } else { opts.depth };

    // BTreeMap so traversal order is stable; the final sort still drives
    // tie-break order, but starting from a stable insertion order makes
    // the per-bucket accumulator deterministic.
    let mut groups: BTreeMap<String, (i64, i64, i64, i64)> = BTreeMap::new();

    for fi in files {
        if fi.is_dir {
            continue;
        }
        let key = truncate_path(&fi.path, depth);
        let entry = groups.entry(key).or_insert((0, 0, 0, 0));
        entry.0 += fi.tokens;
        entry.1 += 1;
        entry.2 += fi.symbols;
        entry.3 += fi.churn;
    }

    let mut buckets: Vec<Bucket> = Vec::with_capacity(groups.len());
    for (path, (tokens, files_cnt, symbols, churn)) in groups.into_iter() {
        let weight = weight_for(&opts.by, tokens, files_cnt, symbols, churn);
        if weight <= 0.0 {
            continue;
        }
        buckets.push(Bucket {
            path,
            tokens,
            files: files_cnt,
            symbols,
            churn,
            weight,
        });
    }

    // sort.SliceStable: weight desc, path asc as tiebreaker.
    buckets.sort_by(|a, b| {
        if a.weight != b.weight {
            // Descending weight.
            b.weight
                .partial_cmp(&a.weight)
                .unwrap_or(std::cmp::Ordering::Equal)
        } else {
            a.path.cmp(&b.path)
        }
    });

    buckets
}

/// TopN returns the first n buckets, or all of them when n <= 0 or n
/// exceeds len(buckets). The slice is left untouched (Aggregate already
/// sorted).
pub fn top_n(buckets: Vec<Bucket>, n: i64) -> Vec<Bucket> {
    if n <= 0 || (n as usize) >= buckets.len() {
        return buckets;
    }
    let mut out = buckets;
    out.truncate(n as usize);
    out
}

/// Total sums the Weight across all buckets.
pub fn total(buckets: &[Bucket]) -> f64 {
    buckets.iter().map(|b| b.weight).sum()
}

/// TotalTokens sums the Tokens field across all buckets.
pub fn total_tokens(buckets: &[Bucket]) -> i64 {
    buckets.iter().map(|b| b.tokens).sum()
}

/// truncate_path collapses a file path to its first `depth` directory
/// segments. depth=0 collapses everything to "."; files at the root
/// (no directory component) also fold to ".".
pub fn truncate_path(path: &str, depth: i64) -> String {
    if depth == 0 {
        return ".".to_string();
    }
    // Mirror filepath.Dir + filepath.ToSlash. The Go side already
    // normalised to forward slashes via filepath.ToSlash before calling
    // truncatePath, so we work on the raw `/`-separated form.
    let dir = parent_dir(path);
    if dir.is_empty() || dir == "." {
        return ".".to_string();
    }
    let mut parts: Vec<&str> = dir.split('/').collect();
    if parts.len() > depth as usize {
        parts.truncate(depth as usize);
    }
    parts.join("/")
}

/// parent_dir mimics Go filepath.Dir behaviour on a forward-slash path:
///  - "a/b/c.go" -> "a/b"
///  - "main.go"  -> "."
///  - "a/b/"     -> "a/b" (trailing slash dropped)
fn parent_dir(path: &str) -> String {
    // Strip trailing slashes (path/filepath.Dir trims them before splitting).
    let trimmed = path.trim_end_matches('/');
    match trimmed.rfind('/') {
        Some(idx) => trimmed[..idx].to_string(),
        None => ".".to_string(),
    }
}

/// weight_for maps the user-facing `--by` axis to the numeric weight
/// that drives squarify. Unknown axes degrade to tokens (matches Go).
pub fn weight_for(by: &str, tokens: i64, files: i64, symbols: i64, churn: i64) -> f64 {
    match by {
        "files" => files as f64,
        "symbols" => symbols as f64,
        "churn" => churn as f64,
        // "tokens", "", or anything else.
        _ => tokens as f64,
    }
}

/// format_number inserts thousand separators ("8420" -> "8,420").
/// Lives in the aggregate module because every renderer needs it.
pub fn format_number(n: i64) -> String {
    if n < 0 {
        return format!("-{}", format_number(-n));
    }
    let s = n.to_string();
    if s.len() <= 3 {
        return s;
    }
    let bytes = s.as_bytes();
    let mut parts: Vec<&str> = Vec::with_capacity((s.len() + 2) / 3);
    let mut end = bytes.len();
    while end > 3 {
        let start = end - 3;
        parts.push(std::str::from_utf8(&bytes[start..end]).unwrap());
        end = start;
    }
    parts.push(std::str::from_utf8(&bytes[..end]).unwrap());
    parts.reverse();
    parts.join(",")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metric(path: &str, tokens: i64, symbols: i64) -> FileMetric {
        FileMetric {
            path: path.into(),
            is_dir: false,
            tokens,
            symbols,
            churn: 0,
        }
    }

    fn metric_churn(path: &str, churn: i64) -> FileMetric {
        FileMetric {
            path: path.into(),
            is_dir: false,
            tokens: 0,
            symbols: 0,
            churn,
        }
    }

    #[test]
    fn aggregate_churn_axis_sizes_by_commit_count() {
        let files = vec![
            metric_churn("internal/hot/a.go", 30),
            metric_churn("internal/hot/b.go", 12),
            metric_churn("internal/cold/x.go", 1),
        ];
        let buckets = aggregate(
            &files,
            &AggregateOptions {
                by: "churn".into(),
                depth: 2,
                top: 0,
            },
        );
        assert_eq!(buckets[0].path, "internal/hot");
        assert_eq!(buckets[0].churn, 42);
        assert_eq!(buckets[0].weight, 42.0);
        // cold dir survives (churn > 0) but ranks last.
        assert_eq!(buckets.last().unwrap().path, "internal/cold");
    }

    #[test]
    fn aggregate_tokens_by_depth_2() {
        let files = vec![
            metric("internal/cli/root.go", 1000, 10),
            metric("internal/cli/where.go", 500, 5),
            metric("internal/walk/walk.go", 300, 3),
            metric("cmd/ctx/main.go", 100, 1),
            metric("README.md", 200, 0),
        ];
        let buckets = aggregate(
            &files,
            &AggregateOptions {
                by: "tokens".into(),
                depth: 2,
                top: 0,
            },
        );
        let by_path: std::collections::HashMap<&str, i64> = buckets
            .iter()
            .map(|b| (b.path.as_str(), b.tokens))
            .collect();
        assert_eq!(by_path.get("internal/cli"), Some(&1500));
        assert_eq!(by_path.get("internal/walk"), Some(&300));
        assert_eq!(by_path.get("cmd/ctx"), Some(&100));
        assert_eq!(by_path.get("."), Some(&200));
        // sorted desc
        for w in buckets.windows(2) {
            assert!(w[0].weight >= w[1].weight);
        }
    }

    #[test]
    fn aggregate_depth_zero_collapses_to_root() {
        let files = vec![metric("a/b/c.go", 100, 1), metric("d/e.go", 50, 0)];
        let buckets = aggregate(
            &files,
            &AggregateOptions {
                by: "tokens".into(),
                depth: 0,
                top: 0,
            },
        );
        assert_eq!(buckets.len(), 1);
        assert_eq!(buckets[0].path, ".");
        assert_eq!(buckets[0].tokens, 150);
    }

    #[test]
    fn aggregate_files_axis_picks_widest_dir() {
        let files = vec![
            metric("internal/big/a.go", 1, 100),
            metric("internal/big/b.go", 1, 100),
            metric("internal/wide/x.go", 100, 1),
            metric("internal/wide/y.go", 100, 1),
            metric("internal/wide/z.go", 100, 1),
        ];
        let by_files = aggregate(
            &files,
            &AggregateOptions {
                by: "files".into(),
                depth: 2,
                top: 0,
            },
        );
        assert_eq!(by_files[0].path, "internal/wide");
        let by_syms = aggregate(
            &files,
            &AggregateOptions {
                by: "symbols".into(),
                depth: 2,
                top: 0,
            },
        );
        assert_eq!(by_syms[0].path, "internal/big");
    }

    #[test]
    fn aggregate_drops_zero_weight() {
        let files = vec![
            metric("internal/binary/blob.png", 0, 0),
            metric("internal/code/foo.go", 100, 1),
        ];
        let buckets = aggregate(
            &files,
            &AggregateOptions {
                by: "tokens".into(),
                depth: 2,
                top: 0,
            },
        );
        assert!(buckets.iter().all(|b| b.path != "internal/binary"));
    }

    #[test]
    fn top_n_bounds() {
        let buckets = vec![
            Bucket {
                path: "a".into(),
                weight: 10.0,
                ..Default::default()
            },
            Bucket {
                path: "b".into(),
                weight: 5.0,
                ..Default::default()
            },
            Bucket {
                path: "c".into(),
                weight: 1.0,
                ..Default::default()
            },
        ];
        let t = top_n(buckets.clone(), 2);
        assert_eq!(t.len(), 2);
        assert_eq!(t[0].path, "a");
        assert_eq!(t[1].path, "b");
        let t0 = top_n(buckets.clone(), 0);
        assert_eq!(t0.len(), 3);
        let tbig = top_n(buckets, 99);
        assert_eq!(tbig.len(), 3);
    }

    #[test]
    fn format_number_thousand_separator() {
        assert_eq!(format_number(0), "0");
        assert_eq!(format_number(999), "999");
        assert_eq!(format_number(1_000), "1,000");
        assert_eq!(format_number(8420), "8,420");
        assert_eq!(format_number(11_310), "11,310");
        assert_eq!(format_number(92_340), "92,340");
        assert_eq!(format_number(-8420), "-8,420");
    }
}
