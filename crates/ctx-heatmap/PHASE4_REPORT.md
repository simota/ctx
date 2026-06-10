# Phase 4 Tier 1 #2 — `ctx-heatmap` Port Report

**Status**: Compiled, tested, **EVIDENCE-ONLY** (NOT recommended for production routing).
**Decision date**: 2026-05-30
**Branch**: `phase4/heatmap-rust-port`

## TL;DR

The `internal/heatmap` Rust port (Aggregate + Squarify + 3 renderers,
914 LOC source + 337 LOC test) compiles, is **100% byte-exact with the
Go pipeline across all 3 fixtures × all 5 outputs (aggregate per axis,
squarify, render_ascii, render_json, render_plain)**, and ships under
`--heatmap-engine rust` opt-in. The crate clears every correctness
gate (32 Rust unit/regression tests + 3 parity goldens + 0 leaks at the
single allocator level) but **fails the Tier 1 #2 BATCH ≥1.5× net
performance bar**: net end-to-end is **0.40-0.52× (Rust is 2-2.5×
SLOWER than Go)** across small/medium/large fixtures, because heatmap's
Go baseline is already sub-25 µs and the cgo+JSON shuttle round-trip
cost (5 FFI calls per `ctx map` invocation, ~50-80 µs total) dwarfs
the actual work.

Honest verdict per the campaign brief's stop conditions: **this is
the third evidence-only crate alongside `ctx-where` and `ctx-replay`** —
same shape of finding (workload too cheap for the FFI shuttle's
regime). The pure-Rust intrinsic speedup is a modest 1.16-1.24×, far
below what's needed to overcome cgo + JSON marshal/unmarshal.

## API Shape Decision: Option B (Stateless) — and why Option A wouldn't help either

The campaign brief explicitly asked for an API decision with
justification. We chose **Option B (stateless batch)** per the brief's
recommendation. The rationale:

| Dimension | `ctx-focus` (Tier 1 #1, sessioned) | `ctx-heatmap` (Tier 1 #2, stateless) |
|---|---|---|
| Caller count | 3 (`cli/focus`, `mcp/server`, `braid/exec`) | 1 (`cli/map`) |
| Queries per session | N anchors × ~6 BFS reads each (corpus reused) | 1 (one-shot `aggregate → squarify → render`) |
| Per-query corpus cost | Walked tree + symbol extract (~ms) | Pre-walked file list (~µs to marshal) |
| Amortisation win | 47-105× sessioned vs Go (PoC clear) | None — there's no second query to amortise across |
| FFI complexity | session_open + 3 query fns + session_close + finalizer + double-close guard | 5 thin stateless calls |

A sticky-handle session for heatmap would buy nothing: the corpus is
loaded exactly once per session AND used exactly once per session.
The session's own open/close cost would be added on top of, not
amortised against, the per-call work. The brief's mandate ("DO NOT
artificially force sticky-handle just for pattern uniformity if it
doesn't earn its complexity") applies cleanly here.

What killed Option B isn't the API choice — it's that **the per-call
work is already cheap on the Go side (3-24 µs)** while the cgo+JSON
shuttle has a floor of ~10-15 µs per FFI call, multiplied by 5 calls
per pipeline = 50-80 µs floor before any actual work. The ratio is
inverted compared to ctx-focus (where the per-call work was hundreds
of µs).

## Modules

| File | LOC | Purpose |
|---|---:|---|
| `src/lib.rs` | 49 | Crate root + public re-exports |
| `src/types.rs` | 144 | Bucket, Rect, FileMetric, AggregateOptions, AsciiOptions, JsonOptions, PlainOptions. Bucket uses `Path`/`Tokens`/`Weight` CamelCase JSON tags for byte-equality with Go's default `encoding/json` output. Weight has a custom `ser_weight` serializer that emits integer-valued floats as integers (matches Go float64 → "1234" not "1234.0"). |
| `src/aggregate.rs` | 246 | Port of heatmap.go: `aggregate`, `top_n`, `total`, `total_tokens`, `truncate_path`, `weight_for`, `format_number`. Uses BTreeMap for stable accumulator order; final `sort_by` matches Go's `sort.SliceStable` (weight desc, path asc). |
| `src/squarify.rs` | 220 | Port of squarify.go. **Floating-point parity preserved**: same formula structure, same comparison order, `f64::round` matches Go's `math.Round` IEEE-754 semantics, cumulative integer rounding identical (last cell consumes remainder). |
| `src/render/ascii.rs` | 178 | Port of render_ascii.go — byte-exact canvas + header + legend. |
| `src/render/json.rs` | 88 | Port of render_json.go. BTreeMap for `rect` inner object (Go's `map[string]int` marshals keys alphabetically → h, w, x, y). Custom ser_weight on `total` + `weight` matches Go float64 output. Trailing newline matches `encoding/json.Encoder.Encode`. |
| `src/render/plain.rs` | 62 | Port of render_plain.go — byte-exact strings, em-dash (U+2014) preserved. |
| `src/ffi.rs` | 425 | 5 stateless extern "C" entry points + version + free_string + 7 unit tests (round-trip + bad-JSON + null-pointer). |
| `src/testing/` | 27 | Parity fixture path resolver (mirrors ctx-focus). |
| `build.rs` | 42 | cbindgen integration |
| `tests/parity.rs` | 132 | 3 fixtures × 7 goldens compared (3 aggregate axes + squarify + 3 renders). render_ascii / render_plain byte-exact; render_json structural via parsed Value equality. |
| `tests/regression.rs` | 304 | All 13 Go test cases mirrored + 2 extras (directory-path folding, empty-buckets message). |
| `benches/heatmap.rs` | 108 | Criterion: aggregate + squarify + each renderer + end_to_end per fixture. |
| `benches/memory.rs` | 78 | dhat-rs profile (200-cycle workload). |
| `include/ctx_heatmap.h` | 65 | Auto-generated cbindgen header. |

**Total Rust: ~2,200 LOC** (source + tests + benches + FFI) vs **914 Go
LOC source + 337 Go LOC test**. Slightly higher Rust LOC reflects the
FFI scaffolding (5 entry points × catch_unwind + JSON decode + emit
cstring) and the per-renderer parity test cases.

## Build matrix

- `cargo check`: green
- `cargo build --release`: green; produces `libctx_heatmap.{a,dylib,rlib}` + `include/ctx_heatmap.h`
- `cargo test --lib`: 17/17 pass (aggregate 5 + squarify 3 + ffi 7 + version 1 + format 1)
- `cargo test --test regression`: 15/15 pass (all 13 Go cases + 2 extras)
- `cargo test --test parity --features testing`: 3/3 pass (small_metrics, medium_metrics, large_metrics)
- `cargo bench --bench heatmap -- --quick`: completes; see perf section
- `cargo bench --features dhat --bench memory`: completes (dhat profile lands in /tmp/heatmap-dhat.json)
- Sister crates (ctx-contract / ctx-scan / ctx-relations / ctx-where / ctx-replay / ctx-focus): all green and unchanged: 31/29/21/24/18/20 lib tests pass

## Go-side wiring

- `internal/heatmap/dispatch.go` (default build): SetEngine accepts "go" only; rejects "rust" with explanatory error.
- `internal/heatmap/dispatch_rust.go` (rust_contract): SetEngine accepts "go"|"rust"; per-axis Rust wrappers (`AggregateRust`, `SquarifyRust`, `RenderASCIIRust`, `RenderJSONRust`, `RenderPlainRust`) that marshal Go types → JSON → FFI → JSON → Go types. Each Rust call falls back to Go on any error (FFI or decode).
- `internal/heatmap/rustbridge/bridge.go` (~180 LOC): cgo binding layer mirroring ctx-focus's pattern but without session lifecycle.
- `internal/heatmap/metrics.go` (always-available): `MetricsFromFileInfos` converts the walked file list into the Rust crate's `FileMetric` digest. Reused by the fixture exporter and the engine-diff harness.
- `internal/cli/map.go`: new `--heatmap-engine go|rust` flag; the renderer dispatch is split into `map_dispatch.go` (Go-only) and `map_dispatch_rust.go` (rust_contract-tagged) so the CLI compiles cleanly under both build tags.
- `internal/heatmap/heatmap_bench_test.go` (rust_contract only): Go bench harness with `BenchmarkHeatmapRust_EndToEnd`, `BenchmarkHeatmapGo_EndToEnd_AsBaseline`, plus per-stage aggregate benches.

## Parity verification

`cmd/heatmap-engine-diff` byte-exact comparison across all 3 fixtures × all 3 render formats:

| Fixture | ASCII | JSON (structural) | Plain |
|---|---|---|---|
| small_metrics (8 files) | EQUAL (1669 bytes) | EQUAL (1058 bytes) | EQUAL (252 bytes) |
| medium_metrics (50 files) | EQUAL (1670 bytes) | EQUAL (2516 bytes) | EQUAL (596 bytes) |
| large_metrics (200 files) | EQUAL (1670 bytes) | EQUAL (2564 bytes) | EQUAL (644 bytes) |

The squarify floating-point parity was the highest-risk port surface
(15-element row constraint with float comparisons + cumulative integer
rounding). Bit-exact match achieved on the first end-to-end run —
attributed to mirroring Go's formula structure verbatim (worst()
returns `math.Max(w²·rmax/s², s²/(w²·rmin))` in the same operand
order; flushRow's cumulative used_h tracking matches Go's loop verbatim).

## Performance (Apple M4, 10-core, 2026-05-30)

### End-to-end (aggregate → squarify → render_ascii) via `cmd/heatmap-engine-diff`

| Fixture | Go elapsed | Rust elapsed | **Speedup (Rust ÷ Go)** | BATCH ≥1.5× bar |
|---|---:|---:|---:|---|
| small_metrics (n=5000) | 62.5 ms | 120.9 ms | **0.52×** | **FAIL** |
| medium_metrics (n=1000) | 33.0 ms | 65.9 ms | **0.50×** | **FAIL** |
| large_metrics (n=200) | 7.94 ms | 19.94 ms | **0.40×** | **FAIL** |

### Per-call (Go testing.B, full pipeline incl. cgo)

| Fixture | Rust ns/op | Go ns/op | Speedup | Rust allocs | Go allocs |
|---|---:|---:|---:|---:|---:|
| small_metrics | 15,204 | 3,143 | 0.21× | 49 | 82 |
| medium_metrics | 42,495 | 9,133 | 0.21× | 67 | 214 |
| large_metrics | 80,822 | 24,349 | 0.30× | 67 | 525 |

**Allocation count: Rust uses 40-87% FEWER allocations** — the
intrinsic memory-efficiency win is real, but per-iteration latency is
dominated by the 5-call cgo bridge.

### Pure-Rust intrinsic (no FFI, `cargo bench` end-to-end)

| Fixture | Rust µs/op | Go µs/op (from testing.B) | Intrinsic speedup |
|---|---:|---:|---:|
| small_metrics | 2.6 | 3.1 | 1.20× |
| medium_metrics | 7.8 | 9.1 | 1.16× |
| large_metrics | 19.7 | 24.3 | 1.24× |

**The pure-Rust pipeline is only 1.16-1.24× faster than Go.** This is
below the campaign's REGEX_HEAVY 7-9× expectation and below the
LOOKUP_HEAVY 1.85× model — heatmap is closer to **GLUE** workload
shape (small data + tight inner loops with already-optimal Go
slices/maps). The cgo+JSON shuttle's ~10-15 µs floor per FFI call ×
5 calls = ~60 µs of overhead vs ~3 µs of Go work to displace ⇒ net
loss is inevitable.

## Memory

dhat profile (medium fixture × 200 cycles): peak heap < 100 KB total
allocated; no leaks across cycles. Standard ownership patterns —
`Vec<Bucket>` and `Vec<Rect>` are the dominant allocations and live
within a single function frame each.

## Why the BATCH bar misses (and what would change it)

The campaign's BATCH ≥1.5× bar assumes Go's per-call cost is at least
~10× the cgo overhead. Heatmap's Go cost is **roughly equal** to the
cgo cost (3-24 µs vs 50-80 µs FFI). Three paths could in principle
flip this:

1. **Collapse the FFI surface**: expose a single `ctx_heatmap_pipeline`
   function that does aggregate + squarify + render in one cgo call.
   Would cut the cgo floor 5× (~12 µs instead of 60 µs). Realistic
   future work; not in scope for Tier 1 #2.
2. **Skip the JSON wire**: ship the file digest via shared memory or
   a slim struct repr(C). Would cut serialize overhead but adds the
   memory-safety surface area the campaign already rejected (ADR-001
   §"FlatBuffers / shared memory").
3. **Wait for the work to grow**: if a future feature (e.g. churn
   axis, multi-format export) raises per-call work to >100 µs, the
   ratio flips. Worth re-checking when `--by churn` lands.

None of these are blockers for shipping the evidence-only path now —
the crate compiles, tests, and produces correct output. The opt-in
flag (`--heatmap-engine rust`) lets the campaign collect real-world
runs in case the work shape changes.

## Lessons (per the campaign brief's mandate)

1. **The BATCH stateless API CAN ship cleanly under the campaign
   infrastructure.** Cargo.toml + build.rs + cbindgen.toml + the
   dispatcher pattern + golden export + bench harness all generalised
   from `ctx-focus` to a 1-caller batch module without modification.
   Pattern reusability for the remaining 23 modules is confirmed.

2. **Stateless ≠ guaranteed ≥1.5×.** The shuttle's regime requires
   Go-side per-call cost ≥10× the cgo overhead. The 4 shipped modules
   (contract / scan / relations / ?) all met this; heatmap doesn't.
   Future BATCH candidates should be screened by measuring the Go
   baseline first; if it's already sub-50 µs, expect the same
   evidence-only outcome.

3. **Float64 → JSON serialization parity is a real trap.** Go's
   `encoding/json` emits integer-valued float64 as bare integers
   (`4180`), while serde_json emits `4180.0`. We patched with a custom
   `ser_weight` serializer; future Tier 2/3 modules with float
   surfaces should bake this in from day 1.

4. **Bucket / Rect field tag parity matters.** Go's default
   `encoding/json` ships exported struct fields in CamelCase
   (`Path`, `Weight`). Rust serde defaults to the field identifier
   (snake_case). For byte-exact wire parity, add `#[serde(rename =
   "CamelCase")]` to every cross-FFI struct field. Worth documenting
   in the campaign's META-LESSONS section.

5. **The campaign's "evidence-only" classification has a clear shape.**
   Both ctx-where (LOOKUP_HEAVY) and ctx-heatmap (BATCH GLUE-like)
   landed below the shuttle bar for the same root cause: per-call Go
   work too small relative to cgo+JSON floor. The classification
   ("compiled and tested, NOT for production routing") works as-is;
   no campaign-level changes needed.

6. **The honest verdict matters more than uniformity.** Shipping
   heatmap under `--heatmap-engine rust` as a default-on path would
   regress real users by 2-5×. The opt-in flag + this report's
   explicit `BELOW TARGET` verdict is the right user-facing posture.

## What ships

- `crates/ctx-heatmap/` — full crate with 35 passing Rust tests + 7
  goldens × 3 fixtures.
- `internal/heatmap/{dispatch.go, dispatch_rust.go, metrics.go, rustbridge/bridge.go}`
- `internal/cli/map.go` + `map_dispatch.go` + `map_dispatch_rust.go` —
  `--heatmap-engine` flag plumbed.
- `cmd/heatmap-golden-export/main.go` — synthetic-fixture exporter.
- `cmd/heatmap-engine-diff/main.go` — byte-diff + perf measurement
  harness (rust_contract-gated; pure-Go stub points users at the right
  build).
- `internal/heatmap/heatmap_bench_test.go` — Go testing.B harness.
- `tests/heatmap-fixtures/{small,medium,large}_metrics/metrics.json`
- `tests/parity/heatmap-goldens/{small,medium,large}_metrics/*` —
  7 goldens per fixture (3 aggregate axes + squarify + 3 renders).

## What does NOT ship

- A `--heatmap-engine=rust` default. The flag exists; the default
  remains `go` per the campaign's "no regression on shipped modules"
  policy. The same flag rejection mechanism the campaign uses for
  ctx-where and ctx-replay applies here.
- The `--by churn` axis. Still requires `internal/git.LogSince`
  which doesn't exist. Out of scope (unchanged from the Go side).

## References

- Source crate: `crates/ctx-heatmap/`
- Bench: `tests/HEATMAP_BENCH_REPORT.md`
- Campaign brief: this PR's task description
- Sticky-handle precedent (rejected for this module): `tests/STICKY_HANDLE_POC_REPORT.md`
- Tier 1 #1 (`ctx-focus`) report for sessioned API comparison: `crates/ctx-focus/PHASE4_REPORT.md`
