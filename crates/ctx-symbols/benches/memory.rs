// crates/ctx-symbols/benches/memory.rs — dhat heap profile.

#[cfg(feature = "dhat")]
use dhat::{Dhat, DhatAlloc};

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: DhatAlloc = DhatAlloc;

use ctx_symbols::testing::synthetic_corpus;
use ctx_symbols::{resolve, LookupArgs, LookupSession};

fn main() {
    #[cfg(feature = "dhat")]
    let _dhat = Dhat::start_heap_profiling();

    let corpus = synthetic_corpus(2000, 25);
    let args = LookupArgs {
        name: "BuildIndex".to_string(),
        ..Default::default()
    };

    let mut sink = 0usize;
    for _ in 0..200 {
        let r = resolve(&corpus, &args);
        sink = sink.wrapping_add(r.len());
    }

    let session = LookupSession::open("/repo", corpus);
    for _ in 0..200 {
        let r = session.resolve(&args);
        sink = sink.wrapping_add(r.len());
    }
    std::hint::black_box(sink);
}
