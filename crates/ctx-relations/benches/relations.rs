// crates/ctx-relations/benches/relations.rs
//
// Criterion benches for the Phase 2 relations port. Mirrors the layout
// of crates/ctx-scan/benches/scan.rs.
//
// Run:
//   cargo bench --bench relations --manifest-path crates/ctx-relations/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_relations::build::{build, build_cached, invalidate_cache};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("relations-fixtures")
}

fn bench_build_per_fixture(c: &mut Criterion) {
    let root = fixtures_dir();
    let fixtures = ["go_project", "jsts_project", "jvm_project", "mixed_project"];
    let mut group = c.benchmark_group("build");
    for fx in &fixtures {
        let p = root.join(fx);
        if !p.exists() {
            continue;
        }
        let path = p.to_string_lossy().to_string();
        // crude throughput proxy: rough source bytes under the fixture
        let bytes: u64 = walkdir_bytes(&p);
        group.throughput(Throughput::Bytes(bytes));
        group.bench_with_input(
            BenchmarkId::new("rust", fx),
            &path,
            |b, path| b.iter(|| build(path).unwrap()),
        );
    }
    group.finish();
}

fn bench_build_cached(c: &mut Criterion) {
    let root = fixtures_dir();
    let mut group = c.benchmark_group("build_cached");
    for fx in &["mixed_project"] {
        let p = root.join(fx);
        if !p.exists() {
            continue;
        }
        let path = p.to_string_lossy().to_string();
        group.bench_with_input(
            BenchmarkId::new("rust_first", fx),
            &path,
            |b, path| {
                b.iter(|| {
                    invalidate_cache(path);
                    build_cached(path).unwrap()
                })
            },
        );
        // hit path
        let _ = build_cached(&path).unwrap();
        group.bench_with_input(
            BenchmarkId::new("rust_hit", fx),
            &path,
            |b, path| b.iter(|| build_cached(path).unwrap()),
        );
    }
    group.finish();
}

fn walkdir_bytes(root: &std::path::Path) -> u64 {
    let mut total: u64 = 0;
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        if let Ok(entries) = std::fs::read_dir(&dir) {
            for entry in entries.flatten() {
                if let Ok(ft) = entry.file_type() {
                    if ft.is_dir() {
                        stack.push(entry.path());
                    } else if ft.is_file() {
                        if let Ok(meta) = entry.metadata() {
                            total += meta.len();
                        }
                    }
                }
            }
        }
    }
    total.max(1)
}

criterion_group!(benches, bench_build_per_fixture, bench_build_cached);
criterion_main!(benches);
