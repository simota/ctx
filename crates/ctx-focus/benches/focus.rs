// crates/ctx-focus/benches/focus.rs
//
// Criterion bench for the Phase 4 focus port. Measures resolve + expand
// against the pre-walked fixtures (in-process; no cgo).
//
// Run:
//   cargo bench --bench focus --manifest-path crates/ctx-focus/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_focus::{expand, resolve_anchor, types::ExpandOptions, FileInput};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("focus-fixtures")
}

fn load_fixture(name: &str) -> Option<(Vec<FileInput>, String)> {
    let dir = fixtures_dir().join(name);
    let raw = std::fs::read(dir.join("files.json")).ok()?;
    let files: Vec<FileInput> = serde_json::from_slice(&raw).ok()?;
    let anchor = std::fs::read_to_string(dir.join("anchor.txt")).ok()?;
    Some((files, anchor.trim().to_string()))
}

fn bench_focus(c: &mut Criterion) {
    let fixtures = ["small_repo", "medium_repo", "large_repo"];
    let mut group = c.benchmark_group("focus");
    for fx in &fixtures {
        let Some((files, anchor)) = load_fixture(fx) else {
            continue;
        };
        let n = files.len() as u64;
        group.throughput(Throughput::Elements(n.max(1)));

        group.bench_with_input(
            BenchmarkId::new("resolve", fx),
            &(files.clone(), anchor.clone()),
            |b, (files, anchor)| {
                b.iter(|| {
                    let _ = resolve_anchor(files, anchor);
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("expand_hops1", fx),
            &(files.clone(), anchor.clone()),
            |b, (files, anchor)| {
                let a = resolve_anchor(files, anchor).expect("resolve");
                b.iter(|| {
                    let _ = expand(files, &a, &ExpandOptions { hops: 1 });
                })
            },
        );
        group.bench_with_input(
            BenchmarkId::new("expand_hops2", fx),
            &(files, anchor),
            |b, (files, anchor)| {
                let a = resolve_anchor(files, anchor).expect("resolve");
                b.iter(|| {
                    let _ = expand(files, &a, &ExpandOptions { hops: 2 });
                })
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_focus);
criterion_main!(benches);
