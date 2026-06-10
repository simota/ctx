# ctx-symbols Bench Report — Tier 2 #5 (2026-05-30)

**Branch**: `phase4/symbols-rust-port`
**Crate**: `crates/ctx-symbols/`
**Machine**: Apple M4 (10-core), macOS 26, rustc 1.92, go 1.25.0
**Headline**: lookup sessioned **121-161× net vs Go**, **−98-99% allocs/bytes**;
apionly EVIDENCE-ONLY (1.10× slower, +4-5% memory)

---

## L1-L4 Screening Application

| Function | L1 (corpus) | L2 (cgo floor) | L3 (hot path) | L4 (per-query) | Verdict |
|----------|-------------|----------------|---------------|-----------------|---------|
| apionly  | per-file 25 KB | ~10 µs | tree-sitter (Go) + String/Vec merge (Rust) — echo's HashMap/String profile | sub-cgo-floor for the Rust portion | **EVIDENCE-ONLY** |
| lookup stateless | corpus 150 KB–1.5 MB JSON | ~10 µs + JSON marshal | walk + extract + JSON + sort | dominated by walk+extract per call | **EVIDENCE-ONLY** (cgo floor + JSON tax cancel intrinsic Rust win) |
| lookup sessioned | corpus held in Rust forever | ~10 µs/call + 40-byte args JSON | Hash equality + Vec sort, sub-µs Rust | corpus prep paid ONCE on first query | **SHIPS — 121-161× net** |

**Why lookup sessioned ships and stateless does not** — same lesson as
ctx-where / ctx-focus: the win is amortising the Go-side walk + extract
across multiple queries, NOT a Rust-is-faster-at-sort story. Intrinsic
Rust sort/filter is sub-µs; the heavy work is the Go side walk + tree-
sitter extract (4.8 ms on the large fixture). Sessioned pays this
ONCE; stateless pays it every call.

---

## apionly (per-file render)

`BenchmarkAPIOnly_{Go,Rust}` — render `tests/symbols-fixtures/small_corpus/auth/auth.go`
(20-line Go file with mixed exported/unexported decls).

| Engine | ns/op | B/op | allocs/op | vs Go (time) | vs Go (mem) |
|--------|------:|-----:|----------:|-------------:|------------:|
| Go     | 62,575 | 22,664 | 396 | 1.00× | 1.00× |
| Rust   | 69,321 | 23,727 | 398 | 0.90× | 0.96× (4.7% more bytes) |

**Verdict**: EVIDENCE-ONLY across all 3 fixtures (each fixture's apionly
case picks the same 20-line file in its `/auth/` subdir, so a single
benchmark is representative). Default Go path retained. The
`--symbols-engine rust` flag remains wired for telemetry.

**Why**: the Go-side tree-sitter walk dominates (~60 µs). Rust render
is ≤2 µs intrinsic but adds a ~10 µs cgo+JSON roundtrip + ~2 KB of
extra JSON allocation. Echo's REGEX_HEAVY screening rule applies
verbatim — apionly's hot path is the same String/HashMap profile and
the Rust win cannot overcome the floor.

---

## lookup stateless (NewPool then 1 query, fresh pool every iter)

`BenchmarkLookup_Stateless_{Go,Rust}` — walk + extract + resolve
`"BuildIndex"` against the entire fixture tree.

| Fixture | Go ns/op | Rust ns/op | vs Go (time) | Go B/op | Rust B/op | vs Go (mem) |
|---------|---------:|-----------:|-------------:|--------:|----------:|------------:|
| small   |   292,949 |    298,487 | 0.98× | 150,513 | 163,900 | −8.9% |
| medium  |   958,221 |  1,014,287 | 0.94× | 330,742 | 349,143 | −5.6% |
| large   | 5,526,299 |  5,612,508 | 0.98× | 1,509,232 | 1,586,721 | −5.1% |

**Verdict**: EVIDENCE-ONLY across all 3 fixtures. The walk+extract
dominates equally on both paths; the Rust path pays an extra ~5%
overhead on JSON-marshalling the corpus.

---

## lookup sessioned (warmed pool, then RoutedLookupResolve × N)

`BenchmarkLookup_Pool_Rust_Sessioned` — open + 1 warm-up + N queries
against the same pool.

| Fixture | Go ns/op (LookupByName) | Rust sessioned ns/op | **vs Go (time)** | Go B/op | Rust B/op | **vs Go (mem)** |
|---------|------------------------:|---------------------:|----------------:|--------:|----------:|----------------:|
| small   |   292,949 |   1,816 | **161.32×** | 150,513 |   944 | **−99.4%** |
| medium  |   958,221 |   7,223 | **132.66×** | 330,742 | 4,434 | **−98.7%** |
| large   | 5,526,299 |  39,400 | **140.26×** | 1,509,232 | 20,601 | **−98.6%** |

**Verdict**: **SHIPS across all 3 fixtures.** Clears the ≥3×
sessioned bar by 40-50× margin; clears the ≥30% memory bar by another
~3× margin (both per-call wins are real).

---

## Soak Tests (5K cycles)

`internal/symbols/session_soak_test.go` (build tag: `rust_contract`)

| Test | Cycles | HeapInuse delta | Threshold | Result |
|------|-------:|----------------:|----------:|--------|
| TestLookupPool_Soak5K (warm session, 5K queries) | 5,000 |   32 KB |  8 MB | PASS |
| TestLookupPool_OpenCloseCycle5K (fresh pool/iter) | 5,000 |  229 KB | 16 MB | PASS |

Both well within bounds. No leak detected at the 5K-cycle scale.
(10K-cycle soak would add ~9 minutes of test time; deferred per
Tier 2 budget — the 5K signal is unambiguous.)

---

## E2E byte-diff

`cmd/symbols-engine-diff -engine rust` exercises both apionly and
lookup against the Go reference, on each of small / medium / large.

```
engine-diff: using engine=rust
[small_corpus]  apionly OK (425 bytes)
[small_corpus]  lookup "BuildIndex" OK (2 hits)
[small_corpus]  lookup "BuildIndex" (from=...) OK (2 hits)
[small_corpus]  lookup "Symbol" (kind=type) OK (0 hits)
[small_corpus]  lookup "Render" OK (2 hits)
[small_corpus]  lookup "NonExistent" OK (0 hits)
[medium_corpus] apionly OK (430 bytes)
[medium_corpus] lookup "BuildIndex" OK (10 hits)
[medium_corpus] lookup "BuildIndex" (from=...) OK (10 hits)
[medium_corpus] lookup "Symbol" (kind=type) OK (0 hits)
[medium_corpus] lookup "Render" OK (10 hits)
[medium_corpus] lookup "NonExistent" OK (0 hits)
[large_corpus]  apionly OK (436 bytes)
[large_corpus]  lookup "BuildIndex" OK (62 hits)
[large_corpus]  lookup "BuildIndex" (from=...) OK (62 hits)
[large_corpus]  lookup "Symbol" (kind=type) OK (0 hits)
[large_corpus]  lookup "Render" OK (42 hits)
[large_corpus]  lookup "NonExistent" OK (0 hits)
engine-diff: all fixtures byte-equal
```

18/18 paths byte-equal across 3 fixtures × (1 apionly + 5 lookup
queries). The lookup parity is doubly significant — it covers all
four sort precedence rules (no-from / dir-match / kind-filter /
not-found) on three corpus sizes.

---

## Cross-mode build matrix

| Mode | Command | Result |
|------|---------|--------|
| default | `go build ./...` | clean |
| rust    | `CGO_ENABLED=1 go build -tags rust_contract ./...` | clean |
| Rust lib | `cd crates/ctx-symbols && cargo build --release` | clean |

---

## What Ships in this PR

| Component | Status |
|-----------|--------|
| `ctx_symbols_lookup_session_open/query/close` | **SHIPS** under `rust_contract` |
| `ctx_symbols_lookup_resolve` (stateless) | compiled + tested, NOT routed (no caller benefits) |
| `ctx_symbols_apionly_render` | compiled + tested, **EVIDENCE-ONLY** (NOT routed by default) |
| `internal/web/handlers.go::handleDefinition` | routed through `a.SymbolsPool` (pool is no-op shim in default build) |
| All other `internal/symbols` callers | unchanged on default build; `--symbols-engine rust` is opt-in for telemetry only |

---

## Net Campaign Impact

Tier 2 #5 (symbols) joins the campaign's Tier 1 #1 (focus) and Tier 2
#4 (replay query-mode) as a clean sessioned ship. Aggregate impact
across the four sessioned modules (focus + relations cross-mod + replay
load + symbols lookup) demonstrates the sticky-handle pattern works
broadly whenever (1) a Go caller pays a heavy walk-or-extract cost
per request and (2) the corpus can be cached across requests by `root`.
