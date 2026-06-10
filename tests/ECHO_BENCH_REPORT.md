# ctx-echo Bench Report — Tier 2 #3

**Date**: 2026-05-30
**Branch**: `phase4/echo-rust-port`
**Hardware**: Apple M4 (10 cores, native arm64)
**Rust toolchain**: stable (release profile, LTO default)
**Go toolchain**: 1.x stable (default optimisation)

This document accompanies `crates/ctx-echo/PHASE4_REPORT.md` and provides
the cross-language bench data in one place for the migration roadmap
update.

---

## Methodology

* **Rust**: `cargo bench --bench echo` (criterion, 100 samples per
  fixture, release build).
* **Go**: `go test -bench BenchmarkEvaluate -benchmem -benchtime=3s
  ./internal/echo` (Go testing.B, ReportAllocs enabled).
* **Engine-diff (cgo)**: `go run -tags rust_contract
  ./cmd/echo-engine-diff <fixture>` — runs the **same** Go-side
  `RoutedEvaluate(opts)` loop with `SetEngine("go")` then
  `SetEngine("rust")`, comparing wall-clock and JSON output byte-equality
  (with ULP-tolerant fallback for BM25 sum-order divergence).
* **Memory**: `cargo bench --features dhat-heap --bench memory` per
  fixture (50 reps) for Rust; `B/op` and `allocs/op` from the Go bench
  for Go.

Fixtures:
* `small_pack.md` — 1,349 B, hand-authored, ~12 chunks.
* `medium_pack.md` — 51,282 B, synthetically generated, ~593 chunks.
* `large_pack.md` — 524,542 B, synthetically generated, ~6,069 chunks.

Canonical goal: `"rate limit burst handler"` (4 unique tokens after
stop-word filter).

---

## Per-fixture results

### small (1.3 KB)

|              | ns/op       | B/op         | allocs/op |
|--------------|-------------|--------------|-----------|
| Go           | 72,670      | 1,077,873    | 550       |
| Rust native  | 35,624      | 73,510       | 1,479     |
| Rust via cgo | 50,326      | n/a          | n/a       |

* Rust native speedup vs Go: **2.04×**
* Engine-diff (Rust via cgo) speedup: **1.59×**
* Rust bytes vs Go: **−93%** (PASS memory bucket)
* Rust allocs vs Go: **+169%** (FAIL alloc count)

### medium (51 KB)

|              | ns/op       | B/op         | allocs/op |
|--------------|-------------|--------------|-----------|
| Go           | 1,253,441   | 2,347,065    | 23,769    |
| Rust native  | 1,472,300   | 2,938,987    | 59,320    |
| Rust via cgo | 1,516,298   | n/a          | n/a       |

* Rust native speedup vs Go: **0.85×** (Rust slower)
* Engine-diff speedup: **0.83×**
* Rust bytes vs Go: **+25%** (FAIL)
* Rust allocs vs Go: **+149%** (FAIL)

### large (525 KB)

|              | ns/op        | B/op          | allocs/op |
|--------------|--------------|---------------|-----------|
| Go           | 14,139,054   | 15,128,124    | 242,529   |
| Rust native  | 15,040,000   | 30,595,022    | 605,380   |
| Rust via cgo | 15,799,399   | n/a           | n/a       |

* Rust native speedup vs Go: **0.94×** (Rust slower)
* Engine-diff speedup: **0.86×**
* Rust bytes vs Go: **+102%** (FAIL)
* Rust allocs vs Go: **+149%** (FAIL)

---

## Verdict

Per ADR-002 sticky-handle screening rule (modified for stateless batch):

* **perf-shipped bar (≥5×)**: NO across all fixtures.
* **perf-partial bar (≥2×)**: small only via direct call; sub-2× via cgo.
* **memory-bucket bar (≥30% bytes/op AND/OR ≥30% allocs/op savings)**:
  small only on bytes (−93%); medium and large REGRESS.

**Classification**: EVIDENCE ONLY, no production routing.

---

## Why it didn't ship — root cause

See `crates/ctx-echo/PHASE4_REPORT.md` § "Why it didn't ship" for the
full diagnostic. Summary:

1. Echo's hot path is **String + small-HashMap allocation**, not
   regex/byte-scan. Rust's String / HashMap stdlib has no asymptotic
   advantage over Go's GC at the chunk-count scale (6k chunks → 600k
   small allocs).
2. `ScoredChunk` cloning the entire `Chunk` (with its `Vec<String>`
   tokens) inflates Rust's per-call bytes by ~100% on large fixtures.
   Go's escape analysis avoids the equivalent deep copy.
3. cgo+JSON shuttle overhead (~10-15 µs) dominates the small fixture
   speedup, dragging it from 2.04× (raw) to 1.59× (via cgo).

---

## Updated screening rule

Proposed (carry into `tests/MIGRATION_ROADMAP.md`):

> A REGEX_HEAVY stateless module ships only if all hold:
>
>   (a) Hot path is regex/byte-scan over `&[u8]`, NOT String/HashMap
>       allocation. Stage check: profile the Go version with `pprof
>       -alloc_objects` — if `strings.*` + `make(map…)` >25% of
>       allocations, expect no Rust win.
>   (b) Go baseline ≥ 100 µs for the smallest representative fixture.
>       Below that, cgo overhead alone wipes out the speedup.
>   (c) Per-call structured output < 10 KB. Larger JSON results cap
>       speedup at the JSON encode/decode floor.

Echo fails (a) (String/HashMap dominated), passes (b) at large only
(14 ms), and passes (c) (~1.7 KB result JSON).

---

## Tests & parity

* **All 21 ctx-echo unit tests pass.**
* **All 8 regression tests pass** (one per `echo_test.go` case + 3
  ChunkPack/Render tests).
* **All 4 parity tests pass** (ULP-tolerant float comparison; see
  `tests/parity.rs::floats_close`).
* **Engine-diff agrees** on 3/3 fixtures modulo BM25 sum-order ULP
  divergence (≤ 3 ULP, well within retrieval behavioural threshold).
* **9 prior shipped/evidence crates' tests** all green (spot-checked:
  ctx-contract 38 tests, ctx-scan 13+ tests).
* **`go test ./...` (pure Go)**: ALL PASS.
* **`go test -tags rust_contract ./internal/echo/...`**: PASS.
* **`go build ./...` (default)**: clean.
* **`CGO_ENABLED=1 go build -tags rust_contract ./...`**: clean.

End of bench report.
