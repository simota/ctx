// crates/ctx-relations/benches/sticky_handle.rs
//
// Rust-only intrinsic bench: measures the per-query overhead of the
// sticky-handle session against the stateless build_cached path,
// excluding cgo crossing cost. This is the floor of how fast the FFI
// session API can theoretically be served — any gap between this and
// the Go-side BenchmarkRelationsEdges_Sessioned numbers is cgo +
// JSON-marshal overhead.
//
// Run:
//   cargo bench --bench sticky_handle --manifest-path crates/ctx-relations/Cargo.toml

use std::path::PathBuf;

use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};

use ctx_relations::build::build_cached;
use ctx_relations::session::RelationsSession;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("relations-fixtures")
}

fn paths_for(fx: &str) -> &'static [&'static str] {
    match fx {
        "go_project" => &["main.go", "lib/a.go", "lib/b.go", "lib/sub/c.go"],
        "jsts_project" => &["src/main.ts", "src/foo.ts", "src/baz.ts", "src/bar.js"],
        "jvm_project" => &[
            "src/com/example/Main.kt",
            "src/com/example/A.java",
            "src/com/example/B.java",
            "src/com/example/util/Helper.java",
        ],
        "mixed_project" => &[
            "cmd/main.go",
            "internal/core/core.go",
            "web/index.ts",
            "web/greet.ts",
        ],
        _ => &[],
    }
}

fn bench_session_query_refs(c: &mut Criterion) {
    let root = fixtures_dir();
    let mut group = c.benchmark_group("session_query_refs");
    for fx in &["go_project", "jsts_project", "jvm_project", "mixed_project"] {
        let p = root.join(fx);
        if !p.exists() {
            continue;
        }
        let session = RelationsSession::open(&p.to_string_lossy()).unwrap();
        let paths = paths_for(fx);
        group.bench_with_input(BenchmarkId::new("rust_session", fx), &session, |b, sess| {
            let mut i = 0usize;
            b.iter(|| {
                let arg = format!("{{\"path\":\"{}\"}}", paths[i % paths.len()]);
                let _ = sess.query("refs", &arg).unwrap();
                i = i.wrapping_add(1);
            });
        });
    }
    group.finish();
}

fn bench_session_query_edges(c: &mut Criterion) {
    let root = fixtures_dir();
    let mut group = c.benchmark_group("session_query_edges");
    for fx in &["go_project", "jsts_project", "jvm_project", "mixed_project"] {
        let p = root.join(fx);
        if !p.exists() {
            continue;
        }
        let session = RelationsSession::open(&p.to_string_lossy()).unwrap();
        let paths = paths_for(fx);
        group.bench_with_input(BenchmarkId::new("rust_session", fx), &session, |b, sess| {
            let mut i = 0usize;
            b.iter(|| {
                let arg = format!("{{\"path\":\"{}\"}}", paths[i % paths.len()]);
                let _ = sess.query("edges", &arg).unwrap();
                i = i.wrapping_add(1);
            });
        });
    }
    group.finish();
}

fn bench_stateless_full_index(c: &mut Criterion) {
    let root = fixtures_dir();
    let mut group = c.benchmark_group("stateless_full_index");
    for fx in &["go_project", "jsts_project", "jvm_project", "mixed_project"] {
        let p = root.join(fx);
        if !p.exists() {
            continue;
        }
        let path = p.to_string_lossy().to_string();
        group.bench_with_input(
            BenchmarkId::new("rust_build_cached", fx),
            &path,
            |b, path| {
                b.iter(|| {
                    let idx = build_cached(path).unwrap();
                    // Touch a field so the optimiser doesn't elide.
                    let _ = idx.imports.len();
                });
            },
        );
    }
    group.finish();
}

criterion_group!(
    benches,
    bench_session_query_refs,
    bench_session_query_edges,
    bench_stateless_full_index
);
criterion_main!(benches);
