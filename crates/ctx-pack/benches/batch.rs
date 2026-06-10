// crates/ctx-pack/benches/batch.rs
//
// Criterion bench for the stateless batch APIs (diff / redact /
// from_where / preset). Each one fires once per `ctx pack` CLI
// invocation; the bench measures the pure Rust path so the Go side
// can compare against equivalent Go output. Expect modest absolute
// numbers — these helpers are tiny — and accept evidence-only
// verdicts if the cgo overhead dominates.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use ctx_pack::diff::render as diff_render;
use ctx_pack::from_where::parse as from_where_parse;
use ctx_pack::preset::apply_preset;
use ctx_pack::redact::redact_lines;
use ctx_pack::types::{DiffEntry, DiffOptions, WarningInput};

fn bench_diff(c: &mut Criterion) {
    let diffs: Vec<DiffEntry> = (0..32)
        .map(|i| DiffEntry {
            path: format!("internal/pkg/file_{i}.go"),
            before_content: "before\nsecond line\n".to_string(),
            after_content: "after\nsecond line\n".to_string(),
            before_commit: format!("a{i:040}"),
            after_commit: format!("b{i:040}"),
            patch: format!(
                "--- a/internal/pkg/file_{i}.go\n+++ b/internal/pkg/file_{i}.go\n@@ -1,1 +1,1 @@\n-before\n+after\n"
            ),
            added: false,
            deleted: false,
            binary: false,
        })
        .collect();
    let mut group = c.benchmark_group("batch_diff");
    for layout in ["sequential", "unified", "side-by-side"] {
        group.bench_function(format!("layout_{layout}"), |b| {
            let opts = DiffOptions {
                layout: layout.into(),
                preset: String::new(),
            };
            b.iter(|| {
                let out = diff_render(black_box(&diffs), black_box(&opts));
                black_box(out.len())
            });
        });
    }
    group.finish();
}

fn bench_redact(c: &mut Criterion) {
    let data = (0..512).fold(String::new(), |mut a, i| {
        a.push_str(&format!("line {i}: lorem ipsum dolor sit amet consectetur adipiscing\n"));
        a
    });
    let warnings: Vec<WarningInput> = (1..=512)
        .step_by(8)
        .map(|line| WarningInput {
            path: String::new(),
            line,
            kind: "env".into(),
        })
        .collect();
    let mut group = c.benchmark_group("batch_redact");
    group.bench_function("512_lines_64_redacts", |b| {
        b.iter(|| {
            let out = redact_lines(black_box(data.as_bytes()), black_box(&warnings));
            black_box(out.len())
        });
    });
    group.finish();
}

fn bench_from_where(c: &mut Criterion) {
    let json: Vec<u8> = {
        let mut s = String::from("[");
        for i in 0..256 {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "{{\"path\":\"internal/pkg/file_{i}.go\",\"score\":{}}}",
                (256 - i) as f64 / 256.0
            ));
        }
        s.push(']');
        s.into_bytes()
    };
    let lines = (0..256)
        .map(|i| format!("internal/pkg/file_{i}.go"))
        .collect::<Vec<_>>()
        .join("\n");
    let mut group = c.benchmark_group("batch_from_where");
    group.bench_function("json_256", |b| {
        b.iter(|| black_box(from_where_parse(black_box(&json)).unwrap()));
    });
    group.bench_function("lines_256", |b| {
        b.iter(|| black_box(from_where_parse(black_box(lines.as_bytes())).unwrap()));
    });
    group.finish();
}

fn bench_preset(c: &mut Criterion) {
    let mut group = c.benchmark_group("batch_preset");
    for n in ["blog", "review", "debug", "llm"] {
        group.bench_function(format!("preset_{n}"), |b| {
            b.iter(|| black_box(apply_preset(black_box(n)).unwrap()));
        });
    }
    group.finish();
}

criterion_group!(benches, bench_diff, bench_redact, bench_from_where, bench_preset);
criterion_main!(benches);
