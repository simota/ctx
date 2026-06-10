# tests/WHERE_BENCH_REPORT.md — Phase 3 where bench results

Run with:

```
go test -bench=. -benchmem -run='^$' ./internal/where/
cargo bench --bench where --manifest-path crates/ctx-where/Cargo.toml
cargo bench --features dhat --bench memory \
    --manifest-path crates/ctx-where/Cargo.toml
CGO_ENABLED=1 go run -tags rust_contract ./cmd/where-engine-diff \
    ./tests/where-fixtures/large_repo
```

Hardware: Apple M4, Darwin 25.5.0. Single-threaded, release mode.

## Criterion (Rust, in-process, isolated hot path)

| Benchmark | Rust µs/op | Throughput |
|---|---:|---:|
| `search/rust/small_repo`   | **6.45** | 1.24 Melem/s |
| `search/rust/medium_repo`  | **15.40** | 1.16 Melem/s |
| `search/rust/large_repo`   | **3,458** | 610 Kelem/s |

## Go testing.B (Go, in-process, full pipeline)

| Benchmark | Go ns/op | bytes/op | allocs/op |
|---|---:|---:|---:|
| `BenchmarkSearch_PerFixture/small_repo-10`   | 264,731   | 327,059    | 1,410   |
| `BenchmarkSearch_PerFixture/medium_repo-10`  | 663,103   | 808,365    | 2,376   |
| `BenchmarkSearch_PerFixture/large_repo-10`   | 101,041,392 | 86,358,586 | 250,285 |

## Intrinsic vs net comparison

| Fixture | Intrinsic ratio | Net ratio (cgo) | Verdict |
|---|---:|---:|---|
| small_repo  | **41×** | **0.97×** | FAIL (vs 1.3× exit) |
| medium_repo | **43×** | **0.92×** | FAIL |
| large_repo  | **29×** | **0.95×** | FAIL |

The cgo+JSON shuttle wipes out the 30-40× intrinsic margin.

## Memory profile

### dhat-rs (Rust, 200 iters medium_repo)

| Metric | Value |
|---|---:|
| Total allocations | 4,396,027 bytes / 98,239 blocks |
| At t-gmax | 348,281 bytes / 311 blocks |
| At t-end | 25,490 bytes / 86 blocks |

### Go MemAlloc (200 iters medium_repo)

| Metric | Value |
|---|---:|
| bytes_alloc/op | 808,075 |
| allocs/op | ~12 (steady-state) |
| heap_alloc_after_bytes | 3,643,672 |

**Rust scoring uses ~36× less memory per call** than the Go scoring.
This is real and persists across cgo; it just isn't visible as wall-
clock speedup because allocs aren't the bottleneck.

## End-to-end measurement (`cmd/where-engine-diff`)

| Fixture | reps | Go elapsed | Rust elapsed | Net | Verdict |
|---|---:|---:|---:|---:|---|
| small_repo  | 2000 | 526.7 ms | 544.4 ms | **0.97×** | FAIL |
| medium_repo | 2000 | 1.234 s | 1.336 s | **0.92×** | FAIL |
| large_repo  | 20 | 1.900 s | 1.998 s | **0.95×** | FAIL |

## FRAGILITY VERDICT

The Phase 3 stop-condition trigger fires: net `where` speedup is below
the 1.2× soft floor on every fixture size we tested. Per
`tests/MIGRATION_ROADMAP.md`:

> Stop-condition trigger: if `where` shows <1.2× net speedup, do NOT
> port `focus` or other LOOKUP_HEAVY modules. Document and stop the
> LOOKUP_HEAVY thesis.

**Recommendation: STOP LOOKUP_HEAVY ports.** Phase 4 should not start
the `focus` / `heatmap` ports until the cgo+JSON FFI shape is
redesigned (FlatBuffers, shared-memory walker, ARC-passed handles).

## Why the cgo overhead dominates

For every `where` call the dispatcher must:

1. Walk the repo (Go side) — N file stat() calls
2. Extract symbols via tree-sitter (Go side) — N parses
3. JSON-marshal the file list (Go side) — N×(symbols + lines) bytes
4. cgo crossing into Rust — ~10μs overhead
5. JSON-unmarshal in Rust — heap allocs proportional to step 3
6. Run the LOOKUP_HEAVY scoring loop (Rust side) — the fast part
7. JSON-marshal results (Rust side)
8. cgo crossing back to Go
9. JSON-unmarshal results (Go side)

Steps 1-2 dominate runtime on real corpora. Of the remaining tail,
steps 3-5 + 7-9 add the same wall-clock as the saved time in step 6,
neutralising the win.

## Action items

1. Honor the stop-condition in `tests/MIGRATION_ROADMAP.md`.
2. Open an investigation ticket for "LOOKUP_HEAVY FFI redesign" before
   any future LOOKUP port attempt.
3. Treat the memory win (~36× less heap per call) as the only
   shippable advantage of this port — it may motivate keeping the
   crate alive as an opt-in for memory-constrained operators.
