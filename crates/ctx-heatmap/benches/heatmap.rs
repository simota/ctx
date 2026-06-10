// crates/ctx-heatmap/benches/heatmap.rs
//
// Criterion bench for the Phase 4 Tier 1 #2 heatmap port. Measures
// aggregate + squarify + render (each format) on small/medium/large
// fixtures (in-process; no cgo).
//
// Run:
//   cargo bench --bench heatmap --manifest-path crates/ctx-heatmap/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_heatmap::{
    aggregate, render_ascii, render_json, render_plain, squarify, AggregateOptions,
    AsciiOptions, FileMetric, JsonOptions, PlainOptions,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("heatmap-fixtures")
}

fn load_metrics(name: &str) -> Option<Vec<FileMetric>> {
    let path = fixtures_dir().join(name).join("metrics.json");
    let raw = std::fs::read(&path).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn bench_heatmap(c: &mut Criterion) {
    let fixtures = ["small_metrics", "medium_metrics", "large_metrics"];
    let mut group = c.benchmark_group("heatmap");
    let opts = AggregateOptions {
        by: "tokens".into(),
        depth: 2,
        top: 0,
    };
    let ascii_opts = AsciiOptions::default();
    let json_opts = JsonOptions {
        root: ".".into(),
        by: "tokens".into(),
        budget: None,
    };
    let plain_opts = PlainOptions {
        root: ".".into(),
        by: "tokens".into(),
        budget: 0,
    };
    for fx in &fixtures {
        let Some(metrics) = load_metrics(fx) else { continue };
        let n = metrics.len() as u64;
        group.throughput(Throughput::Elements(n.max(1)));

        group.bench_with_input(BenchmarkId::new("aggregate", fx), &metrics, |b, m| {
            b.iter(|| {
                let _ = aggregate(m, &opts);
            })
        });

        let buckets = aggregate(&metrics, &opts);

        group.bench_with_input(
            BenchmarkId::new("squarify", fx),
            &buckets,
            |b, bk| {
                b.iter(|| {
                    let _ = squarify(bk, 80, 20);
                })
            },
        );

        let rects = squarify(&buckets, 80, 20);

        group.bench_with_input(BenchmarkId::new("render_ascii", fx), &rects, |b, r| {
            b.iter(|| {
                let _ = render_ascii(r, &ascii_opts);
            })
        });

        group.bench_with_input(BenchmarkId::new("render_json", fx), &rects, |b, r| {
            b.iter(|| {
                let _ = render_json(r, &json_opts).unwrap();
            })
        });

        group.bench_with_input(BenchmarkId::new("render_plain", fx), &buckets, |b, bk| {
            b.iter(|| {
                let _ = render_plain(bk, &plain_opts);
            })
        });

        // End-to-end (the per-call workload in `ctx map`).
        group.bench_with_input(
            BenchmarkId::new("end_to_end_ascii", fx),
            &metrics,
            |b, m| {
                b.iter(|| {
                    let bk = aggregate(m, &opts);
                    let rc = squarify(&bk, 80, 20);
                    let _ = render_ascii(&rc, &ascii_opts);
                })
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_heatmap);
criterion_main!(benches);
