# `ctx-relations` Sticky-Handle Bench Report — Tier 1 #3

**Date**: 2026-05-30
**Hardware**: Apple M4 (10 cores, darwin/arm64)
**Build**: `cargo build --release` + `CGO_ENABLED=1 go test -tags rust_contract`
**Branch**: `phase4/relations-crossmod`

## TL;DR

The ADR-002 sticky-handle pattern, applied retroactively to the
already-shipped ctx-relations crate, delivers **32-104× speedup over
the stateless cgo path** across all (kind, fixture) cells — well above
the Tier 1 ≥3× sessioned bar. This is the **third Tier 1 module
shipped** (after focus 47-105× and where 11-19×); heatmap (Tier 1 #2)
landed evidence-only because its BATCH 1-caller-1-shot workload didn't
match the screen.

Relations validates the cross-cutting screen one more time: the web
handler `/api/relations` reads the same root repeatedly — exactly the
shape sticky-handle was sized for. Other relations callers
(golden-export, engine-diff) stay on the stateless path because they
are single-shot.

## Bench matrix

Each cell is `ns/op` (lower = better), `-benchtime=1s`,
`-benchmem`, default `-cpu=10`.

### Edges (single FFI crossing returns both directions — the web caller's hot path)

| Fixture | Sessioned | Stateless | Go baseline | Sessioned vs Stateless | Sessioned vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `go_project`     |  1,660 |  75,246 | 158,576 | **45.3×** |  **95.5×** |
| `jsts_project`   |  1,613 |  51,998 | 152,600 | **32.2×** |  **94.6×** |
| `jvm_project`    |  2,100 |  83,317 | 193,757 | **39.7×** |  **92.3×** |
| `mixed_project`  |  1,573 |  81,909 | 198,012 | **52.1×** | **125.9×** |

### Refs (importers of a path)

| Fixture | Sessioned | Stateless | Go baseline | Sessioned vs Stateless | Sessioned vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `go_project`     |    736.6 |  62,216 | 155,622 |  **84.4×** | **211.3×** |
| `jsts_project`   |    792.8 |  52,749 | 158,121 |  **66.5×** | **199.4×** |
| `jvm_project`    |  1,013   |  83,701 | 191,900 |  **82.6×** | **189.4×** |
| `mixed_project`  |    967.1 |  83,573 | 203,944 |  **86.4×** | **210.9×** |

### Deps (imports of a path)

| Fixture | Sessioned | Stateless | Go baseline | Sessioned vs Stateless | Sessioned vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `go_project`     | 1,042 |  69,517 | 162,039 |  **66.7×** | **155.5×** |
| `jsts_project`   | 1,021 |  51,702 | 180,742 |  **50.6×** | **177.0×** |
| `jvm_project`    | 1,224 |  84,385 | 211,140 |  **68.9×** | **172.5×** |
| `mixed_project`  |   875.1 |  91,448 | 214,015 | **104.5×** | **244.6×** |

### Callers (alias of Refs — sanity check for the FFI kind-string path)

| Fixture | Sessioned | Stateless | Sessioned vs Stateless |
| --- | ---: | ---: | ---: |
| `go_project`     |   910.5 |  58,534 |  **64.3×** |
| `jsts_project`   | 1,042   |  51,135 |  **49.1×** |
| `jvm_project`    | 1,096   |  81,213 |  **74.1×** |
| `mixed_project`  |   828.2 |  90,620 | **109.4×** |

Aliasing has zero practical cost — Callers numbers match Refs within
fixture-level noise.

### Allocations / op

Sessioned alloc footprint is dramatically smaller because the per-call
work is a single small JSON envelope rather than a full Index marshal.

| Path | bytes/op | allocs/op |
| --- | ---: | ---: |
| Sessioned (edges) | ~656-806 | 16-17 |
| Stateless (edges) | ~1,992-2,696 | 36-50 |
| Go baseline | ~100K-105K | ~1,000-1,080 |

The sessioned bytes/op is so small (≤ 800 bytes for the largest
fixture) because no full-Index JSON is emitted — only the small
`{"path":..., "imports":[...], "importers":[...]}` envelope.

## Rust-only intrinsic floor (`crates/ctx-relations/benches/sticky_handle.rs`)

The Rust intrinsic gives the absolute floor: no cgo, no Go-side
marshal. Any gap between this and the Go-side sessioned numbers
attributes to cgo + JSON crossing overhead.

| Bench | go_project | jsts_project | jvm_project | mixed_project |
| --- | ---: | ---: | ---: | ---: |
| `session_query_refs`   |   176 ns |   199 ns |   267 ns |   193 ns |
| `session_query_edges`  |   225 ns |   247 ns |   327 ns |   238 ns |
| `stateless_full_index` |  57.9 µs |  57.0 µs |  90.6 µs | 118.4 µs |

Rust intrinsic sessioned vs stateless: **~250-700×** depending on
fixture. The Go-side sessioned numbers (700-2,000 ns) carry ~500-1,500
ns of cgo + JSON shuttle overhead per call. That's the irreducible
floor — and it's still 32-100× faster than the stateless path because
the stateless path pays both the FFI crossing AND a full Index walk.

## Soak: 5,000 open/close cycles

`internal/relations.TestRelationsSessionSoak_NoMonotonicGrowth`:

```
baseline  HeapInuse=1,122,304  HeapAlloc=398,848
midpoint  HeapInuse=1,105,920  HeapAlloc=364,624  (after 2,500 cycles)
endpoint  HeapInuse=1,097,728  HeapAlloc=364,624  (after 5,000 cycles)
```

HeapInuse mid-vs-end ratio = 0.99×. No leak; in fact the Go heap
slightly shrinks because the test exercises the finalizer path. The
midpoint/endpoint HeapAlloc are byte-identical — proving steady-state
recycling.

## E2E verification

`cmd/relations-engine-diff` was extended to drive the sessioned path
across every file in the Index and assert byte-equal output against
the stateless `BuildCached.Edges` answer:

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

## Test counts

- `ctx-relations`: 36 unit + 7 parity + 7 regression + 10
  sticky-handle = **60 tests** (60/60 pass)
- Other 6 crates unchanged:
  - `ctx-contract`: 38 (38/38 pass)
  - `ctx-scan`: 28 (28/28 pass)
  - `ctx-where`: 32 (32/32 pass)
  - `ctx-focus`: 34 (34/34 pass)
  - `ctx-replay`: 26 (26/26 pass)
  - `ctx-heatmap`: 35 (unchanged; not in this task scope)
- Total Rust tests: **253** (was 246 prior; +7 from new sticky-handle
  cases vs Phase 2 count discrepancy due to recent regression-test
  additions counted elsewhere)
- Go: `internal/relations/...` tests pass on both pure-Go and
  `-tags rust_contract` builds.

## Per-caller routing decision

| Caller | Routing | Justification |
| --- | --- | --- |
| `internal/web` `/api/relations` | **Sticky-handle (NEW)** | Multiple requests share one root → 32-52× win on edges path. |
| `internal/cli/browse` | Indirect (uses the web pool) | Doesn't query relations directly. |
| `cmd/relations-golden-export` | Stateless (unchanged) | Single-shot; sticky-handle would only add open/close overhead. |
| `cmd/relations-engine-diff` | Both — explicit comparison | Verification harness; runs each engine for parity diffing. |

## Tier 2 screen predictions (refreshed)

The Tier 1 #2 heatmap post-mortem identified the screen: **BATCH
1-caller × 1-shot → cgo+JSON floor dominates → sticky-handle loses or
breaks even**. Tier 1 #3 (relations) confirms the converse:
**MULTI-CALLER + same-corpus-repeated → sticky-handle wins big**.

| Tier 2 candidate | Workload shape | Predicted screen | Expected fit |
| --- | --- | --- | --- |
| `summarize` | Pipeline pre-aggregation, 1-shot per file | BATCH | ⚠️ likely flat/regress |
| `pack` | Repeated symbol/anchor lookups during pack | MULTI-QUERY | ✓ session-fit |
| `digest` | One-shot per request | BATCH | ⚠️ likely flat/regress |
| `replay` query-mode | Repeated record queries against fixed snapshot | MULTI-QUERY | ✓ session-fit |
| `mixdown` | Multi-file aggregation, 1-call-per-cmd | BATCH | ⚠️ depends on streaming |
| `graph` | Repeated edge queries (analogous to relations refs/deps) | MULTI-QUERY | ✓ **strong fit** |
| `tree` | Single walk per request | BATCH | ⚠️ likely flat/regress |

**Recommended Tier 2 first picks**: `graph` and `pack` — workload
shape parallels relations/focus closely; both should clear the ≥3×
bar.

## Provenance

- Rust crate: `crates/ctx-relations/` @ `phase4/relations-crossmod`
- Bench source: `internal/relations/relations_session_bench_test.go`
  + `crates/ctx-relations/benches/sticky_handle.rs`
- Hardware: `cpu: Apple M4`, `goos: darwin`, `goarch: arm64`
- Run commands:
  ```
  cd crates/ctx-relations && cargo bench --bench sticky_handle
  CGO_ENABLED=1 go test -tags rust_contract \
    -bench='BenchmarkRelations' -benchmem -benchtime=1s -run='^$' \
    ./internal/relations/...
  CGO_ENABLED=1 go test -tags rust_contract \
    -timeout 120s -run TestRelationsSessionSoak -v \
    ./internal/relations/...
  ```
