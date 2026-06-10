// crates/ctx-echo/benches/echo.rs
//
// Criterion benchmarks for the ctx-echo Evaluate hot path. Fixtures are
// loaded from tests/echo-fixtures/ at the repo root so this harness
// exercises byte-identical inputs to the Go testing.B harness in
// internal/echo/echo_bench_test.go.
//
// Run from the repo root:
//   cargo bench --manifest-path crates/ctx-echo/Cargo.toml
//
// See tests/ECHO_BENCH_REPORT.md for the cross-language comparison.

use std::fs;
use std::path::{Path, PathBuf};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_echo::evaluate;
use ctx_echo::types::Options;

fn bench_inputs_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("go.mod").exists() {
            return dir.join("tests").join("echo-fixtures");
        }
        if !dir.pop() {
            panic!("repo root not found above {}", env!("CARGO_MANIFEST_DIR"));
        }
    }
}

fn must_read(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn bench_evaluate(c: &mut Criterion) {
    let root = bench_inputs_root();
    let cases = [
        ("small", "small_pack.md"),
        ("medium", "medium_pack.md"),
        ("large", "large_pack.md"),
    ];
    let mut group = c.benchmark_group("Evaluate");
    for (name, file) in cases.iter() {
        let path = root.join(file);
        let body = must_read(&path);
        group.throughput(Throughput::Bytes(body.len() as u64));
        let opts = Options {
            goal: "rate limit burst handler".to_string(),
            top: 10,
            ..Default::default()
        };
        group.bench_with_input(BenchmarkId::from_parameter(name), &body, |b, body| {
            b.iter(|| {
                let res = evaluate(black_box("inline"), black_box(body), black_box(&opts));
                assert!(res.chunks_total >= 0);
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_evaluate);
criterion_main!(benches);
