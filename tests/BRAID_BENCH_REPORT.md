# `ctx-braid` Bench Report — Phase 4 Tier 2 #1

**Crate**: `crates/ctx-braid/`
**Status**: COMPILED + TESTED + **EVIDENCE-ONLY** (NOT recommended for production routing)
**Tier**: 2 #1 (first Tier 2 module — BATCH stateless pure-compute port)
**Date**: 2026-05-30
**Hardware**: Apple M4, 10-core
**Build**: `CGO_ENABLED=1 -tags rust_contract` (release Rust)

## Headline

Net end-to-end speedup **0.43-0.53×** (Rust is 1.9-2.3× SLOWER than
Go) across all 3 fixtures. **Memory delta: -43-50% bytes/op,
-27-51% allocs/op** (clears the campaign's ≥30% memory bar — secondary
success criterion).

This is the **fourth evidence-only crate** alongside `ctx-where`,
`ctx-replay`, and `ctx-heatmap`. All four share the same regime:
sub-50 µs Go baseline × 1×-caller × 1×-invocation per command,
where the cgo+JSON shuttle floor dominates.

The Tier 2 #1 outcome **validates the heatmap screening criterion**:
Tier 2 candidates with Go baselines below ~50 µs should be expected to
land in evidence-only territory unless they amortise across many
queries per session.

## Verdict per bar

| Bar | Threshold | Observed | Status |
|---|---|---|---|
| Time speedup (BATCH ≥1.5×) | ≥1.5× | 0.43-0.53× | **FAIL** |
| Memory delta | ≥30% bytes OR allocs | -43-50% bytes, -27-51% allocs | **PASS** |
| Parity (byte-exact across fixtures) | 100% | 100% (3/3 fixtures × 4/4 outputs) | **PASS** |
| Rust suite | green | 57/57 (38 lib + 16 regression + 3 parity) | **PASS** |
| Sister-crate regression | none | 167/167 sister-crate tests pass | **PASS** |

**Verdict**: ship as `--braid-engine rust` opt-in for telemetry;
default remains `go`. Memory wins are documented and credible at
high-concurrency scenarios but do not justify defaulting to Rust given
the wall-clock regression.

## Measurement methodology

### 1. End-to-end via `cmd/braid-engine-diff`

Loops the full pipeline (LoadConfig → Validate → Allocate → MergePaths
→ ShellQuote) N times per engine, asserts byte-exact output, measures
wall-clock per engine.

```
CGO_ENABLED=1 go run -tags rust_contract ./cmd/braid-engine-diff <fixture-name>
```

| Fixture | Reps | Go elapsed | Rust elapsed | Rust speedup |
|---|---:|---:|---:|---:|
| simple (1 strand) | 10,000 | 66.9 ms | 129.9 ms | **0.52×** |
| multi_strand (3 strands) | 10,000 | 134.9 ms | 256.2 ms | **0.53×** |
| complex (4 strands w/ share overflow) | 5,000 | 92.4 ms | 217.2 ms | **0.43×** |

### 2. Per-call via `go test -bench`

Standard testing.B with cgo, allocs-per-op tracked.

```
CGO_ENABLED=1 go test -tags rust_contract \
    -bench=BenchmarkBraid -benchmem -benchtime=2s -run='^$' \
    ./internal/braid/
```

| Bench | Rust ns/op | Go ns/op | Rust B/op | Go B/op | Rust allocs | Go allocs |
|---|---:|---:|---:|---:|---:|---:|
| EndToEnd/simple | 11,002 | 4,151 | 3,169 | 6,360 | 69 | 94 |
| EndToEnd/multi_strand | 24,156 | 10,386 | 5,748 | 11,552 | 90 | 182 |
| EndToEnd/complex | 40,588 | 16,124 | 11,538 | 20,283 | 125 | 256 |
| Allocate/simple | 1,846 | 29.5 | 800 | 96 | 17 | 2 |
| Allocate/multi_strand | 4,213 | 48.1 | 1,585 | 192 | 23 | 2 |
| Allocate/complex | 6,108 | 237 | 2,537 | 328 | 29 | 4 |

**Reading the Allocate-only row**: the cgo+JSON marshal/unmarshal
floor is ~1.8-6 µs even for the tiniest workload. Go's Allocate is
29 ns. The shuttle floor exceeds the Go work by 50-200×.

### 3. Pure-Rust intrinsic via Criterion

In-process Rust, no FFI. Establishes the intrinsic ceiling.

```
cargo bench --bench braid --manifest-path crates/ctx-braid/Cargo.toml -- --quick
```

| Bench | Time |
|---|---:|
| load_validate / simple | 1.94 µs |
| load_validate / multi_strand | 7.84 µs |
| load_validate / complex | 14.89 µs |
| validate_only / simple | 184 ns |
| validate_only / multi_strand | 388 ns |
| validate_only / complex | 555 ns |
| allocate / simple | 18 ns |
| allocate / multi_strand | 81 ns |
| allocate / complex | 144 ns |
| merge_paths / simple | (sub-µs) |
| merge_paths / multi_strand | 1.66 µs |
| merge_paths / complex | 4.82 µs |
| shell_split (fixed sample) | 247 ns |

**Pure-Rust Allocate**: 18-144 ns vs Go's 29-237 ns → modest 0.6-1.65×
intrinsic. Even without FFI, the Rust win is small. Adding the cgo
+ JSON marshal overhead inverts it.

### 4. Memory profiler via dhat

```
cargo bench --features dhat --bench memory --manifest-path crates/ctx-braid/Cargo.toml
```

Result (complex fixture × 1000 cycles): peak total bytes allocated
< 100 KB; no leaks across cycles; deterministic per-cycle pattern.

## Parity verification

`cmd/braid-engine-diff` asserts byte-exact equality of the 4 outputs
per fixture per engine. All 3 × 4 = 12 assertions pass:

| Fixture | LoadConfig | Allocate | MergePaths | ShellQuote |
|---|---|---|---|---|
| simple | EQUAL | EQUAL | EQUAL | EQUAL |
| multi_strand | EQUAL | EQUAL | EQUAL | EQUAL |
| complex | EQUAL (incl. normalisation warning byte-exact) | EQUAL | EQUAL | EQUAL |

Float64-as-int serialization parity: the Allocate normalisation
warning is a string formatted as `"... shares total 1.350 > 1.0 ..."`
(byte-equal to Go's `fmt.Fprintf(...%.3f...)`). Allocation.Share fields
use a custom `ser_share` serializer that emits integer-valued floats
as integers (matches Go `encoding/json` behaviour). Same trap heatmap
hit; same fix applied.

## Why the BATCH bar misses

Identical structure to heatmap. The campaign's BATCH ≥1.5× bar
assumes Go's per-call cost is ≥10× the cgo overhead. Braid's actual
ratio:

| Stage | Go ns | cgo floor (per FFI call) | Ratio |
|---|---:|---:|---:|
| LoadConfig | ~700 ns (simple) → ~5 µs (complex) | ~10-15 µs | 0.05-0.5 |
| Allocate | 29-237 ns | ~1.8-6 µs | 0.005-0.04 |
| MergePaths | ~200 ns - 5 µs | ~10-15 µs | 0.02-0.5 |
| ShellQuote | ~300 ns | ~5-10 µs | 0.05 |

Across the pipeline (4 FFI calls × ~12 µs each = ~50 µs cgo floor)
the Go work is 4-16 µs. The cgo floor is 3-12× the Go work it tries
to displace. Inverted regime, exactly as the screening criterion
predicted.

## Memory win — the headline takeaway

While time regresses, memory wins meaningfully:

- **-50% bytes/op** on simple and multi_strand
- **-43% bytes/op** on complex
- **-51% allocs/op** on multi_strand and complex
- **-27% allocs/op** on simple

Root cause: Rust's serde-derived JSON paths reuse buffers more
efficiently than `encoding/json`, and `Vec<>` with known capacity
avoids the repeated slice grows that pad Go's allocation count.

For CLI tools where wall-clock is sub-millisecond anyway and memory
pressure under concurrent invocations matters, the memory delta is
the right success criterion. The campaign should formalise
"evidence-only with documented memory ≥30%" as a distinct ship
bucket going forward.

## Tier 2 implications

1. **Screening predicted the verdict.** The Tier 1 #2 META-LESSON
   (heatmap) said: if Go baseline is sub-50 µs and the caller shape
   is 1× × 1× per command, expect evidence-only. Braid lands exactly
   where predicted. **Future Tier 2/3 candidates should be screened
   before implementation begins.**

2. **Apply the screening criterion to the remaining Tier 2 queue**
   (see RELEASE_NOTES.md update). Predicted classifications:
   - `summarize`: BATCH; small per-call cost. Expected evidence-only.
   - `pack`: MULTI-QUERY-ish (called from braid, mcp, cli) — could be sessioned. Re-evaluate before porting.
   - `digest`: BATCH; sub-ms Go cost. Expected evidence-only.
   - `replay` query-mode: already ev-only at Tier 1. Skip until amortised.
   - `mixdown`: BATCH. Likely evidence-only.
   - `graph`: MULTI-QUERY (corpus-resident). **Best Tier 2 ship candidate** — expect sessioned ≥3×.
   - `tree`: BATCH. Likely evidence-only.

3. **The pure-compute scope split works.** Tier 2 #1 split a single
   Go package (internal/braid) across the FFI boundary: pure-compute
   helpers in Rust, orchestrator (exec.go, Run()) in Go. The Routed*
   dispatcher pattern is the seam. This pattern generalises to any
   future module with mixed responsibilities.

## References

- Source crate: `crates/ctx-braid/`
- Full Phase 4 report: `crates/ctx-braid/PHASE4_REPORT.md`
- Tier 1 #2 sister report (BATCH precedent): `tests/HEATMAP_BENCH_REPORT.md`
- Campaign roadmap: `tests/MIGRATION_ROADMAP.md`
- Tier 1 #1 sessioned API report (perf ceiling reference): `crates/ctx-focus/PHASE4_REPORT.md`
