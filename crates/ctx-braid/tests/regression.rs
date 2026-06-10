// crates/ctx-braid/tests/regression.rs
//
// Mirror of internal/braid/braid_test.go regression cases. Verifies
// each behavioural assertion from the Go test file lands identically
// in Rust.

use ctx_braid::{
    allocate, load, merge_paths, shell_split, sorted_strand_names, strand_subcommand,
    strip_ctx_and_sub, validate, BraidResult, Config, MergedFile, PolicyKind, StrandSelection,
};

const VALID_TOML: &[u8] = br#"schema_version = 1

[[strand]]
name = "api-surface"
source = "where 'handler' --format json"
share  = 0.4

[[strand]]
name = "recent-changes"
source = "digest --since 7d --format json"
share  = 0.3
policy = "prefer-newer"

[[strand]]
name = "anchor-deep"
source = "focus RateLimiter --hops 2 --format json"
share  = 0.3
"#;

const INVALID_SOURCE_TOML: &[u8] = br#"schema_version = 1

[[strand]]
name = "bogus"
source = "unknown-subcommand --flag"
share  = 0.5
"#;

const SHARE_OVERFLOW_TOML: &[u8] = br#"schema_version = 1

[[strand]]
name = "a"
source = "where 'foo'"
share  = 0.7

[[strand]]
name = "b"
source = "where 'bar'"
share  = 0.6
"#;

#[test]
fn load_valid_returns_three_strands_with_default_policy() {
    let cfg = load(VALID_TOML).unwrap();
    assert_eq!(cfg.schema_version, 1);
    assert_eq!(cfg.strands.len(), 3);
    assert_eq!(cfg.strands[0].policy.unwrap_or_merge(), PolicyKind::Merge);
    assert_eq!(
        cfg.strands[1].policy.unwrap_or_merge(),
        PolicyKind::PreferNewer
    );
}

#[test]
fn load_invalid_source_errors_with_unsupported_source_substring() {
    let err = load(INVALID_SOURCE_TOML).unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("unsupported source"),
        "error message must mention 'unsupported source', got {msg}"
    );
}

#[test]
fn validate_rejects_duplicate_strand_name() {
    let mut cfg = Config {
        schema_version: 1,
        strands: vec![
            ctx_braid::Strand {
                name: "x".into(),
                source: "where 'a'".into(),
                share: 0.5,
                policy: Default::default(),
            },
            ctx_braid::Strand {
                name: "x".into(),
                source: "focus B".into(),
                share: 0.5,
                policy: Default::default(),
            },
        ],
    };
    let err = validate(&mut cfg).unwrap_err();
    assert!(
        err.to_string().contains("duplicate strand name"),
        "error should mention duplicate-strand-name: {err}"
    );
}

#[test]
fn validate_rejects_share_out_of_range() {
    let mut cfg = Config {
        schema_version: 1,
        strands: vec![ctx_braid::Strand {
            name: "x".into(),
            source: "where 'a'".into(),
            share: 0.0,
            policy: Default::default(),
        }],
    };
    let err = validate(&mut cfg).unwrap_err();
    assert!(err.to_string().contains("share must be in"));
}

#[test]
fn allocate_normalises_overflow() {
    let cfg = load(SHARE_OVERFLOW_TOML).unwrap();
    let out = allocate(&cfg, 10_000);
    assert!(
        out.warning.contains("normalising to 1.0"),
        "warning text must mention normalisation, got {:?}",
        out.warning
    );
    let total: f64 = out.allocations.iter().map(|a| a.share).sum();
    assert!((total - 1.0).abs() < 1e-6, "shares should sum to ~1.0");

    let sum_budget: i64 = out.allocations.iter().map(|a| a.budget).sum();
    assert!(
        (9990..=10010).contains(&sum_budget),
        "budget sum {} should be within rounding of 10000",
        sum_budget
    );
}

#[test]
fn allocate_under_one_preserved_no_warning() {
    let cfg = Config {
        schema_version: 1,
        strands: vec![
            ctx_braid::Strand {
                name: "a".into(),
                source: "where 'x'".into(),
                share: 0.3,
                policy: Default::default(),
            },
            ctx_braid::Strand {
                name: "b".into(),
                source: "where 'y'".into(),
                share: 0.4,
                policy: Default::default(),
            },
        ],
    };
    let out = allocate(&cfg, 1000);
    assert!(
        out.warning.is_empty(),
        "warning unexpected: {:?}",
        out.warning
    );
    assert_eq!(out.allocations[0].budget, 300);
    assert_eq!(out.allocations[1].budget, 400);
}

#[test]
fn merge_paths_merge_policy_keeps_all() {
    let got = merge_paths(&[
        StrandSelection {
            name: "a".into(),
            policy: PolicyKind::Merge,
            paths: vec!["x.go".into(), "y.go".into()],
        },
        StrandSelection {
            name: "b".into(),
            policy: PolicyKind::Merge,
            paths: vec!["y.go".into(), "z.go".into()],
        },
    ]);
    assert_eq!(
        got,
        vec![
            MergedFile {
                path: "x.go".into(),
                origin: "a".into()
            },
            MergedFile {
                path: "y.go".into(),
                origin: "a".into()
            },
            MergedFile {
                path: "y.go".into(),
                origin: "b".into()
            },
            MergedFile {
                path: "z.go".into(),
                origin: "b".into()
            },
        ]
    );
}

#[test]
fn merge_paths_prefer_newer_evicts_earlier() {
    let got = merge_paths(&[
        StrandSelection {
            name: "a".into(),
            policy: PolicyKind::Merge,
            paths: vec!["x.go".into(), "y.go".into()],
        },
        StrandSelection {
            name: "b".into(),
            policy: PolicyKind::PreferNewer,
            paths: vec!["y.go".into(), "z.go".into()],
        },
    ]);
    assert_eq!(
        got,
        vec![
            MergedFile {
                path: "x.go".into(),
                origin: "a".into()
            },
            MergedFile {
                path: "y.go".into(),
                origin: "b".into()
            },
            MergedFile {
                path: "z.go".into(),
                origin: "b".into()
            },
        ]
    );
}

#[test]
fn merge_paths_exclude_overlap_drops_later() {
    let got = merge_paths(&[
        StrandSelection {
            name: "a".into(),
            policy: PolicyKind::Merge,
            paths: vec!["x.go".into(), "y.go".into()],
        },
        StrandSelection {
            name: "b".into(),
            policy: PolicyKind::ExcludeOverlap,
            paths: vec!["y.go".into(), "z.go".into()],
        },
    ]);
    assert_eq!(
        got,
        vec![
            MergedFile {
                path: "x.go".into(),
                origin: "a".into()
            },
            MergedFile {
                path: "y.go".into(),
                origin: "a".into()
            },
            MergedFile {
                path: "z.go".into(),
                origin: "b".into()
            },
        ]
    );
}

#[test]
fn merge_paths_line_ranges_allow_non_overlapping_overlap_policy() {
    let got = merge_paths(&[
        StrandSelection {
            name: "a".into(),
            policy: PolicyKind::Merge,
            paths: vec!["x.go:1-10".into()],
        },
        StrandSelection {
            name: "b".into(),
            policy: PolicyKind::ExcludeOverlap,
            paths: vec!["x.go:11-20".into(), "x.go:5-6".into()],
        },
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
fn merge_paths_line_range_prefer_newer_only_evicts_overlaps() {
    let got = merge_paths(&[
        StrandSelection {
            name: "a".into(),
            policy: PolicyKind::Merge,
            paths: vec!["x.go:1-10".into(), "x.go:20-30".into()],
        },
        StrandSelection {
            name: "b".into(),
            policy: PolicyKind::PreferNewer,
            paths: vec!["x.go:5-15".into()],
        },
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

#[test]
fn shell_split_single_quote_preserves_run() {
    let got = shell_split("where 'multi word' --limit 5").unwrap();
    assert_eq!(got, vec!["where", "multi word", "--limit", "5"]);
}

#[test]
fn shell_split_double_quote_preserves_run() {
    let got = shell_split(r#"where "multi word" --regex "a|b""#).unwrap();
    assert_eq!(got, vec!["where", "multi word", "--regex", "a|b"]);
}

#[test]
fn strip_ctx_and_sub_mixed_quotes() {
    let tokens = strip_ctx_and_sub("where 'handler' --regex 'router|Handler'").unwrap();
    assert_eq!(tokens, vec!["handler", "--regex", "router|Handler"]);
}

#[test]
fn shell_split_unclosed_quote_errors() {
    assert!(shell_split("where 'unclosed").is_err());
    assert!(shell_split(r#"where "unclosed"#).is_err());
    assert!(strip_ctx_and_sub("where 'unclosed").is_err());
}

#[test]
fn strand_subcommand_extracts_first_token() {
    assert_eq!(strand_subcommand("where 'foo' --format json"), "where");
    assert_eq!(strand_subcommand("ctx focus Bar"), "focus");
    assert_eq!(strand_subcommand("  digest --since 7d"), "digest");
    assert_eq!(strand_subcommand(""), "");
}

#[test]
fn sorted_strand_names_returns_alpha_order() {
    let cfg = Config {
        schema_version: 1,
        strands: vec![
            ctx_braid::Strand {
                name: "zebra".into(),
                source: "where 'x'".into(),
                share: 0.5,
                policy: Default::default(),
            },
            ctx_braid::Strand {
                name: "alpha".into(),
                source: "where 'y'".into(),
                share: 0.5,
                policy: Default::default(),
            },
        ],
    };
    assert_eq!(sorted_strand_names(&cfg), vec!["alpha", "zebra"]);
}

#[test]
fn braid_result_type_is_usable() {
    // Sanity check: the type is reachable via the public re-export.
    let _ = BraidResult::default();
}
