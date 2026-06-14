// crates/ctx-heatmap/tests/regression.rs
//
// Regression tests pinning the 13 test cases from the Go heatmap_test.go
// and the edge cases discovered during the Phase 4 Tier 1 #2 port.

use ctx_heatmap::{
    aggregate, render_ascii, render_json, render_plain, squarify, AggregateOptions, AsciiOptions,
    Bucket, FileMetric, JsonOptions, PlainOptions,
};

fn metric(path: &str, tokens: i64, symbols: i64) -> FileMetric {
    FileMetric {
        path: path.into(),
        is_dir: false,
        tokens,
        symbols,
        churn: 0,
    }
}

// ----- mirrors TestAggregate_TokensByDepth ---------------------------------

#[test]
fn aggregate_tokens_by_depth_matches_go() {
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
    assert_eq!(buckets.len(), 4);
    for w in buckets.windows(2) {
        assert!(w[0].weight >= w[1].weight);
    }
}

// ----- mirrors TestAggregate_DepthZeroCollapsesToRoot ----------------------

#[test]
fn aggregate_depth_zero_collapses_root() {
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

// ----- mirrors TestAggregate_FilesAndSymbolsAxes ---------------------------

#[test]
fn aggregate_files_and_symbols_axes() {
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

// ----- mirrors TestAggregate_DropsZeroWeightBuckets ------------------------

#[test]
fn aggregate_drops_zero_weight_buckets() {
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

// ----- mirrors TestSquarify_AreaConservation ------------------------------

#[test]
fn squarify_area_conservation() {
    let buckets = vec![
        Bucket {
            path: "a".into(),
            weight: 50.0,
            ..Default::default()
        },
        Bucket {
            path: "b".into(),
            weight: 30.0,
            ..Default::default()
        },
        Bucket {
            path: "c".into(),
            weight: 15.0,
            ..Default::default()
        },
        Bucket {
            path: "d".into(),
            weight: 5.0,
            ..Default::default()
        },
    ];
    let (w, h) = (80, 20);
    let rects = squarify(&buckets, w, h);
    assert_eq!(rects.len(), buckets.len());

    let mut total_area: i64 = 0;
    let mut covered = vec![vec![false; w as usize]; h as usize];
    for r in &rects {
        assert!(r.x >= 0 && r.y >= 0 && r.x + r.w <= w && r.y + r.h <= h);
        total_area += r.w * r.h;
        for y in r.y..r.y + r.h {
            for x in r.x..r.x + r.w {
                assert!(!covered[y as usize][x as usize]);
                covered[y as usize][x as usize] = true;
            }
        }
    }
    assert_eq!(total_area, w * h);
}

// ----- mirrors TestSquarify_AspectRatioReasonable -------------------------

#[test]
fn squarify_aspect_ratio_reasonable() {
    let buckets = vec![
        Bucket {
            path: "a".into(),
            weight: 40.0,
            ..Default::default()
        },
        Bucket {
            path: "b".into(),
            weight: 30.0,
            ..Default::default()
        },
        Bucket {
            path: "c".into(),
            weight: 20.0,
            ..Default::default()
        },
        Bucket {
            path: "d".into(),
            weight: 10.0,
            ..Default::default()
        },
    ];
    let rects = squarify(&buckets, 60, 20);
    for r in &rects {
        assert!(r.w > 0 && r.h > 0);
        let mut ratio = r.w as f64 / r.h as f64;
        if ratio < 1.0 {
            ratio = 1.0 / ratio;
        }
        assert!(ratio <= 10.0);
    }
}

// ----- mirrors TestSquarify_EmptyAndDegenerate ----------------------------

#[test]
fn squarify_empty_and_degenerate() {
    assert!(squarify(&[], 80, 20).is_empty());
    assert!(squarify(
        &[Bucket {
            path: "a".into(),
            weight: 1.0,
            ..Default::default()
        }],
        0,
        20
    )
    .is_empty());
    assert!(squarify(
        &[Bucket {
            path: "a".into(),
            weight: 0.0,
            ..Default::default()
        }],
        80,
        20
    )
    .is_empty());
}

// ----- mirrors TestRenderASCII_HeaderAndCellLabels ------------------------

#[test]
fn render_ascii_header_and_labels() {
    let buckets = vec![
        Bucket {
            path: "internal/cli".into(),
            weight: 60.0,
            tokens: 6000,
            files: 11,
            symbols: 31,
            churn: 0,
        },
        Bucket {
            path: "internal/walk".into(),
            weight: 40.0,
            tokens: 4000,
            files: 4,
            symbols: 12,
            churn: 0,
        },
    ];
    let rects = squarify(&buckets, 60, 10);
    let out = render_ascii(
        &rects,
        &AsciiOptions {
            width: 60,
            height: 10,
            by: "tokens".into(),
            root: ".".into(),
            budget: 0,
        },
    );

    assert!(
        out.contains("Heatmap (by tokens, root=., total=10,000 tokens)"),
        "{out}"
    );
    for want in ["internal/cli", "internal/walk"] {
        assert!(out.contains(want), "missing {want} in {out}");
    }
    assert!(out.contains('+'));
    assert!(out.contains('-'));
    assert!(out.contains('|'));
    assert!(!out.contains("Legend:"));
}

// ----- mirrors TestRenderASCII_BudgetLegendAndOver ------------------------

#[test]
fn render_ascii_budget_legend_and_over() {
    let buckets = vec![
        Bucket {
            path: "small".into(),
            weight: 100.0,
            tokens: 100,
            files: 1,
            symbols: 1,
            churn: 0,
        },
        Bucket {
            path: "big".into(),
            weight: 900.0,
            tokens: 900,
            files: 1,
            symbols: 1,
            churn: 0,
        },
    ];
    let rects = squarify(&buckets, 60, 10);
    let out = render_ascii(
        &rects,
        &AsciiOptions {
            width: 60,
            height: 10,
            by: "tokens".into(),
            root: ".".into(),
            budget: 200,
        },
    );
    assert!(out.contains("Legend:"), "{out}");
    assert!(out.contains("budget=200"), "{out}");
}

// ----- mirrors TestRenderJSON_ShapeAndInBudget ----------------------------

#[test]
fn render_json_shape_and_in_budget() {
    let buckets = vec![
        Bucket {
            path: "a".into(),
            weight: 100.0,
            tokens: 100,
            files: 2,
            symbols: 5,
            churn: 0,
        },
        Bucket {
            path: "b".into(),
            weight: 50.0,
            tokens: 50,
            files: 1,
            symbols: 2,
            churn: 0,
        },
    ];
    let rects = squarify(&buckets, 80, 20);
    let bytes = render_json(
        &rects,
        &JsonOptions {
            root: ".".into(),
            by: "tokens".into(),
            budget: Some(120),
        },
    )
    .unwrap();
    let decoded: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(decoded["root"], ".");
    assert_eq!(decoded["by"], "tokens");
    assert_eq!(decoded["budget"], 120);
    let arr = decoded["buckets"].as_array().unwrap();
    assert_eq!(arr.len(), 2);
    assert_eq!(arr[0]["in_budget"], true);
    assert_eq!(arr[1]["in_budget"], false);
    assert!(arr[0]["rect"]["w"].as_i64().unwrap() > 0);
    assert!(arr[0]["rect"]["h"].as_i64().unwrap() > 0);
}

// ----- mirrors TestRenderPlain_FormatAndOrdering --------------------------

#[test]
fn render_plain_format_and_ordering() {
    let buckets = vec![
        Bucket {
            path: "internal/web".into(),
            weight: 8420.0,
            tokens: 8420,
            files: 14,
            symbols: 87,
            churn: 0,
        },
        Bucket {
            path: "internal/cli".into(),
            weight: 2890.0,
            tokens: 2890,
            files: 11,
            symbols: 31,
            churn: 0,
        },
    ];
    let out = render_plain(
        &buckets,
        &PlainOptions {
            root: ".".into(),
            by: "tokens".into(),
            budget: 0,
        },
    );
    assert!(
        out.starts_with("Heatmap (by tokens, root=., total=11,310 tokens)\n"),
        "{out}"
    );
    for want in [
        "1. internal/web \u{2014} 8,420 tokens (14 files, 87 symbols).\n",
        "2. internal/cli \u{2014} 2,890 tokens (11 files, 31 symbols).\n",
    ] {
        assert!(out.contains(want), "missing {want:?}\n{out}");
    }
    assert!(!out.contains("+--"));
    assert!(!out.contains('|'));
}

// ----- mirrors TestRenderPlain_BudgetTagging ------------------------------

#[test]
fn render_plain_budget_tagging() {
    let buckets = vec![
        Bucket {
            path: "small".into(),
            weight: 100.0,
            tokens: 100,
            files: 1,
            symbols: 1,
            churn: 0,
        },
        Bucket {
            path: "big".into(),
            weight: 900.0,
            tokens: 900,
            files: 1,
            symbols: 1,
            churn: 0,
        },
    ];
    let out = render_plain(
        &buckets,
        &PlainOptions {
            root: ".".into(),
            by: "tokens".into(),
            budget: 200,
        },
    );
    assert!(out.contains("[in budget]"), "{out}");
    assert!(out.contains("[over budget]"), "{out}");
}

// ----- mirrors TestTopN ----------------------------------------------------

#[test]
fn top_n_semantics() {
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
    let t2 = ctx_heatmap::top_n(buckets.clone(), 2);
    assert_eq!(t2.len(), 2);
    assert_eq!(t2[0].path, "a");
    assert_eq!(t2[1].path, "b");
    let t0 = ctx_heatmap::top_n(buckets.clone(), 0);
    assert_eq!(t0.len(), 3);
    let t99 = ctx_heatmap::top_n(buckets, 99);
    assert_eq!(t99.len(), 3);
}

// ----- regression: trailing-slash directory paths fold sanely -------------

#[test]
fn directory_paths_skipped() {
    let files = vec![
        FileMetric {
            path: "internal/".into(),
            is_dir: true,
            tokens: 0,
            symbols: 0,
            churn: 0,
        },
        metric("internal/cli/a.go", 100, 1),
    ];
    let buckets = aggregate(
        &files,
        &AggregateOptions {
            by: "tokens".into(),
            depth: 2,
            top: 0,
        },
    );
    assert_eq!(buckets.len(), 1);
    assert_eq!(buckets[0].path, "internal/cli");
}

// ----- regression: empty render emits "No content to display." ------------

#[test]
fn render_plain_empty_buckets_message() {
    let out = render_plain(
        &[],
        &PlainOptions {
            root: ".".into(),
            by: "tokens".into(),
            budget: 0,
        },
    );
    assert!(out.contains("No content to display."), "{out}");
}
