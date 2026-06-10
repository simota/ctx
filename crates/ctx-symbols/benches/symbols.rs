// crates/ctx-symbols/benches/symbols.rs — criterion harness.

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use ctx_symbols::testing::{make_request, synthetic_corpus};
use ctx_symbols::{render_api, resolve, LookupArgs, LookupSession};

fn bench_apionly_render_small(c: &mut Criterion) {
    let lines: Vec<&str> = (0..50)
        .map(|_| "// LoginUser authenticates a user.")
        .collect();
    let mut owned: Vec<String> = lines.iter().map(|s| (*s).to_string()).collect();
    owned.push("func LoginUser(ctx context.Context) (*Session, error) {".to_string());
    let ls: Vec<&str> = owned.iter().map(|s| s.as_str()).collect();
    let req = make_request(&ls, vec![(0, ls.len() as i32 - 1, None)]);
    c.bench_function("apionly_render_small_50lines", |b| {
        b.iter(|| {
            let s = render_api(black_box(&req));
            black_box(s);
        });
    });
}

fn bench_apionly_render_medium(c: &mut Criterion) {
    let mut lines: Vec<String> = Vec::with_capacity(500);
    let mut ranges = Vec::new();
    for i in 0..50 {
        let base = lines.len() as i32;
        lines.push(format!("// Doc {i}"));
        lines.push(format!("func F{i}() {{}}"));
        lines.push(String::new());
        ranges.push((base, base + 1, None));
    }
    let ls: Vec<&str> = lines.iter().map(|s| s.as_str()).collect();
    let req = make_request(&ls, ranges);
    c.bench_function("apionly_render_medium_50ranges", |b| {
        b.iter(|| {
            let s = render_api(black_box(&req));
            black_box(s);
        });
    });
}

fn bench_lookup_stateless_small(c: &mut Criterion) {
    let corpus = synthetic_corpus(20, 10);
    let args = LookupArgs {
        name: "BuildIndex".to_string(),
        ..Default::default()
    };
    c.bench_function("lookup_stateless_small_20files", |b| {
        b.iter(|| {
            let r = resolve(black_box(&corpus), black_box(&args));
            black_box(r);
        });
    });
}

fn bench_lookup_stateless_medium(c: &mut Criterion) {
    let corpus = synthetic_corpus(200, 25);
    let args = LookupArgs {
        name: "BuildIndex".to_string(),
        from: "internal/pkg5/file50.go".to_string(),
        kind: String::new(),
    };
    c.bench_function("lookup_stateless_medium_200files", |b| {
        b.iter(|| {
            let r = resolve(black_box(&corpus), black_box(&args));
            black_box(r);
        });
    });
}

fn bench_lookup_stateless_large(c: &mut Criterion) {
    let corpus = synthetic_corpus(2000, 25);
    let args = LookupArgs {
        name: "BuildIndex".to_string(),
        ..Default::default()
    };
    c.bench_function("lookup_stateless_large_2000files", |b| {
        b.iter(|| {
            let r = resolve(black_box(&corpus), black_box(&args));
            black_box(r);
        });
    });
}

fn bench_lookup_sessioned_amortise(c: &mut Criterion) {
    let corpus = synthetic_corpus(2000, 25);
    let session = LookupSession::open("/repo", corpus);
    let args = LookupArgs {
        name: "BuildIndex".to_string(),
        ..Default::default()
    };
    c.bench_function("lookup_sessioned_query_amortised", |b| {
        b.iter(|| {
            let r = session.resolve(black_box(&args));
            black_box(r);
        });
    });
}

criterion_group!(
    symbols,
    bench_apionly_render_small,
    bench_apionly_render_medium,
    bench_lookup_stateless_small,
    bench_lookup_stateless_medium,
    bench_lookup_stateless_large,
    bench_lookup_sessioned_amortise,
);
criterion_main!(symbols);
