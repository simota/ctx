// crates/ctx-where/benches/memory.rs
//
// dhat-rs memory profiler bench. Phase 2 lesson #5 baked in from day 1.
//
// Run:
//   cargo bench --features dhat --bench memory \
//     --manifest-path crates/ctx-where/Cargo.toml

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::path::PathBuf;

use ctx_where::search::{search_with_options, FileInput, Options};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("where-fixtures")
}

fn main() {
    #[cfg(feature = "dhat")]
    {
        let out_path =
            std::env::var("CTX_DHAT_OUT").unwrap_or_else(|_| "/tmp/where-dhat.json".to_string());
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
    let dir = fixtures_dir().join("medium_repo");
    let Ok(raw) = std::fs::read(dir.join("files.json")) else {
        eprintln!("fixtures missing — generate via cmd/where-golden-export");
        return;
    };
    let files: Vec<FileInput> = serde_json::from_slice(&raw).expect("files.json");
    let query = std::fs::read_to_string(dir.join("query.txt")).unwrap_or_else(|_| "user".into());
    let query = query.trim();
    for _ in 0..200 {
        let _ = search_with_options(&files, query, &Options::default());
    }
}
