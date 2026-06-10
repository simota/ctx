// crates/ctx-replay/src/diff.rs
//
// Port of internal/replay/diff.go — Compute + ComputeSelectionDiff.

use std::collections::{BTreeMap, HashMap};

use crate::types::{
    ChangeKind, DiffSummary, Entry, FileChange, Manifest, SelectionCategory, SelectionChange,
    SelectionSummary,
};

#[derive(Debug, Clone, Copy, Default)]
pub struct DiffOptions {
    /// Strict, when true, surfaces token-only deltas as modified even when
    /// the SHA-256 matches.
    pub strict: bool,
}

/// Mirrors `replay.Compute`.
pub fn compute(base: &Manifest, current: &Manifest, opts: DiffOptions) -> DiffSummary {
    let base_by_path = index_entries(&base.entries);
    let cur_by_path = index_entries(&current.entries);

    // Collect union of paths in deterministic sorted order (BTreeMap).
    let mut paths: BTreeMap<&str, ()> = BTreeMap::new();
    for k in base_by_path.keys() {
        paths.insert(k.as_str(), ());
    }
    for k in cur_by_path.keys() {
        paths.insert(k.as_str(), ());
    }

    let mut summary = DiffSummary::default();
    for path in paths.keys() {
        let b = base_by_path.get(*path);
        let c = cur_by_path.get(*path);
        match (b, c) {
            (Some(b), Some(c)) => {
                let mut change = FileChange {
                    path: (*path).to_string(),
                    kind: ChangeKind::Unchanged,
                    base_sha256: b.sha256.clone(),
                    cur_sha256: c.sha256.clone(),
                    base_tokens: b.tokens,
                    cur_tokens: c.tokens,
                    token_delta: c.tokens - b.tokens,
                    reason: String::new(),
                };
                if b.sha256 == c.sha256 {
                    if opts.strict && b.tokens != c.tokens {
                        change.kind = ChangeKind::Modified;
                        change.reason = "token-only delta in strict mode".into();
                        summary.modified += 1;
                        summary.token_delta += change.token_delta;
                    } else {
                        change.kind = ChangeKind::Unchanged;
                        change.token_delta = 0;
                        summary.unchanged += 1;
                    }
                } else {
                    change.kind = ChangeKind::Modified;
                    summary.modified += 1;
                    summary.token_delta += change.token_delta;
                }
                summary.changes.push(change);
            }
            (Some(b), None) => {
                summary.removed += 1;
                summary.token_delta -= b.tokens;
                summary.changes.push(FileChange {
                    path: (*path).to_string(),
                    kind: ChangeKind::Removed,
                    base_sha256: b.sha256.clone(),
                    base_tokens: b.tokens,
                    token_delta: -b.tokens,
                    ..Default::default()
                });
            }
            (None, Some(c)) => {
                summary.added += 1;
                summary.token_delta += c.tokens;
                summary.changes.push(FileChange {
                    path: (*path).to_string(),
                    kind: ChangeKind::Added,
                    cur_sha256: c.sha256.clone(),
                    cur_tokens: c.tokens,
                    token_delta: c.tokens,
                    ..Default::default()
                });
            }
            (None, None) => unreachable!(),
        }
    }
    summary
}

fn index_entries(entries: &[Entry]) -> HashMap<String, &Entry> {
    let mut out = HashMap::with_capacity(entries.len());
    for e in entries {
        out.insert(e.path.clone(), e);
    }
    out
}

// ---------------------------------------------------------------------
// Selection diff
// ---------------------------------------------------------------------

fn tier_rank(tier: &str) -> i32 {
    match tier {
        "High" => 2,
        "Medium" => 1,
        _ => 0,
    }
}

fn compare_tiers(a: &str, b: &str) -> i32 {
    tier_rank(a) - tier_rank(b)
}

const SCORE_CHANGE_THRESHOLD: f64 = 0.30;

fn score_change_significant(base: i64, cur: i64) -> bool {
    let mut b = (base as f64).abs();
    if b < 1.0 {
        b = 1.0;
    }
    ((cur - base) as f64).abs() / b >= SCORE_CHANGE_THRESHOLD
}

/// Mirrors `replay.ComputeSelectionDiff`.
pub fn compute_selection_diff(a: &Manifest, b: &Manifest) -> SelectionSummary {
    let a_map = index_entries(&a.entries);
    let b_map = index_entries(&b.entries);

    let mut result = SelectionSummary::default();
    result.a = a.id.clone();
    result.b = b.id.clone();

    // Match Go's iteration: range b_map first (which in Go has random
    // order, then the result is later re-sorted by SortSelectionDiff).
    // For determinism we iterate sorted keys.
    let mut b_keys: Vec<&String> = b_map.keys().collect();
    b_keys.sort();
    for path in b_keys {
        let b_entry = b_map[path];
        match a_map.get(path) {
            None => {
                result.changes.added.push(SelectionChange {
                    path: path.clone(),
                    category: SelectionCategory::Added,
                    cur_score: b_entry.score,
                    cur_tier: b_entry.relevance.clone(),
                    cur_tokens: b_entry.tokens,
                    ..Default::default()
                });
                result.summary.added += 1;
                result.summary.token_delta += b_entry.tokens;
            }
            Some(a_entry) => {
                let tier_cmp = compare_tiers(&a_entry.relevance, &b_entry.relevance);
                if tier_cmp < 0 {
                    result.changes.promoted.push(SelectionChange {
                        path: path.clone(),
                        category: SelectionCategory::Promoted,
                        base_score: a_entry.score,
                        cur_score: b_entry.score,
                        base_tier: a_entry.relevance.clone(),
                        cur_tier: b_entry.relevance.clone(),
                        base_tokens: a_entry.tokens,
                        cur_tokens: b_entry.tokens,
                        reason_change: format!(
                            "tier:{}→{}",
                            a_entry.relevance, b_entry.relevance
                        ),
                    });
                    result.summary.promoted += 1;
                    result.summary.token_delta += b_entry.tokens - a_entry.tokens;
                } else if tier_cmp > 0 {
                    result.changes.demoted.push(SelectionChange {
                        path: path.clone(),
                        category: SelectionCategory::Demoted,
                        base_score: a_entry.score,
                        cur_score: b_entry.score,
                        base_tier: a_entry.relevance.clone(),
                        cur_tier: b_entry.relevance.clone(),
                        base_tokens: a_entry.tokens,
                        cur_tokens: b_entry.tokens,
                        reason_change: format!(
                            "tier:{}→{}",
                            a_entry.relevance, b_entry.relevance
                        ),
                    });
                    result.summary.demoted += 1;
                    result.summary.token_delta += b_entry.tokens - a_entry.tokens;
                } else if score_change_significant(a_entry.score, b_entry.score) {
                    let base = if a_entry.score == 0 { 1 } else { a_entry.score };
                    let pct = (((b_entry.score - a_entry.score) as f64)
                        / (base as f64).abs()
                        * 100.0)
                        .round() as i64;
                    let sign = if pct < 0 { "" } else { "+" };
                    result.changes.score_changed.push(SelectionChange {
                        path: path.clone(),
                        category: SelectionCategory::ScoreChanged,
                        base_score: a_entry.score,
                        cur_score: b_entry.score,
                        base_tier: a_entry.relevance.clone(),
                        cur_tier: b_entry.relevance.clone(),
                        base_tokens: a_entry.tokens,
                        cur_tokens: b_entry.tokens,
                        reason_change: format!("{sign}{pct}%"),
                    });
                    result.summary.score_changed += 1;
                    result.summary.token_delta += b_entry.tokens - a_entry.tokens;
                }
            }
        }
    }

    let mut a_keys: Vec<&String> = a_map.keys().collect();
    a_keys.sort();
    for path in a_keys {
        if b_map.contains_key(path) {
            continue;
        }
        let a_entry = a_map[path];
        result.changes.removed.push(SelectionChange {
            path: path.clone(),
            category: SelectionCategory::Removed,
            base_score: a_entry.score,
            base_tier: a_entry.relevance.clone(),
            base_tokens: a_entry.tokens,
            ..Default::default()
        });
        result.summary.removed += 1;
        result.summary.token_delta -= a_entry.tokens;
    }

    result
}

/// Mirrors `replay.SortSelectionDiff`. Sorts each category slice in place.
pub fn sort_selection_diff(s: &mut SelectionSummary, by: &str) {
    sort_group(&mut s.changes.added, by);
    sort_group(&mut s.changes.removed, by);
    sort_group(&mut s.changes.promoted, by);
    sort_group(&mut s.changes.demoted, by);
    sort_group(&mut s.changes.score_changed, by);
    // help borrow-checker by reborrowing
    let _ = &s.changes;
}

fn sort_group(items: &mut Vec<SelectionChange>, by: &str) {
    match by {
        "tokens" => items.sort_by(|a, b| {
            let ta = if a.cur_tokens == 0 { a.base_tokens } else { a.cur_tokens };
            let tb = if b.cur_tokens == 0 { b.base_tokens } else { b.cur_tokens };
            tb.cmp(&ta) // descending
        }),
        "score" => items.sort_by(|a, b| {
            let da = (a.cur_score - a.base_score).abs();
            let db = (b.cur_score - b.base_score).abs();
            db.cmp(&da)
        }),
        _ => items.sort_by(|a, b| {
            let tier_a = if a.cur_tier.is_empty() { &a.base_tier } else { &a.cur_tier };
            let tier_b = if b.cur_tier.is_empty() { &b.base_tier } else { &b.cur_tier };
            let ra = tier_rank(tier_a);
            let rb = tier_rank(tier_b);
            if ra != rb {
                rb.cmp(&ra)
            } else {
                let sa = if a.cur_score == 0 { a.base_score } else { a.cur_score };
                let sb = if b.cur_score == 0 { b.base_score } else { b.cur_score };
                sb.cmp(&sa)
            }
        }),
    }
}

/// Mirrors `replay.WriteSelectionDiffMarkdown` for parity verification.
pub fn write_selection_diff_markdown(s: &SelectionSummary) -> String {
    let mut out = String::new();
    out.push_str(&format!("# Replay Diff — {} → {}\n\n", s.a, s.b));

    if !s.changes.added.is_empty() {
        let token_sum: i64 = s.changes.added.iter().map(|c| c.cur_tokens).sum();
        out.push_str(&format!(
            "**Added** ({} file(s), +{} tokens)\n",
            s.changes.added.len(),
            token_sum
        ));
        out.push_str("| Path | Score | Tier | Tokens |\n|---|---|---|---|\n");
        for c in &s.changes.added {
            let tier = if c.cur_tier.is_empty() { "-".to_string() } else { c.cur_tier.clone() };
            out.push_str(&format!(
                "| {} | {} | {} | {} |\n",
                c.path, c.cur_score, tier, c.cur_tokens
            ));
        }
        out.push('\n');
    }

    // Other categories follow similar patterns; for parity we keep the
    // helper minimal — the dispatcher only needs the JSON path.
    if s.summary.added == 0
        && s.summary.removed == 0
        && s.summary.promoted == 0
        && s.summary.demoted == 0
        && s.summary.score_changed == 0
    {
        out.push_str("_No selection changes between the two snapshots._\n");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str, sha: &str, tokens: i64) -> Entry {
        Entry {
            path: path.into(),
            sha256: sha.into(),
            tokens,
            relevance: "High".into(),
            score: 0,
            reason: String::new(),
        }
    }

    #[test]
    fn diff_added_modified_removed_unchanged() {
        let base = Manifest {
            entries: vec![
                entry("a", "aa", 10),
                entry("b", "bb", 20),
                entry("c", "cc", 30),
            ],
            ..Default::default()
        };
        let cur = Manifest {
            entries: vec![
                entry("a", "aa", 10),     // unchanged
                entry("b", "BB", 25),     // modified
                entry("d", "dd", 40),     // added
                                          // c removed
            ],
            ..Default::default()
        };
        let s = compute(&base, &cur, DiffOptions::default());
        assert_eq!(s.added, 1);
        assert_eq!(s.modified, 1);
        assert_eq!(s.removed, 1);
        assert_eq!(s.unchanged, 1);
    }

    #[test]
    fn strict_token_only_delta_is_modified() {
        let base = Manifest {
            entries: vec![entry("a", "aa", 10)],
            ..Default::default()
        };
        let cur = Manifest {
            entries: vec![entry("a", "aa", 12)],
            ..Default::default()
        };
        let s = compute(&base, &cur, DiffOptions { strict: true });
        assert_eq!(s.modified, 1);
        assert_eq!(s.unchanged, 0);
    }

    #[test]
    fn selection_diff_added_only() {
        let a = Manifest { id: "a".into(), ..Default::default() };
        let mut b = Manifest::default();
        b.id = "b".into();
        b.entries.push(Entry {
            path: "x".into(),
            sha256: "xx".into(),
            tokens: 5,
            relevance: "High".into(),
            score: 10,
            reason: String::new(),
        });
        let s = compute_selection_diff(&a, &b);
        assert_eq!(s.summary.added, 1);
        assert_eq!(s.changes.added.len(), 1);
    }
}
