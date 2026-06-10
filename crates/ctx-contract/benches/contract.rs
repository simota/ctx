// crates/ctx-contract/benches/contract.rs
//
// Criterion benchmarks for the three contract hot paths: ExtractReferences,
// Verify, and ParseFromPack. Fixtures are loaded from tests/bench-inputs/
// at the repo root so this harness exercises byte-identical inputs to the
// Go testing.B harness in internal/contract/contract_bench_test.go.
//
// Run from the repo root:
//
//   cargo bench --manifest-path crates/ctx-contract/Cargo.toml
//
// See tests/BENCH_REPORT.md for the cross-language comparison.

use std::fs;
use std::path::{Path, PathBuf};

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_contract::embed::parse_from_pack;
use ctx_contract::parse_refs::extract_references;
use ctx_contract::types::{Contract, VerifyOptions};
use ctx_contract::verify::verify;

/// Walk up from the crate manifest dir to find the repo root (the
/// directory containing go.mod) so we can resolve tests/bench-inputs/
/// regardless of where cargo is invoked from.
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

// ---------------------------------------------------------------------
// ExtractReferences
// ---------------------------------------------------------------------

fn bench_extract_references(c: &mut Criterion) {
    let root = bench_inputs_root();
    let cases = [
        ("small", "extract_small.txt"),
        ("medium", "extract_medium.txt"),
        ("large", "extract_large.txt"),
    ];
    let mut group = c.benchmark_group("ExtractReferences");
    for (name, file) in cases.iter() {
        let data = must_read(&root.join(file));
        group.throughput(Throughput::Bytes(data.len() as u64));
        group.bench_with_input(BenchmarkId::from_parameter(name), &data, |b, data| {
            b.iter(|| {
                let refs = extract_references(black_box(data));
                assert!(!refs.is_empty());
            });
        });
    }
    group.finish();
}

// ---------------------------------------------------------------------
// Verify
// ---------------------------------------------------------------------

fn bench_verify(c: &mut Criterion) {
    let root = bench_inputs_root();
    let contract_raw = must_read(&root.join("verify_contract.json"));
    let response = must_read(&root.join("verify_response.txt"));
    let contract: Contract =
        serde_json::from_slice(&contract_raw).expect("decode verify_contract.json");

    let mut group = c.benchmark_group("Verify");
    group.throughput(Throughput::Bytes(response.len() as u64));

    group.bench_function("default", |b| {
        let opts = VerifyOptions::default();
        b.iter(|| {
            let res = verify(black_box(&contract), black_box(&response), black_box(&opts));
            assert_ne!(res.schema_version, 0);
        });
    });

    group.bench_function("strict", |b| {
        let opts = VerifyOptions {
            strict: true,
            ..Default::default()
        };
        b.iter(|| {
            let res = verify(black_box(&contract), black_box(&response), black_box(&opts));
            assert_ne!(res.schema_version, 0);
        });
    });

    group.finish();
}

// ---------------------------------------------------------------------
// ParseFromPack
// ---------------------------------------------------------------------

fn bench_parse_from_pack(c: &mut Criterion) {
    let root = bench_inputs_root();
    let md = must_read(&root.join("parse_md.txt"));
    let js = must_read(&root.join("parse_json.json"));

    let mut group = c.benchmark_group("ParseFromPack");

    group.throughput(Throughput::Bytes(md.len() as u64));
    group.bench_function("markdown", |b| {
        b.iter(|| {
            let c = parse_from_pack(black_box(&md)).expect("markdown parse");
            assert_ne!(c.schema_version, 0);
        });
    });

    group.throughput(Throughput::Bytes(js.len() as u64));
    group.bench_function("json", |b| {
        b.iter(|| {
            let c = parse_from_pack(black_box(&js)).expect("json parse");
            assert_ne!(c.schema_version, 0);
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_extract_references,
    bench_verify,
    bench_parse_from_pack
);
criterion_main!(benches);
