// crates/ctx-heatmap/src/lib.rs
//
// Rust port of internal/heatmap/. Phase 4 Tier 1 #2 BATCH-style shipper.
//
//   types.rs       — Bucket, Rect, FileMetric, AggregateOptions,
//                    AsciiOptions, JsonOptions, PlainOptions
//   aggregate.rs   — Aggregate, TopN, Total*, format_number,
//                    truncate_path, weight_for
//   squarify.rs    — Bruls/Huijsen/van Wijk squarified treemap layout
//   render/        — ascii / json / plain renderers, byte-exact with Go
//   ffi.rs         — extern "C" stateless surface (Option B from the
//                    Tier 1 #2 brief).
//
// API SHAPE CHOICE (Option B — stateless):
//
// internal/heatmap is invoked exactly once per `ctx map` command —
// aggregate → squarify → render, end. Unlike ctx-focus / ctx-where
// (which serve 3 callers × N anchors per session), heatmap has 1
// caller × 1 invocation; the sticky-handle pattern's amortisation
// curve does not earn its complexity here. We expose a stateless
// batch API: each FFI entry point owns its own decode + work, no
// session lifetime to track.
//
// The bench bar is therefore the BATCH ≥1.5× target (vs the sticky-
// handle ≥5× Tier 1 bar), as called out explicitly in the campaign
// brief.

pub mod aggregate;
pub mod ffi;
pub mod render;
pub mod squarify;
pub mod types;

#[cfg(any(test, feature = "testing"))]
pub mod testing;

pub use crate::aggregate::{aggregate, format_number, top_n, total, total_tokens, truncate_path};
pub use crate::render::ascii::render_ascii;
pub use crate::render::json::render_json;
pub use crate::render::plain::render_plain;
pub use crate::render::svg::{render_svg, SvgOptions};
pub use crate::squarify::squarify;
pub use crate::types::{
    AggregateOptions, AsciiOptions, Bucket, FileMetric, JsonOptions, PlainOptions, Rect,
};
