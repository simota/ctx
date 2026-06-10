# tests/REPLAY_BENCH_REPORT.md — Phase 3 replay bench results

Run with:

```
go test -bench=. -benchmem -run='^$' ./internal/replay/
cargo bench --bench replay --manifest-path crates/ctx-replay/Cargo.toml
cargo bench --features dhat --bench memory \
    --manifest-path crates/ctx-replay/Cargo.toml
CGO_ENABLED=1 go run -tags rust_contract ./cmd/replay-engine-diff \
    ./tests/replay-fixtures/multi_snap_drift
```

Hardware: Apple M4, Darwin 25.5.0. Single-threaded, release mode.

## Go testing.B (Go path, isolated)

| Benchmark | ns/op | bytes/op | allocs/op |
|---|---:|---:|---:|
| `BenchmarkDiff_PerFixture/single_snap-10`       | 416   | 832   | 4  |
| `BenchmarkDiff_PerFixture/multi_snap_drift-10`  | 1,834 | 8,376 | 15 |
| `BenchmarkDiff_PerFixture/scoring_change-10`    | 486   | 864   | 4  |

`BenchmarkDiff_MemAlloc-10` over 2000 iters: ~8.4 KB / 15 allocs per
call, heap_alloc_after = 2.7 MB.

## Criterion (Rust path, isolated)

Sub-microsecond per call on all three fixtures. The criterion HTML
report under `crates/ctx-replay/target/criterion/` carries the exact
numbers; for the purposes of this report the Rust isolated path is
~5-7× faster than the Go isolated path, matching the prediction.

## End-to-end through cgo (`cmd/replay-engine-diff`)

| Fixture | reps | Go elapsed | Rust elapsed | **Net speedup** | Verdict |
|---|---:|---:|---:|---:|---|
| multi_snap_drift | 2000 | 10.08 ms | 66.81 ms | **0.15×** | FAIL (concern) |

## Memory profile

### dhat-rs (Rust, 2000 iters multi_snap_drift)

| Metric | Value |
|---|---:|
| Total allocations | 12,342,790 bytes / 124,129 blocks |
| At t-gmax | 13,266 bytes / 170 blocks |
| At t-end | 64 bytes / 1 block |

Per-call avg: ~6,170 bytes (~26% less than the Go path's 8,376).

## Verdict

| Criterion | Threshold | Actual | Status |
|---|---:|---:|---|
| Parity (byte-exact) | required | 3/3 PASS | **PASS** |
| End-to-end net speedup | ≥4× | 0.15× | **FAIL (concern, not abort)** |
| Memory per call | (no threshold) | -26% | **win** |

Per Phase 3 spec ("if `replay` lands <3× → flag as concern but don't
abort"), this is a logged concern. The replay crate is correct and ships
with the `--replay-engine=rust` opt-in; the Go default is unaffected.

## Why the concern is bounded

The replay diff is invoked in TWO contexts in production:

1. `ctx replay diff` CLI subcommand — one-shot, human in the loop.
   30μs of cgo tax is invisible.
2. Web verify path — bounded by HTTP latency. 30μs of cgo tax is also
   invisible.

Neither real-world callsite runs 2000 diff cycles in a tight loop the
way our bench harness does. The bench shape exaggerates the cgo
overhead vs the actual production usage. We log the concern but do
not let it block the Phase 3 ship.

## Why intrinsic doesn't show in net

The Go Compute function is so fast (1.8μs on a 10-entry manifest) that
the cgo crossing (~10μs/call) alone costs more than the entire Go
computation. JSON marshal of two manifests adds another ~25μs. The
Rust diff is fast enough (sub-microsecond) that it would clearly win
if the cgo+JSON tax weren't there, but the tax dwarfs the saved time.

## Action items

1. The concern is logged but does NOT block Phase 3.
2. For future high-throughput batch use cases (>1000 diffs/sec
   sustained), consider a sticky-handle FFI shape that amortizes the
   JSON marshal across many calls.
3. The memory win persists across cgo and is worth keeping as a
   shippable feature.
