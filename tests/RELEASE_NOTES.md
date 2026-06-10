# Summit Pioneer Port — Release Notes

## Campaign CLOSED (2026-05-30) — v0.2.0-rust-campaign

The Go→Rust migration campaign opened by ADR-0002 closes today under
**[ADR-0004](../docs/adr/0004-campaign-close.md) (PROPOSED)**. 14 of
31 internal modules were processed (11 ported + 3 screened-skipped);
the remaining 17 modules were batch-screened to 14 SKIP / 2
EVIDENCE-ONLY-MEM / 1 SHIP-CANDIDATE-FOR-FOLLOWUP, none scheduled for
port. ADR-0002 remains ACCEPTED for its FFI thesis; ADR-0004 closes
the campaign that ADR-0002 opened.

### Final state

| Bucket | Count | Notes |
|--------|------:|-------|
| Shipped — full module | 4 | contract / scan / relations / focus |
| Shipped — per-fn or per-query | 5 | pack.from_where, replay.{load,list,prune}, symbols.lookup_sessioned |
| Evidence-only (memory bucket or perf-fail) | 6 crates / 10+ items | where, heatmap, braid, pack(4 fns), echo(small only), replay.diff, symbols(2 fns) |
| Screened-skipped via L1-L4 recipe | 3 | digest, config, walk |
| Batch-screened SKIP (remaining 17) | 14 | audit / cli / git / hooks / mcp / mix / model / noise / onboarding / security / testinsights / tokens / tui / web |
| Batch-screened EVIDENCE-ONLY-MEM | 2 | budget, render |
| Batch-screened SHIP-CANDIDATE-FOR-FOLLOWUP | 1 | skim (LOW confidence) |
| **Total processed** | 14 / 31 | 11 Rust crates compilable + tested |

Full per-module screen: [`tests/BATCH_SCREEN_TIER2_REMAINDER.md`](./BATCH_SCREEN_TIER2_REMAINDER.md).

### What stays after the campaign close

- **11 Rust crates** remain compilable and tested. `cargo test
  --features testing` continues to gate them on every PR.
- **9 shipped artifacts** continue as production opt-in via
  `-tags rust_contract` + per-module engine selectors:
  - `ctx contract verify --engine=rust`
  - `ctx pack --scan-engine=rust`
  - `ctx browse --relations-engine=rust` (web `/api/relations`
    wired through `RelationsPool`)
  - `ctx focus --focus-engine=rust` (mcp + braid wired through
    `OpenSessionPool`)
  - `ctx pack --pack-engine=rust` (from_where leg ships net 1.15×)
  - web `/api/replay/*` routes through `ReplayPool` (load/list/prune
    cached; diff inherits the load amortisation)
  - web `/api/definition` routes through `SymbolsPool`
    (lookup_sessioned 121-161×)
- **CI workflows** stay live:
  - `cross-compile.yml` (darwin-{amd64,arm64} × linux-{amd64,arm64};
    Windows best-effort)
  - `perf-regression.yml` (±10% Rust / ±5% Go envelope on the 9
    shipped artifacts)
  - Parity-diff CI (per-PR byte-exact diff between Go and Rust on
    shipped paths)
- **L1-L4 screening recipe** codified as the gate for any future
  port candidate. Three retained `*_screen_bench_test.go` files
  (digest / config / walk) act as regression guards if the underlying
  hot-path composition ever shifts.

### What doesn't change for users

- **Default `go build ./...` produces a pure-Go binary**, no cgo, no
  Rust toolchain dependency, no behavioural change. This is the
  operator-floor guarantee from ADR-0001 / ADR-0002 carried unchanged.
- **All Rust paths remain opt-in** at build time (`-tags
  rust_contract`) and runtime (per-module engine selectors). Default
  at runtime is `go` even on tagged builds.
- **Fail-soft fallback** to Go on FFI/decode error continues for
  every shipped path.
- **No CLI flag removed**; no engine selector deprecated.

### What is closed

- No new module ports under the current cgo+JSON shuttle (with or
  without sticky-handle) without an explicit ADR amendment.
- No expansion of evidence-only crates' surfaces.
- No new build tags introduced for the campaign without ADR amendment.

### v0.2.0-rust-campaign tag message (recommended)

```
v0.2.0-rust-campaign — Go→Rust migration campaign close.

14 of 31 internal modules processed under ADR-0002 sticky-handle FFI:
- 4 full modules shipped (contract, scan, relations, focus)
- 5 per-function/per-query artifacts shipped
- 6 crates as evidence-only (memory bucket or perf-fail under cgo floor)
- 3 modules screened-skipped via L1-L4 desk recipe
- 17 remaining modules batch-screened: 14 SKIP, 2 EV-ONLY-MEM, 1 followup

Campaign closed per ADR-0004 (PROPOSED 2026-05-30). 11 Rust crates remain
compilable + tested; cross-compile + perf-regression CI stay live;
default `go build ./...` produces pure-Go binary unchanged.

No further module ports under current FFI architecture without ADR amendment.
"全Rust化" reframed as "Calibrated Migration": Rust where the empirical
regime supports it, Go where it doesn't.
```

### Reversibility

Per ADR-0004 §Reversibility, the campaign can be reopened under: (1) an
FFI architecture change (FlatBuffers / Rust-side walker / WASM /
shared-memory wire format), (2) sticky-handle v2 with a multi-session
pool, or (3) a workload change (hosted ctx server, daemon-mode CLI).
Each requires its own ADR amendment with empirical justification.

The L1-L4 screening recipe and the dispatcher / sticky-handle / parity-
diff / cross-compile / perf-regression infrastructure are all reusable;
reopening costs only the per-module port effort, not a workflow rewrite.

### References

- [ADR-0004 (PROPOSED)](../docs/adr/0004-campaign-close.md) — the
  campaign-close decision.
- [`tests/BATCH_SCREEN_TIER2_REMAINDER.md`](./BATCH_SCREEN_TIER2_REMAINDER.md) — per-module L1-L4 prediction for the 17 remaining modules.
- [`tests/MIGRATION_ROADMAP.md`](./MIGRATION_ROADMAP.md) §"Amendment 2026-05-30" — final-state classification table.

---

## Tier 2 #8 — `walk` SCREENED-SKIPPED (2026-05-30)

Tier 2 #8 candidate was `internal/walk` (728 LOC source: `walk.go` 553 +
`secure.go` 96 + `timefilter.go` 79, 2 internal deps = `model` + `git`,
8-10 callers across CLI / web / MCP / pack / braid / relations / tui).
It is the **third application of the new screen-before-port recipe**
(after `digest` and `config`).

**Outcome**: no port. The module remains 100% Go. No `--walk-engine`
flag is introduced.

**Why it looked tempting**: the brief named `gitignore.MatchesPath`
(regex pattern matching) and `ParseTimeFilter` (calendar parsing) as
plausibly portable surfaces. Walk's raw latency is high (804 µs small /
12.2 ms medium / 143 ms large) so naive L1 reasoning would have flagged
the medium and large fixtures as port candidates.

**Why the recipe said skip**: pprof on `Walk_MediumTree` (500 files +
10-rule `.gitignore`) shows **97.09% of CPU in `syscall.syscall`**.
Breakdown:

- `countTextStats` (per-file `os.ReadFile` for line count + UTF-8 binary
  detection) — **79.69% cum** (the surprise; this was not in the brief's
  hot-path inventory).
- `os.Lstat` per node — **13.27%**.
- `os.ReadDir` per directory — **4.85%**.
- The named Rust-portable surface `gitignore.MatchesPath` — **0.081%
  cum** (10 ms out of 12 360 ms). A free pattern-matching kernel saves
  <0.1% of Walk.
- `inferRole` string ops — **<0.05%**.

**L4 per-function verdict**:

| Function | per-call | verdict |
| --- | ---: | --- |
| `ParseTimeFilter` | 56 ns | SKIP (sub-cgo-floor by ~900×) |
| `inferRole` / `isConfigFile` / `isDottedTestName` | <5 ns | SKIP (string-prefix checks, <0.05% of Walk) |
| `Walk_SmallTree` (50 files, no ignore) | 804 µs | SKIP (97% syscall, no portable slice) |
| `Walk_MediumTree` (500 files + .gitignore) | 12.2 ms | SKIP (97% syscall, gitignore matcher 0.08%) |
| `Walk_LargeTree` (5000 files + .gitignore + .ctxignore + mtime filter) | 143 ms | SKIP (same composition) |
| `countTextStats` | dominates Walk | SKIP (80% is `os.ReadFile` syscall; post-read UTF-8 + line count is sub-cgo-floor) |
| `buildCommitTimeIndex{Git,GoGit}` | n/a in screen | SKIP (shells out to `git` or walks go-git loose-objects — same shape as digest) |
| `SecretDenyMatcher.Matches` | <100 ns | SKIP (sub-cgo-floor, called rarely) |

**Caller shape**: all 8-10 callers are one-shot per CLI invocation, MCP
tool call, or HTTP request. The web handlers are the multi-call surface,
but each request must observe operator filesystem edits between calls
(verify-stale-on-each-call is the correct semantics). Sessioning a tree
walk over real-time filesystem state is incompatible with the
sticky-handle invariant — the corpus changes between calls.

**Artifacts retained**:

- `tests/WALK_SCREENING.md` — 1-page screening report (Step 0 source-
  read, Step 1 bench numbers + pprof, Step 2 L1/L2/L3/L4 application,
  lessons, recommended alternatives).
- `internal/walk/walk_screen_bench_test.go` — retained as the
  screening evidence and as a regression guard if the hot-path
  composition ever changes (e.g., if `countTextStats` is replaced by a
  lazy reader, or if the gitignore matcher is swapped).

**New screening rule (post-walk)**: a module's *named* portable surface
is not the same as its *measured* hot path. Walk's gitignore regex
matcher (the named candidate) is dwarfed by the per-file `os.ReadFile`
it triggers via `countTextStats` (the actual hot path) to populate
`model.FileInfo.Lines`. Anytime a module discovers paths from the
filesystem AND reads each file's bytes, expect >90% syscall regardless
of whatever "pattern" work happens around the I/O. Together with the
digest and config skips, this third consecutive skip locks in the
**>75% `syscall.syscall` → SKIP** rule as the dominant decision
criterion for filesystem-touching candidates.

**Pre-existing test bug resolved**: `TestWalkSince_NoMatches` in
`internal/walk/walk_test.go` previously failed after 2026-05-22 because
fixture files used real `t.TempDir()` mtimes while the test compared
them against a hard-coded logical `now`. Follow-up commit `4528c96`
fixed this by assigning controlled mtimes with `os.Chtimes`.

---

## Tier 2 #7 — `config` SCREENED-SKIPPED (2026-05-30)

Tier 2 #7 candidate was `internal/config` (479 LOC source: `config.go` 221
+ `roots.go` 258, 0 internal deps, ~20 caller files across CLI / web / MCP
/ pack / braid / security). It is the **second application of the new
screen-before-port recipe** (after `digest`).

**Outcome**: no port. The module remains 100% Go. No `--config-engine`
flag is introduced.

**Why it looked tempting (faintly)**: `LoadRoots` clears the 50 µs L1 bar
at the realistic n=10 corpus (56 µs/op) and `SaveRoots` clears it
comfortably (163 µs at n=10, 487 µs at n=100). Naive L1 reasoning would
have said "borderline port".

**Why the recipe said skip**: pprof on `LoadRoots/n=100` shows **80.7% of
CPU in `syscall.syscall`** (77% in `os.File.Close` driven by
`toml.DecodeFile`'s open/read/close flow). pprof on `SaveRoots/n=100`
shows **97.4% of CPU in `syscall.syscall`** (62% in `syscall.write` for
the temp file, plus mkdir/rename). The `BurntSushi/toml` parser is <7% of
Load total; the encoder is downstream of the write syscall. **The
Rust-portable slice (TOML parse/marshal of a <2 KB file) is too small to
clear the cgo floor (~50 µs round-trip) even at a 10× intrinsic
speedup.**

**L4 per-function verdict**:

| Function | per-call | verdict |
| --- | ---: | --- |
| `RootsPath` | 85 ns | SKIP (sub-cgo-floor, 1 alloc) |
| `Find` | 30 ns | SKIP (sub-cgo-floor) |
| `RemoveRoot` | 22 ns | SKIP (sub-cgo-floor) |
| `AddRoot` | 8.8 µs | SKIP (98% in `EvalSymlinks` syscall) |
| `Canonicalize` (isolated) | 8.6 µs | SKIP (confirms 98% of AddRoot is the syscall) |
| `LoadRoots` n=10 | 56 µs | SKIP (80% syscall, parser <7%) |
| `LoadRoots` n=100 | 421 µs | SKIP (same composition) |
| `SaveRoots` n=10 | 163 µs | SKIP (97% syscall) |
| `SaveRoots` n=100 | 487 µs | SKIP (same composition) |

**Caller shape**: all 20 caller files are one-shot per CLI invocation or
MCP tool call. The web `/api/roots` handlers are the only multi-call
surface, but each request reloads a page-cached <2 KB file — the
OS page cache already delivers the sub-ms read. A future "cache
RootsFile across HTTP requests with TTL" optimisation would be a Go-side
change, not a Rust port.

**Artifacts retained**:

- `tests/CONFIG_SCREENING.md` — 1-page screening report (Step 0 source-
  read, Step 1 bench numbers + pprof, Step 2 L1/L2/L3/L4 application,
  lessons).
- `internal/config/config_screen_bench_test.go` — retained as the
  screening evidence and as a regression guard if the hot-path
  composition ever changes (e.g., if registries grow to thousands of
  entries or if `BurntSushi/toml` is swapped for a different parser).

**New screening rule (post-config)**: "small + I/O-dominated + zero
deps" is the canonical SKIP shape. Any module matching all three of
{<500 LOC source, hot-path pprof >75% `syscall.syscall`, zero internal
deps} is SKIP without writing a Rust crate — the cgo floor (~50 µs
round-trip) is comparable to or greater than the total per-call cost,
and the portable slice (TOML parse, in-memory mutation) is too small to
clear the floor even at 10× intrinsic. Together with the digest skip,
this establishes that **pprof >75% `syscall.syscall` is SKIP regardless
of raw latency or LOC** — the two skips (digest at "medium + I/O via
Go-only dep + 1 internal dep" and config at "small + I/O via stdlib +
zero deps") bracket the syscall-bound region of the screen-out space.

## Tier 2 #6 — `digest` SCREENED-SKIPPED (2026-05-30)

Tier 2 #6 candidate was `internal/digest` (579 LOC source, 1 internal dep
on `internal/tokens`, 4 callers: CLI / braid / MCP). It is the **first
application of the new screen-before-port recipe** codified in PR #76
(Step 0 source-read → Step 1 5-minute Go bench → Step 2 L1/L2/L3/L4
verdict).

**Outcome**: no port. The module remains 100% Go. No `--digest-engine`
flag is introduced.

**Why it looked tempting**: `Generate` runs at 3-83 ms per call across
small/medium/large synthetic-repo benches — well above the ~50 µs cgo
floor. Naive L1 reasoning would have said "port".

**Why the recipe said skip**: pprof on the medium bench shows **83% of
CPU in `syscall.syscall` driven by go-git's loose-object filesystem
walk** (`go-billy.ChrootHelper.Open` 76% / `Tree.PatchContext` 56% /
`DiffTreeWithOptions` 44%). tiktoken BPE and tree-sitter — the only
Rust-portable sub-ops — do not register in the top 60 pprof nodes
(<0.59% each). The Rust-portable slice is <5% of total runtime; the
cgo floor would swallow any improvement on it. Reimplementing go-git
in Rust to ship a 5% improvement is wildly out of scope.

**L4 per-function verdict**:

| Function | per-call | verdict |
| --- | ---: | --- |
| `ParseSince` | 16 ns | SKIP (sub-cgo-floor, 0 allocs) |
| `WriteMarkdown` | 2.8 µs | SKIP (sub-cgo-floor) |
| `WriteJSON` | 5.6 µs | SKIP (sub-cgo-floor) |
| `Generate` small | 3.0 ms | SKIP (root cause is go-git I/O, not portable slice) |
| `Generate` medium | 18.7 ms | SKIP |
| `Generate` large | 82.5 ms | SKIP |

**Caller shape**: all 3 production callers (CLI / braid / MCP) are
one-shot per invocation. No daemon, no repeat surface inside a single
process lifetime. The `replay` data-access-amortisation lane does not
apply because the data-access itself (go-git) is the Go-only library
we cannot host in Rust.

**Artifacts retained**:

- `tests/DIGEST_SCREENING.md` — 1-page screening report (Step 0 source-
  read, Step 1 bench numbers, Step 2 L1/L2/L3/L4 application, lessons).
- `internal/digest/digest_screen_bench_test.go` — retained as the
  screening evidence and as a regression guard if Generate's hot-path
  composition ever changes (e.g., if go-git is swapped for a packfile-
  optimised variant).

**New screening rule (post-digest)**: when L1 raw latency PASSES but
pprof shows the dominant cost is in a Go-only library boundary
(filesystem, network, or a Go-bound CGO dep like tree-sitter), score
the Rust-portable slice in isolation BEFORE deciding to port. If the
portable slice is <10% of total runtime, skip — the cgo floor will
swallow the improvement and we'd ship a regression dressed as a feature.

## Tier 2 #5 — `ctx-symbols` scope-split port (2026-05-30)

Tier 2 #5 ports the **pure-compute layer** of `internal/symbols` to a
new `ctx-symbols` Rust crate. Inspection showed BOTH `apionly.go` and
`lookup.go` transitively depend on tree-sitter (the brief's L3 label
of apionly as "REGEX_HEAVY text extraction" was incorrect — apionly
parses source via `sitter.NewParser`). Per the brief's "do not port
tree-sitter" constraint, only the post-AST text rendering and the
post-extraction sort/filter are portable. The result is a textbook
scope-split: Go owns tree-sitter, Rust owns the cheap post-processing
PLUS a sticky-handle session that caches the pre-extracted corpus
across requests.

**Result — MIXED VERDICT (per-function)**:

| Function | Shape | vs Go (time) | vs Go (mem) | Status |
| --- | --- | ---: | ---: | --- |
| lookup sessioned (small)   | sticky-handle | **161.32×** | **−99.4% bytes** | SHIPPED |
| lookup sessioned (medium)  | sticky-handle | **132.66×** | **−98.7% bytes** | SHIPPED |
| lookup sessioned (large)   | sticky-handle | **140.26×** | **−98.6% bytes** | SHIPPED |
| lookup stateless           | NewPool / query / close per call | 0.94-0.98× | −5 to −9% | EVIDENCE-ONLY |
| apionly render             | stateless | **0.90×** | +4.7% | EVIDENCE-ONLY |

Lookup sessioned is the cleanest sessioned ship after focus (47-105×):
the win is amortising the Go-side walk + tree-sitter extract across N
queries against the same root. The Rust intrinsic sort/filter is
sub-µs — this is NOT a Rust-is-faster story; it is the same
data-access amortisation lane that ctx-where / ctx-focus /
ctx-relations / ctx-replay-load exploited.

**API decision**: ADDITIVE. New surface —
`ctx_symbols_apionly_render` (stateless),
`ctx_symbols_lookup_resolve` (stateless),
`ctx_symbols_lookup_session_open / _query / _close` (sessioned with
six query kinds: resolve, refs, find_references, stats — plus reserved
kinds for future expansion). Tree-sitter extraction stays Go-side via
the existing `TreeSitterExtractor`.

**Caller-routing**: `internal/web/handlers.go::handleDefinition` now
routes through `API.SymbolsPool.RoutedLookupResolve` (lazy-init per
root). All 10 other callers (CLI / MCP / pack / focus / skim / web
tree handler / noise / onboarding / where) are unchanged — single-shot
per command, no amortisation surface, and the apionly path is
EVIDENCE-ONLY so swapping would regress them.

**Parity**: byte-equal across 18 e2e paths (3 fixtures × {1 apionly + 5
lookup queries}) via `cmd/symbols-engine-diff -engine rust`. 37 Rust
lib + 10 regression + 8 sticky-handle + 2 parity tests pass.

**Soak**: 5K cycles on `TestLookupPool_Soak5K` (warm session) — 32 KB
HeapInuse growth. 5K cycles on `TestLookupPool_OpenCloseCycle5K`
(fresh pool / iter) — 229 KB growth. No leak.

**Operator flag**: `symbols.SetEngine("rust")` opt-in (no public
`--symbols-engine` CLI flag yet — wire when a CLI surface is needed).
Default `go build` is unchanged; the Rust path requires
`-tags rust_contract`.

**Lessons**:
- The "session-fit when corpus state amortises" rule from
  STICKY_HANDLE_POC_REPORT applies cleanly to symbols. Even with only
  one production caller (`/api/definition`), the per-request walk +
  extract is so expensive (4.8 ms on large) that any caller that
  hits the same root twice in a process lifetime benefits.
- L3 must be applied via direct source read, not via the brief's
  label. The brief described apionly as "REGEX/byte-scan" — code
  inspection showed tree-sitter.
- The scope-split pattern from braid + pack (orchestrator stays Go,
  pure-compute crosses to Rust) generalises one more layer: now
  "AST walk Go, post-AST processing Rust" is the proven template for
  any internal/* module backed by tree-sitter, libgit2, or other
  C-linked native libraries.

Full evidence: `crates/ctx-symbols/PHASE4_REPORT.md`,
`tests/SYMBOLS_BENCH_REPORT.md`.

---

## Tier 2 #4 — `ctx-replay` sticky-handle retrofit (2026-05-30)

Tier 2 #4 retrofits the **already-shipped (Phase 3, evidence-only)**
`ctx-replay` crate with an ADR-002 sticky-handle session API. Phase 3
had landed replay at net **0.15×** — the cgo+JSON shuttle floor swamped
a sub-2 µs Go diff baseline. This retrofit asks whether session
amortisation can recover the same way the ctx-relations retrofit went
from 1.97× to 32-244× by caching corpus state across calls.

**Result — CONDITIONAL PROMOTION (per-query verdict)**:

| Query | Sessioned vs Stateless | Sessioned vs Go baseline | Status |
| --- | ---: | ---: | --- |
| Diff (single_snap)       | **3.35×** | 0.05× (cgo floor) | EVIDENCE-ONLY |
| Diff (multi_snap_drift)  | **2.29×** | 0.08× (cgo floor) | EVIDENCE-ONLY |
| Diff (scoring_change)    | **3.22×** | 0.06× (cgo floor) | EVIDENCE-ONLY |
| Load (single_snap)       | **2.92×** | **2.95×** | SHIPPED |
| Load (multi_snap_drift)  | **1.92×** | **1.89×** | SHIPPED |
| Load (scoring_change)    | **3.16×** | **3.20×** | SHIPPED |
| Prune candidates         | **634×**  | **633×**  | SHIPPED |
| List (warm cache)        | session-amortised | — | SHIPPED indirectly |

**Web `/api/replay/*` net per-request wall clock**: 2-3× lower vs the
pre-sessioned cgo path. The diff handler keeps the cgo-bound compute
step (same engine) but inherits the cached-base-manifest load — so the
sessioned diff cells above act as evidence for one query inside a
shipped composite route, not as a regression in the handler.

**API decision**: ADDITIVE. The stateless Phase 3 surface
(`ctx_replay_diff`, `ctx_replay_selection_diff`, `ctx_replay_parse_duration`)
is unchanged. Three new exports —
`ctx_replay_session_open / _query / _close` — carry six query kinds
(list, load, diff, diff_ids, selection_diff, prune_candidates) on a
single opaque handle. Bench triplet (Sessioned / Stateless / GoBaseline)
covers diff / load / prune + a 5K open-close soak.

**Caller-routing**: web handlers (`/api/replay/list`, `/api/replay/show`,
`/api/replay/diff`, `/api/replay/verify`, `/api/evidence`) all route
through `API.ReplayPool` (lazy-init per snapshot dir). CLI
`ctx replay list / show / diff` stay stateless (one-shot per command).
`replay-pack` pre-pass also stays stateless: two `ComputeDispatched`
calls against the SAME base id would amortise just one cgo crossing —
not worth the API rewrite ahead of measurement.

**Parity**: 100% byte-equal Go vs Rust-stateless vs Rust-sessioned on
all three Phase 3 fixtures (single_snap / multi_snap_drift /
scoring_change) at n=2000 iterations end-to-end via the extended
`cmd/replay-engine-diff` harness.

**Tests**: 26 lib unit tests + 11 sticky-handle FFI integration tests
+ 3 parity tests + 5 regression tests pass. 4-thread × 25-query
concurrency test clean. 2000-cycle Rust soak + 5000-cycle Go soak
clean.

**New screening rule**: when a module's compute baseline is sub-cgo-floor
BUT its data-access cost is significant, sessioning can ship via the
**data-access amortisation lane** even when the compute lane stays
evidence-only — verdict is per-query, not per-module.

Full evidence: `crates/ctx-replay/PHASE4_REPORT.md`,
`tests/REPLAY_SESSION_REPORT.md`.

## Tier 2 #3 — `ctx-echo` BM25 evaluator (2026-05-30)

Tier 2 #3 ports `internal/echo` (800 LOC, BM25 + tokenize + chunk +
score + format, **0 internal deps, 1 caller**) to Rust as a clean
stateless BATCH module. It was the closest remaining shape to
`ctx-contract` (7-9×) and `ctx-scan` (15-27×); the brief estimated a
5-15× speedup. The actual result is **EVIDENCE ONLY** — performance
sub-2× across the board and memory regresses on medium/large fixtures.

| Module | Shape | Net speedup | Memory delta | Status |
| --- | --- | ---: | ---: | --- |
| `ctx-echo` small (1 KB) | BATCH 1× | **1.59×** | −93% bytes / +169% allocs | MARGINAL |
| `ctx-echo` medium (51 KB) | BATCH 1× | 0.83× | +25% bytes / +149% allocs | FAIL |
| `ctx-echo` large (525 KB) | BATCH 1× | 0.86× | +102% bytes / +149% allocs | FAIL |

**API decision**: STATELESS BATCH. Single `Evaluate(pack_path,
pack_body, opts) → Result` entry point, no corpus reuse, mirrors
ctx-contract / ctx-scan FFI conventions (borrowed inputs, heap-owned
CString out + free helper, panic-safe `catch_unwind`).

**Parity**: 100% byte-equal on 3 fixtures × canonical goal modulo ≤3
ULP BM25 sum-order divergence. Both Go's `map[string]int` iteration
order and Rust's `HashMap` iteration order are non-deterministic, so
the f64 BM25 sum's last 1-3 mantissa bits differ. Engine-diff and
parity tests both use 1e-9 relative tolerance — well below any
retrieval behavioural threshold. 21 Rust unit + 8 regression + 4
parity tests pass; all 5 `echo_test.go` cases mapped.

**Routing**: opt-in `--echo-engine rust` (default `go`).

**Honest mandate**: the brief flagged this as the most informative
scenario — REGEX_HEAVY shape that does NOT ship. Root cause is that
echo's hot path is `String + small-HashMap` allocation (chunk-body
`Vec::join`, per-token lowercase `String`, ScoredChunk deep-cloning
the entire chunk's tokens vector), NOT regex/byte-scan. Rust's stdlib
HashMap + small String allocations are competitive with Go's GC at
this scale (6k chunks → 600k small allocs on the large fixture).
Without the regex/byte-scan super-power, the cgo+JSON shuttle floor
(~10-15 µs) and the per-call deep-clone overhead drag Rust into
parity-or-worse territory.

**New screening rule** (carried into MIGRATION_ROADMAP.md): REGEX_HEAVY
classification is necessary but not sufficient. The hot path must
ALSO be `regex::find_iter` over `&[u8]` (not String/HashMap), with
Go baseline ≥100 µs/op on the smallest fixture, AND per-call result
JSON <10 KB. Echo fails (a) and edges (b).

**Files added**:
- `crates/ctx-echo/` — Rust crate (lib + 21 unit + 8 regression + 4
  parity tests + benches/echo.rs criterion + benches/memory.rs dhat).
- `internal/echo/rustbridge/bridge.go` — cgo binding (rust_contract tag).
- `internal/echo/{dispatch.go, dispatch_rust.go}` — engine selector.
- `internal/echo/echo_bench_test.go` — Go testing.B harness.
- `cmd/echo-golden-export/` — emits goldens under
  `tests/parity/echo-goldens/<fixture>/evaluate.json`.
- `cmd/echo-engine-diff/` — runs both engines on the same fixture,
  compares JSON output (with ULP-tolerant fallback) + reports speedup.
- `tests/echo-fixtures/{small,medium,large}_pack.md` — 1.3 KB / 51 KB
  / 525 KB synthetic packs.
- `crates/ctx-echo/PHASE4_REPORT.md`, `tests/ECHO_BENCH_REPORT.md`.

**CLI flag**: `--echo-engine go|rust` on `ctx echo`. Default `go`.
Pure-Go build rejects `rust` with an actionable error.

**Build**:
- Default: `go build ./...` (no Rust required).
- Tagged: `CGO_ENABLED=1 go build -tags rust_contract ./...` (requires
  `libctx_echo.a` from `cargo build --release` under
  `crates/ctx-echo/target/release/`).

---

## Tier 2 #2 — `ctx-pack` largest-module scope-split (2026-05-30)

Tier 2 #2 lands the campaign's LARGEST single port: `internal/pack`
(3.1 kLOC src+test, 10 internal deps, 8 inbound callers). Following
the scope-split pattern proven on braid, only the pure-compute layer
is moved; the orchestrator stays Go-side.

| Module | Shape | Function | Net speedup | Memory delta | Status |
| --- | --- | --- | ---: | ---: | --- |
| `ctx-pack` relevance | SESSIONED N× | score_relevance loop | **0.71× (FAIL on time)** | **−58% allocs (PASS)** | EVIDENCE-ONLY |
| `ctx-pack` diff | BATCH 1× × 1× | render | 0.42× | −95% allocs | EVIDENCE-ONLY |
| `ctx-pack` redact | STATELESS 1× | redact_lines | 0.41× | −97% allocs | EVIDENCE-ONLY |
| `ctx-pack` from_where | STATELESS 1× | parse | **1.15× (PASS)** | similar | **SHIPPED** |
| `ctx-pack` preset | STATELESS 1× | apply_preset | 0.005× | regress | EVIDENCE-ONLY |

**API decisions per function** (see `crates/ctx-pack/PHASE4_REPORT.md`):
- relevance — SESSIONED. Pack planner scores hundreds-to-thousands of
  files against the same goal/budget per invocation; keyword
  extraction amortises across the corpus. Sticky-handle pattern from
  ctx-where. Sessioned vs. stateless within the Rust path: **1.31–
  1.34× faster** across all corpus sizes. End-to-end vs. Go: 1.41×
  slower (JSON shuttle dominates).
- diff / redact / from_where / preset — STATELESS batch. Each fires
  once per `ctx pack` invocation.

**Honest mandate honored**: the sessioned relevance hypothesis was
that per-file score is heavy enough to amortise the cgo shuttle. The
data says otherwise — relevance is too cheap per file. Ships as
memory-bucket evidence-only with the actual numbers documented.

**Routing**: opt-in `--pack-engine rust` (default `go`). When the
flag is set, internal/pack's planner opens a Rust session handle once
per `buildPlan` call and scores every file through the pool. The Go
orchestrator (Pack / PackWithResult / watch.go / stdin.go IO) is
untouched.

**Determinism note**: as a precondition for byte-equal engine-diff,
`internal/pack/relevance.go::goalAliases` was converted from
`map[string][]string` to a sorted slice. Go's randomized map
iteration would otherwise produce per-process-stable but cross-
process-unstable signal orders, breaking the diff gate. Backwards
compatible — existing tests assert keyword-set membership, not order.

**E2E byte-diff**: 7/7 fixtures byte-equal across both engines
(small/medium/large relevance + 4 batch helpers). See
`cmd/pack-engine-diff/main.go`.

**Tier 2 META-LESSON #2**: even the LARGEST module the campaign has
attempted falls into the memory-bucket on perf when individual
function calls are sub-50µs. The screening criterion stays sharp:
sessioned-shape wins ONLY when corpus state actually amortises and
per-call work is large enough to outweigh the JSON shuttle. The
ctx-pack port proves the scope-split pattern can swallow 3 kLOC
modules without disturbing the orchestrator — that capability is
worth the evidence-only landing.

**Tier 2 queue update**:

| Candidate | Shape | Predicted ship posture |
| --- | --- | --- |
| `graph` | MULTI-QUERY corpus-resident | ✓ **best next** — expect ≥3× sessioned |
| `replay` query-mode | MULTI-QUERY | ✓ amortised; ev-only on per-query |
| `summarize` | BATCH sub-50 µs | ⚠️ likely evidence-only (memory win expected) |
| `digest` | BATCH sub-50 µs | ⚠️ likely evidence-only |
| `mixdown` | BATCH | ⚠️ likely evidence-only |
| `tree` | BATCH | ⚠️ likely evidence-only |

Full evidence: `crates/ctx-pack/PHASE4_REPORT.md`,
`tests/PACK_BENCH_REPORT.md`. ADR-002 stop conditions unaffected:
parity 100%, no regression on shipped modules, all 9 sister-crate
test suites still pass.

---

## Tier 2 KICKOFF — `ctx-braid` pure-compute EVIDENCE-ONLY (2026-05-30)

The first Tier 2 module lands. Per the screening criterion proven on
heatmap (Tier 1 #2), braid was predicted evidence-only before any
Rust code was written; the data confirms.

| Module | Shape | Net speedup | Memory delta | Status | Routing |
| --- | --- | ---: | ---: | --- | --- |
| `ctx-braid` (pure-compute layer) | BATCH 1× × 1× | **0.43-0.53× (FAIL)** | **−43-50% bytes, −27-51% allocs (PASS)** | **EVIDENCE-ONLY** | Opt-in `--braid-engine rust` for telemetry; default `go` |

**Scope refinement applied (per Tier 2 brief)**: only the pure-compute
layer (Allocate / Config Load+Validate / MergePaths / shellquote /
types / format renderers — ~800 LOC source) ported. `exec.go`
(orchestrator dispatching into focus/where/digest internal deps) and
`Run()` (the top-level orchestrator) stayed Go-side and call into the
Rust crate via the `Routed*` dispatchers. This is the first time the
campaign has split a single Go package across the FFI boundary — the
pattern generalises.

**Memory-only ship bucket**: braid clears the campaign's ≥30%
memory bar (−43-50% bytes/op, −27-51% allocs/op) but fails the BATCH
≥1.5× time bar. We ship as evidence-only with the memory delta
documented. Recommend the campaign formalise "evidence-only with
documented memory ≥30%" as a distinct ship classification so future
BATCH-shape ports get explicit credit for the memory win even when
time regresses.

**Tier 2 META-LESSON**:
- The screening criterion **predicted the verdict before writing code**.
  Future Tier 2/3 candidates should be screened first; positive (sessioned-shape) cases get implementation resources, negative (sub-50 µs Go baseline + 1× × 1× caller) cases get a written verdict justifying the screen result.
- The pure-compute scope-split pattern is **the way to handle modules
  with deep internal deps**. Don't chain-port the deps; port the math,
  keep the orchestrator Go-side, route via Routed* helpers.

**Tier 2 queue update (post-braid screening)**:

| Candidate | Shape | Predicted ship posture |
| --- | --- | --- |
| `graph` | MULTI-QUERY corpus-resident | ✓ **best next** — expect ≥3× sessioned |
| `pack` | MULTI-QUERY (braid+mcp+cli) | ✓ session-fit on amortised path |
| `replay` query-mode | MULTI-QUERY | ✓ amortised; ev-only on per-query |
| `summarize` | BATCH sub-50 µs | ⚠️ likely evidence-only (memory win expected) |
| `digest` | BATCH sub-50 µs | ⚠️ likely evidence-only |
| `mixdown` | BATCH | ⚠️ likely evidence-only |
| `tree` | BATCH | ⚠️ likely evidence-only |

**Recommend Tier 2 #2 = `graph`** to land at least one Tier 2 ship-bar
PASS before consuming the remaining BATCH-shape budget. Tier 2 BATCH
candidates should ship in a batch with a unified "memory-win bucket"
classification.

Full evidence: `crates/ctx-braid/PHASE4_REPORT.md`,
`tests/BRAID_BENCH_REPORT.md`. ADR-002 stop conditions unaffected:
parity 100%, no regression on shipped modules, all 167 sister-crate
tests still pass.

---

## Tier 1 COMPLETE (2026-05-30)

The Tier 1 sticky-handle campaign closes today. Three modules ran the
gauntlet under ADR-002; the screening criterion identified after
heatmap (Tier 1 #2) held for every subsequent decision. Tier 2 kicks
off next with the screen applied up-front.

| # | Module | Sessioned vs Stateless | Status | Routing |
| --- | --- | --- | --- | --- |
| 1 | `ctx-focus` | 47-105× | **SHIPPED** | Default opt-in via `-tags rust_contract` + `SetEngine("rust")` |
| 2 | `ctx-heatmap` | 0.40-0.52× (BATCH shape) | **EVIDENCE-ONLY** | Opt-in `--heatmap-engine rust` for telemetry; default `go` |
| 3 | `ctx-relations` | 32-104× (NEW today) | **SHIPPED** | `internal/web` `/api/relations` handler wired through `RelationsPool` |

**Tier 1 lessons codified in the screening criterion**:

- **MULTI-CALLER + same-corpus-repeated → sticky-handle wins big**
  (focus 47-105× / relations 32-104×). Go baseline ≥100 µs or many
  queries per session.
- **BATCH 1-caller × 1-shot at sub-50 µs Go baseline → cgo+JSON
  shuttle floor dominates → expect 0.4-1.2× net** (heatmap 0.40-0.52×,
  matching prior evidence-only finds on where/replay).

**Tier 2 kickoff candidate list (weeks 5-12)** — screen applied:

| Candidate | Workload shape | Predicted screen | Expected ship posture |
| --- | --- | --- | --- |
| `graph` | Repeated edge queries against fixed graph | MULTI-QUERY | ✓ **strong session-fit** — start here |
| `pack` | Repeated symbol/anchor lookups during pack | MULTI-QUERY | ✓ session-fit |
| `replay` query-mode | Repeated record queries against snapshot | MULTI-QUERY | ✓ session-fit |
| `summarize` | Pipeline pre-aggregation, 1-shot per file | BATCH | ⚠️ likely evidence-only |
| `digest` | One-shot per request | BATCH | ⚠️ likely evidence-only |
| `mixdown` | Multi-file aggregation, 1-call-per-cmd | BATCH | ⚠️ likely evidence-only |
| `tree` | Single walk per request | BATCH | ⚠️ likely evidence-only |

Recommend Tier 2 #1 = `graph` (closest shape parity with shipped
relations/focus).

---

## Tier 1 #3 — `ctx-relations` cross-module SHIPPED (2026-05-30)

The third (and final) Tier 1 module under ADR-002 retrofits the
already-shipped ctx-relations crate (Phase 2 / PR #64) with the
sticky-handle session pattern proven on ctx-where, focus, and now
relations itself. The Phase 2 stateless API stays in place verbatim —
the session API is a strict ADDITION serving multi-query callers.

### Headline numbers (Apple M4, `-benchtime=1s`)

| Query | Fixture | Sessioned | Stateless | Sessioned vs Stateless | Sessioned vs Go baseline |
| --- | --- | ---: | ---: | ---: | ---: |
| Edges | go_project    |  1,660 ns |  75,246 ns | **45.3×** |  **95.5×** |
| Edges | jsts_project  |  1,613 ns |  51,998 ns | **32.2×** |  **94.6×** |
| Edges | jvm_project   |  2,100 ns |  83,317 ns | **39.7×** |  **92.3×** |
| Edges | mixed_project |  1,573 ns |  81,909 ns | **52.1×** | **125.9×** |
| Refs  | mixed_project |    967.1 ns |  83,573 ns | **86.4×** | **210.9×** |
| Deps  | mixed_project |    875.1 ns |  91,448 ns | **104.5×** | **244.6×** |

Every (kind, fixture) cell clears the Tier 1 ≥3× bar; the narrowest
cell (Edges on jsts_project) still wins **32×**. The widest (Deps on
mixed_project) wins **104×**.

### What ships

- `crates/ctx-relations/src/session.rs` — new `RelationsSession` +
  per-kind query helpers (`refs`, `deps`, `callers`, `edges`,
  `index_summary`).
- `crates/ctx-relations/src/ffi.rs` — three new exports:
  `ctx_relations_session_{open,query,close}`.
- `crates/ctx-relations/include/ctx_relations.h` — regenerated header
  includes session surface.
- `crates/ctx-relations/tests/sticky_handle.rs` — 10 integration tests.
- `crates/ctx-relations/benches/sticky_handle.rs` — Criterion bench.
- `internal/relations/rustbridge/bridge.go` — `RelationsSession` Go
  type with atomic double-close + `runtime.SetFinalizer`.
- `internal/relations/dispatch_rust.go` — `RelationsPool` (lazy per-
  root session map) + `Routed{Refs,Deps,Callers,Edges}` helpers.
- `internal/relations/dispatch.go` — pure-Go pool stub for build
  parity.
- `internal/web/handlers.go` — `API.RelationsPool`; `handleRelations`
  routes through it.
- `cmd/relations-engine-diff/main.go` — extended to byte-diff the
  sessioned path against the stateless path.

### Caller routing decision

| Caller | Routing | Reason |
| --- | --- | --- |
| `internal/web` `/api/relations` | **Sticky-handle (NEW)** | Multi-query against same root — 32-52× win on edges. |
| `internal/cli/browse` | Indirect (uses web pool) | Doesn't query relations directly. |
| `cmd/relations-golden-export` | Stateless (unchanged) | Single-shot. |
| `cmd/relations-engine-diff` | Both — verification harness | Diffs sessioned vs stateless. |

### Soak

`TestRelationsSessionSoak_NoMonotonicGrowth` (5,000 open/close
cycles): HeapInuse mid-vs-end ratio = 0.99×. No leak.

### Test counts

- `ctx-relations`: 36 unit + 7 parity + 7 regression + 10
  sticky-handle = **60/60 pass**.
- 6 sister crates unchanged and green (38/28/32/34/26/35).

Full report: `crates/ctx-relations/PHASE4_REPORT.md`.
Bench detail: `tests/RELATIONS_SESSION_REPORT.md`.

---

## Tier 1 #2 — `ctx-heatmap` EVIDENCE-ONLY (2026-05-30)

The second Tier 1 module under ADR-002 has landed as **evidence-only**
(compiled and tested, NOT routed in production). `internal/heatmap`
(directory aggregation + Squarified treemap layout + 3 renderers —
914 LOC source + 337 LOC test) now has a Rust crate
(`crates/ctx-heatmap`) wired via a stateless batch FFI.

### Why evidence-only, not production-routed

Heatmap is invoked exactly **once per `ctx map` command** (aggregate →
squarify → render → done). There is no per-query corpus reuse — so
the sticky-handle pattern that earned ctx-focus its 47-105× win
**buys nothing here**. The campaign brief asked us to compare:

| Dimension | Stateless (Option B, chosen) | Sessioned (Option A) |
|---|---|---|
| FFI complexity | 5 thin entry points | session_open + 5 query fns + close + finalizer |
| Amortisation win | 1 (no second query) | 1 (still no second query) |
| Decision | **Option B per brief's "doesn't earn its complexity" guidance** | strictly worse |

The honest perf finding:

| Fixture | Reps | Go elapsed | Rust elapsed | **Net Speedup** | Tier 1 #2 BATCH ≥1.5× bar |
|---|---:|---:|---:|---:|---|
| small_metrics  | 5000 | 62.5 ms | 120.9 ms | **0.52×** | **FAIL** |
| medium_metrics | 1000 | 33.0 ms | 65.9 ms  | **0.50×** | **FAIL** |
| large_metrics  |  200 | 7.94 ms | 19.94 ms | **0.40×** | **FAIL** |

**Pure-Rust intrinsic** (no FFI): 1.16-1.24× — barely above noise.
The Go baseline is already 3-24 µs per call; the cgo+JSON shuttle
floor (~10-15 µs per FFI call × 5 calls per pipeline ≈ 60-80 µs) is
THE dominant cost. This is the same shape of finding as ctx-where
and ctx-replay (the campaign's prior two evidence-only crates).

### What still ships cleanly

- **Parity: 100% byte-exact** across all 3 fixtures × all 3 render
  formats (ASCII + JSON + plain). Squarify's floating-point parity
  (the highest-risk surface) matched bit-exact on first integration.
- **35/35 Rust tests pass** (17 lib + 15 regression + 3 parity); all
  13 Go heatmap_test.go cases mirrored; 6 sister crates unchanged
  and green.
- **Allocation count: Rust uses 60-87% FEWER allocations** on the
  end-to-end pipeline (49-67 vs Go's 82-525). Real memory-pressure
  win even though wall-clock is slower.
- **Build matrix**: default `go build ./...` produces pure-Go binary
  unchanged. `CGO_ENABLED=1 go build -tags rust_contract ./...` links
  **all 7 staticlibs** (contract / scan / relations / replay / where /
  focus / heatmap).
- **CLI flag**: `--heatmap-engine go|rust` on `ctx map`. Default `go`.
  Telemetry-only — passing `rust` will produce a 2-5× regression on
  current workloads.

### Build matrix

- Default `go build ./...`: pure-Go, unchanged behaviour.
- `CGO_ENABLED=1 go build -tags rust_contract ./...`: links all **7
  staticlibs** (contract / scan / relations / replay / where / focus /
  heatmap).
- Opt-in at runtime: pass `--heatmap-engine rust` to `ctx map`. The
  flag is honoured but expected to regress until either (a) the FFI
  surface collapses to a single pipeline call, (b) the workload grows
  beyond ~50 µs Go-side (e.g. with the future `--by churn` axis), or
  (c) a future ADR amendment authorises shared-memory wire format.

### Call sites updated

- `internal/cli/map.go` — adds `--heatmap-engine go|rust` flag; the
  pipeline dispatch is split into `map_dispatch.go` (Go-only) and
  `map_dispatch_rust.go` (rust_contract-tagged) so the CLI compiles
  cleanly under both build tags.
- `internal/heatmap/{dispatch.go, dispatch_rust.go, metrics.go}` —
  engine selector + Rust wrappers; fail-soft fallback to Go on any
  FFI error.

### Test counts (Rust + Go)

- `ctx-heatmap`: 17 lib (incl 7 new FFI round-trip + 1 version +
  format helpers) + 15 regression (all 13 Go cases + 2 edge cases) +
  3 parity = **35/35 pass**.
- Goldens: `tests/parity/heatmap-goldens/{small,medium,large}_metrics/`
  cover `aggregate_{tokens,files,symbols}.json`, `squarify.json`,
  `render_ascii.json`, `render_json.json`, `render_plain.json` —
  7 outputs × 3 fixtures = 21 golden comparisons.
- Sister crates (`ctx-contract`, `ctx-scan`, `ctx-relations`,
  `ctx-replay`, `ctx-where`, `ctx-focus`) all green and unchanged:
  31/29/21/18/24/20 lib tests pass.

### Honest verdict

Per the campaign brief's "honest mandate" — if stateless misses ≥1.5×,
re-evaluate Option A. We did: **Option A is provably worse for a
1-caller × 1-shot workload**. The correct shipping posture is
evidence-only with opt-in flag, identical to ctx-where and ctx-replay.

Full report: `crates/ctx-heatmap/PHASE4_REPORT.md`.
Bench detail: `tests/HEATMAP_BENCH_REPORT.md`.

---

## Tier 1 #1 — `ctx-focus` SHIPPED (2026-05-29)

The first Tier 1 module under ADR-002 has landed. `internal/focus`
(symbol-anchored mini-pack with one-hop expansion — 387 LOC source +
262 LOC test) now has a Rust crate (`crates/ctx-focus`) wired via the
sticky-handle session pattern proven on `ctx-where`.

### Sessioned vs Go baseline (n reps as per fixture size)

| Fixture | Go elapsed | Sessioned elapsed | **Speedup** | Tier 1 ≥5× bar |
|---|---:|---:|---:|---|
| small_repo  (n=2000) | 983.7 ms | 20.8 ms  | **47.39×**  | PASS |
| medium_repo (n=2000) | 4.18 s   | 39.6 ms  | **105.53×** | PASS |
| large_repo  (n=200)  | 1.17 s   | 22.0 ms  | **53.39×**  | PASS |

Per-query bench (go test -bench, b.N steady-state) confirms:

| Fixture | Sessioned ns/op | Go ns/op | Speedup | Mem (sessioned vs Go) |
|---|---:|---:|---:|---|
| small_repo  | 9,199 | 514,414 | **55.9×** | 1.6KB vs 263KB (-99%) |
| medium_repo | 16,758 | 2,146,473 | **128.1×** | 2.1KB vs 593KB (-99.6%) |
| large_repo  | 48,066 | 6,051,976 | **125.9×** | 11KB vs 1.4MB (-99%) |

The 47-105× headline is **higher** than `ctx-where`'s 11-19× because
focus does TWO BFS passes per call (Resolve + Expand) over the symbol
graph, both of which amortise across the cached corpus. The sticky-
handle pattern reusability for the remaining 24 modules is confirmed.

### Build matrix

- Default `go build ./...`: pure-Go, unchanged behaviour.
- `CGO_ENABLED=1 go build -tags rust_contract ./...`: links all **6
  staticlibs** (contract / scan / relations / replay / where / focus).
- Opt-in at runtime: pass `--focus-engine rust` to `ctx focus`. MCP and
  braid call sites also accept the engine selection (set via
  `focus.SetEngine`).
- Sister crates (`ctx-contract`, `ctx-scan`, `ctx-relations`,
  `ctx-replay`, `ctx-where`) all green and unchanged: 31/29/21/18/24
  lib tests + their parity/regression suites all pass.

### Soak + e2e

- 10K-cycle open/query/close soak on medium_repo: HeapInuse stable
  (within 1.5× noise floor, no monotonic growth). **Zero leaks.**
- `cmd/focus-engine-diff` byte-equality verified across all 3 fixtures
  between Go ↔ stateless Rust ↔ sessioned Rust.

### Test counts (Rust + Go)

- `ctx-focus`: 20 lib (incl 12 new FFI tests covering session open/
  resolve/expand/pack/close + concurrent + bad-JSON + null-handle) +
  3 parity + 6 regression + 8 sticky-handle integration = **37/37
  pass**.
- Goldens: `tests/parity/focus-goldens/{small,medium,large}_repo/`
  cover `resolve.json`, `expand_hops1.json`, `expand_hops2.json`,
  `pack.json` — all 4 entry points per fixture, every branch.

### Call sites updated

- `internal/cli/focus.go` — opens a `focus.OpenSessionPool` once,
  uses it for Resolve + Expand. Adds `--focus-engine go|rust` flag.
- `internal/mcp/server.go` — same session-pool pattern in the
  `ctx_focus` MCP handler.
- `internal/braid/exec.go` — same session-pool pattern in the focus
  strand executor.

Full report: `crates/ctx-focus/PHASE4_REPORT.md`.
Bench detail: `tests/FOCUS_BENCH_REPORT.md`.

---

## ADR-002 RATIFIED (2026-05-29) — sticky-handle FFI ACCEPTED; campaign ACTIVE

Leadership redefined the migration goal as full Go→Rust conversion of
all 30+ `internal/*` modules ("全Rust化"). A sticky-handle FFI
Proof-of-Concept on `ctx-where` was authored and benched the same day
on branch `adr/002-sticky-handle`. The PoC cleared every gate; ADR-002
is now **ACCEPTED**; ADR-001 Option D (Freeze) is **SUPERSEDED**.

### PoC headline (Go baseline → sessioned Rust, all 3 fixtures)

| Fixture | Go elapsed | Sessioned elapsed | **Sessioned / Go** | ≥5× GO bar |
|---|---:|---:|---:|---|
| small_repo  (n=2000) | 522 ms  | 38.6 ms  | **13.52×** | PASS |
| medium_repo (n=2000) | 1.290 s | 68.9 ms  | **18.72×** | PASS |
| large_repo  (n=20)   | 1.923 s | 170.6 ms | **11.27×** | PASS |

Stateless Rust (the FROZEN-era opt-in shape) measured at 0.96× / 0.98×
/ 0.98× — i.e., the regression that drove ADR-001's Freeze decision is
real but is fully unlocked by session-shaped FFI.

### Soak test

10,000-cycle open/query/close against medium_repo:

- Baseline HeapInuse: 1,032,192 B
- Midpoint (5,000 cycles): 991,232 B
- End (10,000 cycles): 974,848 B — **−5.6% vs baseline** (heap went
  DOWN, not up)
- Process RSS held ~32 MB throughout (manual `top -pid` check)
- **Zero leaks detected.**

Full report: [`tests/STICKY_HANDLE_POC_REPORT.md`](./STICKY_HANDLE_POC_REPORT.md).
Test counts: ctx-where 24 lib (incl. 6 new FFI: session open/search/close
+ idempotent null close + null-handle safe search + bad-JSON open
rejection + multi-query parity vs stateless + concurrent-query safety)
+ 3 parity + 8 regression = **35 tests, all pass**. Sister crates
(`contract`, `scan`, `relations`, `replay`) all green and unchanged.

### Status changes

- **ADR-001**: ACCEPTED → **SUPERSEDED by ADR-0002** (audit trail
  preserved in `docs/adr/0001-ffi-shuttle-redesign.md`).
- **ADR-002**: PROPOSED → **ACCEPTED**.
- **Phase 4 LOOKUP_HEAVY**: CLOSED → **RESUMED** under sticky-handle.
- **`ctx-where`**: "evidence-only / NOT recommended for production"
  → **PoC-validated sticky-handle target**. Will ship behind the new
  session API once the dispatcher integration lands on `main`.
- **`ctx-replay`**: remains evidence-only for now; ADR-002 §"Sticky-
  handle technical sketch" flags that replay's per-call corpus differs
  per-pair in production, so sticky-handle only helps repeated-pair
  workloads. Re-evaluated post-Tier-2 with the `replay query-mode` port.

### Campaign begins — Tier 1 targets (next 4 weeks)

Per [`tests/MIGRATION_ROADMAP.md`](./MIGRATION_ROADMAP.md) §"Campaign
Execution Status (2026-05-29)":

1. **`focus`** — selects top-N files relevant to a query; ranking
   shares scoring code with `where`. Direct sticky-handle beneficiary.
2. **`heatmap`** — touches the same walked corpus N times with
   different filters; today repeats walk per call.
3. **`relations` cross-module queries** — already has a crate; add a
   session API mirroring `ctx-where`'s.

Tier 2 (weeks 5-12): `summarize`, `pack`, `digest`, `replay`
query-mode, `mixdown`, `graph`, `tree` (7 modules). Tier 3 (weeks
13-25): the write-side opt-in cache modules (~12 modules, needs
multi-session pool — mid-game milestone).

### Elapsed projection

**3-4 calendar months** for the full 25-module campaign for one or two
Rust-fluent engineers (~10-15 person-weeks engineering, plus
review/perf-regression/CI overhead). Each module must still clear the
**≥1.5× net OR ≥30% memory OR documented strategic value** bar; modules
that fail their bar may remain Go ("全Rust化 where economically viable"
softened-goal fallback per ADR-002 §"Reversibility"). The stop
conditions in MIGRATION_ROADMAP §"Stop conditions" remain in force.

### Honesty note on the rapid amendment

ADR-001 ACCEPTED → ADR-002 PROPOSED → PoC → ADR-002 ACCEPTED + ADR-001
SUPERSEDED all landed on 2026-05-29. Both ADRs are honest reads of their
respective decision contexts: ADR-001 optimised for the program scope at
acceptance time; ADR-002 amends because the scope itself changed by
leadership input; the PoC empirically validated ADR-001's secondary
path B. The same-day turnaround is recorded here rather than papered
over. ADR-002 §"Open questions" #7 recommends adding a "strategic inputs
assumed stable" section to the ADR template so future ADRs flag their
dependency on leadership-set goals earlier.

See [ADR-002](../docs/adr/0002-sticky-handle-ffi-amendment.md) for the
full amendment, the per-criterion PoC result table, cost projection,
reversibility plan, and open questions. See
[`tests/STICKY_HANDLE_POC_REPORT.md`](./STICKY_HANDLE_POC_REPORT.md) for
the bench methodology, per-fixture numbers, soak protocol, and Tier
recommendations.

The FROZEN content below is preserved as the formerly-current decision
of record so users tracking the release history can follow the
narrative; it is **not** the current decision.

---

## FORMERLY FROZEN (superseded) — Migration Program FROZEN snapshot (2026-05-29)

The Go→Rust migration program is **frozen** per
[ADR-001](../docs/adr/0001-ffi-shuttle-redesign.md) (accepted 2026-05-29,
Option D — Freeze). Final shipping state:

**Shipped (production opt-in, `-tags rust_contract`)**

| Module | Net speedup | Memory | Runtime flag |
|---|---:|---:|---|
| `internal/contract` (`ctx-contract`) | 7-9× | — | `ctx contract verify --engine=rust` |
| `internal/scan` (`ctx-scan`) | 15-27× | — | `ctx pack --scan-engine=rust` |
| `internal/relations` (`ctx-relations`) | 1.97× time | −73% bytes / −84% allocs | `ctx browse --relations-engine=rust` |

**Evidence-only — NOT recommended for production**

| Module | Net speedup | Why not | Reference |
|---|---:|---|---|
| `internal/where` (`ctx-where`) | **0.92-0.97×** (regression) | cgo+JSON shuttle eats the 30-40× intrinsic margin; LOOKUP_HEAVY shape falls outside the shuttle's regime | `tests/WHERE_BENCH_REPORT.md`, ADR-001 |
| `internal/replay` (`ctx-replay`) | **0.15×** (massive regression) | Go baseline at ~1.8 µs/call is already below the cgo crossing cost (~10 µs); shuttle cannot recover | `tests/REPLAY_BENCH_REPORT.md`, ADR-001 |

The `crates/ctx-where/` and `crates/ctx-replay/` Rust crates remain in the
tree as compiled/tested evidence of the shuttle's failure mode, and to
preserve the memory-only wins (`where` -36% heap, `replay` -26% heap per
call). **Do not extend either crate**; do not add new LOOKUP_HEAVY ports
without amending ADR-001.

**Build instructions (opt-in)**

```bash
# Default — pure-Go binary, no Rust toolchain needed, no behavior change.
go build ./cmd/ctx

# Rust-enabled — links libctx_{contract,scan,relations,where,replay}.a.
# Prerequisite: cargo build --release on each shipped crate.
(cd crates/ctx-contract  && cargo build --release)
(cd crates/ctx-scan      && cargo build --release)
(cd crates/ctx-relations && cargo build --release)
CGO_ENABLED=1 go build -tags rust_contract -o ctx-rust ./cmd/ctx

# Recommended runtime engines (the wins):
ctx-rust contract verify --engine=rust    <pack-file>
ctx-rust pack            --scan-engine=rust ...
ctx-rust browse          --relations-engine=rust ...
```

See [`docs/RUST_OPT_IN_BUILD.md`](../docs/RUST_OPT_IN_BUILD.md) for the
full per-platform build guide, parity verification, and troubleshooting.

**Frozen scope** — Phase 4 (LOOKUP_HEAVY: `focus`, `heatmap`) and all
subsequent expansion is CLOSED. Re-opening requires an amendment to
ADR-001. See `tests/MIGRATION_ROADMAP.md` §"Migration Program — Final
State (2026-05-29)" for the full posture.

---

Branch: `summit/contract-rust-port` (Pioneer, T-01..T-27)
Phase 1 branch: `phase1/scan-rust-port` (Phase 1, scan)
Phase 2 branch: `phase2/relations-rust-port` (Phase 2, relations)
Phase 3 branch: `phase3/where-replay-rust-port` (Phase 3, where + replay parallel)
Goal: "golang to rust" — port core Go modules to Rust behind opt-in
build tags. Pioneer module: `internal/contract` (969 LOC, 6 source
files, 0 internal deps). Phase 1 module: `internal/scan` (218 LOC
source + 76 LOC test, 2 internal deps used). Phase 2 module:
`internal/relations` (1318 LOC source + 880 LOC test, 2 internal deps
used). Phase 3 modules: `internal/where` (1110 LOC source + 1155 LOC
test, LOOKUP_HEAVY fragility test) and `internal/replay` (829 LOC
source + 610 LOC test, JSON_HEAVY safer-shape validator).

## Phase 3 (where + replay) — WIRED (opt-in via `-tags rust_contract`, `--where-engine=rust` / `--replay-engine=rust`)

Two parallel ports landed:

- `crates/ctx-where` ships the LOOKUP_HEAVY port — Levenshtein DP +
  scoring/identifier splitting + suggest_similar. Parity green across
  3 fixtures (small/medium/large 1000-file generated repo).
- `crates/ctx-replay` ships the JSON_HEAVY port — Compute +
  ComputeSelectionDiff with SortSelectionDiff, BuildManifest with
  SHA-256, store I/O, and the d/w-extended ParseDuration. Parity green
  across 3 fixture pairs.

**End-to-end fragility verdict** (cgo overhead included):

| Module | Net speedup | Exit criterion | Status |
|---|---:|---:|---|
| `where` | **0.92×-0.97×** | ≥1.3× | **FAIL — STOP LOOKUP_HEAVY for Phase 4** |
| `replay` | **0.15×** | ≥4× | **FAIL — concern logged, ship anyway** |

Intrinsic speedups (in-process, no cgo) are 30-40× for `where` and
5-7× for `replay`, matching the original predictions. The cgo+JSON
shuttle eats the entire margin because it must marshal the pre-walked
file corpus (where) or two manifests (replay) on every call.

Memory wins persist across the cgo boundary:
- `where`: ~36× less heap per call vs Go
- `replay`: ~26% less heap per call vs Go

See `tests/WHERE_BENCH_REPORT.md` and `tests/REPLAY_BENCH_REPORT.md`
for the per-fixture numbers, and the per-crate `PHASE3_REPORT.md`
files for the lessons-learned section.

The per-PR perf-regression CI workflow (the last hard blocker per
`tests/MIGRATION_ROADMAP.md`) lands in this PR at
`.github/workflows/perf-regression.yml`. It runs criterion benches on
every ported crate plus the Go bench equivalents, compares each PR
against the latest main-branch baseline artifact, and fails the gate
when Rust regresses >10% or Go regresses >5%. First-run safety: when
no baseline artifact exists the workflow emits a baseline-only report
and passes.

`.github/workflows/cross-compile.yml` is extended to build the two new
crates against the 4-target matrix.

New CLI surface (default tags, no behavior change unless opted in):

- `ctx where --where-engine=rust` — only honoured on
  `-tags rust_contract` builds; falls back to Go silently on FFI errors.
- `ctx replay diff --replay-engine=rust` — same opt-in semantics.

**NOT recommended for production**: `--where-engine=rust` and
`--replay-engine=rust` regress wall-clock time vs the default Go path on
every measured fixture (where: 0.92-0.97× net; replay: 0.15× net). The
crates ship as evidence-only — to document the cgo+JSON shuttle's
failure regime and to preserve the memory wins (where -36% heap,
replay -26% heap per call). See `tests/WHERE_BENCH_REPORT.md`,
`tests/REPLAY_BENCH_REPORT.md`, and
[ADR-001](../docs/adr/0001-ffi-shuttle-redesign.md) for the full
analysis and the freeze decision.



## Phase 2 (relations) — WIRED (opt-in via `-tags rust_contract`, `--relations-engine=rust`)

The `crates/ctx-relations` crate ships the REGEX_HEAVY + IO Phase 2
port: byte-exact parity against Go's `internal/relations` across 7
language fixtures (Go/JS+TS/Vue/Svelte/Python/Java/Kotlin/PHP/Swift)
and 2 entry points (Build, BuildCached), with end-to-end engine
diffing confirmed by `cmd/relations-engine-diff` on 4 mixed-language
fixtures.

Bench summary (Apple M4, macOS 26):

| Fixture          | Go ns/op  | Rust ns/op | Speedup |
|------------------|-----------|------------|---------|
| go_project       | 169,167   | 84,300     | 2.01×   |
| jsts_project     | 188,829   | 90,950     | 2.08×   |
| jvm_project      | 244,054   | 135,650    | 1.80×   |
| mixed_project    | 212,413   | 107,770    | 1.97×   |

Memory profile (dhat-rs vs `runtime.MemStats`, mixed_project × 200):

| Metric             | Go         | Rust       | Reduction |
|--------------------|------------|------------|-----------|
| bytes/Build        | 116,589    | 31,289     | −73.2%    |
| allocs/Build       | 1,175      | ~190       | −83.8%    |

Cross-compile coverage (hard-blocker #4 in the migration roadmap) is
resolved by `.github/workflows/cross-compile.yml`, which builds all
four Rust crates (`ctx-contract-probe`, `ctx-contract`, `ctx-scan`,
`ctx-relations`) against darwin-amd64 / darwin-arm64 / linux-amd64 /
linux-arm64. A separate `probe (host)` job preserves the legacy
`ci/cross-compile-probe.sh` smoke check.

Phase 2 reuses the `rust_contract` build tag — a single
`-tags rust_contract` CGO build now links three Rust crates. The
per-crate engine selectors (`--engine`, `--scan-engine`,
`--relations-engine`) remain independent, so callers can mix-and-match.

Test counts after Phase 2:

- `ctx-relations`: 29 unit + 7 parity + 7 regression = **43 tests** (43/43 pass)
- `ctx-scan`: 32/32 pass (unchanged)
- `ctx-contract`: 78/78 pass (unchanged)
- **All Rust crates: 153/153 pass.**

See `crates/ctx-relations/PHASE2_REPORT.md` and
`tests/RELATIONS_BENCH_REPORT.md` for the full close-out, and
`tests/MIGRATION_ROADMAP.md` for the updated Phase 2 → Phase 3
estimates.

## Phase 1 (scan) — WIRED (opt-in via `-tags rust_contract`, `--scan-engine=rust`)

The `crates/ctx-scan` crate ships the REGEX_HEAVY Phase 1 port: byte-
exact parity against Go's `internal/scan`, **15-27× intrinsic speedup**
on the synthetic bench corpus (vs the roadmap's 7-9× prediction for
REGEX_HEAVY paths), and end-to-end CLI parity confirmed by running
`ctx pack --scan-engine={go,rust}` on identical fixtures.

The Phase 1 port reuses the pioneer's `rust_contract` build tag rather
than introducing a new `rust_scan` tag (see
`crates/ctx-scan/PHASE1_REPORT.md` for the rationale: one CGO matrix,
one operator flag, independent per-crate engine selectors so a binary
can mix `--engine=rust` on contract with `--scan-engine=go` on pack).

Test counts after Phase 1:

- `ctx-scan`: 21 unit + 4 parity + 7 regression = **32 tests** (32/32 pass)
- `ctx-contract`: 31 unit + 40 parity + 7 regression = **78 tests** (78/78 pass, unchanged)

See `crates/ctx-scan/PHASE1_REPORT.md` and `tests/SCAN_BENCH_REPORT.md`
for the full close-out, and `tests/MIGRATION_ROADMAP.md` for the
updated Phase 1 → Phase 2 estimates.

## Pioneer (contract) — WIRED (opt-in via `-tags rust_contract`)

The Rust crate is built, tested, and byte-exact parity-verified against the Go
reference implementation, **and is now linked into the `ctx` CLI behind an
opt-in build tag**. The Go implementation remains the default production code
path — `go build ./...` continues to ship a pure-Go binary with no Rust
toolchain dependency. A binary built with `-tags rust_contract` exposes the
Rust path via `ctx contract verify --engine=rust`.

T-26 (FFI shim) and T-27 (CLI integration) — both originally deferred — are
now landed:

- T-26: `crates/ctx-contract/src/ffi.rs` (7 extern "C" functions) +
  `crates/ctx-contract/include/ctx_contract.h` (cbindgen-generated) +
  staticlib `libctx_contract.a` (~22 MiB release build).
- T-27: `internal/contract/rustbridge/bridge.go` (cgo wrappers, raw-JSON
  FFI surface) + `internal/contract/dispatch{,_rust}.go` (build-tag-gated
  dispatcher) + `--engine {go|rust}` flag on `ctx contract verify`.

End-to-end byte-exact parity between the Go and Rust engines is verified by
running the same fixture through both engines and diffing the JSON output:

```
/tmp/ctx-rust contract verify --engine=go   --format=json --response=resp.txt pack.md > go.json
/tmp/ctx-rust contract verify --engine=rust --format=json --response=resp.txt pack.md > rust.json
diff go.json rust.json   # empty
```

## What ships

- **Rust crate** `crates/ctx-contract/` — full Phase 2 P1+P2 surface:
  `Build`, `ExtractReferences`, `ParseFromPack`, `StripContractBlock`,
  `Verify`, `Render`, `Embed{Markdown,XML,Plain,JSON}` (10 functions).
- **66 passing tests** in the Rust crate:
  - 19 unit tests (in-crate, covering format/hash/embed/verify/builder/parse_refs)
  - 40 cross-implementation parity tests (4 packs x 10 functions, all
    byte-exact against Go-generated goldens)
  - 7 regression tests pinning Phase 4 CONFIRMED findings (F-01..F-05, F-07)
- **Go-side `SetNowFunc` clock seam** (+30 LoC in `internal/contract/build.go`,
  opt-in, zero production impact).
- **Go-side golden exporter** `cmd/contract-golden-export` — deterministic
  JSON fixture emitter for the parity harness.
- **Cross-compile probe** `ci/cross-compile-probe.sh` +
  `crates/ctx-contract-probe/` — sweeps `{darwin,linux} x {amd64,arm64}`.
  Verified darwin-arm64 (host). Other targets logged as skip when their
  toolchain is not installed — documented gap, not a regression.
- **Golden corpus** under `tests/parity/goldens/` — 40 JSON fixtures.
- **Pack fixtures** in `internal/contract/testdata/` — `empty_pack.md`,
  `json_pack.json`, `multi_lang_pack.md`.

## What does NOT ship (deferred to follow-up Summit / apex)

| Item | Why deferred |
|---|---|
| Criterion benchmark suite | Phase 2 T-25; was P3 priority, deferred per Phase 3 retry brief |
| F-06 (cross-platform errno→string parity) | Phase 4 LIKELY finding; Phase 5 scope-cut |
| F-08 (non-UTF-8 line handling parity) | Phase 4 LIKELY finding; Phase 5 scope-cut |
| L-02 hardened mode (`follow_symlinks=false`) | Phase 2 D-3 left at parity-only |
| L-08 unbounded JSON parse size cap | Phase 2 D-3 left at parity-only |

## Rollback

This change introduces **new files only** (Rust crate, Go CLI tool, scripts,
fixtures, goldens) **plus one minimal Go file modification** (`build.go`,
+30 LoC clock seam). To roll back:

1. **Preferred** — `git revert <commit-hash>` for each of the recommended
   4-5 commits (in reverse order if multiple).
2. **Manual** — delete `crates/`, `cmd/contract-golden-export/`, `ci/`,
   `tests/`, `internal/contract/testdata/{empty_pack.md,json_pack.json,multi_lang_pack.md}`
   and revert `internal/contract/build.go` to remove `nowFn` / `SetNowFunc`.

The Go production code path is **untouched** (the only Go change is an opt-in
seam that defaults to `time.Now`); rollback risk to `ctx` end users is **zero**.

## Verification commands

```bash
# Rust build (release-quality, optimized)
cargo build --manifest-path crates/ctx-contract/Cargo.toml --release

# Rust test suite — 66 tests (19 unit + 40 parity + 7 regression) + FFI smoke
cargo test --manifest-path crates/ctx-contract/Cargo.toml --features testing

# Regenerate goldens (only needed if internal/contract/ changes shape)
go run ./cmd/contract-golden-export ./internal/contract/testdata ./tests/parity/goldens

# Cross-compile probe (darwin + linux x amd64 + arm64; skips on missing toolchain)
bash ci/cross-compile-probe.sh

# Go-side parity-seam smoke (production behavior unchanged)
go test ./internal/contract/...
```

## Build instructions (T-27 integration)

The Go binary now supports two contract engines selected at build + runtime:

```bash
# Default — pure Go, no Rust toolchain required.
go build ./cmd/ctx

# Rust-enabled — links libctx_contract.a from the Rust crate.
# Prerequisite: cargo build --release on crates/ctx-contract first.
cd crates/ctx-contract && cargo build --release && cd ../..
CGO_ENABLED=1 go build -tags rust_contract -o ctx-rust ./cmd/ctx

# Select the engine at runtime:
ctx-rust contract verify --engine=go   <pack-file>   # Go path (default)
ctx-rust contract verify --engine=rust <pack-file>   # Rust path via cgo
```

The `rust_contract` build tag is the single switch — without it, the
`internal/contract/rustbridge` package is excluded from compilation and the
dispatcher in `internal/contract/dispatch.go` returns an explicit error if a
user passes `--engine=rust`. The Go-only build does not link against
libctx_contract and has no cgo dependency.

The default `ActiveEngine` is `"go"` even on a tagged build; selecting Rust
requires an explicit `--engine=rust` (or programmatic `SetEngine("rust")`).

## Strategic posture

This pioneer is the **smallest defensible unit** of a Go→Rust migration on
this codebase: a leaf module with no internal dependencies, a small public
surface, and a clear parity contract. It is intentionally not wired in —
the point is to **prove the harness, the parity protocol, and the engine
recipe**, not to flip the production code path. Once T-26 + T-27 land, the
same protocol can be re-used to port the next module up the dependency
graph (`pack` candidates, then progressively wider).
