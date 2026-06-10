// crates/ctx-scan/benches/scan.rs
//
// Criterion benchmarks for the scan hot path. Fixtures are loaded from
// tests/bench-inputs/ at the repo root so the Rust harness exercises
// byte-identical inputs to the Go testing.B harness in
// internal/scan/scan_bench_test.go.
//
// Run from the repo root:
//
//   cargo bench --manifest-path crates/ctx-scan/Cargo.toml
//
// See tests/SCAN_BENCH_REPORT.md for the cross-language comparison.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_scan::scan::scan_file_with_options;
use ctx_scan::types::Options;

fn bench_inputs_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join("go.mod").exists() {
            return dir.join("tests").join("bench-inputs");
        }
        if !dir.pop() {
            panic!("repo root not found above {}", env!("CARGO_MANIFEST_DIR"));
        }
    }
}

fn must_read(path: &Path) -> Vec<u8> {
    fs::read(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

/// scan_file expects a real path on disk. The bench fixture files live
/// next to the contract bench files (tests/bench-inputs/scan_*.txt).
/// We copy each fixture into a tmpfile so the file system caches are
/// warm before iter() — criterion's `iter` runs many iterations, and
/// reusing the same on-disk file would re-amortise the read; both
/// languages amortise the same way so the comparison stays fair.
fn write_tmp_for_bench(name: &str, body: &[u8]) -> PathBuf {
    let mut p = std::env::temp_dir();
    let pid = std::process::id();
    p.push(format!("ctx-scan-bench-{pid}-{name}"));
    let mut f = fs::File::create(&p).unwrap();
    f.write_all(body).unwrap();
    p
}

fn bench_scan_file(c: &mut Criterion) {
    let root = bench_inputs_root();
    let cases = [
        ("small", "scan_small.txt"),
        ("medium", "scan_medium.txt"),
        ("large", "scan_large.txt"),
    ];
    let mut group = c.benchmark_group("ScanFile");
    let opts = Options::default();
    for (name, file) in cases.iter() {
        let data = must_read(&root.join(file));
        let tmp = write_tmp_for_bench(name, &data);
        let path = tmp.to_string_lossy().into_owned();
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &path, |b, path| {
            b.iter(|| {
                let warnings = scan_file_with_options(black_box(path), &opts).unwrap();
                black_box(warnings);
            });
        });
    }
    group.finish();
}

fn bench_scan_file_entropy(c: &mut Criterion) {
    let root = bench_inputs_root();
    let data = must_read(&root.join("scan_medium.txt"));
    let tmp = write_tmp_for_bench("entropy-medium", &data);
    let path = tmp.to_string_lossy().into_owned();
    let opts = Options {
        enable_entropy: true,
        ..Default::default()
    };
    let mut group = c.benchmark_group("ScanFileEntropy");
    group.throughput(Throughput::Bytes(data.len() as u64));
    group.bench_function("medium", |b| {
        b.iter(|| {
            let w = scan_file_with_options(black_box(&path), &opts).unwrap();
            black_box(w);
        });
    });
    group.finish();
}

criterion_group!(benches, bench_scan_file, bench_scan_file_entropy);
criterion_main!(benches);
