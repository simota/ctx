// crates/ctx-relations/benches/memory.rs
//
// dhat-rs memory profiler bench (Phase 1 lesson #5).
//
// Run:
//   cargo bench --features dhat --bench memory \
//     --manifest-path crates/ctx-relations/Cargo.toml
//
// On exit, dhat writes /tmp/relations-dhat.json which the operator
// reads to obtain max heap + total allocations. The summary printed
// to stderr also includes the high-water mark.
//
// NOTE: this is a "bench" only in the cargo-build sense — it is a
// single-shot profile, not a criterion harness. Criterion is incompatible
// with dhat because both want to be the global allocator.

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::path::PathBuf;

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("relations-fixtures")
}

fn main() {
    #[cfg(feature = "dhat")]
    {
        let out_path = std::env::var("CTX_DHAT_OUT")
            .unwrap_or_else(|_| "/tmp/relations-dhat.json".to_string());
        let _profiler = dhat::Profiler::builder()
            .file_name(&out_path)
            .build();
        run_workload();
        // Profiler writes on drop.
        eprintln!("dhat profile written to {out_path}");
    }
    #[cfg(not(feature = "dhat"))]
    {
        eprintln!("rebuild with --features dhat to enable instrumentation");
        run_workload();
    }
}

fn run_workload() {
    let root = fixtures_dir();
    let medium = root.join("mixed_project");
    let path = medium.to_string_lossy().to_string();
    // Run Build() a fixed number of times so the profile captures both
    // the steady-state allocations and the first-pass setup costs.
    for _ in 0..200 {
        let _ = ctx_relations::build::build(&path).expect("build");
    }
}
