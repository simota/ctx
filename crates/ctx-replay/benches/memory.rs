// crates/ctx-replay/benches/memory.rs
//
// dhat-rs memory profiler bench. Phase 2 lesson #5 baked in.
//
// Run:
//   cargo bench --features dhat --bench memory \
//     --manifest-path crates/ctx-replay/Cargo.toml

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::path::PathBuf;

use ctx_replay::diff::{compute, DiffOptions};
use ctx_replay::types::Manifest;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("replay-fixtures")
}

fn main() {
    #[cfg(feature = "dhat")]
    {
        let out_path =
            std::env::var("CTX_DHAT_OUT").unwrap_or_else(|_| "/tmp/replay-dhat.json".to_string());
        let _profiler = dhat::Profiler::builder().file_name(&out_path).build();
        run_workload();
        eprintln!("dhat profile written to {out_path}");
    }
    #[cfg(not(feature = "dhat"))]
    {
        eprintln!("rebuild with --features dhat to enable instrumentation");
        run_workload();
    }
}

fn run_workload() {
    let dir = fixtures_dir().join("multi_snap_drift");
    let base = std::fs::read(dir.join("base.json")).unwrap_or_default();
    let cur = std::fs::read(dir.join("current.json")).unwrap_or_default();
    if base.is_empty() || cur.is_empty() {
        eprintln!("fixtures missing — generate via cmd/replay-golden-export");
        return;
    }
    let base: Manifest = serde_json::from_slice(&base).unwrap();
    let cur: Manifest = serde_json::from_slice(&cur).unwrap();
    for _ in 0..2_000 {
        let _ = compute(&base, &cur, DiffOptions::default());
    }
}
