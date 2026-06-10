// crates/ctx-braid/src/merge.rs
//
// Port of internal/braid/policy.go's MergePaths logic. The PolicyKind
// enum + helpers live in policy.rs and types.rs; this file is the
// algorithm.

use std::collections::HashMap;

use crate::types::{MergedFile, PolicyKind, StrandSelection};

/// MergePaths applies each strand's policy in declaration order to
/// produce a final ordered list of (path, origin-strand) tuples.
///
/// Mirrors Go's `MergePaths` 1:1, including the two-pass shape (per-
/// strand dedup, then cross-strand policy resolution). Paths may include
/// optional line ranges (`file.go:10` or `file.go:10-20`); overlap
/// policies compare those ranges instead of treating every repeated file
/// as identical.
pub fn merge_paths(selections: &[StrandSelection]) -> Vec<MergedFile> {
    #[derive(Clone)]
    struct SelectedPath {
        raw: String,
        key: String,
        spec: PathRange,
    }
    struct CleanedSelection {
        name: String,
        policy: PolicyKind,
        paths: Vec<SelectedPath>,
    }

    // First pass: per-strand dedup so each strand contributes each exact
    // path/range at most once.
    let mut cleaned: Vec<CleanedSelection> = Vec::with_capacity(selections.len());
    for sel in selections {
        let mut seen: std::collections::HashSet<String> =
            std::collections::HashSet::with_capacity(sel.paths.len());
        let mut out: Vec<SelectedPath> = Vec::with_capacity(sel.paths.len());
        for p in &sel.paths {
            if p.is_empty() {
                continue;
            }
            let spec = parse_path_range(p);
            let key = spec.key();
            if !seen.insert(key.clone()) {
                continue;
            }
            out.push(SelectedPath {
                raw: p.clone(),
                key,
                spec,
            });
        }
        cleaned.push(CleanedSelection {
            name: sel.name.clone(),
            policy: sel.policy,
            paths: out,
        });
    }

    // Second pass: per-path occurrence index, then resolve later-vs-
    // earlier per the later strand's policy.
    #[derive(Clone)]
    struct Occ<'a> {
        strand_idx: usize,
        key: &'a str,
        spec: PathRange,
    }
    let mut occ_by_path: HashMap<String, Vec<Occ<'_>>> = HashMap::new();
    for (i, sel) in cleaned.iter().enumerate() {
        for p in &sel.paths {
            occ_by_path
                .entry(p.spec.path.clone())
                .or_default()
                .push(Occ {
                    strand_idx: i,
                    key: &p.key,
                    spec: p.spec.clone(),
                });
        }
    }

    // Decide which (strand, path) pairs survive.
    let mut keep: Vec<std::collections::HashSet<String>> = cleaned
        .iter()
        .map(|sel| sel.paths.iter().map(|p| p.key.clone()).collect())
        .collect();

    for occs in occ_by_path.values() {
        if occs.len() < 2 {
            continue;
        }
        for j in 1..occs.len() {
            let later = occs[j].clone();
            let later_policy = cleaned[later.strand_idx].policy;
            match later_policy {
                PolicyKind::PreferNewer => {
                    for k in 0..j {
                        if ranges_overlap(&later.spec, &occs[k].spec) {
                            keep[occs[k].strand_idx].remove(occs[k].key);
                        }
                    }
                }
                PolicyKind::ExcludeOverlap => {
                    for earlier in occs.iter().take(j) {
                        if ranges_overlap(&later.spec, &earlier.spec) {
                            keep[later.strand_idx].remove(later.key);
                            break;
                        }
                    }
                }
                PolicyKind::Merge => {
                    // no-op
                }
            }
        }
    }

    let mut merged: Vec<MergedFile> = Vec::new();
    for (i, sel) in cleaned.iter().enumerate() {
        for p in &sel.paths {
            if !keep[i].contains(&p.key) {
                continue;
            }
            merged.push(MergedFile {
                path: p.raw.clone(),
                origin: sel.name.clone(),
            });
        }
    }
    merged
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct PathRange {
    path: String,
    start: i64,
    end: i64,
}

const WHOLE_FILE_END: i64 = i64::MAX;

fn parse_path_range(raw: &str) -> PathRange {
    let Some(colon_idx) = raw.rfind(':') else {
        return PathRange {
            path: raw.to_string(),
            start: 1,
            end: WHOLE_FILE_END,
        };
    };
    let (path, suffix_with_colon) = raw.split_at(colon_idx);
    let suffix = &suffix_with_colon[1..];
    if path.is_empty() || suffix.is_empty() {
        return PathRange {
            path: raw.to_string(),
            start: 1,
            end: WHOLE_FILE_END,
        };
    }
    let (start_s, end_s) = match suffix.split_once('-') {
        Some((start, end)) => (start, Some(end)),
        None => (suffix, None),
    };
    let Ok(mut start) = start_s.parse::<i64>() else {
        return PathRange {
            path: raw.to_string(),
            start: 1,
            end: WHOLE_FILE_END,
        };
    };
    if start <= 0 {
        return PathRange {
            path: raw.to_string(),
            start: 1,
            end: WHOLE_FILE_END,
        };
    }
    let mut end = start;
    if let Some(end_s) = end_s {
        let Ok(parsed_end) = end_s.parse::<i64>() else {
            return PathRange {
                path: raw.to_string(),
                start: 1,
                end: WHOLE_FILE_END,
            };
        };
        if parsed_end <= 0 {
            return PathRange {
                path: raw.to_string(),
                start: 1,
                end: WHOLE_FILE_END,
            };
        }
        end = parsed_end;
    }
    if end < start {
        std::mem::swap(&mut start, &mut end);
    }
    PathRange {
        path: path.to_string(),
        start,
        end,
    }
}

impl PathRange {
    fn key(&self) -> String {
        format!("{}:{}-{}", self.path, self.start, self.end)
    }
}

fn ranges_overlap(a: &PathRange, b: &PathRange) -> bool {
    a.path == b.path && a.start <= b.end && b.start <= a.end
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sel(name: &str, policy: PolicyKind, paths: &[&str]) -> StrandSelection {
        StrandSelection {
            name: name.into(),
            policy,
            paths: paths.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn merge_keeps_duplicates_across_strands() {
        let got = merge_paths(&[
            sel("a", PolicyKind::Merge, &["x.go", "y.go"]),
            sel("b", PolicyKind::Merge, &["y.go", "z.go"]),
        ]);
        assert_eq!(got.len(), 4);
        assert_eq!(got[0].path, "x.go");
        assert_eq!(got[0].origin, "a");
        assert_eq!(got[1].path, "y.go");
        assert_eq!(got[1].origin, "a");
        assert_eq!(got[2].path, "y.go");
        assert_eq!(got[2].origin, "b");
        assert_eq!(got[3].path, "z.go");
        assert_eq!(got[3].origin, "b");
    }

    #[test]
    fn prefer_newer_evicts_earlier() {
        let got = merge_paths(&[
            sel("a", PolicyKind::Merge, &["x.go", "y.go"]),
            sel("b", PolicyKind::PreferNewer, &["y.go", "z.go"]),
        ]);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].path, "x.go");
        assert_eq!(got[0].origin, "a");
        assert_eq!(got[1].path, "y.go");
        assert_eq!(got[1].origin, "b");
        assert_eq!(got[2].path, "z.go");
    }

    #[test]
    fn exclude_overlap_drops_later() {
        let got = merge_paths(&[
            sel("a", PolicyKind::Merge, &["x.go", "y.go"]),
            sel("b", PolicyKind::ExcludeOverlap, &["y.go", "z.go"]),
        ]);
        assert_eq!(got.len(), 3);
        assert_eq!(got[0].path, "x.go");
        assert_eq!(got[1].path, "y.go");
        assert_eq!(got[1].origin, "a");
        assert_eq!(got[2].path, "z.go");
    }

    #[test]
    fn per_strand_dedup_within_a_strand() {
        let got = merge_paths(&[sel("a", PolicyKind::Merge, &["x.go", "x.go", "y.go"])]);
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].path, "x.go");
        assert_eq!(got[1].path, "y.go");
    }

    #[test]
    fn empty_paths_are_skipped() {
        let got = merge_paths(&[sel("a", PolicyKind::Merge, &["", "x.go", ""])]);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].path, "x.go");
    }

    #[test]
    fn line_ranges_allow_non_overlapping_selections() {
        let got = merge_paths(&[
            sel("a", PolicyKind::Merge, &["x.go:1-10"]),
            sel("b", PolicyKind::ExcludeOverlap, &["x.go:11-20", "x.go:5-6"]),
        ]);
        assert_eq!(
            got,
            vec![
                MergedFile {
                    path: "x.go:1-10".into(),
                    origin: "a".into()
                },
                MergedFile {
                    path: "x.go:11-20".into(),
                    origin: "b".into()
                },
            ]
        );
    }

    #[test]
    fn line_range_prefer_newer_only_evicts_overlaps() {
        let got = merge_paths(&[
            sel("a", PolicyKind::Merge, &["x.go:1-10", "x.go:20-30"]),
            sel("b", PolicyKind::PreferNewer, &["x.go:5-15"]),
        ]);
        assert_eq!(
            got,
            vec![
                MergedFile {
                    path: "x.go:20-30".into(),
                    origin: "a".into()
                },
                MergedFile {
                    path: "x.go:5-15".into(),
                    origin: "b".into()
                },
            ]
        );
    }
}
