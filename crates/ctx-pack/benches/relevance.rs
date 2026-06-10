// crates/ctx-pack/benches/relevance.rs
//
// Criterion bench comparing:
//   * stateless score_relevance — re-extracts keywords each call
//   * session score_file        — keywords precomputed once
//   * session score_corpus      — batch over a pre-loaded corpus
//
// The session_score_corpus path is what the Go dispatcher's
// RelevancePool actually exercises in production; we expect it to
// dominate the stateless call for any N > 1.

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use ctx_pack::relevance::session::RelevanceSession;
use ctx_pack::relevance::{score_relevance, score_relevance_with_ctx, RelevanceContext};
use ctx_pack::types::{FileInput, MetadataInput, SymbolInput};

fn mkfile(path: &str, role: &str, syms: &[(&str, &str)]) -> FileInput {
    FileInput {
        path: path.into(),
        abs_path: String::new(),
        is_dir: false,
        tokens: 100,
        role: role.into(),
        metadata: MetadataInput {
            size: 100,
            tokens_est: 100,
            role: role.into(),
            symbols: syms
                .iter()
                .map(|(n, k)| SymbolInput {
                    name: (*n).into(),
                    kind: (*k).into(),
                    line: 1,
                })
                .collect(),
        },
        content_head: Vec::new(),
    }
}

fn corpus(n: usize) -> Vec<FileInput> {
    let mut out = Vec::with_capacity(n);
    let dirs = ["src/auth", "internal/pack", "internal/scan", "cmd/ctx", "docs"];
    let bases = ["login", "session", "config", "render", "diff", "preset"];
    let roles = ["core", "entry", "test", "doc", "config", "unknown"];
    for i in 0..n {
        let dir = dirs[i % dirs.len()];
        let base = bases[(i / dirs.len()) % bases.len()];
        let role = roles[i % roles.len()];
        let path = format!("{dir}/{base}_{i}.go");
        let syms = vec![("HandleLogin", "function"), ("ValidateSession", "function")];
        out.push(mkfile(&path, role, &syms));
    }
    out
}

fn bench_relevance(c: &mut Criterion) {
    let goal = "ログイン認証";
    let budget: i64 = 50000;

    let mut group = c.benchmark_group("relevance");
    for n in [1usize, 10, 100, 500, 2000].iter() {
        let files = corpus(*n);
        group.bench_function(format!("stateless_n{n}"), |b| {
            b.iter(|| {
                let mut s: i64 = 0;
                for fi in &files {
                    let r = score_relevance(black_box(fi), black_box(goal), fi.tokens, budget);
                    s += r.score;
                }
                black_box(s)
            });
        });

        let ctx = RelevanceContext::new(goal, budget);
        group.bench_function(format!("session_with_ctx_n{n}"), |b| {
            b.iter(|| {
                let mut s: i64 = 0;
                for fi in &files {
                    let r = score_relevance_with_ctx(black_box(fi), &ctx, fi.tokens);
                    s += r.score;
                }
                black_box(s)
            });
        });

        let tc: Vec<i64> = files.iter().map(|fi| fi.tokens).collect();
        let session = RelevanceSession::with_corpus(goal, budget, files.clone(), tc);
        group.bench_function(format!("session_corpus_n{n}"), |b| {
            b.iter(|| {
                let r = session.score_corpus();
                black_box(r.len())
            });
        });

        group.bench_function(format!("session_open_per_corpus_n{n}"), |b| {
            b.iter(|| {
                // Worst-case for the Go bridge: open a fresh session
                // per call. Approximates a degenerate pool that
                // recycles handles every invocation.
                let s = RelevanceSession::new(goal, budget);
                let mut total: i64 = 0;
                for fi in &files {
                    total += s.score_file(fi, fi.tokens).score;
                }
                black_box(total)
            });
        });
    }
    group.finish();
}

criterion_group!(benches, bench_relevance);
criterion_main!(benches);
