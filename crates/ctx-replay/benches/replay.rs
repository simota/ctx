// crates/ctx-replay/benches/replay.rs
//
// Criterion benches for the Phase 3 replay port. Mirrors the layout of
// crates/ctx-relations/benches/relations.rs.
//
// Run:
//   cargo bench --bench replay --manifest-path crates/ctx-replay/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_replay::diff::{compute, DiffOptions};
use ctx_replay::types::Manifest;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("replay-fixtures")
}

fn load_pair(name: &str) -> Option<(Manifest, Manifest)> {
    let dir = fixtures_dir().join(name);
    let base = std::fs::read(dir.join("base.json")).ok()?;
    let cur = std::fs::read(dir.join("current.json")).ok()?;
    let base: Manifest = serde_json::from_slice(&base).ok()?;
    let cur: Manifest = serde_json::from_slice(&cur).ok()?;
    Some((base, cur))
}

fn bench_diff(c: &mut Criterion) {
    let fixtures = ["single_snap", "multi_snap_drift", "scoring_change"];
    let mut group = c.benchmark_group("diff");
    for fx in &fixtures {
        let Some((base, cur)) = load_pair(fx) else { continue };
        let n = (base.entries.len() + cur.entries.len()) as u64;
        group.throughput(Throughput::Elements(n.max(1)));
        group.bench_with_input(
            BenchmarkId::new("rust", fx),
            &(base, cur),
            |b, (base, cur)| {
                b.iter(|| compute(base, cur, DiffOptions::default()))
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_diff);
criterion_main!(benches);
