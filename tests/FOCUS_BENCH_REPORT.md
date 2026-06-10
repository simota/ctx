# `ctx-focus` Bench Report — Tier 1 #1

**Date**: 2026-05-29
**Branch**: `phase4/focus-rust-port`
**Verdict**: Tier 1 ≥5× bar PASSED on all fixtures by 9.5-21× margin.

## Bench protocol

Three paths compared against the same fixture + same anchor rotation:

1. **Sessioned (sticky-handle)** — one prewalk + one session open, N
   (Resolve + Expand hops=1) cycles, one close. The reported per-query
   cost is the steady-state second-onward cost.
2. **Stateless Rust** — each cycle opens a fresh pool (prewalk +
   marshal + session open + Resolve + Expand + close). Models a naive
   cgo invocation pattern.
3. **Go baseline** — each cycle calls `focus.ResolveAnchor + focus.Expand`
   directly. Standard production path before this port.

Run cmd:

```
CGO_ENABLED=1 go test -tags rust_contract \
    -bench=BenchmarkFocus -benchmem -benchtime=2s -run='^$' \
    ./internal/focus/
```

Anchor rotation: `Pack`, `Options`, `helper`, `RenderPack` (small/medium);
`Score`, `Search`, `Op00_00`, `Caller` (large).

Hardware: Apple M4, darwin/arm64, Go 1.x stable, rustc stable.

## Per-query bench (steady-state, b.N adaptive)

| Bench | Fixture | ns/op | B/op | allocs/op |
|---|---|---:|---:|---:|
| `BenchmarkFocusSessioned_PerQuery` | small_repo  | 9,199    | 1,614    | 31 |
| `BenchmarkFocusSessioned_PerQuery` | medium_repo | 16,758   | 2,110    | 35 |
| `BenchmarkFocusSessioned_PerQuery` | large_repo  | 48,066   | 10,972   | 96 |
| `BenchmarkFocusStateless_PerQuery_AsBaseline` | small_repo  | 713,607  | 939,541  | 2,926 |
| `BenchmarkFocusStateless_PerQuery_AsBaseline` | medium_repo | 3,152,418 | 4,595,949 | 6,351 |
| `BenchmarkFocusStateless_PerQuery_AsBaseline` | large_repo  | 9,341,995 | 14,907,774 | 15,072 |
| `BenchmarkFocusGoBaseline_PerQuery` | small_repo  | 514,414   | 262,698  | 2,524 |
| `BenchmarkFocusGoBaseline_PerQuery` | medium_repo | 2,146,473 | 593,132  | 4,647 |
| `BenchmarkFocusGoBaseline_PerQuery` | large_repo  | 6,051,976 | 1,407,070 | 9,629 |

### Speedup table (Sessioned / Go)

| Fixture | Sessioned ns/op | Go ns/op | **Speedup** |
|---|---:|---:|---:|
| small_repo  | 9,199    | 514,414   | **55.9×**  |
| medium_repo | 16,758   | 2,146,473 | **128.1×** |
| large_repo  | 48,066   | 6,051,976 | **125.9×** |

### Memory (Sessioned vs Go, per query)

| Fixture | Sessioned B/op | Go B/op | Reduction |
|---|---:|---:|---:|
| small_repo  | 1,614  | 262,698   | **-99.4%** |
| medium_repo | 2,110  | 593,132   | **-99.6%** |
| large_repo  | 10,972 | 1,407,070 | **-99.2%** |

## End-to-end (cmd/focus-engine-diff)

Wall-clock for the full `Resolve + Expand` loop, n reps per fixture:

| Fixture | Reps | Go | Rust stateless | Rust sessioned | **Sessioned speedup** | Sticky vs stateless |
|---|---:|---:|---:|---:|---:|---:|
| small_repo  | 2000 | 983.7 ms | 1.366 s | 20.8 ms  | **47.39×**  | 65.83× |
| medium_repo | 2000 | 4.18 s   | 6.25 s  | 39.6 ms  | **105.53×** | 157.65× |
| large_repo  |  200 | 1.17 s   | 1.84 s  | 22.0 ms  | **53.39×**  | 83.85× |

The stateless Rust path is slower than Go by 28-36% in every fixture
(rust=0.64-0.72×). This is the same regression that drove ADR-001's
Freeze decision (where ctx-where measured 0.92-0.97× stateless). The
sticky-handle session API recovers all of it and then some — the
amortised prewalk + symbol extraction dominate, exactly as predicted.

## Memory bench (dhat-rs)

```
cargo bench --features dhat --bench memory \
    --manifest-path crates/ctx-focus/Cargo.toml
```

Run on medium_repo (200 iterations):

| Stat | Value |
|---|---|
| Total bytes allocated | 25,878,001 (≈ 130 KB/iter) |
| Total blocks allocated | 242,078 |
| Peak resident bytes | 437,037 |
| Peak resident blocks | 609 |
| At-end bytes | 87,672 |
| At-end blocks | 275 |

Peak resident of 437 KB is healthy — under 1 MB at any point — and at-
end of 88 KB is the corpus-resident `Vec<FileInput>` that gets
released on session close.

## Soak — 10K open/query/close cycles

```
go test -tags rust_contract -run TestSessionSoak -count=1 \
    ./internal/focus/
```

| Sample point | HeapInuse | HeapAlloc |
|---|---|---|
| Baseline (post-warmup, GC'd) | ~1 MB | ~1 MB |
| Midpoint (5,000 cycles) | within baseline noise (no monotonic growth) | same |
| Endpoint (10,000 cycles) | within baseline noise | same |

**Result: PASS — no leaks detected over 10,000 cycles.** Wall-clock
16.1 s. Assertion guard: endpoint < 1.5× baseline + 4 MB floor AND
mid→end ratio < 1.5×.

## Stop-condition checks (per ADR-002 / MIGRATION_ROADMAP)

| Stop condition | Threshold | Observed | Verdict |
|---|---|---|---|
| Net end-to-end speedup | ≥1.5× | 47-105× | well over |
| Memory delta | ≥30% reduction OR neutral | -99% | well over |
| Parity vs Go | byte-equal goldens | 4 goldens × 3 fixtures match | PASS |
| Soak leak rate | 0 | 0 over 10K cycles | PASS |
| Sister-crate regression | 0 | 0 (145 tests still green) | PASS |

## Recommendation

**SHIP.** Tier 1 #1 is in the bank. Pattern reuse for the remaining 24
modules looks straightforward — focus is the *higher* end of the
expected range (47-105×) because it does two amortised BFS passes per
call. Modules with a single hot-path pass should still beat the ≥5×
bar; modules without a session-resident corpus shape (write-side,
per-call mutable state) will need a different pattern (multi-session
pool — Tier 3).

Next: `heatmap` (LOOKUP_HEAVY, same-corpus repeated touch — direct
sticky-handle beneficiary), then `relations` cross-module
(already-shipped crate gets a session API).
