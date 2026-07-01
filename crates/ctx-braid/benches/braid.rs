// crates/ctx-braid/benches/braid.rs
//
// Criterion bench for the Phase 4 Tier 2 #1 braid pure-compute port.
// Measures the four pure helpers (load + validate, allocate, merge_paths,
// shell_split) on small/multi/complex fixtures (in-process; no cgo).
//
// Run:
//   cargo bench --bench braid --manifest-path crates/ctx-braid/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_braid::{allocate, load, merge_paths, shell_split, validate, Config, StrandSelection};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("braid-fixtures")
}

fn load_toml(name: &str) -> Option<Vec<u8>> {
    let path = fixtures_dir().join(format!("{name}.toml"));
    std::fs::read(&path).ok()
}

fn load_selections(name: &str) -> Option<Vec<StrandSelection>> {
    let path = fixtures_dir().join(format!("{name}_selections.json"));
    let raw = std::fs::read(&path).ok()?;
    serde_json::from_slice(&raw).ok()
}

fn bench_braid(c: &mut Criterion) {
    let fixtures = ["simple", "multi_strand", "complex"];
    let mut group = c.benchmark_group("braid");
    let sample_source = "where 'handler' --regex 'router|Handler' --limit 50";
    for fx in &fixtures {
        let Some(toml_bytes) = load_toml(fx) else {
            continue;
        };
        group.throughput(Throughput::Bytes(toml_bytes.len() as u64));

        group.bench_with_input(
            BenchmarkId::new("load_validate", fx),
            &toml_bytes,
            |b, data| {
                b.iter(|| {
                    let _ = load(data).unwrap();
                });
            },
        );

        let cfg: Config = load(&toml_bytes).unwrap();

        group.bench_with_input(BenchmarkId::new("validate_only", fx), &cfg, |b, c| {
            b.iter(|| {
                let mut local = c.clone();
                let _ = validate(&mut local);
            });
        });

        group.bench_with_input(BenchmarkId::new("allocate", fx), &cfg, |b, c| {
            b.iter(|| {
                let _ = allocate(c, 32000);
            });
        });

        if let Some(sels) = load_selections(fx) {
            group.bench_with_input(BenchmarkId::new("merge_paths", fx), &sels, |b, s| {
                b.iter(|| {
                    let _ = merge_paths(s);
                });
            });
        }
    }
    group.bench_function("shell_split", |b| {
        b.iter(|| {
            let _ = shell_split(sample_source).unwrap();
        });
    });
    group.finish();
}

criterion_group!(benches, bench_braid);
criterion_main!(benches);
