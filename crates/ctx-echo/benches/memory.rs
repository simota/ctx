// crates/ctx-echo/benches/memory.rs
//
// dhat-heap memory profile for ctx-echo. Enabled only under the
// dhat-heap feature so production builds skip the allocator hook.
//
// Run from the repo root:
//   cargo bench --manifest-path crates/ctx-echo/Cargo.toml \
//               --features dhat-heap --bench memory
//
// Output: a dhat-heap.json + summary stats on stderr — pair with the
// Go pprof allocs profile from internal/echo/echo_bench_test.go's
// AllocsPerOp() column to compute Δbytes/op + Δallocs/op for the
// memory bucket.

#[cfg(feature = "dhat-heap")]
use std::fs;
#[cfg(feature = "dhat-heap")]
use std::path::PathBuf;

#[cfg(feature = "dhat-heap")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

#[cfg(feature = "dhat-heap")]
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

#[cfg(feature = "dhat-heap")]
fn main() {
    let _profiler = dhat::Profiler::new_heap();
    let root = bench_inputs_root();
    let cases = [
        ("small", "small_pack.md"),
        ("medium", "medium_pack.md"),
        ("large", "large_pack.md"),
    ];
    let opts = ctx_echo::Options {
        goal: "rate limit burst handler".to_string(),
        top: 10,
        ..Default::default()
    };
    // One fixture per run (selectable via $CTX_ECHO_FIXTURE env), 50
    // reps so the alloc footprint amortises the dhat-init noise.
    // Default to "large".
    let fixture = std::env::var("CTX_ECHO_FIXTURE").unwrap_or_else(|_| "large".to_string());
    let (name, file) = cases
        .iter()
        .find(|(n, _)| *n == fixture)
        .copied()
        .unwrap_or(cases[2]);
    let body = fs::read_to_string(root.join(file)).expect("read fixture");
    let reps: usize = std::env::var("CTX_ECHO_REPS")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);
    for _ in 0..reps {
        let res = ctx_echo::evaluate("inline", &body, &opts);
        std::hint::black_box(res);
    }
    eprintln!(
        "memory: fixture={} bytes={} reps={}",
        name,
        body.len(),
        reps
    );
}

#[cfg(not(feature = "dhat-heap"))]
fn main() {
    eprintln!("memory bench requires --features dhat-heap");
}
