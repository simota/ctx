# ctx-relations Phase 4 (Tier 1 #3) — Sticky-Handle Retrofit

**Status**: SHIPPED (opt-in `--relations-engine=rust` via `-tags rust_contract`)
**Date**: 2026-05-30
**Branch**: `phase4/relations-crossmod`
**Predecessor**: Phase 2 (PR #64) — stateless `ctx_relations_build_cached` + per-root `Arc<Mutex<Option<...>>>` cache

## What Phase 4 added

A SECOND access point on top of the already-shipped ctx-relations crate:
an ADR-002 sticky-handle session API. The Phase 2 stateless path is
retained verbatim for callers that only need a one-shot Index
serialization; the session API is the FAST path for callers that ask
many questions of the same root (the web `/api/relations` handler is
the canonical example — every request hits the same root).

The retrofit adds, per ADR-002:

- `src/session.rs` — new module owning `RelationsSession` + per-kind
  query helpers (`refs`, `deps`, `callers`, `edges`, `index_summary`).
- `src/ffi.rs` — three new exported functions:
  `ctx_relations_session_open`, `ctx_relations_session_query`,
  `ctx_relations_session_close`. The existing build / build_cached /
  invalidate_cache / version surface is unchanged.
- `tests/sticky_handle.rs` — 10 integration tests covering parity
  against the stateless path, concurrent queries, idempotent close,
  open/close-no-leak (256 cycles), and a 2000-cycle soak.
- `benches/sticky_handle.rs` — Criterion bench measuring per-query
  intrinsic cost vs. the stateless `build_cached` path.

Go side (`-tags rust_contract` only):

- `internal/relations/rustbridge/bridge.go` — `RelationsSession` Go
  type with atomic double-close guard + `runtime.SetFinalizer`,
  hand-rolled JSON args to avoid encoding/json in the hot path.
- `internal/relations/dispatch_rust.go` — `RelationsPool` (per-root
  lazy session map) + `Routed{Refs,Deps,Callers,Edges}` helpers that
  fall back to the in-process `BuildCached` path on any FFI error.
- `internal/relations/dispatch.go` (pure-Go stub) — same surface
  signature; degrades to direct `BuildCached`+`Edges` calls so the web
  handler can share code between tagged and untagged builds.
- `internal/web/handlers.go` — `API` now carries a `RelationsPool`;
  `handleRelations` routes through `pool.RoutedEdges(...)` instead of
  re-calling `BuildCachedDispatched` on every request.

## Caller wiring decision

| Caller | Workload shape | Routing | Reason |
| --- | --- | --- | --- |
| `internal/web` (`handleRelations`) | Many requests, same root | **Sticky-handle via `API.RelationsPool`** | This is the workload the session was sized for — multiple queries per process lifetime against one root. The pool is opened lazily on the first request, then reused for the lifetime of the API instance. |
| `internal/cli/browse.go` | Launches the web server | **Indirect — uses the web pool** | `browse` doesn't query relations itself; it boots the embedded server. Routing through that server's pool is the cleanest path — no separate session needed in the CLI. |
| `cmd/relations-golden-export` | One-shot golden export | **Stateless (unchanged)** | Tier 2 screening criterion says single-shot workloads don't benefit from sticky-handle. This stays on `BuildCached`. |
| `cmd/relations-engine-diff` | Verification harness | **Both — explicit comparison** | Extended to also run the sessioned path and assert byte-equality against the stateless path. |

## Bench results (Apple M4, `-benchtime=1s`)

### `BenchmarkRelationsEdges_*` (the web handler's exact workload)

| Fixture | Sessioned ns/op | Stateless ns/op | **Sessioned speedup** | Go baseline ns/op |
| --- | ---: | ---: | ---: | ---: |
| `go_project`     |   1,660 |  75,246 | **45.3×** | 158,576 |
| `jsts_project`   |   1,613 |  51,998 | **32.2×** | 152,600 |
| `jvm_project`    |   2,100 |  83,317 | **39.7×** | 193,757 |
| `mixed_project`  |   1,573 |  81,909 | **52.1×** | 198,012 |

### `BenchmarkRelationsRefs_*`

| Fixture | Sessioned | Stateless | **Speedup vs Stateless** | Go baseline | Speedup vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `go_project`     |    736.6 ns |  62,216 ns |  **84.4×** | 155,622 ns |  **211.3×** |
| `jsts_project`   |    792.8 ns |  52,749 ns |  **66.5×** | 158,121 ns |  **199.4×** |
| `jvm_project`    |  1,013 ns |  83,701 ns |  **82.6×** | 191,900 ns |  **189.4×** |
| `mixed_project`  |    967.1 ns |  83,573 ns |  **86.4×** | 203,944 ns |  **210.9×** |

### `BenchmarkRelationsDeps_*`

| Fixture | Sessioned | Stateless | Speedup vs Stateless | Go baseline | Speedup vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `go_project`     | 1,042 ns | 69,517 ns | **66.7×** | 162,039 ns | **155.5×** |
| `jsts_project`   | 1,021 ns | 51,702 ns | **50.6×** | 180,742 ns | **177.0×** |
| `jvm_project`    | 1,224 ns | 84,385 ns | **68.9×** | 211,140 ns | **172.5×** |
| `mixed_project`  |   875.1 ns | 91,448 ns | **104.5×** | 214,015 ns | **244.6×** |

### `BenchmarkRelationsCallers_*` (alias-of-Refs sanity check)

| Fixture | Sessioned | Stateless | Speedup vs Stateless |
| --- | ---: | ---: | ---: |
| `go_project`     |   910.5 ns |  58,534 ns |  **64.3×** |
| `jsts_project`   | 1,042 ns |  51,135 ns |  **49.1×** |
| `jvm_project`    | 1,096 ns |  81,213 ns |  **74.1×** |
| `mixed_project`  |   828.2 ns |  90,620 ns | **109.4×** |

**Conclusion**: across every (kind, fixture) cell the sessioned path
beats the stateless cgo path by ≥**32×**. The widest gap is on
`mixed_project` deps (104.5×), the narrowest on `jsts_project` edges
(32.2×). All four fixtures clear the Tier 1 ≥3× session-speedup bar.

### Rust-only intrinsic (`cargo bench --bench sticky_handle`)

| Bench | go_project | jsts_project | jvm_project | mixed_project |
| --- | ---: | ---: | ---: | ---: |
| `session_query_refs`   |   176 ns |   199 ns |   267 ns |   193 ns |
| `session_query_edges`  |   225 ns |   247 ns |   327 ns |   238 ns |
| `stateless_full_index` |  57.9 µs |  57.0 µs |  90.6 µs | 118.4 µs |

Rust-side per-query cost: 175-330ns. Stateless `build_cached` cost:
57-118µs. **Intrinsic sticky-handle wins 300-500×** — the Go-side
sessioned numbers (700-2000ns) are dominated by cgo + JSON marshal.

## Soak

`TestRelationsSessionSoak_NoMonotonicGrowth` (5,000 open/close cycles):

```
baseline  HeapInuse=1,122,304  HeapAlloc=398,848
midpoint  HeapInuse=1,105,920  HeapAlloc=364,624  (after 2,500 cycles)
endpoint  HeapInuse=1,097,728  HeapAlloc=364,624  (after 5,000 cycles)
```

Heap actually shrinks slightly across the soak — no monotonic growth,
no leak.

## Tier 2 screen prediction

Given the relations result, the screening criterion from the heatmap
post-mortem (PR #70) holds tightly:

- **MULTI-CALLER + same-root-repeated workload → sessioned wins**:
  relations confirms this with 32-100× across all four fixtures.
- **BATCH 1-caller × 1-shot workload → cgo+JSON shuttle floor inverts
  the ratio**: heatmap proved this with 0.4-0.5× net.

Apply this screen to Tier 2 candidates:

| Candidate | Expected shape | Predicted screen |
| --- | --- | --- |
| `summarize` | Pipeline pre-aggregation — typically 1-shot per file | ⚠️ likely **BATCH-shape**; needs streaming session to win |
| `pack` | Repeated symbol/anchor lookups during pack | ✓ **session-shape** |
| `digest` | One-shot per request | ⚠️ **BATCH-shape** |
| `replay` query-mode | Repeated record queries against a fixed snapshot | ✓ **session-shape** |
| `mixdown` | Multi-file aggregation, 1-call-per-cmd | ⚠️ **BATCH-shape**; depends on session vs streaming |
| `graph` | Repeated edge queries (analogous to relations refs/deps) | ✓ **session-shape, strong candidate** |
| `tree` | Single walk per request | ⚠️ **BATCH-shape** |

Recommended Tier 2 first picks: **`graph`** and **`pack`** — both
should clear the ≥3× bar based on shape parity with relations/focus.

## Files touched (Phase 4)

Rust (ctx-relations):

- NEW: `src/session.rs` (~150 LOC + tests)
- NEW: `tests/sticky_handle.rs` (10 tests)
- NEW: `benches/sticky_handle.rs`
- MODIFIED: `src/lib.rs` (+1 line — `pub mod session;`)
- MODIFIED: `src/ffi.rs` (+~120 LOC appended — session_open / query / close)
- MODIFIED: `Cargo.toml` (+ sticky_handle bench entry)
- MODIFIED: `include/ctx_relations.h` (regenerated via cbindgen)

Go:

- MODIFIED: `internal/relations/rustbridge/bridge.go` (+~150 LOC for session API)
- MODIFIED: `internal/relations/dispatch_rust.go` (+~200 LOC for RelationsPool)
- MODIFIED: `internal/relations/dispatch.go` (+~60 LOC pure-Go stub)
- MODIFIED: `internal/web/handlers.go` (API.RelationsPool, route handleRelations through pool)
- MODIFIED: `cmd/relations-engine-diff/main.go` (added sessioned-path byte-equality check)
- NEW: `internal/relations/relations_session_bench_test.go` (benches + soak)

## E2E verification

```
$ CGO_ENABLED=1 go run -tags rust_contract ./cmd/relations-engine-diff ./tests/relations-fixtures/mixed_project
ok    engines agree on ./tests/relations-fixtures/mixed_project (297 bytes)
ok    session path agrees with stateless on 2 files

$ CGO_ENABLED=1 go run -tags rust_contract ./cmd/relations-engine-diff ./tests/relations-fixtures/go_project
ok    engines agree on ./tests/relations-fixtures/go_project (285 bytes)
ok    session path agrees with stateless on 1 files

$ CGO_ENABLED=1 go run -tags rust_contract ./cmd/relations-engine-diff ./tests/relations-fixtures/jvm_project
ok    engines agree on ./tests/relations-fixtures/jvm_project (505 bytes)
ok    session path agrees with stateless on 2 files
```

## Constraints honored

- Phase 2 stateless surface (`ctx_relations_build`, `_build_cached`,
  `_invalidate_cache`, `_free_string`, `_version`) unchanged.
- Existing per-root `Arc<Mutex<Option<...>>>` cache (`src/cache.rs`)
  unchanged.
- Default Go build behavior unchanged — sticky-handle code lives in
  `_rust.go` files behind the existing `rust_contract` tag.
- No code in the other 6 crates was modified.
- No new top-level engines exposed in pure-Go builds — the `RelationsPool`
  stub degrades to in-process `BuildCached`.
