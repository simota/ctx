// crates/ctx-focus/benches/memory.rs
//
// dhat-rs memory profiler bench. Phase 2 lesson #5 baked in from day 1.
//
// Run:
//   cargo bench --features dhat --bench memory \
//     --manifest-path crates/ctx-focus/Cargo.toml

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::path::PathBuf;

use ctx_focus::{expand, resolve_anchor, types::ExpandOptions, FileInput};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("focus-fixtures")
}

fn main() {
    #[cfg(feature = "dhat")]
    {
        let out_path =
            std::env::var("CTX_DHAT_OUT").unwrap_or_else(|_| "/tmp/focus-dhat.json".to_string());
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
        eprintln!("fixtures missing — generate via cmd/focus-golden-export");
        return;
    };
    let files: Vec<FileInput> = serde_json::from_slice(&raw).expect("files.json");
    let anchor_raw =
        std::fs::read_to_string(dir.join("anchor.txt")).unwrap_or_else(|_| "Pack".into());
    let anchor_str = anchor_raw.trim();
    for _ in 0..200 {
        if let Ok(a) = resolve_anchor(&files, anchor_str) {
            let _ = expand(&files, &a, &ExpandOptions { hops: 2 });
        }
    }
}
