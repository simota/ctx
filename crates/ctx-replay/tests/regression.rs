// crates/ctx-replay/tests/regression.rs
//
// Regression tests pinning edge cases discovered during the Phase 3 port.

use ctx_replay::diff::{compute, compute_selection_diff, DiffOptions};
use ctx_replay::prune::parse_duration;
use ctx_replay::types::{ChangeKind, Entry, Manifest};

fn entry(path: &str, sha: &str, tokens: i64, tier: &str, score: i64) -> Entry {
    Entry {
        path: path.into(),
        sha256: sha.into(),
        tokens,
        relevance: tier.into(),
        score,
        reason: String::new(),
    }
}

#[test]
fn empty_manifests_produce_zero_summary() {
    let a = Manifest::default();
    let b = Manifest::default();
    let s = compute(&a, &b, DiffOptions::default());
    assert_eq!(s.added, 0);
    assert_eq!(s.modified, 0);
    assert_eq!(s.removed, 0);
    assert_eq!(s.unchanged, 0);
    assert!(s.changes.is_empty());
}

#[test]
fn identical_manifests_are_unchanged() {
    let m = Manifest {
        entries: vec![entry("x.go", "aa", 10, "High", 5)],
        ..Default::default()
    };
    let s = compute(&m, &m, DiffOptions::default());
    assert_eq!(s.unchanged, 1);
    assert_eq!(s.modified, 0);
    let change = &s.changes[0];
    assert!(matches!(change.kind, ChangeKind::Unchanged));
    assert_eq!(change.token_delta, 0);
}

#[test]
fn promotion_recognised() {
    let a = Manifest {
        entries: vec![entry("x", "aa", 10, "Medium", 5)],
        ..Default::default()
    };
    let b = Manifest {
        entries: vec![entry("x", "aa", 10, "High", 5)],
        ..Default::default()
    };
    let s = compute_selection_diff(&a, &b);
    assert_eq!(s.summary.promoted, 1);
}

#[test]
fn demotion_recognised() {
    let a = Manifest {
        entries: vec![entry("x", "aa", 10, "High", 5)],
        ..Default::default()
    };
    let b = Manifest {
        entries: vec![entry("x", "aa", 10, "Medium", 5)],
        ..Default::default()
    };
    let s = compute_selection_diff(&a, &b);
    assert_eq!(s.summary.demoted, 1);
}

#[test]
fn strict_token_only_change_is_modified() {
    let a = Manifest {
        entries: vec![entry("x", "aa", 10, "High", 5)],
        ..Default::default()
    };
    let b = Manifest {
        entries: vec![entry("x", "aa", 15, "High", 5)],
        ..Default::default()
    };
    let s_strict = compute(&a, &b, DiffOptions { strict: true });
    assert_eq!(s_strict.modified, 1);
    let s_default = compute(&a, &b, DiffOptions { strict: false });
    assert_eq!(s_default.unchanged, 1);
}

#[test]
fn parse_duration_supports_day_week_compound() {
    let week_plus_2h = parse_duration("1w2h").unwrap();
    let manual = 7 * 24 * 3600 * 1_000_000_000_i64 + 2 * 3600 * 1_000_000_000_i64;
    assert_eq!(week_plus_2h, manual);
}

#[test]
fn parse_duration_rejects_unitless() {
    assert!(parse_duration("5").is_err());
    assert!(parse_duration("abc").is_err());
}

#[test]
fn diff_path_order_is_sorted() {
    let a = Manifest {
        entries: vec![
            entry("b.go", "aa", 1, "High", 0),
            entry("a.go", "bb", 1, "High", 0),
        ],
        ..Default::default()
    };
    let b = Manifest::default();
    let s = compute(&a, &b, DiffOptions::default());
    let paths: Vec<&str> = s.changes.iter().map(|c| c.path.as_str()).collect();
    assert_eq!(paths, vec!["a.go", "b.go"]);
}
