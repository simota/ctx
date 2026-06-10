// crates/ctx-pack/benches/memory.rs
//
// dhat-rs heap profile harness. Run via:
//
//   cargo +stable bench --features dhat --bench memory
//
// Produces dhat-heap.json next to target/criterion which can be
// inspected with `dh_view.html`. The bench compares peak/total
// allocations between the stateless and sessioned relevance paths;
// the sessioned path should win by re-using the keyword cache.

#[cfg(feature = "dhat")]
use dhat::HeapStats;

use criterion::{black_box, criterion_group, criterion_main, Criterion};

use ctx_pack::relevance::session::RelevanceSession;
use ctx_pack::relevance::score_relevance;
use ctx_pack::testing::synth_corpus;

#[cfg(feature = "dhat")]
fn print_stats(prefix: &str) {
    let s = HeapStats::get();
    eprintln!(
        "{prefix}: total_blocks={} total_bytes={} max_blocks={} max_bytes={}",
        s.total_blocks, s.total_bytes, s.max_blocks, s.max_bytes
    );
}

fn bench_memory(c: &mut Criterion) {
    #[cfg(feature = "dhat")]
    let _profiler = dhat::Profiler::new_heap();

    let files = synth_corpus(2048);
    let goal = "ログイン認証";
    let budget: i64 = 50000;
    let mut group = c.benchmark_group("memory");

    group.bench_function("stateless_score_2048", |b| {
        b.iter(|| {
            let mut s: i64 = 0;
            for fi in &files {
                let r = score_relevance(black_box(fi), goal, fi.tokens, budget);
                s += r.score;
            }
            black_box(s)
        });
    });

    let session = RelevanceSession::new(goal, budget);
    group.bench_function("session_score_2048", |b| {
        b.iter(|| {
            let mut s: i64 = 0;
            for fi in &files {
                let r = session.score_file(black_box(fi), fi.tokens);
                s += r.score;
            }
            black_box(s)
        });
    });

    group.finish();
    #[cfg(feature = "dhat")]
    print_stats("final");
}

criterion_group!(benches, bench_memory);
criterion_main!(benches);
