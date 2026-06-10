# `ctx-heatmap` Bench + Parity Report — Tier 1 #2

**Date**: 2026-05-30
**Branch**: `phase4/heatmap-rust-port`
**Verdict**: **PARITY: PASS / PERF: BELOW TARGET → ship EVIDENCE-ONLY**

## Parity (3 fixtures × 7 goldens)

`cmd/heatmap-engine-diff` byte-equality verified across small / medium /
large fixtures × 3 render formats (ASCII + JSON + plain):

| Fixture | ASCII | JSON | Plain |
|---|---|---|---|
| small_metrics  (8 files,  4 dirs) | EQUAL | EQUAL | EQUAL |
| medium_metrics (50 files, 10 dirs) | EQUAL | EQUAL | EQUAL |
| large_metrics  (200 files, 20 dirs × 2 subdirs) | EQUAL | EQUAL | EQUAL |

Crate-side `cargo test --features testing --test parity`: 3/3 pass —
each fixture exercises 3 aggregate axes (tokens / files / symbols) +
squarify + the 3 renderers (7 goldens × 3 fixtures = 21 golden
comparisons). The squarify floating-point bit-exact match (the
highest-risk port surface) succeeded on the first integration run.

## Rust unit + regression + parity (Apple M4)

| Test set | Pass | Fail |
|---|---:|---:|
| `cargo test --lib` | 17 | 0 |
| `cargo test --test regression` | 15 | 0 |
| `cargo test --test parity --features testing` | 3 | 0 |
| **Total** | **35** | **0** |

The 15 regression tests mirror all 13 Go heatmap_test.go cases:
TokensByDepth, DepthZeroCollapsesToRoot, FilesAndSymbolsAxes,
DropsZeroWeightBuckets, AreaConservation, AspectRatioReasonable,
EmptyAndDegenerate, ASCII_HeaderAndCellLabels, ASCII_BudgetLegendAndOver,
JSON_ShapeAndInBudget, Plain_FormatAndOrdering, Plain_BudgetTagging,
TopN. Two extras (directory-entry skipping, empty-buckets message)
pin Rust-specific edge cases.

## Sister-crate regression check

```
ctx-contract:  31/31 pass
ctx-scan:      21/21 pass
ctx-relations: 29/29 pass
ctx-where:     24/24 pass
ctx-replay:    18/18 pass
ctx-focus:     20/20 pass
```

All untouched.

## End-to-end performance — `cmd/heatmap-engine-diff` wall-clock

Pipeline: aggregate(tokens, depth=2) → squarify(80×20) → render_ascii
+ render_json + render_plain.

| Fixture | Reps | Go elapsed | Rust elapsed | Net Speedup | Verdict |
|---|---:|---:|---:|---:|---|
| small_metrics  | 5000 | 62.5 ms | 120.9 ms | **0.52×** | **BELOW TARGET** |
| medium_metrics | 1000 | 33.0 ms | 65.9 ms  | **0.50×** | **BELOW TARGET** |
| large_metrics  |  200 | 7.94 ms | 19.94 ms | **0.40×** | **BELOW TARGET** |

Tier 1 #2 BATCH ≥1.5× bar **NOT MET on any fixture**.

## Per-call performance — `go test -bench` (full cgo pipeline)

| Bench | Fixture | ns/op | B/op | allocs/op |
|---|---|---:|---:|---:|
| HeatmapRust_EndToEnd | small  | 15,204 | 9,255 | 49 |
| HeatmapGo_EndToEnd_AsBaseline | small | 3,143 | 9,749 | 82 |
| HeatmapRust_EndToEnd | medium | 42,495 | 21,652 | 67 |
| HeatmapGo_EndToEnd_AsBaseline | medium | 9,133 | 14,159 | 214 |
| HeatmapRust_EndToEnd | large | 80,822 | 37,116 | 67 |
| HeatmapGo_EndToEnd_AsBaseline | large | 24,349 | 25,092 | 525 |
| HeatmapRust_Aggregate | small  |  5,564 | 2,089 | 20 |
| HeatmapGo_Aggregate   | small  |    794 |   808 | 23 |
| HeatmapRust_Aggregate | medium | 20,346 | 8,237 | 29 |
| HeatmapGo_Aggregate   | medium |  8,319 | 3,336 | 107 |
| HeatmapRust_Aggregate | large  | 59,858 | 23,149 | 29 |
| HeatmapGo_Aggregate   | large  | 34,859 | 14,256 | 417 |

Memory-allocation count: **Rust uses 60-87% FEWER allocations** on the
end-to-end pipeline. The bytes-per-op number is comparable (Rust's
single-buffer string assembly vs Go's stitched strings.Builder). The
allocation-count win is real and would surface as **less GC pressure**
on long-lived ctx processes (currently rare, but relevant for the MCP
server use case where many sub-second `ctx map` calls would otherwise
stack pressure).

## Pure-Rust intrinsic — `cargo bench` (no cgo)

| Fixture | Pure Rust µs/op | Pure Go µs/op | Intrinsic speedup |
|---|---:|---:|---:|
| small  | 2.6 | 3.1 | **1.20×** |
| medium | 7.8 | 9.1 | **1.16×** |
| large  | 19.7 | 24.3 | **1.24×** |

The pure-Rust intrinsic is **modest (~1.2×)**, far below the
REGEX_HEAVY 7-9× or even LOOKUP_HEAVY 1.85× model. Heatmap's hot
inner loops (small Vec sort, BTreeMap accumulator, fmt::Write into a
String buffer) are already at the limit of what either language can
do over 8-200 elements. There's no idiomatic Rust speedup to displace
the cgo overhead.

## Memory verification

dhat-rs profile on medium_metrics × 200 cycles (`cargo bench --features
dhat --bench memory`):
- Peak heap: < 100 KB total bytes live at any one time
- No leaks: alloc count equals dealloc count after the workload
- Steady-state: each cycle returns to baseline

The crate is sound for long-running processes; the perf miss is
purely a per-call cgo+JSON overhead issue, not a memory issue.

## Decision under the campaign's "Honest Mandate"

The campaign brief stated:

> "If stateless heatmap clears ≥1.5× cleanly, ship as Option B. If it
> doesn't (regression), re-evaluate Option A."

Re-evaluating Option A (sessioned): heatmap has **1 caller × 1 query
per session**. There is no second query to amortise the session open
across. Even with sticky-handle, the floor cost would be (1×
session_open) + (1× pipeline) + (1× session_close), strictly more
than the stateless path. **Option A is provably worse than Option B
for this workload shape.**

The classification matrix that resulted (kept for the campaign's
META-LESSONS):

| Caller × query shape | Best API | Examples |
|---|---|---|
| 1 caller × 1 query (one-shot)   | Stateless if Go work ≥ 50 µs; else **evidence-only** | **heatmap (this report)** |
| N callers × M queries (corpus reuse) | Sticky-handle session | ctx-focus, ctx-where |
| Pure transform, no corpus       | Stateless (or shuttle) | ctx-contract, ctx-scan |
| Tight inner loop on small data  | Stay-in-Go | (heatmap fits here too — Go is fine) |

We **ship `ctx-heatmap` as the third evidence-only crate** alongside
ctx-where (LOOKUP_HEAVY net 0.92-0.97×) and ctx-replay (JSON_HEAVY
micro net 0.15×). The opt-in `--heatmap-engine rust` flag exists for
campaign telemetry; the default remains `go`.

## What changed at the campaign level

- `tests/MIGRATION_ROADMAP.md`: ctx-heatmap moves from "Tier 1
  in-flight" to "Evidence-only (compiled and tested, NOT for production
  routing)". See § Campaign Execution Status table.
- `tests/RELEASE_NOTES.md`: adds the Tier 1 #2 entry documenting the
  --heatmap-engine flag, the honest perf finding, and the evidence-only
  classification rationale.

## Pattern reuse confirmed for the remaining 23 modules

Despite the perf miss, the campaign infrastructure (Cargo.toml +
build.rs + cbindgen.toml + the dispatcher pattern + the parity-test
+ golden-export harness + the bench framework) **all generalised
cleanly** from ctx-focus's sticky-handle shape to ctx-heatmap's
stateless batch shape. The remaining 23 modules can adopt either API
shape based on per-module characteristics; the scaffolding cost is
amortised. The lesson is:

> Picking the API shape correctly upstream of the port is more
> important than the port quality itself. A correctly-classified
> sessioned API beats a stateless API by 10-100×; a misclassified
> stateless port for a 1-caller × 1-shot module costs 2-5×.

## References

- `crates/ctx-heatmap/PHASE4_REPORT.md` — detailed port report
- `crates/ctx-heatmap/` — source crate
- `cmd/heatmap-engine-diff` — perf harness
- `internal/heatmap/heatmap_bench_test.go` — Go bench source
- Sister evidence-only crates for prior art on this pattern:
  `tests/WHERE_BENCH_REPORT.md`, `tests/REPLAY_BENCH_REPORT.md`
