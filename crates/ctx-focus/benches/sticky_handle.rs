// crates/ctx-focus/benches/sticky_handle.rs
//
// ADR-002 sticky-handle PoC bench for focus (Rust-only, no cgo).
//
// Compares:
//
//   * focus/sticky-rust-only/<fixture>
//       Loads files.json ONCE, then runs N (resolve + expand) calls
//       against a pre-built Vec<FileInput>. This is the in-process
//       equivalent of the sticky-handle session.
//
//   * focus/stateless-rust-only/<fixture>
//       For each iteration, re-parses files.json AND calls pack().
//       Mirrors the cost the Phase 3 cgo path pays on every call.

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};

use ctx_focus::{expand, pack, resolve_anchor, types::ExpandOptions, FileInput};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("focus-fixtures")
}

fn load_fixture(name: &str) -> Option<(Vec<u8>, Vec<FileInput>, Vec<String>)> {
    let dir = fixtures_dir().join(name);
    let raw = std::fs::read(dir.join("files.json")).ok()?;
    let files: Vec<FileInput> = serde_json::from_slice(&raw).ok()?;
    let anchor = std::fs::read_to_string(dir.join("anchor.txt")).ok()?;
    let main_anchor = anchor.trim().to_string();
    // Rotate a few targets so we exercise different resolution paths.
    let anchors = vec![main_anchor.clone(), "helper".into(), "Pack".into()];
    Some((raw, files, anchors))
}

fn bench_sticky(c: &mut Criterion) {
    let fixtures = ["small_repo", "medium_repo", "large_repo"];
    let mut group = c.benchmark_group("focus");
    for fx in &fixtures {
        let Some((raw, files, anchors)) = load_fixture(fx) else { continue };
        let n_files = files.len() as u64;
        group.throughput(Throughput::Elements(n_files.max(1)));

        // Sticky: corpus parsed once outside the timing loop.
        group.bench_with_input(
            BenchmarkId::new("sticky-rust-only", fx),
            &(files.clone(), anchors.clone()),
            |b, (files, anchors)| {
                let opts = ExpandOptions { hops: 2 };
                let mut i: usize = 0;
                b.iter(|| {
                    let q = &anchors[i % anchors.len()];
                    i += 1;
                    if let Ok(a) = resolve_anchor(files, q) {
                        let _ = expand(files, &a, &opts);
                    }
                })
            },
        );

        // Stateless: corpus is re-parsed every iteration.
        group.bench_with_input(
            BenchmarkId::new("stateless-rust-only", fx),
            &(raw, anchors),
            |b, (raw, anchors)| {
                let opts = ExpandOptions { hops: 2 };
                let mut i: usize = 0;
                b.iter(|| {
                    let q = &anchors[i % anchors.len()];
                    i += 1;
                    let files: Vec<FileInput> =
                        serde_json::from_slice(raw).expect("re-parse fixture");
                    let _ = pack(&files, q, &opts);
                })
            },
        );
    }
    group.finish();
}

criterion_group!(benches, bench_sticky);
criterion_main!(benches);
