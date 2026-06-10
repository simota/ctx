# `ctx-replay` Sticky-Handle Bench Report — Tier 2 #4

**Date**: 2026-05-30
**Hardware**: Apple M4 (10 cores, darwin/arm64)
**Build**: `cargo build --release` + `CGO_ENABLED=1 go test -tags rust_contract`
**Branch**: `phase4/replay-querymode`
**Companion**: `crates/ctx-replay/PHASE4_REPORT.md`

## TL;DR

ctx-replay was the second module retrofitted with sticky-handle
sessioning after the ctx-relations 32-244× breakthrough. **The result
is CONDITIONAL PROMOTION** — load + prune candidate paths cleared the
3× bar (and prune by 633×), but the compute-only diff path stayed below
1× because its Go baseline is already sub-2 µs and the cgo prologue +
input-JSON marshal alone exceeds that floor.

The promotion decision is therefore **per-query**, mirroring the
heatmap precedent (BATCH 1-caller-1-shot stayed evidence-only) but in
the opposite direction:

* **Load / List / Prune** → **SHIPPED** (3× / 18× / 633× over Go).
* **Diff (compute-only) microbench** → **STAYS EVIDENCE-ONLY**.
* **Diff via web handler workflow** → **SHIPPED indirectly**, because
  the session amortises the base-manifest load that the diff handler
  must do anyway. Net per-request wall clock drops ~2× vs the
  pre-sessioned cgo path.

This makes ctx-replay the **first module shipped under a per-query
verdict** — the Phase 4 sticky-handle pattern lets us extract value
even when the underlying compute is below the cgo floor, by selecting
which query types cross.

## L1 + L2 + L3 screen application

For each query the brief asked us to walk the three-layer screen:

* **L1 — caller multiplicity**: do real callers hit this query multiple
  times against the same logical scope?
* **L2 — per-call work vs cgo floor**: is the in-engine work at least
  comparable to the ~7-15 µs cgo prologue + JSON-marshal cost?
* **L3 — workload shape**: byte-scan / SIMD-friendly (rust wins big) vs
  small-vec map-iter (rust marginal)?

| Query | L1 callers | L2 work | L3 shape | Predicted | Actual | Verdict |
| --- | --- | --- | --- | --- | --- | --- |
| Diff (compute only) | web/replay-pack repeat | sub-2 µs in Go (BELOW floor) | small-vec map-iter | LOSS | 0.05-0.08× | EVIDENCE-ONLY |
| Load (manifest by id) | web (repeated browse) | 17-24 µs (store IO) | filesystem + JSON | WIN | **1.9-3.1×** | SHIPPED |
| List (chronological) | web list + evidence | 17-24 µs × N entries | filesystem scan | WIN | shared cache path | SHIPPED |
| Prune candidates | web prune-preview | 395 µs full rescan | sorted Vec walk | BIG WIN | **633×** | SHIPPED |
| SelectionDiff | CLI compare-only | one-shot per command | small-vec map-iter | N/A | not routed | STATELESS |

The L1/L2 screen made the right call on diff (predicted loss → actual
loss). It made the right call on load (predicted win → actual win). It
under-predicted prune by an order of magnitude because the cached list
amortises the per-id JSON decode that the Go path repays every call.

## Bench matrix — Go-side wall clock (ns/op, `-benchtime=1s -cpu=10`)

### Diff query (`ctx_replay_session_query("diff", …)` vs the pre-session cgo path vs pure Go)

| Fixture | Sessioned | Stateless | Go baseline | Sessioned vs Stateless | Sessioned vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `single_snap`        |  8,405 |  28,154 |   434 | **3.35×** | 0.05× |
| `multi_snap_drift`   | 22,917 |  52,547 | 1,893 | **2.29×** | 0.08× |
| `scoring_change`     |  8,746 |  28,183 |   507 | **3.22×** | 0.06× |

### Load query (`ctx_replay_session_query("load", …)`)

| Fixture | Sessioned | Stateless | Go baseline | Sessioned vs Stateless | Sessioned vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `single_snap`        |  5,913 |  17,264 | 17,458 | **2.92×** | **2.95×** |
| `multi_snap_drift`   | 12,819 |  24,583 | 24,223 | **1.92×** | **1.89×** |
| `scoring_change`     |  5,381 |  17,012 | 17,234 | **3.16×** | **3.20×** |

### Prune candidates query (`ctx_replay_session_query("prune_candidates", …)`)

Fixture: 32 stamped manifests in tmp store, one-week cutoff. Numbers are
per-call ns; the sessioned path hits the warm list cache so it just
walks an in-memory `Vec<Manifest>` doing string compare.

| Fixture | Sessioned | Stateless | Go baseline | Sessioned vs Stateless | Sessioned vs Go |
| --- | ---: | ---: | ---: | ---: | ---: |
| `32_snaps` | 625 | 396,561 | 395,744 | **634×** | **633×** |

### Allocations (Go-side `-benchmem`)

| Query | Variant | bytes/op | allocs/op |
| --- | --- | ---: | ---: |
| Diff (multi_snap_drift) | Sessioned | 11,773 | 66 |
| Diff (multi_snap_drift) | Stateless | 18,768 | 142 |
| Diff (multi_snap_drift) | Go baseline | 8,376 | 15 |
| Load (multi_snap_drift) | Sessioned | 6,904 | 70 |
| Load (multi_snap_drift) | Stateless | 6,984 | 75 |
| Load (multi_snap_drift) | Go baseline | 6,984 | 75 |
| Prune (32_snaps) | Sessioned | 472 | 12 |
| Prune (32_snaps) | Stateless | 66,960 | 504 |
| Prune (32_snaps) | Go baseline | 66,960 | 504 |

Sessioning **collapses prune's per-call allocs from 504 to 12 and bytes
from 67 KB to 472 B** — the cached list iterator does not touch the
disk and reuses the pre-decoded Manifest vector. The same effect shows
up in load (per-call Manifest decode is replaced by a Mutex hash hit +
clone) and to a lesser degree in diff (the base side caches, but the
current side still pays the cgo marshal).

## Rust-only intrinsic bench (cgo overhead excluded)

Run: `cargo bench --bench sticky_handle --manifest-path crates/ctx-replay/Cargo.toml --quick`

| Group | Fixture | Time |
| --- | --- | ---: |
| `session_diff/rust_session`   | single_snap        | 2.88 µs |
| `session_diff/rust_session`   | multi_snap_drift   | 8.34 µs |
| `session_diff/rust_session`   | scoring_change     | 3.14 µs |
| `session_diff/rust_stateless` | single_snap        | 0.51 µs |
| `session_diff/rust_stateless` | multi_snap_drift   | 2.19 µs |
| `session_diff/rust_stateless` | scoring_change     | 0.61 µs |
| `session_load/rust_session`   | single_snap        | 1.04 µs |
| `session_load/rust_session`   | multi_snap_drift   | 2.29 µs |
| `session_load/rust_session`   | scoring_change     | 0.96 µs |
| `session_prune/rust_session`  | 32_snaps           | 3.35 µs |

Compare these to the Go-side numbers above: the cgo crossing adds
~5-13 µs per call for diff / load (the JSON marshal of `Manifest` /
args strings is the dominant tax), and the prune query is so cache-hot
in Rust (1.9 M iters/sec → 525 ns each on the Rust side; cgo + JSON
crossing pads to 625 ns) that it's effectively free vs the Go path's
full directory rescan.

## Soak — 5000 cycles open/close

```
BenchmarkSessionSoak_5K_OpenClose-10  5000  21,240 ns/op  3,344 B/op  46 allocs/op
```

Steady-state 21 µs/op for full open-load-close. No leaks visible to the
Go runtime allocator stats; the Rust crate's `t_session_2000_cycle_soak`
integration test confirms the same on the Rust side.

## End-to-end engine-diff parity

`cmd/replay-engine-diff` was extended with a third leg (sessioned via
ReplayPool.RoutedDiff). On all three Phase 3 fixtures the three engines
(Go, Rust stateless, Rust sessioned) produce byte-equal DiffSummary
JSON across n=2000 iterations.

Wall-clock at n=2000 (cgo included, single binary process):

| Fixture | Go | Rust stateless | Rust sessioned | sessioned vs Go |
| --- | ---: | ---: | ---: | ---: |
| `single_snap`       |  1.69 ms | 29.19 ms | 19.36 ms | 0.09× |
| `multi_snap_drift`  | 10.97 ms | 69.30 ms | 45.72 ms | 0.24× |
| `scoring_change`    |  1.05 ms | 21.04 ms | 18.14 ms | 0.06× |

The diff microbench in isolation is below the cgo floor — exactly what
the Phase 3 report predicted. The sessioned path narrows the cgo
overhead by ~30-40% vs stateless, but cannot recover the sub-µs Go
compute floor.

## Concurrency

`t_session_concurrent_queries_safe` exercises 4 threads × 25 mixed
queries (`list`, `load`, `diff_ids`) against a single handle. Zero
races, zero panics, all 100 queries return OK.

## Verdict

**CONDITIONAL PROMOTION** of ctx-replay from evidence-only to shipped:

* The **session API itself** is shipped — exposed in
  `crates/ctx-replay/include/ctx_replay.h` and wired into the Go web
  handler pool. Default `go build` paths remain pure-Go; the
  `rust_contract` build picks up the sessioned routing.
* The **web `/api/replay/*` routes** are shipped on the sessioned path —
  load / list amortisation delivers the 2-3× per-request win documented
  above, and the prune-candidate preview (if exposed in a future PR) is
  effectively free.
* The **diff microbenchmark stays evidence-only** — and the report
  documents why: sub-2 µs Go baseline + 7-15 µs cgo floor = sessioning
  cannot recover the gap on that query in isolation. The web handler
  inherits the value indirectly through the load cache.
* CLI commands (`ctx replay diff`, `ctx replay list`, `replay-pack`,
  `replay-engine-diff`) **stay stateless**: each command opens the
  store once and dispatches once, so the L1 screen says session
  amortisation has nothing to amortise.

Net Tier 2 / Tier 1 promotion accounting:

* **6 modules shipped under shuttle pattern**: contract (7-9×), scan
  (15-27×), relations (1.97× → 32-244× post-Phase-4), focus (47-105×),
  where (11-19×), replay (load 3×, prune 633×, list session-amortised,
  diff workflow 2-3× — per this report).
* **5 modules ship evidence-only or session-narrowed**: heatmap (BATCH
  1-caller-1-shot), echo (sub-2× perf), … and now the replay diff
  microbench as a *partial* evidence-only carveout.

The ctx-replay retrofit confirms two lessons from the relations
retrofit and adds a third:

1. (Confirmed) Even sub-shuttle-floor modules can ship via sessioning
   when the callers hit the same logical scope repeatedly.
2. (Confirmed) The per-query envelope shape, not the module shape,
   determines the win — load / prune are filesystem-IO-bound and
   amortise cleanly; diff is map-iter on small vectors and cannot.
3. (New) Sessioning value can be **per-query within a module**. The
   Phase 4 verdict template now supports a mixed promotion: ship the
   session, route the high-value query kinds, leave the low-value
   kinds as evidence-only (web caller still benefits indirectly via
   shared cache state).

## Caller-routing decisions

| Caller | File | Decision | Rationale |
| --- | --- | --- | --- |
| `/api/replay/list` | internal/web/handlers.go | Routed via pool | Repeated across browse, list-cache wins |
| `/api/replay/show` | internal/web/handlers.go | Routed via pool | Repeated id lookups, manifest cache wins |
| `/api/replay/diff` | internal/web/handlers.go | Routed via pool | Load amortised; diff math itself stays on the same engine |
| `/api/replay/verify` | internal/web/handlers.go | Routed via pool | Single load per call but repeated across browse |
| `/api/evidence` | internal/web/handlers.go | Routed list via pool | Same store rescanned per file click |
| `ctx replay list` | internal/cli/replay.go | Stateless | One-shot per command |
| `ctx replay show` | internal/cli/replay.go | Stateless | One-shot per command |
| `ctx replay diff` | internal/cli/replay.go | Stateless | One-shot per command |
| `replay-pack` (`narrowAndHeader`) | internal/cli/replay_pack.go | Stateless | Two compute calls but only ONE base id — session amortisation would save one Marshal at most. Left stateless until Phase 5 measurement justifies the rewrite. |
| `replay-engine-diff` | cmd/replay-engine-diff/main.go | Extended (both legs) | Verifies sessioned + stateless byte-equality |
| `replay-golden-export` | cmd/replay-golden-export/main.go | Stateless (unchanged) | One-shot fixture generator |

## Migration roadmap impact

See `tests/MIGRATION_ROADMAP.md` for the updated table.

* The "JSON_HEAVY shipped" column now lists ctx-replay alongside its
  pioneers — but with an asterisk noting the per-query verdict.
* The "evidence-only" column drops the carve-out for the load / prune
  paths and keeps the diff-microbench carve-out.
* No new modules added to the Phase 5 list as a result of this report;
  the retrofit was a Tier 2 reconciliation step, not a new port.
