// crates/ctx-where/benches/sticky_handle.rs
//
// ADR-002 sticky-handle PoC bench (Rust-only, no cgo).
//
// Compares:
//
//   * search/sticky-rust-only/<fixture>
//       Loads files.json ONCE, then runs N queries via a pre-built
//       Vec<FileInput>. This is the in-process equivalent of the
//       sticky-handle session — it tells us the intrinsic per-query
//       cost when the corpus is already resident.
//
//   * search/stateless-rust-only/<fixture>
//       For each iteration, re-parses files.json AND calls
//       search_with_options. This mirrors the cost the Phase 3
//       cgo path pays on every call (minus cgo crossing itself).
//
// Run:
//
//   cargo bench --bench sticky_handle \
//       --manifest-path crates/ctx-where/Cargo.toml

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

fn load_fixture(name: &str) -> Option<(Vec<u8>, Vec<FileInput>, Vec<String>)> {
    let dir = fixtures_dir().join(name);
    let raw = std::fs::read(dir.join("files.json")).ok()?;
    let files: Vec<FileInput> = serde_json::from_slice(&raw).ok()?;
    let qtxt = std::fs::read_to_string(dir.join("query.txt")).ok()?;
    let main_query = qtxt.trim().to_string();
    // Rotate a small bouquet so we exercise different scoring paths.
    let queries = vec![
        main_query,
        "session save".into(),
        "score breakdown".into(),
        "keyword extract".into(),
    ];
    Some((raw, files, queries))
}

fn bench_sticky(c: &mut Criterion) {
    let fixtures = ["small_repo", "medium_repo", "large_repo"];
    let mut group = c.benchmark_group("search");
    for fx in &fixtures {
        let Some((raw, files, queries)) = load_fixture(fx) else { continue };
        let n_files = files.len() as u64;
        group.throughput(Throughput::Elements(n_files.max(1)));

        // Sticky: corpus parsed once outside the timing loop.
        group.bench_with_input(
            BenchmarkId::new("sticky-rust-only", fx),
            &(files.clone(), queries.clone()),
            |b, (files, queries)| {
                let opts = Options::default();
                let mut i: usize = 0;
                b.iter(|| {
                    let q = &queries[i % queries.len()];
                    i += 1;
                    search_with_options(files, q, &opts)
                })
            },
        );

        // Stateless: corpus is re-parsed every iteration (this is the
        // closest in-process proxy for the Phase 3 per-call cgo path).
        group.bench_with_input(
            BenchmarkId::new("stateless-rust-only", fx),
            &(raw, queries),
            |b, (raw, queries)| {
                let opts = Options::default();
                let mut i: usize = 0;
                b.iter(|| {
                    let q = &queries[i % queries.len()];
                    i += 1;
                    let files: Vec<FileInput> =
                        serde_json::from_slice(raw).expect("re-parse fixture");
                    search_with_options(&files, q, &opts)
                })
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_sticky);
criterion_main!(benches);
