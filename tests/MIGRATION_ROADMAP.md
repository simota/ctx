# ctx Go→Rust Migration Roadmap

**Document type**: Strategic technical roadmap
**Status**: **CLOSED 2026-05-30 per [ADR-0004](../docs/adr/0004-campaign-close.md)
(PROPOSED) — calibrated migration end-state.** 14 of 31 internal modules
processed; 9 distinct shipped artifacts; 17 remaining modules batch-screened
to 14 SKIP / 2 EVIDENCE-ONLY-MEM / 1 SHIP-CANDIDATE-FOR-FOLLOWUP
([`BATCH_SCREEN_TIER2_REMAINDER.md`](./BATCH_SCREEN_TIER2_REMAINDER.md)).
No further module ports under the current cgo+JSON shuttle (with or without
sticky-handle) without an ADR amendment. The history below (ACTIVE under
ADR-002, FROZEN under ADR-001) is preserved as the campaign's audit trail;
the "Final state (2026-05-30, ADR-0004)" section near the top of the
amendment is the current decision of record.
**Author**: Summit Phase 6 (Step D); finalized 2026-05-29; amended 2026-05-29 (ADR-002 proposed); ratified 2026-05-29 (ADR-002 accepted post-PoC); CLOSED 2026-05-30 (ADR-004 proposed)
**Audience**: Engineering leadership, platform owners, release management
**Source evidence**: `tests/SUMMIT_EXECUTION_REPORT.md`, `tests/BENCH_REPORT.md`,
`tests/NEXT_MODULES_ANALYSIS.md`, `crates/ctx-contract/PHASE5_REPORT.md`,
`internal/contract/rustbridge/T27_INTEGRATION_REPORT.md`, `tests/RELEASE_NOTES.md`,
`tests/STICKY_HANDLE_POC_REPORT.md` (PoC outcome),
`docs/adr/0001-ffi-shuttle-redesign.md`, `docs/adr/0002-sticky-handle-ffi-amendment.md`
**Decision recorded**: 2026-05-29 (ADR-001 ACCEPTED — Option D Freeze) → 2026-05-29 (ADR-002 PROPOSED — amend, activate Option B Sticky-handle) → 2026-05-29 (ADR-002 ACCEPTED post-PoC; ADR-001 SUPERSEDED)
**Time horizon**: Tier 1 next 4 weeks; Tier 2 weeks 5-12; Tier 3 weeks 13-25 (3-4 calendar months total for 25-module campaign for one or two Rust-fluent engineers).

---

## Executive Summary

The migration program is **FROZEN as of 2026-05-29** per ADR-001 (Option D —
Freeze). **3 modules shipped** under the cgo+JSON shuttle pattern
(`contract` 7-9×, `scan` 15-27×, `relations` 1.97× time + −73% bytes / −84%
allocs). **2 modules ship evidence-only, NOT recommended for production
routing**: `ctx-where` (LOOKUP_HEAVY, net 0.92-0.97×) and `ctx-replay`
(JSON_HEAVY micro, net 0.15×) — both fall outside the shuttle's economic
regime. Phase 4 (LOOKUP_HEAVY: `focus`, `heatmap`) and all subsequent
expansion is **CLOSED**: no new Go→Rust ports without an amendment to
ADR-001. The bench framework, perf-regression CI, and cross-compile workflows
remain in service of the 3 shipped modules. Default `go build` produces a
pure-Go binary; Rust paths are opt-in at build time
(`-tags rust_contract`) and runtime (`--engine=rust`,
`--scan-engine=rust`, `--relations-engine=rust`). The original Phase 1
through Phase 4 plan below is preserved for historical context; the
"Migration Program — Final State" section below records the locked-in
end state.

---

## Amendment 2026-05-30 (ADR-0004 — CAMPAIGN CLOSED, PROPOSED)

**This section closes the campaign.** ADR-0002 remains ACCEPTED for its FFI
thesis (sticky-handle works for the workloads where it works). ADR-0004
concludes the campaign that ADR-0002 opened, with the empirical finding that
the cgo+JSON shuttle — with or without sticky-handle — has a defined
economic regime that excludes ~50-60% of remaining `internal/*` modules.
Full ADR: [`docs/adr/0004-campaign-close.md`](../docs/adr/0004-campaign-close.md).

### Final-state classification (2026-05-30)

| Tier | Count | Modules / Notes |
|------|------:|------------------|
| **Shipped — full module** | 4 | `ctx-contract` (7-9×), `ctx-scan` (15-27×), `ctx-relations` (1.97× / sessioned 32-244×), `ctx-focus` (47-105×) |
| **Shipped — per-function / per-query** | 5 | `pack.from_where` (1.15×), `replay.load` (1.9-3.2×), `replay.list` (amortised), `replay.prune` (633×), `symbols.lookup_sessioned` (121-161×) |
| **Evidence-only (memory bucket or perf-fail)** | 6 crates / 10+ items | `where` (-36% heap), `heatmap` (-60-87% allocs), `braid` (-43-50% bytes), `pack` (relevance/diff/redact/preset), `echo` (small only), `replay.diff` micro, `symbols` (apionly + lookup stateless) |
| **Screened-skipped (3 trials of L1-L4 recipe)** | 3 | `digest` (83% syscall via go-git), `config` (80-97% syscall via stdlib I/O), `walk` (97% syscall via os.ReadFile + os.Lstat) |
| **Batch-screened SKIP (remaining)** | 14 | audit, cli, git, hooks, mcp, mix, model, noise, onboarding, security, testinsights, tokens, tui, web |
| **Batch-screened EVIDENCE-ONLY-MEM (remaining)** | 2 | budget, render |
| **Batch-screened SHIP-CANDIDATE-FOR-FOLLOWUP (remaining)** | 1 | skim (LOW confidence; needs pprof probe + multi-file caller) |
| **Total processed** | 14 / 31 internal modules | 11 Rust crates compilable; 9 distinct shipped artifacts |

The 17-module batch screen is documented at
[`tests/BATCH_SCREEN_TIER2_REMAINDER.md`](./BATCH_SCREEN_TIER2_REMAINDER.md).

### Empirical regime boundary (from 14 processed modules)

Three rules locked in by the campaign:

- **>75% pprof `syscall.syscall` → SKIP** regardless of raw latency or
  LOC (digest / config / walk; applied to 7 of the 17 batch-screen rows).
- **Sub-50 µs Go baseline × 1-caller × 1-shot → EVIDENCE-ONLY at best**
  under the cgo shuttle (heatmap / braid / pack.relevance / echo
  medium-large; applied to 2 of the 17 batch-screen rows).
- **REGEX label ≠ REGEX hot path** — verify by source-read + pprof, not
  by the brief's noun (echo, symbols.apionly; applied to skim's
  LOW-confidence label).

### What is closed under ADR-0004

- No new module ports under current cgo+JSON shuttle without ADR amendment.
- 17 remaining modules categorised but NOT scheduled for port.
- "全Rust化" reframed as "Calibrated Migration": Rust where the empirical
  regime supports it (4 full modules + 5 per-fn/query), Go elsewhere.

### What stays unchanged

- 11 Rust crates remain compilable; `cargo test --features testing` gates them.
- `cross-compile.yml` + `perf-regression.yml` stay live.
- All shipped artifacts remain production opt-in via `-tags rust_contract`.
- Default `go build ./...` produces a pure-Go binary with no Rust toolchain.

### Reversibility (per ADR-0004 §Reversibility)

The campaign can be reopened under any of: (1) FFI architecture change
(FlatBuffers / Rust-side walker / WASM / shared memory), (2) sticky-handle v2
with multi-session pool, (3) workload change (hosted ctx server, daemon
mode CLI). Each requires its own ADR amendment with empirical justification.

---

## Amendment 2026-05-29 (ADR-002 — RATIFIED post-PoC)

**This section amends — but does not delete — the FROZEN posture recorded
above.** The original FROZEN content is preserved as historical record of
the decision taken earlier on 2026-05-29 under ADR-001. ADR-002 (proposed
later the same day, **ratified the same day after a successful PoC**)
responds to a strategic redirection from leadership that the FROZEN
decision is no longer compatible with. **ADR-001 is now SUPERSEDED;
ADR-002 is the decision of record.**

### Strategic redirection (leadership input)

Project leadership has redefined "migration complete" as **full Go→Rust
conversion of all 30+ `internal/*` modules ("全Rust化")**, not "the 3+2
shipped modules that cleared the shuttle's regime." Under that goal:

- ADR-001 Option D (Freeze) is incompatible with the directive — it caps
  Rust footprint at ~5 crates by design.
- ADR-001 Option B (Sticky-handle / session-based FFI) is the path that
  ADR-001 itself explicitly preserved for "if leadership prioritises
  continued Rust expansion for organisational reasons." That trigger has
  fired.

### Decision under amendment

Per ADR-002:

1. **ADR-001 Option D (Freeze) is WITHDRAWN as the current decision.**
   The migration is no longer frozen.
2. **ADR-001 Option B (Sticky-handle) is ACTIVATED as the new primary
   FFI strategy** for modules where the cgo+JSON shuttle fails its
   ≥1.5× ship bar.
3. **The 3 already-shipped modules (`contract`, `scan`, `relations`)
   continue under the cgo+JSON shuttle unchanged.** No regression on
   their measured wins is tolerated.
4. **Phase 4 LOOKUP_HEAVY (and the 25 remaining modules from
   `tests/NEXT_MODULES_ANALYSIS.md`) move from CLOSED to ACTIVE-
   pending-PoC.**

### PoC outcome (executed 2026-05-29)

The sticky-handle PoC for `ctx-where` ran on branch `adr/002-sticky-handle`.
Full report: [`tests/STICKY_HANDLE_POC_REPORT.md`](./STICKY_HANDLE_POC_REPORT.md).

| Gate | Bar | Observed | Verdict |
|---|---|---|---|
| Net end-to-end speedup vs Go baseline | ≥5× on all 3 fixtures | small 13.52× / medium 18.72× / large 11.27× | **PASS** (2.25-3.74× margin over the bar) |
| Parity regression on existing fixtures | 0 | 3/3 fixtures byte-exact across Go / stateless / sessioned | **PASS** |
| Session leak rate (10K cycles) | 0 | HeapInuse −5.6% over 10K cycles; RSS stable ~32 MB | **PASS** |
| Rust suite | green | 35/35 ctx-where (24 lib incl. 6 new FFI + 3 parity + 8 regression); sister crates unchanged | **PASS** |
| Cross-compile matrix | all 4 targets green | sticky-handle path links under `-tags rust_contract`; existing matrix untouched | **PASS** |

**Verdict: GO across every gating criterion.** ADR-002 is RATIFIED.
ADR-001 moves to SUPERSEDED. Phase 4 LOOKUP_HEAVY is RESUMED (HALTED →
CLOSED → now RESUMED per ADR-002). The 25-module campaign begins per
the Tier 1/2/3 schedule below.

### Campaign shape (ACTIVE — PoC GO)

The campaign runs in three tiers grouped by amortisation fit. Modules
in higher tiers map more directly to sticky-handle's session-resident
corpus model and were the LOOKUP_HEAVY HALTED targets that the FROZEN
posture closed; the tier ordering mirrors the PoC report's §5
"Recommendations" breakdown.

**Tier 1 (next 4 weeks) — high-value, same-corpus-repeated workloads.**
Direct beneficiaries of the session model. Were the LOOKUP_HEAVY HALTED
modules under ADR-001.

| # | Module | Rationale |
|---|---|---|
| 1 | `focus` | Selects top-N files relevant to a query; ranking shares scoring code with `where`. Same corpus, multiple queries per session. |
| 2 | `heatmap` | Touches the same walked corpus N times with different filters; today repeats walk per call. |
| 3 | `relations` cross-module queries | Already has a crate; add a session API mirroring `ctx-where`'s. |

**Tier 2 (weeks 5-12) — moderate-fit modules.** 7 modules where sticky-
handle helps but the amortisation per session is smaller.

| # | Module |
|---|---|
| 4 | `summarize` |
| 5 | `pack` |
| 6 | `digest` (**SCREENED-SKIPPED 2026-05-30** — see `tests/DIGEST_SCREENING.md`) |
| 7 | `replay` query-mode |
| 8 | `mixdown` |
| 9 | `graph` |
| 10 | `tree` rendering with cached corpus |

**Tier 3 (weeks 13-25) — write-side opt-in cache modules.** Needs a
multi-session pool (out of PoC scope); mid-game milestone. ~12 modules
in the longer tail (annotate, watch, daemon mode, query-router, RAG
bridges, etc.).

### Campaign Execution Status (2026-05-30)

| Bucket | Count | Modules / Notes |
|---|---:|---|
| **Shipped (production opt-in)** | 3 | `contract` (7-9×), `scan` (15-27×), `relations` (1.97× time + −73% bytes / −84% allocs) |
| **PoC validated (sticky-handle proven)** | 1 | `ctx-where` — 11-19× net on small/medium/large; ratifies ADR-002 |
| **Tier 1 shipped** | 2 | `focus` — **47-105× net** sessioned vs Go; 10K-cycle soak clean; 37/37 Rust tests pass; e2e byte-equal across all 3 fixtures. `relations` cross-module — **32-104× net** sessioned vs stateless cgo (Edges 32-52× / Refs 66-86× / Deps 50-104× / Callers 49-109×); 5K-cycle soak clean; 60/60 Rust tests pass; e2e byte-equal sessioned-vs-stateless across all 3 fixtures; web `/api/relations` handler rewired through pool. Full evidence: `crates/ctx-relations/PHASE4_REPORT.md`, `tests/RELATIONS_SESSION_REPORT.md` |
| **Tier 1 evidence-only (compiled + tested, NOT for production routing)** | 1 | `heatmap` — **0.40-0.52× net** (Rust 2-2.5× SLOWER than Go); parity 100% byte-exact on 3 fixtures × 3 render formats; 35/35 Rust tests pass; cause: BATCH 1-caller × 1-shot workload + sub-25 µs Go baseline → cgo+JSON shuttle floor (~60 µs) inverts the ratio. Pure-Rust intrinsic only 1.16-1.24×. Option A (sessioned) would be strictly worse for this caller shape. Ships under `--heatmap-engine rust` opt-in for telemetry; default remains `go`. Full evidence: `crates/ctx-heatmap/PHASE4_REPORT.md`, `tests/HEATMAP_BENCH_REPORT.md` |
| **Tier 1 COMPLETE** | 3 of 3 | focus + relations shipped; heatmap evidence-only. Ready to begin Tier 2 (weeks 5-12) with screening criterion applied. |
| **Tier 2 in-progress (weeks 5-12)** | 3 of 7 | `braid` (pure-compute layer: Allocate + Load/Validate + MergePaths + shellquote) — **0.43-0.53× net** (Rust 1.9-2.3× SLOWER than Go) but **−43-50% bytes/op + −27-51% allocs/op** (memory ≥30% bar PASS). Parity 100% byte-exact on 3 fixtures × 4 outputs. 57/57 Rust tests pass. Per-call work too cheap (Go Allocate is 29 ns) for the 4-FFI-call cgo floor (~50 µs) — exactly as the heatmap screening criterion predicted. Ships under `--braid-engine rust` opt-in for telemetry; default remains `go`. Pure-compute scope split validated: orchestrator (exec.go, Run()) stayed Go-side, routed through Routed* dispatchers. Full evidence: `crates/ctx-braid/PHASE4_REPORT.md`, `tests/BRAID_BENCH_REPORT.md`. **`pack` (Tier 2 #2)** — largest single module ported (3.1 kLOC), scope-split into sessioned relevance (open(goal,budget)→score(file)×N→close) and stateless diff/redact/from_where/preset. 53/53 Rust tests pass + 3 Go soak tests. Byte-equal across all 7 e2e fixtures (small/medium/large relevance + 4 batch). Sessioned relevance: **1.41× slower vs Go but −58% allocs** across all corpus sizes (memory-bucket OK). `from_where` ships net **1.15× faster** on 256-element JSON input. `diff`/`redact`/`preset` regress on perf but post −73 to −97% alloc count. **Pattern proven**: largest-module scope-split with sticky-handle session is viable as evidence-only ship — confirms the "session-fit when corpus state amortises" rule. Full evidence: `crates/ctx-pack/PHASE4_REPORT.md`, `tests/PACK_BENCH_REPORT.md`. **`echo` (Tier 2 #3)** — clean stateless BATCH port of BM25 evaluator (800 LOC, 0 deps, 1 caller). Parity 100% on 3 fixtures (modulo ≤3 ULP BM25 sum-order divergence, well within 1e-9 retrieval tolerance). 21 Rust unit + 8 regression + 4 parity tests pass. Engine-diff: small **1.59×** / medium **0.83×** / large **0.86×** (Rust slower at scale). Memory: small **−93% bytes** but medium **+25%** / large **+102%** bytes vs Go. **EVIDENCE ONLY — memory bucket FAIL on medium/large; default remains `go`.** Root cause: hot path is String + small-HashMap allocation (chunk body `Vec::join`, per-token lowercase `String`, `ScoredChunk` deep-cloning the entire `Chunk` with its tokens), not regex/byte-scan. Rust's stdlib HashMap + String allocations have no super-power over Go's GC at chunk-count scale (6k chunks → 600k small allocs on large). **New screening rule (post-echo)**: REGEX_HEAVY ships only if hot path is `regex::find_iter` over `&[u8]` (NOT String/HashMap), Go baseline ≥100 µs/op on smallest fixture, AND per-call result JSON <10 KB. Echo fails (a) and is at the edge of (b). Full evidence: `crates/ctx-echo/PHASE4_REPORT.md`, `tests/ECHO_BENCH_REPORT.md` |
| **Tier 2 queued (weeks 5-12)** | 2 | `summarize`, `mixdown`, `graph`, `tree`. **Post-echo screening update**: `graph` remains the strongest remaining ship candidate (MULTI-QUERY corpus-resident, sessioned shape — expect ≥3×). `summarize`, `mixdown`, `tree` are all BATCH shape with sub-50 µs Go baselines AND String/map-allocation dominated hot paths (per echo's new rule) → **expected evidence-only**, likely memory regression on medium+. Recommend `graph` next so Tier 2 ships at least one ≥3× speedup before the queue is exhausted. **`digest` was screened and skipped 2026-05-30** — see Tier 2 #6 row below. |
| **Tier 2 #4 — `replay` query-mode (2026-05-30)** | 1 | Sticky-handle retrofit of the already-shipped (Phase 3, evidence-only) `ctx-replay` crate. **CONDITIONAL PROMOTION**: load **2.9-3.2×** vs Go, prune-candidates **633×** vs Go, list amortised across browse, sessioned diff still 0.05-0.08× because base Go diff is sub-2 µs (cgo floor dominates) — but web `/api/replay/diff` inherits 2-3× net per-request via cached base-manifest load. Diff microbench STAYS EVIDENCE-ONLY (per-query verdict precedent). 11 Rust sticky-handle integration tests + 26 lib tests pass; e2e parity byte-equal across 3 fixtures × 2000 iters (go / rust-stateless / rust-sessioned). Pool routed in `internal/web/handlers.go` (List, Show, Diff, Verify, Evidence) and `cmd/replay-engine-diff` (sessioned leg). CLI replay-list/show/diff + replay-pack stay stateless (one-shot per command, L1 screen says no amortisation surface). Full evidence: `crates/ctx-replay/PHASE4_REPORT.md`, `tests/REPLAY_SESSION_REPORT.md`. **New screening rule (post-replay)**: when a module's compute baseline is sub-cgo-floor BUT its data-access cost is significant, sessioning can ship via the **data-access amortisation lane** even when the compute lane stays evidence-only — verdict is per-query, not per-module. |
| **Tier 2 #6 — `digest` (2026-05-30)** | 1 | **SCREENED-SKIPPED — first application of the new Step 0 → Step 1 → Step 2 recipe (post-PR #76).** `Generate` raw latency is high (3 ms small / 19 ms medium / 83 ms large per call) and naively looks portable. **pprof shows 83% of CPU is in `syscall.syscall` driven by go-git's loose-object filesystem walk** (`go-billy.ChrootHelper.Open` 76% / `Tree.PatchContext` 56% / `DiffTreeWithOptions` 44%). tiktoken BPE and tree-sitter — the only Rust-portable sub-ops — do not register in the top 60 pprof nodes (<0.59% each). L4 per-function verdict: `ParseSince` (16 ns, 0 allocs) and `WriteMarkdown`/`WriteJSON` (2.8-5.6 µs) are sub-cgo-floor; `Generate` has high latency but no portable sub-slice that clears the bar with a real root cause. **No ship even as evidence-only.** Callers are all single-shot (CLI / braid / MCP — one Generate per invocation, no daemon, no repeat surface). The `replay` data-access-amortisation lane does not apply because the data-access itself (go-git) is the Go-only library we cannot host in Rust. Module remains 100% Go; no `--digest-engine` flag. Full evidence: `tests/DIGEST_SCREENING.md` + retained screening bench `internal/digest/digest_screen_bench_test.go`. **New screening rule (post-digest)**: when L1 raw latency PASSES but pprof shows the dominant cost is in a Go-only library boundary (filesystem, network, or a Go-bound CGO dep like tree-sitter), score the Rust-portable slice in isolation BEFORE deciding to port. If the portable slice is <10% of total runtime, skip — the cgo floor will swallow the improvement and we'd ship a regression dressed as a feature. |
| **Tier 2 #8 — `walk` (2026-05-30)** | 1 | **SCREENED-SKIPPED — third application of the screen-before-port recipe.** 728 LOC source (`walk.go` 553 + `secure.go` 96 + `timefilter.go` 79), 2 internal deps (`model` + `git`), 8-10 callers across braid/relations/pack/web/mcp/tui/cli. Bench: `Walk_SmallTree` 50 files 804 µs / `Walk_MediumTree` 500 files + .gitignore 12.2 ms / `Walk_LargeTree` 5000 files + .gitignore + .ctxignore + Since-mtime 143 ms; `ParseTimeFilter` 56 ns (sub-cgo-floor by ~900×). **pprof on Walk_MediumTree: 97.09% syscall.syscall** — `countTextStats` (`os.ReadFile` per file for line count + UTF-8 detection) is 79.69% cum, `os.Lstat` 13.27%, `os.ReadDir` 4.85%. The candidate Rust-portable slice — the `sabhiram/go-gitignore` regex matcher `MatchesPath` — is **0.081% of total** (10 ms cum out of 12 360 ms). Even a free pattern-matching kernel saves <0.1% of Walk. `inferRole` string ops are <0.05%. All callers are single-shot per request/invocation (CLI/MCP/web/braid/pack/tui — each opens a fresh `*Walker` against a live filesystem state that must reflect operator edits between calls; sessioning a tree walk over real-time disk state is incompatible with the sticky-handle invariant). Module remains 100% Go; no `--walk-engine` flag. Full evidence: `tests/WALK_SCREENING.md` + retained screening bench `internal/walk/walk_screen_bench_test.go`. **New screening rule (post-walk)**: a module's *named* portable surface (here: gitignore regex matching) is not the same as its *measured* hot path. Walk's gitignore matcher is dwarfed by the per-file `os.ReadFile` it triggers via `countTextStats` to populate `model.FileInfo.Lines`. Anytime a module discovers paths from the filesystem AND reads each file's bytes, expect >90% syscall regardless of whatever "pattern" work happens around the I/O. Together with digest and config, this third consecutive skip locks in the **>75% `syscall.syscall` → SKIP** rule as the dominant decision criterion for filesystem-touching candidates. |
| **Tier 2 #7 — `config` (2026-05-30)** | 1 | **SCREENED-SKIPPED — second application of the screen-before-port recipe.** 479 LOC source (`config.go` 221 + `roots.go` 258), 0 internal deps, ~20 caller files (CLI/web/MCP/pack/braid/security). Hot-path bench: `LoadRoots` n=10 56 µs / n=100 421 µs; `SaveRoots` n=10 163 µs / n=100 487 µs; `AddRoot` 8.8 µs (98% in `EvalSymlinks` syscall); `Find`/`RemoveRoot`/`RootsPath` 22-85 ns (sub-cgo-floor). **pprof on n=100: 80.7% syscall.syscall on Load (77% in `os.File.Close` driven by `toml.DecodeFile`); 97.4% syscall.syscall on Save (62% `syscall.write` for tempfile, plus mkdir/rename).** The `BurntSushi/toml` parser is <7% of Load total; the encoder is downstream of the write syscall. No daemon caller, no session amortisation surface (web `/api/roots` reloads the page-cached <2 KB file per request — would be addressed by a Go-side in-memory TTL cache, not a Rust port). Module remains 100% Go; no `--config-engine` flag. Full evidence: `tests/CONFIG_SCREENING.md` + retained screening bench `internal/config/config_screen_bench_test.go`. **New screening rule (post-config)**: "small + I/O-dominated + zero deps" is the canonical SKIP shape. Any module matching all three of {<500 LOC source, hot-path pprof >75% `syscall.syscall`, zero internal deps} is SKIP without writing a Rust crate — the cgo floor (~50 µs round-trip) is comparable to or greater than the total per-call cost, and the portable slice (TOML parse, in-memory mutation) is too small to clear the floor even at 10× intrinsic. Together with digest, this establishes that **pprof >75% syscall is SKIP regardless of raw latency or LOC**. |
| **Tier 2 #5 — `symbols` (2026-05-30)** | 1 | Scope-split port of internal/symbols's pure-compute layer (extractor stays Go-side because tree-sitter is already CGO). **MIXED VERDICT**: lookup sessioned **121-161× net vs Go** across small/medium/large fixtures (small 161× / medium 133× / large 140×) with **−98-99% bytes & allocs/op** — cleanest sessioned ship after focus and a clear validation of the data-access-amortisation lane on the lookup hot path; apionly EVIDENCE-ONLY (0.90× / +5% memory) because per-call work is dominated by Go-side tree-sitter walk. lookup stateless EVIDENCE-ONLY (0.94-0.98×). 37 Rust lib + 10 regression + 8 sticky-handle + 2 parity (×3 fixtures) tests pass; 5K-cycle soak clean on both warm-session and open/close-cycle paths (HeapInuse delta 32 KB / 229 KB). Pool routed in `internal/web/handlers.go::handleDefinition` (the sole multi-request caller). Other 10 callers stay on Go (single-shot from CLI/MCP/pack/focus). cmd/symbols-engine-diff byte-equal across 18 paths (1 apionly + 5 lookup queries × 3 fixtures). Tree-sitter extractor + apionly AST walk left Go-side per scope brief. Full evidence: `crates/ctx-symbols/PHASE4_REPORT.md`, `tests/SYMBOLS_BENCH_REPORT.md`. **Lesson**: the brief's L3 prediction labelled apionly REGEX_HEAVY — inspection revealed it is in fact tree-sitter dominated, and the post-AST render is the only portable slice. Apply the scope-split pattern (Go owns tree-sitter, Rust owns post-walk pure-compute) plus sticky-handle session for any future module where Go's hot work is a walk + extract that the same root keeps repeating. |
| **Tier 3 queued (weeks 13-25)** | ~12 | write-side opt-in cache modules (multi-session pool required) |

**Tier 1 #2 META-LESSON (heatmap)**: the BATCH stateless API ships
correctly under the campaign infrastructure (Cargo.toml + build.rs +
cbindgen + dispatcher + goldens + benches all generalised cleanly from
the sessioned ctx-focus template). What MISSES the ≥1.5× bar is when
per-call Go cost is already below ~50 µs — the cgo+JSON shuttle floor
(~10-15 µs per FFI call × N calls) dominates. **Tier 2/3 modules
should screen the Go baseline FIRST: if it's already sub-50 µs and the
caller shape is 1× × 1× per command, expect the same evidence-only
outcome as heatmap / where / replay.** The 4 successful production
shippers (contract / scan / relations / focus) all had per-call costs
≥100 µs OR amortised across many queries per session.

**Tier 2 #1 META-LESSON (braid)**: the screening criterion above was
**validated by braid before any Rust code was written** — predicted
evidence-only, landed evidence-only (0.43-0.53× net). The pure-compute
scope-split pattern (Rust port for the math/types, Go for the
orchestrator that pulls in other internal deps) also worked cleanly:
braid's exec.go (calling focus/where/digest) stayed Go-side; the
Routed* dispatcher routes Allocate/Load/Validate/MergePaths/ShellQuote
through FFI when --braid-engine=rust. **This pattern generalises to
any future Tier 2/3 module with deep internal-dep graphs.** Memory
delta in braid (−43-50% bytes, −27-51% allocs) cleared the ≥30% bar —
the campaign should formalise "evidence-only with documented memory
≥30%" as a distinct ship bucket so future BATCH-shape ports get
explicit credit for the memory win even when time regresses.

Each module must still clear **≥1.5× net OR ≥30% memory OR documented
strategic value** (memory safety, future-port enabler, removes a Go dep).
Per-shape ADRs replace per-module ADRs (ADR-002 open question #1
recommendation). The stop conditions in §"Stop conditions (any phase,
any time)" remain active and supersede campaign momentum if triggered.

### Generalisation lessons carried from the PoC

(Verbatim from [`tests/STICKY_HANDLE_POC_REPORT.md`](./STICKY_HANDLE_POC_REPORT.md) §5.)

- **The `Vec<DomainObject>` corpus shape is the unit of caching.** Plan
  the corpus representation as a session-resident object FIRST, then
  design FFI around handing query/result pairs through it.
- **`Box::into_raw` + opaque `*mut c_void` is a known-good pattern.**
  No need to revisit FlatBuffers, shared memory, or zerocopy crates for
  the 25-module campaign — the simple Box pattern already delivered
  11-19×.
- **`atomic.Uint32` double-close guard + `runtime.SetFinalizer` is the
  right Go shape.** Re-use verbatim in every new session crate.

What is `where`-specific (does NOT generalise): the 11-19× headline
includes walk+symbol-extract savings on the Go side. Modules that don't
do per-call walk will see smaller end-to-end wins (expect 1.3-2×) —
still GO, just not as dramatic. The 1.5× pure-Rust intrinsic is the
most defensible "what sticky-handle FFI actually buys you" number.

### Cost (honest projection)

- Sticky-handle PoC: **DONE.** Actual cost was within the 1-2 person-
  week envelope; report at `tests/STICKY_HANDLE_POC_REPORT.md`.
- 25-module campaign (PoC GO ratified, now ACTIVE): **10-15 person-weeks
  engineering, 3-4 calendar months elapsed** for one or two Rust-fluent
  engineers. Tier 1 burns ~4 weeks; Tier 2 ~8 weeks; Tier 3 ~13 weeks.
- Steady-state maintenance burden post-campaign: **1.5-2.0 FTE-
  equivalent** (vs ADR-001's 0.6-0.8 FTE for the 3-module program).
- CI compute: roughly **6× cross-compile minutes** per PR touching any
  crate, **~$15-35K/year additional** GitHub Actions compute or
  equivalent self-hosted.

### What this amendment does NOT change

- The 3 shipped modules' user-visible behaviour, build tags, runtime
  flags, and engine selectors.
- The default `go build` producing a pure-Go binary with no Rust
  toolchain dependency.
- The pioneer pattern (build-tag-gated dispatcher, fail-soft fallback
  to Go on FFI error, parity-diff CI gate).
- The bench framework, perf-regression CI (±10% Rust / ±5% Go), and
  cross-compile workflow.
- The stop conditions documented in the FROZEN content below — they
  remain active and supersede campaign momentum if triggered.

### Honesty note on the rapid amendment

ADR-001 was ACCEPTED earlier on 2026-05-29; ADR-002 amends it the same
day. Both ADRs are honest reads of their respective decision contexts:
ADR-001 optimised for the program scope at acceptance time (close the
shuttle's regime question, lock in proven wins); ADR-002 amends because
the scope itself changed by leadership input. Future ADRs should flag
"strategic inputs assumed stable" explicitly so this kind of
amendment loop is surfaced earlier. The rapid turnaround is recorded
here in full rather than papered over.

### References

- [`docs/adr/0002-sticky-handle-ffi-amendment.md`](../docs/adr/0002-sticky-handle-ffi-amendment.md) — full ADR
- [`docs/adr/0001-ffi-shuttle-redesign.md`](../docs/adr/0001-ffi-shuttle-redesign.md) — the amended ADR, Option B (Sticky-handle) §123-175 and Option D (Freeze) §239-282
- `tests/WHERE_BENCH_REPORT.md` — the data that proves sticky-handle is needed
- `tests/REPLAY_BENCH_REPORT.md` — the sub-2μs problem (sticky-handle helps only on repeated-pair workloads)
- `tests/NEXT_MODULES_ANALYSIS.md` — 25-module campaign inventory

---

## Migration Program — Final State (2026-05-29)

### Shipped (production opt-in via `-tags rust_contract`)
| Module | Speedup | Memory | Status |
|--------|--------|--------|--------|
| ctx-contract | 7-9× | — | shipped |
| ctx-scan | 15-27× | — | shipped |
| ctx-relations | 1.97× time | -73% / -84% allocs | shipped |

### Evidence-only (compiled and tested, NOT for production routing)
| Module | Speedup | Memory | Status |
|--------|--------|--------|--------|
| ctx-where | net 0.92-0.97× | -36% heap | evidence-only (cgo+JSON shuttle regime fails) |
| ctx-replay | net 0.15× | -26% | evidence-only (sub-2μs Go baseline) |

### Frozen scope
- Phase 4 LOOKUP_HEAVY (focus, heatmap): CLOSED per ADR-001
- Future REGEX_HEAVY-batch candidates: re-open only if a clear hot path emerges + ADR amendment
- No new modules added without ADR-002+ updating the FFI strategy

### Maintenance posture
- 5 Rust crates remain compilable; `cargo test` must continue to pass
- cross-compile.yml + perf-regression.yml stay live; gate changes that touch any crate
- `crates/ctx-where/` and `crates/ctx-replay/` kept as evidence; do not extend
- Go-side dispatchers + bridge.go in `internal/{where,replay}/` remain wired but `--engine=rust` continues to be a foot-gun for those two; document that explicitly in RELEASE_NOTES

---

## Empirical Foundation

### Speedup table (verbatim from `tests/BENCH_REPORT.md`)

| Hot path           | Input             | Go ns/op  | Rust ns/op | **Speedup** |
|--------------------|-------------------|-----------|------------|-------------|
| ExtractReferences  | small (10 refs)   | 93,788    | 12,508     | **7.50×**   |
| ExtractReferences  | medium (100 refs) | 926,027   | 127,030    | **7.29×**   |
| ExtractReferences  | large (1000 refs) | 9,300,728 | 1,314,500  | **7.07×**   |
| Verify             | default           | 142,147   | 77,036     | **1.85×**   |
| Verify             | strict            | 142,268   | 78,209     | **1.82×**   |
| ParseFromPack      | markdown (500 KB) | 5,073,044 | 547,460    | **9.27×**   |
| ParseFromPack      | json (500 KB)     | 1,387,593 | 197,420    | **7.03×**   |

Measurement environment: Apple M4 (10-core), macOS 26, rustc 1.92.0, go 1.25.0.
**Single-platform; cross-platform behaviour is unverified.**

### Empirical workload model

The pioneer benches calibrate the following per-workload expectation:

| Workload shape        | Expected intrinsic speedup | Net speedup after cgo? |
|-----------------------|----------------------------|-------------------------|
| **REGEX_HEAVY**       | **7-9×**                   | **5-7× on >1 KB inputs** (cgo ~1-2 µs amortised) |
| **JSON_HEAVY**        | **~7×**                    | **4-6× on >10 KB payloads** |
| **LOOKUP_HEAVY**      | **~1.85×**                 | **1.3-1.7× after cgo tax** (the most fragile case) |
| IO_HEAVY              | <1.5× (disk-bound)         | likely a wash or net loss |
| GLUE                  | NEGATIVE                   | cgo overhead exceeds intrinsic work |

The model holds **only if** the regex/JSON occurrences in source correspond to
genuine runtime hot paths. Static grep is a proxy, not a profile. **Phase 1 must
attach pprof to a representative workload to confirm this assumption before
Phase 2 commits.**

### Acknowledged limitations

1. Bench numbers were collected on Apple M4 only. ARM-vs-x86 regex-engine
   performance differs; Windows is entirely unmeasured. Phase 1 must
   re-benchmark on linux-amd64 and linux-arm64 minimum.
2. Memory reduction was NOT rigorously quantified (criterion's default profile
   omits allocator stats). The pioneer mission charter's "≥30% memory" alt-target
   remains unclaimed. Add `dhat-rs` instrumentation in Phase 1.
3. The cgo bridge was excluded from bench numbers. End-to-end latency under
   cgo will be slightly worse than the intrinsic numbers reported.

---

## Migration Scope

### In scope (3 modules confirmed)

Selected from the top-7 ranked candidates in `NEXT_MODULES_ANALYSIS.md`,
filtered for workload-shape × inbound-impact × FFI-fit triangulation:

| Rank | Module               | LOC  | Workload shape                          | Expected speedup | Inbound | Why ship |
|------|----------------------|------|-----------------------------------------|------------------|---------|----------|
| #1   | `internal/scan`      | 218  | REGEX_HEAVY (16 patterns)               | 7-9× intrinsic   | 2       | Smallest LOC, biggest speedup, runs on every `ctx pack` |
| #2   | `internal/relations` | 1318 | REGEX_HEAVY (12 patterns) + light IO    | 5-7× intrinsic   | 1       | Largest regex surface; powers `web` graph builds |
| #3   | `internal/where`     | 1110 | LOOKUP_HEAVY + light regex              | 1.85-2.5×        | 5       | Highest inbound of LOOKUP candidates; user-visible search |

### Conditionally in-scope (depends on Phase 1+2 outcomes)

| Module               | Workload shape           | Hinges on                                                            |
|----------------------|--------------------------|----------------------------------------------------------------------|
| `internal/focus`     | LOOKUP_HEAVY + BFS       | `relations` ported first (composes by keeping graph in Rust memory)  |
| `internal/replay`    | JSON_HEAVY + IO          | Web verification path remains hot; JSON ~7× holds in net measurement |
| `internal/render`    | JSON_HEAVY (small)       | Payloads grow enough that cgo overhead stops dominating              |
| `internal/heatmap`   | LOOKUP_HEAVY             | `ctx map` graduates from cold-path to perceived-slow                 |

### Out of scope (anti-recommendations)

Pulled verbatim from `NEXT_MODULES_ANALYSIS.md` and confirmed by review:

| Module                    | Reason                                                                   |
|---------------------------|--------------------------------------------------------------------------|
| `model`                   | Pure data types (89 LOC); mirror as `serde` structs inside other ports, never standalone |
| `walk`                    | IO-bound; `filepath.Walk` is already a thin syscall wrapper              |
| `git`                     | Wraps `go-git`; reimplement-on-libgit2 cost dwarfs any speedup           |
| `tokens`                  | Per-call work too small; cgo overhead dominates BPE encode               |
| `symbols`                 | Tree-sitter parsing is in C either way; both bindings marshal across cgo |
| `mcp`                     | Long-lived JSON-RPC server; per-message JSON cost dwarfed by IO/dispatch |
| `web`                     | 54-type API surface; months of Tokio/Axum porting for a network-IO wash  |
| `cli`                     | 4140 LOC of GLUE; porting removes Cobra ergonomics with no speedup       |
| `audit`, `digest`, `noise`, `skim`, `mix`, `echo`, `braid`, `budget`, `hooks`, `onboarding`, `tui`, `testinsights`, `config`, `security` | GLUE-shape or low inbound; negative ROI under cgo overhead |

---

## Phased Schedule

**Calendar anchor**: today = 2026-05-29. Phase 1 starts 2026-Q3 (July 2026).
**Hard ceiling**: 2027-Q4 (December 2027). No commitment past 18 months.

### Phase 1 — Calibration confirmation (2026-Q3, July-September 2026)

**Status (2026-05-29): COMPLETE on branch `phase1/scan-rust-port`.**
See `crates/ctx-scan/PHASE1_REPORT.md` and `tests/SCAN_BENCH_REPORT.md`
for the full close-out. Headline outcome: **15-27× intrinsic speedup
on REGEX_HEAVY paths (vs 7-9× predicted), ~1 day of effort (vs 7-12
days planned).** The pioneer's copy-pasteable infrastructure
(Cargo.toml, build.rs, cbindgen.toml, FFI scaffolding, dispatcher
pattern, bench harness) is the dominant factor.

| Field | Value (predicted → actual) |
|-------|----------------------------|
| Modules in flight | `internal/scan` only |
| Engineering effort | 3-5 person-weeks → **~1 day** |
| Calendar window | 2026-07-01 → 2026-09-30 → executed 2026-05-29 (pull-forward) |
| Decision gate at end | Continue to Phase 2 (see "Phase 2 implications" below) |

**Deliverables (delivered)**

- `crates/ctx-scan/` Rust crate with parity goldens against Go's `internal/scan`.
  - 21 unit + 4 parity + 7 regression = **32 tests**, 32/32 pass.
- cgo bridge + dispatcher gated on `-tags rust_contract` build tag
  (**REUSED the contract crate's tag** rather than `rust_scan` — see
  PHASE1_REPORT for the rationale; one CGO matrix, one operator flag,
  per-crate engine selectors stay independent).
- Cross-platform bench: darwin-arm64 only in this iteration (the
  cross-compile probe stayed Phase-1-internal — promotion to CI
  workflow tagged as a follow-up; see Phase 2 prerequisites below).
- pprof + dhat-rs instrumentation deferred — `#TODO(agent): land
  dhat-rs in Phase 2 to finally close the ≥30% memory alt-target`.
- ADR-001 (Atlas-owned) deferred — `#TODO(agent): file ADR-001 once
  Phase 2 lands so it captures BOTH module ports' lessons`.

**Exit criteria (all met)**

1. End-to-end `ctx pack` byte-identical output between
   `--scan-engine=go` and `--scan-engine=rust` on all 4 parity
   fixtures. PASS.
2. Parity goldens 100% byte-exact across all 4 fixtures. PASS.
3. linux-amd64 / linux-arm64 cross-compile: **NOT verified in this
   iteration** (darwin-arm64 only). Treat as Phase 2 prerequisite.
4. Memory delta: NOT measured (no dhat-rs yet); allocator counts in
   the Go bench show 42 → 3525 allocs scaling with input, suggesting
   substantial headroom — Phase 2 must close this.
5. No regression on default Go build. PASS — `go build ./...` and
   `go test ./internal/contract/...` (78/78), `./internal/scan/...`,
   `./internal/pack/...` all green.

**Phase 2 implications**

- Re-estimate Phase 2 (`relations`) downward to **2-4 days** (vs
  7-10). Infrastructure is now copy-pasteable.
- Re-estimate Phase 2 speedup upward: **15-25× intrinsic** if
  relations is also multi-regex per line; 7-9× if it's single-regex.
- Pre-write Phase 2's bridge to accept `[]byte` everywhere; the
  `string`→`[]byte` cgo lifetime trap cost Phase 1 ~10 min of
  debugging when the Rust side returned `ERR_BAD_JSON` on a dangling
  GC'd path buffer. Mitigation: `runtime.KeepAlive` on caller-held
  byte slices, plus a package-comment in the bridge documenting the
  rationale once per crate.
- Cross-compile probe → CI workflow must land BEFORE Phase 2 merges,
  otherwise a Phase 2 PR that breaks linux-musl will not be caught.

### Phase 2 — REGEX_HEAVY validation at scale (2026-Q4, October-December 2026) → **EXECUTED 2026-05-29**

| Field | Value (predicted → actual) |
|-------|-------|
| Modules in flight | `internal/relations` |
| Engineering effort | 6-9 person-weeks → **~1 day** (pattern reuse compounded) |
| Calendar window | 2026-10-01 → 2026-12-31 → executed 2026-05-29 (further pull-forward after Phase 1) |
| Decision gate at end | **Continue to Phase 3** (see Phase 3 implications below) |

**Deliverables (delivered)**

- `crates/ctx-relations/` Rust crate with parity goldens against Go's
  `internal/relations`. Full per-language extractor coverage
  (Go / JS / TS / Vue / Svelte / Python / Java / Kotlin / PHP / Swift)
  + cache.
  - 29 unit + 7 parity + 7 regression = **43 tests**, 43/43 pass.
- cgo bridge + dispatcher gated on `-tags rust_contract` (REUSED tag —
  one CGO build now links three crates: contract, scan, relations).
- `--relations-engine go|rust` flag on `ctx browse`.
- Cross-compile workflow: `.github/workflows/cross-compile.yml` lands
  with 4-target matrix (darwin-amd64/arm64, linux-amd64/arm64,
  `fail-fast: false`) and a probe (host) job preserving developer-local
  smoke-check parity. (Hard-blocker #4 RESOLVED — see "Decisions
  pending" table below.)
- dhat-rs memory profile (`crates/ctx-relations/benches/memory.rs`) +
  Go MemAlloc bench harness
  (`internal/relations/relations_bench_test.go::BenchmarkBuild_MemAlloc`)
  → 73% reduction in bytes/op, 84% reduction in allocs/op vs Go.
- See `crates/ctx-relations/PHASE2_REPORT.md` and
  `tests/RELATIONS_BENCH_REPORT.md` for the full close-out.

**Exit criteria (all met)**

1. End-to-end `relations.BuildDispatched()` byte-identical between
   `--relations-engine=go` and `--relations-engine=rust` across all 7
   parity fixtures (verified by `cmd/relations-engine-diff`). PASS.
2. Parity goldens 100% byte-exact across 7 fixtures × 2 entry points
   (Build + BuildCached) = 14/14 PASS.
3. Cross-compile workflow lands (host job green; full matrix runs on
   first CI invocation). PASS. Reviewer must flip required-status on
   the branch protection rules post-merge.
4. Memory delta: **−73.2% bytes/Build, −83.8% allocs/Build** — well
   above the 30% Phase 1 target. PASS.
5. Time speedup: **1.80–2.08×** intrinsic across 4 mixed fixtures.
   Within Phase 1's amended LOOKUP_HEAVY model range (the IO portion
   of relations is doing more work than pure regex). PASS at the
   ≥1.5× bar.
6. No regression on prior Rust suites: ctx-contract 78/78,
   ctx-scan 32/32. PASS.
7. No regression on default Go build (`go build ./...`,
   `go test ./internal/relations/...`). PASS.

**Phase 3 implications**

- Re-estimate Phase 3 (`where` + `replay` parallel) downward to **3-6
  days** (vs 9-13 person-weeks). Phase 2 compounded Phase 1's
  infrastructure savings — copy-pasting the patterns crate + cache
  module took ~30 min combined.
- The relations cache.rs file pattern (per-root Arc<Mutex<Option<…>>>)
  generalises to any cached-by-path module. Reuse for `where` and
  `replay` if either grows a similar memoisation layer.
- Cross-compile workflow now exists; Phase 3 just adds a target row
  per new crate to the matrix builds. No additional infrastructure
  needed for Phase 3.
- Phase 3 ADR-001 (multi-language stack) and the
  consolidated-vs-per-crate tag decision SHOULD land at the start of
  Phase 3, not the end of Phase 2, so it can capture all three port
  experiences in one document.

### Phase 3 — JSON_HEAVY + LOOKUP_HEAVY validation (2027-Q1, January-March 2027)

| Field | Value |
|-------|-------|
| Modules in flight | `internal/where` AND `internal/replay` (parallel tracks) |
| Engineering effort | 9-13 person-weeks (2 Rust eng for full quarter) |
| Calendar window | 2027-01-01 → 2027-03-31 |
| Decision gate at end | Commit Phase 4 conditional expansion / pause / abort |
| **Status** | **EXECUTED 2026-05-29 — see verdict below** |

#### Phase 3 verdict (recorded 2026-05-29)

- **`where` net end-to-end speedup**: 0.92×-0.97× (FAIL — below 1.2× soft floor)
- **`replay` net end-to-end speedup**: 0.15× (FAIL — flagged as concern, not abort)
- **Parity**: 3/3 fixtures green per module, byte-exact JSON
- **Memory**: where -36×, replay -26% per call (real, persistent wins)

**STOP recommendation for Phase 4 LOOKUP**: do NOT port `focus`,
`heatmap`, or other LOOKUP_HEAVY modules until the cgo+JSON shuttle
shape is redesigned. The intrinsic 30-40× Rust scoring margin is real
but consumed entirely by JSON marshal of the pre-walked file corpus.
See `tests/WHERE_BENCH_REPORT.md` for the detailed analysis. **Gating
doc before any further LOOKUP_HEAVY or JSON-micro port: `docs/adr/0001-ffi-shuttle-redesign.md` (ACCEPTED 2026-05-29, Option D — Freeze).**

**PROCEED-with-caution recommendation for Phase 4 JSON**: the replay
concern is bounded — production callers (CLI one-shot + web verify
behind HTTP) do not see the 30μs cgo tax that the bench harness
exposes. The crate ships, the concern is logged, the default Go path
remains production.

**Rationale for parallel tracks**: `where` validates LOOKUP_HEAVY (the
1.85× shape, the most fragile case under cgo overhead); `replay` validates
JSON_HEAVY (~7×, safer). Running both in parallel surfaces the LOOKUP_HEAVY
risk while keeping aggregate momentum.

**Deliverables**

- `crates/ctx-where/` + `crates/ctx-replay/` Rust crates.
- Two more build tags (`-tags rust_where`, `-tags rust_replay`) or a
  consolidated `-tags rust_all` umbrella tag (decision in Phase 2 ADR).
- E2E latency measurements for `ctx where` (interactive surface).
- Decision on whether LOOKUP_HEAVY ports survive cgo overhead in practice.

**Exit criteria**

1. `ctx where` p95 latency reduction ≥1.3× end-to-end (acknowledging cgo tax).
2. `replay` JSON throughput ≥4× end-to-end on web verify path.
3. **Stop-condition trigger**: if `where` shows <1.2× net speedup, do NOT
   port `focus` or other LOOKUP_HEAVY modules. Document and stop the
   LOOKUP_HEAVY thesis.

### Phase 4 — RESUMED (2026-05-29 per ADR-002 ratification)

**Status**: RESUMED. Trajectory: HALTED (post-Phase-3 close) → CLOSED
(2026-05-29 under ADR-001 Option D Freeze) → **RESUMED 2026-05-29 under
ADR-002** after the sticky-handle PoC for `ctx-where` cleared its ≥5×
GO bar on every fixture and showed zero leaks in the 10K-cycle soak.

Phase 4 was previously HALTED at the close of Phase 3 (where net 0.92-0.97×,
replay net 0.15× — both below the LOOKUP_HEAVY / JSON-micro ship bar) and
then CLOSED on 2026-05-29 under ADR-001. With ADR-002 ratified the same
day after a successful sticky-handle PoC, the CLOSED status is lifted:

- `internal/focus`, `internal/heatmap`, and other LOOKUP_HEAVY targets
  are **back in scope** as the Tier 1 batch of the 25-module campaign.
- The sticky-handle FFI shape (open/query/close + `Box::into_raw` +
  `atomic.Uint32` double-close guard + `runtime.SetFinalizer`) is the
  template for every LOOKUP_HEAVY / JSON-micro port. The cgo+JSON
  shuttle remains the default for batch + long-per-call shapes
  (`contract`, `scan`, `relations` ship unchanged).
- The bench framework, perf-regression CI, and cross-compile workflows
  expand from "service of 3 shipped + 2 evidence-only" to "service of
  3 shipped + 1 PoC-validated + ~22 in-flight campaign crates."
- Each port still must clear ≥1.5× net OR ≥30% memory OR documented
  strategic value. The stop conditions in §"Stop conditions" remain in
  force and supersede campaign momentum if triggered.

The original Phase 4 plan is preserved below for historical context only.

| Field | Value (historical, NOT executed) |
|-------|-------|
| Modules in flight | TBD per Phase 3 outcome — `focus` (if relations + where succeeded) and/or `render` / `heatmap` |
| Engineering effort | Re-estimated at Phase 3 gate (provisional: 6-10 person-weeks) |
| Calendar window | 2027-Q2 → 2027-Q4 (provisional ceiling) |
| Decision gate | Re-evaluate strategic value: is the dual-language stack still net-positive? |

**Phase 4 only proceeds if Phases 1-3 demonstrate sustained org capability**:
≥2 Rust-fluent maintainers, per-PR perf-regression CI live, cross-platform CI
green, and no major external-contributor friction. If any of those fail, stop
at Phase 3. (Historical gating language; superseded by the CLOSED status
above.)

### Stop conditions (any phase, any time)

Trigger any of these → halt new ports, ship what's already merged, decide
whether to keep or rip out:

1. **Two consecutive ports fail to meet their net-speedup exit criteria.**
2. **Single Rust-fluent maintainer leaves**, no successor within 6 weeks.
3. **Cross-compile breakage** on any officially supported `go install` target
   (windows-amd64, linux-amd64, linux-arm64, darwin-amd64, darwin-arm64) that
   can't be fixed within 2 weeks.
4. **External-contributor complaint volume** about Rust toolchain crosses a
   threshold (operational definition: ≥5 unique reporters in 30 days).
5. **A Rust panic crosses FFI and crashes a user's `ctx` invocation** in the
   wild — even once — without immediate root-cause fix.

---

## Organizational Dependencies

| Dependency | Owner | Resolution by | Blocker if unresolved? |
|------------|-------|---------------|------------------------|
| cgo target-platform matrix (which OS × arch officially supported) | Build/Release | Phase 1 kickoff (2026-06-26) | **YES (Phase 1)** |
| pure-Go fallback long-term policy (deprecate? keep forever?) | Eng leadership | Phase 1 kickoff | NO (default = keep) |
| Rust training/hiring plan (≥2 maintainers by Phase 2 end) | Eng management | Phase 2 kickoff (2026-09-15) | **YES (Phase 2+)** |
| Per-PR perf-regression CI (criterion + benchstat) | Platform | Phase 1 mid (2026-08-15) | **RESOLVED 2026-05-29** — `.github/workflows/perf-regression.yml` lands with Phase 3 (criterion + go bench, baseline cache, PR comment + 10%/5% gate) |
| Dev-experience for non-Rust contributors (docs, dev container, fallback) | DX team | Phase 1 end (2026-09-30) | NO (warn-only initially) |
| Cross-compile CI matrix (T-25b probe promoted to production) | Build | Phase 1 mid | **RESOLVED 2026-05-29** — `.github/workflows/cross-compile.yml` lands with Phase 2 (4-target matrix + host probe job) |
| ADR-001 for the multi-language stack decision | Architecture (Atlas) | Phase 1 mid | NO (record-only) |
| Panic-safety convention at FFI boundary (catch_unwind audit) | Rust maintainer | Phase 2 end | **YES (Phase 3+)** |
| Toolchain pinning strategy (rustc/cargo version cadence) | Build/Release | Phase 2 mid | NO (default = pin to stable -1) |

---

## Engineering Effort Model

### Per-module effort breakdown

The pioneer (`contract`, 1229 LOC) consumed ~4 hours of effective work time
(excluding the 5-hour codex hang) across Phases 0-6. That ~4 hours included
infrastructure (parity harness, cross-compile probe, FFI shim, dispatcher) that
**will not be re-paid for subsequent ports**. The pioneer multiplier is
~3-5× higher than steady-state because of the infra build-out.

Steady-state effort model per module:

| Activity | Days (low) | Days (high) | Justification |
|----------|-----------:|------------:|---------------|
| Rust code translation | 2 | 5 | Scales with LOC: ~1 day per 250-500 LOC |
| Parity test authoring | 1 | 3 | 4 fixture packs × N functions, mostly mechanical |
| FFI integration (bridge + dispatcher) | 1 | 2 | Pioneer pattern is copy-paste |
| Adversarial review + fix loop | 1 | 3 | Pioneer needed 1 loop; budget 1-2 |
| Multi-platform verification | 1 | 2 | Cross-compile + bench on 3+ platforms |
| Buffer (FFI surprises, hidden state) | 1 | 3 | 20-30% buffer on cgo work |
| **Total per module** | **7 days** | **18 days** | ~1.4 to 3.6 person-weeks |

### Phase-level effort rollup

| Phase | Modules | Per-module range | Phase total (person-weeks) |
|-------|---------|------------------|----------------------------|
| Phase 1 | scan (small) | 7-12 days | **3-5** |
| Phase 2 | relations (medium-large) | 14-18 days + 5 days web integration | **6-9** |
| Phase 3 | where (medium-large) + replay (medium) parallel | 14-18 + 10-14 days | **9-13** |
| Phase 4 (provisional) | focus and/or render and/or heatmap | 7-15 days × N | **6-10** |
| **18-month total** | 3-5 modules | — | **24-37 person-weeks** |

Across 18 months that is **0.3 to 0.5 FTE sustained Rust engineering**, plus
~0.2 FTE review burden and ~0.1 FTE release/CI burden. **Total org commitment:
roughly 0.6 to 0.8 FTE-equivalent across the migration.**

---

## Cost Model

### One-time costs (paid in Phase 1)

| Item | Estimate (USD) | Estimate (person-days) | Notes |
|------|----------------|------------------------|-------|
| cgo cross-platform CI matrix (5 targets) | $2-5K compute setup | 5-10 days build eng | GitHub Actions runners or self-hosted |
| Rust training (2 engineers, course + ramp) | $1-3K | 10-20 days each | "Rust for Rustaceans" or equivalent self-study |
| ADR drafting + architecture review | — | 3-5 days | Atlas-authored |
| Per-PR perf-regression infra (criterion + benchstat + diff bot) | $0 (open-source) | 5-8 days | One-time wire-up |
| dhat-rs / jemalloc-stats instrumentation harness | — | 2-3 days | Per-module reusable |
| **One-time total** | **$3-8K + ~30-50 person-days** | | |

### Per-module ongoing costs

| Item | Per-module cost |
|------|-----------------|
| Dual-build CI minutes (Go-only + Rust-tagged) | 2x baseline CI minutes per PR touching the module |
| Dual-maintenance burden (parity goldens drift) | ~0.5-1 day per Go-side change that affects parity |
| Cross-compile verification per release | ~0.5 day per release per platform |

### Yearly ongoing costs (post-Phase 3, steady state)

| Item | Yearly estimate |
|------|-----------------|
| Sustained Rust engineering | 0.3-0.5 FTE = $50K-$120K depending on level |
| Sustained reviewer + release overhead | 0.2-0.3 FTE = $30K-$70K |
| Dual-build CI compute | $2-6K/year (GitHub Actions or equivalent) |
| Toolchain bumps + breakage triage | ~10-15 person-days/year |
| **Yearly total** | **~$85K-$200K + ~$5K compute** |

### Break-even projection

**Inputs**: assume `ctx pack` runs N times/day per user across U users.
Pioneer benches predict ~5-7× end-to-end speedup on REGEX/JSON paths after
cgo overhead, translating to ~80-200 ms saved per invocation on representative
500 KB packs (Go baseline ~5 ms→Rust ~0.5 ms intrinsic, plus surrounding
work). At 100 users × 50 invocations/day × 100 ms saved = **~14 hours of
compounded user-perceived latency reduction per day**.

**Honest assessment**: break-even is **not** about dollar-cost — at this scale
the migration is dominated by maintenance burden, not compute savings. The
business case is **user-perceived responsiveness on interactive surfaces**
(`ctx where`, MCP `pack`, web server graph builds) and **the ability to ship
a faster product without rewriting the world**. If interactive responsiveness
is not a leadership priority, **the migration's strategic case collapses**.

---

## Risk Register

| ID  | Risk | Likelihood | Impact | Mitigation | Owner |
|-----|------|------------|--------|------------|-------|
| R-1 | Rust hiring/training lag stalls Phase 2+ (single-maintainer bus factor) | **HIGH** | HIGH | Hard gate at Phase 2 kickoff: ≥2 Rust-fluent reviewers required. Allocate training budget in Phase 1. | Eng management |
| R-2 | cgo cross-compile breakage on Windows / musl / unusual targets | **HIGH** | MEDIUM | Phase 1 promotes the probe to production CI. Windows treated as "best effort, not blocking" unless leadership upgrades it. | Build/Release |
| R-3 | cgo overhead (~1-2 µs/call) eats perf on fine-grained call sites, especially LOOKUP_HEAVY (`where`, 1.85× intrinsic margin) | **MEDIUM** | HIGH | Phase 1 instruments end-to-end latency, not intrinsic. Phase 3 stop-condition: <1.2× net speedup on `where` → halt LOOKUP_HEAVY ports. | Rust maintainer |
| R-4 | Dual-language code review backlog (Rust PRs queue behind reviewers) | MEDIUM | MEDIUM | Per-PR perf-regression CI auto-gates. Reviewer rotation. Limit Rust PR fan-in to 2-3/week. | Eng lead |
| R-5 | Pure-Go fallback bit-rot (Go path drifts from Rust path, parity silently breaks) | **HIGH** | HIGH | Per-PR parity diff is a CI gate (block merge if Go/Rust differ on goldens). Rebuild goldens on every change to either side. | Rust maintainer |
| R-6 | Parity test maintenance burden grows superlinearly across modules | MEDIUM | MEDIUM | Reuse pioneer's parity-fixture-builder pattern; cap goldens at ~40 per module; deprioritize edge-case goldens. | Rust maintainer |
| R-7 | Toolchain drift (rustc/cargo updates breaking lib reproducibility, libc compatibility) | MEDIUM | MEDIUM | Pin rustc to stable -1; allow stable -2 for libc-pinned targets. Test rustc bumps in a dedicated CI lane. | Build/Release |
| R-8 | Rust panic crosses FFI and crashes Go (panic-safety regression) | LOW (with catch_unwind) | **CRITICAL** | Mandatory `catch_unwind` audit at every FFI boundary. Stop-condition #5 in scope. Pioneer's `ffi.rs` already follows this — replicate. | Rust maintainer |
| R-9 | External contributors can't build the project (Rust toolchain not installed) | MEDIUM | MEDIUM | Default `go build` stays pure-Go forever. Document `-tags rust_*` as opt-in. Provide dev container in Phase 1 end. | DX team |
| R-10 | Bench gains don't translate to user-facing latency (cgo + serialization tax) | **MEDIUM** | **HIGH** | Phase 1 measures end-to-end on representative workloads. Each phase's exit criteria are user-visible numbers, not intrinsic benches. If a phase fails the net target, do not proceed. | Rust maintainer + product |

**Highest-priority risks: R-1 (hiring), R-2 (cross-compile), R-3 (cgo overhead
killing LOOKUP), R-5 (Go path bit-rot), R-10 (no user-visible win).** These
five define the gating questions for the leadership decision.

---

## Decision Gates

### After Phase 1 (2026-09-30)

**Measurement protocol**: produce a Phase 1 close-out report with the
following sections — multi-platform bench delta, end-to-end `ctx pack`
latency delta on 5 representative repos (small/medium/large/JS-heavy/
Python-heavy), pprof flame graph confirming `scan` is the predicted hot
path, parity diff CI green for ≥30 days, cross-platform CI green for ≥14 days.

| Outcome | Action |
|---------|--------|
| End-to-end ≥1.5× speedup AND cross-platform green AND no regressions | **Continue to Phase 2** |
| End-to-end 1.2-1.5× speedup, but other criteria met | **Pause** — root-cause why cgo + serialization is eating gains; do not commit Phase 2 until path is clear |
| End-to-end <1.2× speedup OR cross-platform red OR regressions | **Abort** — ship `scan` Rust crate behind opt-in tag only, do not start Phase 2 |

### After Phase 2 (2026-12-31)

| Outcome | Action |
|---------|--------|
| `web` graph-build endpoint shows ≥3× speedup on 1000+ file repos AND ≥2 Rust reviewers AND perf-regression CI live | **Continue to Phase 3** |
| 2-3× speedup but reviewer/CI gate not met | **Pause** — resolve org dependencies first; do not start Phase 3 until both gates pass |
| <2× speedup OR major regression | **Abort** — `relations` was the largest predicted REGEX win; if it didn't pan out, the thesis is broken |

### After Phase 3 (2027-03-31)

| Outcome | Action |
|---------|--------|
| `where` p95 ≥1.3× AND `replay` ≥4× net AND aggregate maintenance burden tracking ≤30% of one FTE | **Conditional Phase 4 (re-baseline before each module)** |
| `where` <1.2× | **Stop LOOKUP_HEAVY thesis** — do not port `focus`, `heatmap`. Re-evaluate `replay` and `render` standalone |
| `replay` <3× | **Stop JSON_HEAVY thesis** — do not port `render` |

### Emergency stop conditions (any time)

See "Stop conditions" under Phase 4 — these supersede phase-gate logic and
halt all new ports immediately. Existing merged Rust crates stay shipped
behind opt-in tags; only **new** porting work stops.

---

## Success Metrics

### Per-module metrics (measured at each phase exit)

- **Intrinsic speedup** (criterion vs Go testing.B): primary headline, but
  not load-bearing for the go/no-go decision.
- **End-to-end speedup** (user-visible latency on representative workloads):
  the actual go/no-go number.
- **Allocation reduction** (dhat-rs vs Go `B/op`): target ≥20% (non-blocking).
- **Parity test count**: target ≥40 byte-exact goldens.
- **Time-to-merge for parity-breaking PRs**: target <3 business days.

### Org-level metrics (measured quarterly)

- **% of hot paths in Rust**: track by inbound-weighted LOC.
- **% of binary footprint Rust-sourced**: track for binary-size oversight.
- **Dual-build CI pass rate**: target ≥99% across all phases.
- **Perf-regression incidents** (detected by CI): target ≤1 per quarter; >3
  triggers process review.
- **External-contributor PR friction**: track via comment-thread analysis; >2
  Rust-toolchain complaints per month triggers DX intervention.

### User-facing metrics (continuous)

- **End-to-end `ctx pack` latency** on representative workloads (p50/p95/p99
  over time). Baseline: pre-Phase-1 Go-only numbers.
- **End-to-end `ctx where` latency** (interactive surface, p95 is the headline).
- **MCP request handler latency** (if MCP eventually consumes the Rust crates).
- **Web server graph-build response time** (Phase 2 onward).

---

## Rollback Strategy

### Per-phase rollback

The pioneer model — `-tags rust_<module>` build tag flips between Rust and
Go dispatch, with Rust crate as additive only — **generalises to every phase**.
Per-phase rollback:

1. **Default Go build is never touched**: `go build ./cmd/ctx` always produces
   a pure-Go binary, no cgo, no Rust toolchain dependency, identical bytes
   to pre-migration.
2. **Each Rust path is opt-in at build time**: `-tags rust_scan`,
   `-tags rust_relations`, etc. Disabling a tag falls back to Go transparently.
3. **Each Rust path is opt-in at runtime**: `--engine=rust|go` flag (pioneer
   pattern, generalised). Default at runtime is `go` even on tagged builds.
4. **`git revert <commit>`** removes a phase's additions cleanly. The clock
   seam pattern (one-line indirection + `SetNowFunc`) is the only pattern
   that touches Go production code, and each is +30 LoC, easily revertable.

### Per-module rollback

The pioneer's `dispatch.go` + `dispatch_rust.go` pattern (build-tag-gated
mutual-exclusion files) **generalises directly**:

- `dispatch.go` (`//go:build !rust_<module>`): always Go.
- `dispatch_rust.go` (`//go:build rust_<module>`): routes through `rustbridge`
  with **fail-soft fallback to Go on FFI/decode error**.

The fail-soft pattern means a corrupt staticlib does NOT brick the binary —
it logs a warning and serves the Go result. This is the right default; it
also means **silent regressions in the Rust crate would not surface unless
the parity diff is part of CI**. Per R-5, parity diff CI is mandatory.

### Worst-case: full migration abandonment

Cost of abandoning at any point and keeping the merged Rust crates:

- Maintenance: ~0.1 FTE to keep parity goldens current (or remove them if
  Rust path is deprecated).
- CI burden: ~10% of total CI compute for dual-build verification.
- Disk: ~20-50 MB of Rust crate sources per module (negligible).

Cost of abandoning and **ripping out** the merged Rust crates:

- Engineering: ~3-5 days per merged module (delete crate, dispatch files,
  build tag, FFI bridge; restore single-implementation Go path).
- CI: simplifies (single-build only).
- Risk: low — pioneer demonstrated the rollback path; identical for every
  port.

**Strategic recommendation: abandonment cost is low at every phase boundary.**
This is by design; it's why the pioneer pattern is the right one. Leadership
can pull the cord at any decision gate with minimal sunk-cost penalty.

---

## Out-of-Scope (Explicit Deferrals)

### Modules we will NOT port even on the success path

| Module | Rationale |
|--------|-----------|
| `mcp` | JSON-RPC stdio server; process-IO and dispatch dwarf marshal cost; cgo would add latency per request |
| `web` | 54-type API + goroutines + auth + websocket; months of work for a network-IO-bound wash |
| `cli` | 4140 LOC of GLUE; loses Cobra ergonomics with no speedup |
| `walk`, `git`, `tokens`, `symbols` | Per `NEXT_MODULES_ANALYSIS.md` anti-recommendations; covered above |
| `model` | Pure data types; mirrored as `serde` structs **inside** ports, never standalone |

### Features deferred from the pioneer that remain deferred

| Item | Why still deferred |
|------|---------------------|
| F-06 (cross-platform errno→string parity) | Out of pioneer scope; address only if a port surfaces a permission-denied path that breaks parity |
| F-08 (non-UTF-8 line handling parity) | Out of pioneer scope; address only if a fixture surfaces it |
| L-02 hardened mode (`follow_symlinks=false`) | Security-hardening sweep, not a migration item |
| L-08 unbounded JSON parse size cap | CLI-level `--max-pack-bytes` guard, not a migration item |
| FFI completion items (`Result` → `VerifyResult` rename, etc.) | API tidy; address only when next FFI surface is added |

---

## Recommended Decision

**What to commit to today (2026-05-29)**

Commit to **Phase 1 only**: port `internal/scan` to Rust under the same
build-tag-gated pattern as the pioneer. This is **3-5 person-weeks of work
across 2026-Q3**, with a hard decision gate on 2026-09-30. Resolve four
organizational dependencies before kickoff: cgo target-platform matrix,
per-PR perf-regression CI, ADR for the multi-language stack, and confirmation
of a single Rust-fluent maintainer for the quarter. The technical risk is
low (the pioneer pattern generalises directly); the organizational risk is
real (one maintainer, untested cross-compile beyond darwin-arm64) but
contained within the rollback envelope.

**What to defer pending data**

Do not commit Phase 2 or beyond. The empirical question Phase 1 must answer
is whether the pioneer's 7-9× intrinsic speedup survives cgo overhead and
serialisation tax in real `ctx pack` end-to-end measurements. The
`NEXT_MODULES_ANALYSIS.md` static-grep proxy strongly predicts yes; the
benches confirm intrinsic perf; the end-to-end story is unmeasured. **Do not
fund the full 24-37 person-week migration on intrinsic numbers alone.**
Phase 2 should be funded only if Phase 1 closes ≥1.5× end-to-end with the
org dependencies green.

**What the next 30 days look like operationally**

- Week 1-2 (2026-06-01 → 2026-06-12): leadership decision review of this
  document. Atlas drafts ADR-001 in parallel.
- Week 3 (2026-06-15 → 2026-06-19): if committed, build/release scopes the
  cgo target-platform matrix and stands up the per-PR perf-regression CI
  workflow scaffolding.
- Week 4 (2026-06-22 → 2026-06-26): confirm Phase 1 owner, kickoff date,
  and Phase 1 exit-criteria measurement plan. Phase 1 starts 2026-07-01.

---

## Open Questions Requiring Leadership Resolution

Pulled from `NEXT_MODULES_ANALYSIS.md` §"Open questions" and supplemented
by roadmap-level questions:

1. **Target platform matrix for cgo**: Do we extend beyond darwin-arm64 +
   linux-amd64 to windows-amd64 and linux-arm64 before Phase 2, or stage
   them per-phase? Each platform multiplies CI cost. Recommendation: commit
   to all four Tier-1 (`darwin-{amd64,arm64}`, `linux-{amd64,arm64}`) by
   Phase 2 kickoff; treat windows-amd64 as best-effort.

2. **Pure-Go fallback long-term policy**: Keep forever or deprecate once cgo
   coverage stabilises? Recommendation: **keep forever** — it is the
   `go install` story, the dev-experience floor, and the rollback envelope.

3. **Dev-experience for non-Rust contributors**: Contribution policy when a
   PR touches a Rust-backed module? Recommendation: **PRs touching Go-side
   only are accepted as-is; Rust-side changes require a Rust-fluent
   reviewer**. Document in CONTRIBUTING.md by Phase 1 end.

4. **Rust hiring/training**: Who owns the Rust crates long-term? Currently
   one author. Recommendation: **fund training for 1-2 additional engineers
   in Phase 1**; gate Phase 2 on having 2+ reviewers.

5. **Benchmark CI**: Per-PR criterion + benchstat regression check?
   Recommendation: **mandatory before Phase 2**; this is cheap now, expensive
   to retrofit later.

6. **Build-tag taxonomy**: Per-module tags (`rust_scan`, `rust_relations`)
   or umbrella `rust_all`? Recommendation: **per-module tags through Phase
   3, then re-evaluate**. Per-module preserves rollback granularity.

7. **Engine-selection runtime UX**: `--engine=rust` per command (pioneer
   pattern) or env var (`CTX_ENGINE=rust`) or auto-detect? Recommendation:
   **keep the per-command flag, add env var for convenience**. Do not
   auto-default to Rust until Phase 3 exits successfully.

8. **Acceptable cgo overhead ceiling**: What's the maximum per-call cgo
   overhead that justifies routing through Rust? Pioneer measured 1-2 µs;
   the LOOKUP_HEAVY thesis lives or dies on this number. Recommendation:
   **set a hard ceiling of 5 µs/call**; modules whose intrinsic perf
   doesn't dominate this are NOT ported.

9. **Panic-across-FFI policy**: Even one Rust panic crossing FFI is in the
   emergency-stop list. Is this the right severity? Recommendation: **yes**
   — the failure mode is end-user crash, not silent perf regression. Treat
   as critical.

10. **Cross-language code-review cadence**: Do Rust PRs get same SLA as Go
    PRs? Recommendation: **yes**, but cap concurrent Rust PRs at 2-3/week
    to prevent reviewer burnout.

---

## Appendix: Traceability to Summit Evidence

| Recommendation / Claim | Source report | Section / table |
|------------------------|---------------|------------------|
| 7-9× regex speedup | `tests/BENCH_REPORT.md` | "ExtractReferences" / "ParseFromPack markdown" |
| ~7× JSON speedup | `tests/BENCH_REPORT.md` | "ParseFromPack" json table |
| 1.85× lookup speedup | `tests/BENCH_REPORT.md` | "Verify" |
| ~1-2 µs cgo overhead | `tests/BENCH_REPORT.md` "Anomalies"; `internal/contract/rustbridge/T27_INTEGRATION_REPORT.md` §6 | |
| Top-7 module ranking | `tests/NEXT_MODULES_ANALYSIS.md` | "Ranked candidates" |
| Anti-recommendations | `tests/NEXT_MODULES_ANALYSIS.md` | "Anti-recommendations" |
| Pioneer effort (~4 hours effective) | `tests/SUMMIT_EXECUTION_REPORT.md` | "Cost-and-time summary" |
| Build-tag-gated dispatcher pattern | `internal/contract/rustbridge/T27_INTEGRATION_REPORT.md` | §3 "Dispatcher mechanism" |
| Fail-soft fallback design | `internal/contract/rustbridge/T27_INTEGRATION_REPORT.md` | §6 "Known limitations" |
| Pioneer rollback model | `tests/RELEASE_NOTES.md` | "Rollback" |
| Deferred items (F-06, F-08, L-02, L-08) | `crates/ctx-contract/PHASE5_REPORT.md` | "Deferred to follow-up Summit / apex" |
| Open questions for leadership | `tests/NEXT_MODULES_ANALYSIS.md` | "Open questions for product/eng leadership" |
| Engine distribution risk (single-engine bus factor) | `tests/SUMMIT_EXECUTION_REPORT.md` | "Engine distribution audit" |
| Multi-platform bench limitation | `tests/BENCH_REPORT.md` | "Environment" + "Anomalies / caveats" |

---

## Change History

| Version | Date | Author | Notes |
|---------|------|--------|-------|
| Draft v1 | 2026-05-29 | Summit Phase 6 (Step D) | Initial authoring; awaiting leadership review |

---

## Next Actions

1. **Leadership review** (target: 2026-06-12) — accept / reject / revise scope.
2. **Atlas authors ADR-001** in parallel — multi-language stack decision record.
3. **Build/Release scopes Tier-1 cgo platform matrix** — drives Phase 1 kickoff readiness.
4. **Phase 1 owner confirmation** — single Rust-fluent maintainer + reviewer pair.
5. **Per-PR perf-regression CI scaffold** — wire criterion + benchstat into the workflow.
6. **Phase 1 kickoff** — 2026-07-01 if all four organizational dependencies green.
