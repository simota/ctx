// crates/ctx-braid/benches/memory.rs
//
// dhat-rs memory profiler bench for ctx-braid.
//
// Run:
//   cargo bench --features dhat --bench memory \
//     --manifest-path crates/ctx-braid/Cargo.toml

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::path::PathBuf;

use ctx_braid::{allocate, load, merge_paths, shell_split, StrandSelection};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("braid-fixtures")
}

fn main() {
    #[cfg(feature = "dhat")]
    {
        let out_path =
            std::env::var("CTX_DHAT_OUT").unwrap_or_else(|_| "/tmp/braid-dhat.json".to_string());
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
    let dir = fixtures_dir();
    let Ok(toml_bytes) = std::fs::read(dir.join("complex.toml")) else {
        eprintln!("fixtures missing — generate via cmd/braid-golden-export");
        return;
    };
    let sels_raw = std::fs::read(dir.join("complex_selections.json")).unwrap_or_default();
    let sels: Vec<StrandSelection> = serde_json::from_slice(&sels_raw).unwrap_or_default();
    let sample = "where 'handler' --regex 'router|Handler' --limit 50";

    for _ in 0..1000 {
        let cfg = load(&toml_bytes).unwrap();
        let _ = allocate(&cfg, 32000);
        let _ = merge_paths(&sels);
        let _ = shell_split(sample).unwrap();
    }
}
