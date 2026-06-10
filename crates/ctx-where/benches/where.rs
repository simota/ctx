// crates/ctx-where/benches/where.rs
//
// Criterion benches for the Phase 3 where port. Mirrors the layout of
// crates/ctx-relations/benches/relations.rs.
//
// Run:
//   cargo bench --bench where --manifest-path crates/ctx-where/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_where::search::{search_with_options, FileInput, Options};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("where-fixtures")
}

fn load_fixture(name: &str) -> Option<(Vec<FileInput>, String)> {
    let dir = fixtures_dir().join(name);
    let raw = std::fs::read(dir.join("files.json")).ok()?;
    let files: Vec<FileInput> = serde_json::from_slice(&raw).ok()?;
    let query = std::fs::read_to_string(dir.join("query.txt")).ok()?;
    Some((files, query.trim().to_string()))
}

fn bench_search(c: &mut Criterion) {
    let fixtures = ["small_repo", "medium_repo", "large_repo"];
    let mut group = c.benchmark_group("search");
    for fx in &fixtures {
        let Some((files, query)) = load_fixture(fx) else { continue };
        let n = files.len() as u64;
        group.throughput(Throughput::Elements(n.max(1)));
        group.bench_with_input(
            BenchmarkId::new("rust", fx),
            &(files, query),
            |b, (files, query)| {
                b.iter(|| {
                    search_with_options(files, query, &Options::default())
                })
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_search);
criterion_main!(benches);
