# Phase 4 Report — `ctx-focus` port (Tier 1 #1)

**Date**: 2026-05-29
**Branch**: `phase4/focus-rust-port`
**Status**: SHIPPED — Tier 1 first LOOKUP_HEAVY module under ADR-002

## Summary

`internal/focus` (symbol-anchored mini-pack with one-hop expansion;
387 LOC source + 262 LOC test) is now mirrored by the Rust crate
`crates/ctx-focus`. The port uses the **sticky-handle FFI session
pattern** ratified on `ctx-where` (11.27-18.72× net) and beats that
target by another order of magnitude on focus's BFS workload.

## Architecture

The walk + tree-sitter symbol extraction stay on the Go side
(`internal/focus/dispatch_rust.go::preWalkForFocus`). The Rust hot
path receives a pre-walked, pre-symbolised, pre-line-read corpus and
runs the LOOKUP-heavy BFS: ResolveAnchor (symbol → basename → path
fallback) and Expand (anchor-origin → same-dir → basename-prefix →
name-match, plus hop-2 transitive closure).

```
                            ┌─────────────────────────┐
                            │  cli/focus, mcp/server, │
                            │     braid/exec          │
                            └────────────┬────────────┘
                                         │ focus.OpenSessionPool(root)
                                         ▼
               ┌──────────────────────────────────────────┐
               │ internal/focus/dispatch_rust.go          │
               │   SessionPool — lazy prewalk + session   │
               └──────────────────────────────────────────┘
                                         │ files.json + opts.json
                                         ▼
   ┌─────────────────────────────────────────────────────────────────┐
   │ crates/ctx-focus/src/ffi.rs                                     │
   │   ctx_focus_session_open  → Box<FocusSession>                    │
   │   ctx_focus_session_resolve / _expand / _pack                    │
   │   ctx_focus_session_close                                        │
   └─────────────────────────────────────────────────────────────────┘
                                         │ (in-process Vec<FileInput>)
                                         ▼
                       ┌─────────────────────────────┐
                       │ resolve.rs / expand.rs      │
                       │ pack.rs                     │
                       └─────────────────────────────┘
```

## Modules shipped

| File | Purpose | LOC |
|---|---|---:|
| `Cargo.toml` | crate manifest + dhat feature | 49 |
| `cbindgen.toml` + `build.rs` + `include/ctx_focus.h` | C-header generation | 60 |
| `src/lib.rs` | module wiring + re-exports | 38 |
| `src/types.rs` | wire types (Anchor, Candidate, FileInfo, FileInput, PackResult, ExpandOptions, SymbolInfo, ErrAmbiguous) | 116 |
| `src/resolve.rs` | three-pass anchor resolution | 200 |
| `src/expand.rs` | BFS expansion with hop-2 superset | 270 |
| `src/pack.rs` | orchestrator (resolve + expand) | 60 |
| `src/ffi.rs` | sticky-handle + stateless FFI + 12 tests | 540 |
| `src/testing/*` | parity_fixture_builder paths helper | 30 |
| `tests/parity.rs` | golden parity (resolve/expand_hops1/expand_hops2/pack × 3 fixtures) | 110 |
| `tests/regression.rs` | edge-case pins | 100 |
| `tests/sticky_handle.rs` | session FFI integration + small soak | 200 |
| `benches/focus.rs` | criterion bench (Rust-only) | 80 |
| `benches/memory.rs` | dhat-rs profile harness | 60 |
| `benches/sticky_handle.rs` | sticky vs stateless Rust-only bench | 100 |

Total: **~2,000 LOC Rust + ~250 LOC test fixtures** for the 387 LOC Go
source module. Most growth is FFI boilerplate; the algorithmic core
(resolve.rs + expand.rs ≈ 470 LOC) is direct.

## Test counts

| Suite | Tests | Result |
|---|---:|---|
| `cargo test --tests` (unit, lib) | 20 | ALL PASS |
| `cargo test --features testing --test parity` | 3 | ALL PASS |
| `cargo test --test regression` | 6 | ALL PASS |
| `cargo test --test sticky_handle` (incl. 2K-cycle soak) | 8 | ALL PASS |
| **Total Rust** | **37** | **ALL PASS** |
| `go test ./internal/focus/` (existing 7 tests) | 7 | ALL PASS |
| Sister crates regression (`ctx-where`, `ctx-scan`, `ctx-relations`, `ctx-contract`, `ctx-replay`) | 145 | ALL PASS, unchanged |

## Performance — end-to-end (cmd/focus-engine-diff)

Wall-clock end-to-end. n is reps per fixture (size-adapted).

| Fixture | Go | Rust stateless | Rust sessioned | Sessioned speedup | Sticky-vs-stateless |
|---|---:|---:|---:|---:|---:|
| small_repo  (n=2000) | 983.7 ms | 1.366 s | 20.8 ms | **47.39×** | 65.83× |
| medium_repo (n=2000) | 4.18 s   | 6.25 s  | 39.6 ms | **105.53×** | 157.65× |
| large_repo  (n=200)  | 1.17 s   | 1.84 s  | 22.0 ms | **53.39×** | 83.85× |

All three blow past the **Tier 1 ≥5× sessioned bar** (per ADR-002) by
9.5-21× margin. The stateless Rust path is *slower* than Go by 28-36%
because it re-pays the prewalk per call — the same regime where
`ctx-where` measured 0.92-0.97×. The amortisation moat is what
sticky-handle is FOR.

## Performance — per-query (go test -bench, b.N steady-state)

| Fixture | Sessioned ns/op | Stateless ns/op | Go ns/op | Sessioned vs Go |
|---|---:|---:|---:|---:|
| small_repo  | 9,199    | 713,607   | 514,414   | **55.9×** |
| medium_repo | 16,758   | 3,152,418 | 2,146,473 | **128.1×** |
| large_repo  | 48,066   | 9,341,995 | 6,051,976 | **125.9×** |

Memory (allocs + bytes per b.N op):

| Fixture | Sessioned | Stateless | Go |
|---|---:|---:|---:|
| small_repo  | 1,614 B / 31 allocs | 939,541 B / 2,926 allocs | 262,698 B / 2,524 allocs |
| medium_repo | 2,110 B / 35 allocs | 4,595,949 B / 6,351 allocs | 593,132 B / 4,647 allocs |
| large_repo  | 10,972 B / 96 allocs | 14,907,774 B / 15,072 allocs | 1,407,070 B / 9,629 allocs |

Sessioned uses **125-280× less heap per query** than the Go baseline.

## Memory (dhat)

`cargo bench --features dhat --bench memory` on medium_repo (200 reps):

```
dhat: Total:     25,878,001 bytes in 242,078 blocks
dhat: At t-gmax: 437,037 bytes in 609 blocks
dhat: At t-end:  87,672 bytes in 275 blocks
```

- 25.8 MB total allocation across 200 iterations = ~129 KB/iter
- Peak resident: 437 KB (well under the 1 MB target shape)
- At-end: 88 KB residual (corpus-resident; freed on session close)

## Soak — 10K cycles open/query/close

`TestSessionSoak_NoMonotonicGrowth` on medium_repo:

```
baseline HeapInuse=∼1 MB
midpoint  HeapInuse  (after 5,000 cycles): within noise of baseline
endpoint  HeapInuse  (after 10,000 cycles): within noise of baseline
```

`go test -tags rust_contract -run TestSessionSoak -count=1
./internal/focus/` completes in **16.1 s** with **zero leaks
detected** (HeapInuse end < 1.5× baseline + 4 MB floor; mid→end ratio
< 1.5×). Same shape as the `ctx-where` PoC soak (which also passed).

## Build matrix

| Build mode | Outcome |
|---|---|
| `go build ./...` (default, no cgo) | OK — pure-Go, unchanged behaviour |
| `CGO_ENABLED=1 go build -tags rust_contract ./...` | OK — links 6 staticlibs (contract / scan / relations / replay / where / focus) |
| `cargo build --release` (all 6 crates) | OK |
| Existing `cmd/where-engine-diff` (sanity that ctx-where still wires) | OK |

## Call-site updates

- `internal/cli/focus.go` — opens `focus.OpenSessionPool` once per
  command, runs Resolve + Expand through the pool. Adds new flag
  `--focus-engine go|rust` (default `go`) mirroring `--where-engine`,
  `--scan-engine`, `--relations-engine`.
- `internal/mcp/server.go` — same session-pool pattern in the
  `ctx_focus` MCP handler (ResolveAnchor + Expand share corpus per
  call).
- `internal/braid/exec.go` — same session-pool pattern in the focus
  strand executor.

## Lessons (sticky-handle pattern re-application)

What carried over verbatim from `ctx-where`:

- `Box::into_raw` + opaque `*mut c_void` handle model.
- `atomic.Uint32` double-close guard + `runtime.SetFinalizer` on the
  Go side.
- `runtime.KeepAlive(filesJSON)` after every cgo call.
- Lazy `OnceLock`-style init for static patterns (`IDENTIFIER_RX_CACHE`).
- Per-call `catch_unwind(AssertUnwindSafe(...))` envelope.
- Pre-walk + symbol extraction on the Go side; Rust receives a
  pre-decoded `Vec<FileInput>` corpus.
- dhat feature + bench gated behind `[features] dhat`.
- 10K-cycle soak harness shape (`TestSessionSoak_NoMonotonicGrowth`).

What was focus-specific:

- BFS-over-symbol-graph rather than single-pass scoring (focus has
  TWO amortised passes per call: Resolve + Expand). This is the
  source of the higher (47-105× vs 11-19×) end-to-end win.
- Pack envelope JSON: success returns `Anchor` / `[FileInfo]`,
  ambiguous returns `{"ambiguous":true,"candidates":...}`, not-found
  returns `{"error":...}`. The Go decoder dispatches on the
  discriminator (see `dispatch_rust.go::decodeResolveEnvelope`).
- Hop-2 BFS uses a sorted symbol list so the iteration order is
  deterministic across runs (Go's `map` iteration is non-deterministic
  but the result *set* is invariant; we sort to keep goldens stable).

## Integration story

The 3 callers (`cli/focus`, `mcp/server`, `braid/exec`) all moved to
`focus.OpenSessionPool`. The pool is opened ONCE per command (CLI),
once per MCP handler invocation, once per braid strand — and reused
for the Resolve + Expand pair. For the CLI case this means a `ctx
focus Pack --focus-engine rust` invocation pays the cgo crossing TWICE
total (one resolve + one expand), not 2N times. This is the
straightforward sticky-handle win.

For workloads that batch focus calls (planned multi-anchor mode), the
pool is the natural reuse point — call `pool.Resolve` + `pool.Expand`
in a loop and the per-anchor cgo cost drops to ~10-50 μs (per the
bench table above).

## Verdict

**Tier 1 #1 SHIPPED, every gate cleared.**

| Gate | Bar | Observed | Verdict |
|---|---|---|---|
| Net end-to-end sessioned vs Go | ≥5× | 47-105× across 3 fixtures | **PASS** (9.5-21× margin) |
| Parity vs Go on goldens | byte-equal | 4 goldens × 3 fixtures all match | **PASS** |
| 10K-cycle soak | zero leak | HeapInuse stable, <1.5× growth | **PASS** |
| Rust suite | green | 37/37 ctx-focus + 145 sister crates | **PASS** |
| Default `go build` unchanged | OK | pure-Go path untouched | **PASS** |
| Cross-compile matrix | 6 staticlibs link | all 6 link under `-tags rust_contract` | **PASS** |
| Memory delta | session-resident only | 1.6-11 KB/query vs 263-1407 KB Go (-99%) | **PASS** |

Next Tier 1 targets: `heatmap`, `relations` cross-module.
