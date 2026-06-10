// crates/ctx-heatmap/benches/memory.rs
//
// dhat-rs memory profiler bench for ctx-heatmap.
//
// Run:
//   cargo bench --features dhat --bench memory \
//     --manifest-path crates/ctx-heatmap/Cargo.toml

#[cfg(feature = "dhat")]
#[global_allocator]
static ALLOC: dhat::Alloc = dhat::Alloc;

use std::path::PathBuf;

use ctx_heatmap::{
    aggregate, render_ascii, render_json, render_plain, squarify, AggregateOptions,
    AsciiOptions, FileMetric, JsonOptions, PlainOptions,
};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("heatmap-fixtures")
}

fn main() {
    #[cfg(feature = "dhat")]
    {
        let out_path = std::env::var("CTX_DHAT_OUT")
            .unwrap_or_else(|_| "/tmp/heatmap-dhat.json".to_string());
        let _profiler = dhat::Profiler::builder()
            .file_name(&out_path)
            .build();
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
    let dir = fixtures_dir().join("medium_metrics");
    let Ok(raw) = std::fs::read(dir.join("metrics.json")) else {
        eprintln!("fixtures missing — generate via cmd/heatmap-golden-export");
        return;
    };
    let metrics: Vec<FileMetric> = serde_json::from_slice(&raw).expect("metrics.json");
    let agg_opts = AggregateOptions {
        by: "tokens".into(),
        depth: 2,
        top: 0,
    };
    let ascii_opts = AsciiOptions::default();
    let json_opts = JsonOptions {
        root: ".".into(),
        by: "tokens".into(),
        budget: None,
    };
    let plain_opts = PlainOptions {
        root: ".".into(),
        by: "tokens".into(),
        budget: 0,
    };
    for _ in 0..200 {
        let buckets = aggregate(&metrics, &agg_opts);
        let rects = squarify(&buckets, 80, 20);
        let _ = render_ascii(&rects, &ascii_opts);
        let _ = render_json(&rects, &json_opts);
        let _ = render_plain(&buckets, &plain_opts);
    }
}
