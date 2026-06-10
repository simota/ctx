# Summit Execution Report — `summit/contract-rust-port`

**Mission**: "golang to rust" — port one Go module to Rust as the calibration
shot for a broader migration thesis.
**Pioneer module**: `internal/contract` (969 LOC, 6 source files, 0 internal deps).
**Branch**: `summit/contract-rust-port`.
**Report date**: 2026-05-29.
**Final status**: SUCCESS (proof-of-concept; production wiring deferred).

---

## TL;DR

A leaf Go module was ported to Rust under a 6-phase recipe that combined
dual-engine consensus reasoning, deterministic golden parity, adversarial
multi-engine code review, and a compressed fix loop. The final Rust crate is
byte-exact against the Go reference across **40 golden fixtures** and passes
**66/66 tests** (19 unit + 40 parity + 7 regression). The Go production code
path is **untouched** apart from a 30-LoC opt-in clock seam. The crate is
**not yet wired into the `ctx` CLI**; FFI + integration are explicit deferred
follow-ups.

The recipe was designed for **tri-engine execution** (codex / claude / agy).
In practice it degraded to **single-engine execution with dual-engine intent**
because agy was blocked at Claude Code's safety classifier from Phase 1
onward and codex hung in Phase 3 for ~5 hours producing zero output. This
gap and its implications are documented explicitly in the "Engine
distribution audit" section below.

---

## Phase 0 — Mission Charter

**Output**: pioneer module selection.

The charter evaluated candidate Go packages for the pioneer port under four
constraints: small LOC, low internal-fan-in, well-defined I/O, and high test
coverage in the existing Go suite. `internal/contract` won on every axis:

- **969 LOC** across 6 source files (small enough to port in one Summit cycle).
- **Zero internal `ctx` dependencies** — the package imports only stdlib +
  `crypto/sha256` + `encoding/json`. No import-cycle risk into `internal/pack`
  or `internal/model`.
- **Pure data transform surface** — deterministic functions from byte input
  to byte output, ideal for golden parity testing.
- **Pre-existing Go test coverage** — provides ground truth for the parity
  fixtures rather than requiring us to author the contract from scratch.

Pioneer rationale: prove the *recipe* (parity harness, engine consensus,
adversarial review, fix loop) on the smallest defensible module before
investing in dependency-graph-heavy ports.

---

## Phase 1 — Dual-Engine Consensus

**Output**: 27 findings across all engines, classified.

| classification | count |
|---|---|
| CONFIRMED (both engines independently flagged) | 9 |
| LIKELY (one engine, plausible on review) | 18 |
| disputed | 0 |

**Engine attribution**: claude + codex contributed. **agy was blocked** at
Claude Code's safety classifier on `--dangerously-skip-permissions` and
produced no Phase 1 output. The recipe's tri-engine intent silently degraded
to dual-engine here; this was the first leak point.

The 9 CONFIRMED findings became the non-negotiable parity targets for Phase 2
implementation. The 18 LIKELY findings were prioritized in Phase 2 D-3
(parity vs hardening trade-off) and scope-cut where they did not affect
byte-exact JSON output.

---

## Phase 2 — Execution Plan

**Output**: `execution_plan.yaml` with 29 tasks, 5 design decisions (D-1..D-5),
DAG topological sort.

Key design decisions:

- **D-1**: Use byte-exact JSON parity (not structural) as the contract.
  Forces us to match Go's `encoding/json` field ordering and zero-value
  emission semantics.
- **D-2**: Frozen-clock seam in Go (`SetNowFunc`) rather than mock-time at
  call sites. Minimal Go-side footprint (+30 LoC, opt-in).
- **D-3**: Parity-only for L-02 (symlink follow), L-08 (JSON parse size cap),
  and other hardening LIKELY findings. Hardened mode deferred — the pioneer
  proves parity, not hardening.
- **D-4**: 4-pack fixture matrix (`sample_pack`, `empty_pack`, `json_pack`,
  `multi_lang_pack`) x 10 functions = 40 goldens. Picked to exercise
  empty-input, non-Markdown input, and multi-codepoint edge paths.
- **D-5**: FFI shim (T-26) + CLI integration (T-27) tasks scheduled but
  marked low priority; pioneer scope ends at "crate green on parity".

Tasks: 29 total (T-01..T-29). Of these, **T-25 (criterion benches), T-26
(FFI), T-27 (CLI integration)** were deferred in the Phase 3 retry brief.

---

## Phase 3 — Implementation

**Planned engine**: codex (high-throughput implementation pass).
**Actual engine**: Claude opus (full retry).

**What happened**: codex was launched on the Phase 2 plan and hung for
approximately **5 hours** producing zero observable output (no file writes,
no log lines, no stdout). The session was terminated and the Phase 3 brief
was re-launched against Claude opus, which delivered the full P1+P2 surface
(`Build`, `ExtractReferences`, `ParseFromPack`, `StripContractBlock`,
`Verify`, `Render`, `Embed{Markdown,XML,Plain,JSON}` + supporting types,
hash, format, parse_refs, embed, verify, builder) in a single pass.

**Output**:
- `crates/ctx-contract/src/{lib,types,format,hash,parse_refs,embed,verify,builder}.rs`
- `crates/ctx-contract/src/testing/{mod,parity_fixture_builder}.rs`
- `cmd/contract-golden-export/main.go` (Go-side golden exporter)
- `internal/contract/build.go` — `SetNowFunc` clock seam
- `internal/contract/testdata/{empty_pack.md,json_pack.json,multi_lang_pack.md}`
- 40 goldens in `tests/parity/goldens/`
- `crates/ctx-contract-probe/` + `ci/cross-compile-probe.sh`

Reports: `crates/ctx-contract/PHASE3_REPORT.md` (Claude),
`tests/parity/PHASE3_CLAUDE_REPORT.md`.

---

## Phase 4 — Verification

**Output**: parity runner + adversarial review.

- **Parity runner**: 40/40 PASS on first execution after Phase 3 delivery.
  See `tests/parity/PARITY_REPORT.md`.
- **Adversarial review**: claude + Sentinel multi-engine review surfaced
  **4 CONFIRMED bugs** (F-01..F-05 + F-07 / F-13) and a handful of LIKELY
  findings (F-06, F-08 deferred). See
  `crates/ctx-contract/PHASE4_REVIEW.md`.
- **codex Phase 4 verification was SKIPPED** to avoid a second multi-hour
  hang. Adversarial review degraded to Claude + Sentinel only.

The 4 CONFIRMED findings became the Phase 5 mandate.

---

## Phase 5 — Improvement Loop

**Output**: 1-loop compressed fix pass.

| id   | file                                                | LOC | regression test                                                                                                | status   |
|------|-----------------------------------------------------|----:|----------------------------------------------------------------------------------------------------------------|----------|
| F-01 | `crates/ctx-contract/src/parse_refs.rs`             |  ~10 | `f01_oversized_line_terminates_scanning_like_go_bufio`                                                          | RESOLVED |
| F-02 | `crates/ctx-contract/src/verify.rs`                 |  ~20 | `f02_relative_dotdot_collapses_against_preceding_component`                                                     | RESOLVED |
| F-03 | `crates/ctx-contract/src/embed.rs`                  |   ~8 | `f03_contract_null_decodes_as_zero_contract_with_schema_version`                                                | RESOLVED |
| F-04 | `crates/ctx-contract/src/parse_refs.rs`, `embed.rs` |   ~8 | `f04_diff_header_rejects_unicode_whitespace_separator`, `f04_diff_header_rejects_ideographic_space_separator`, `f04_strip_contract_block_treats_nbsp_as_non_whitespace` | RESOLVED |
| F-05 | `crates/ctx-contract/src/embed.rs`                  |   ~8 | `f05_embed_json_patch_sorts_top_level_keys_alphabetically`                                                      | RESOLVED |
| F-07 | `crates/ctx-contract/src/verify.rs`                 |   ~5 | (covered via parity goldens)                                                                                    | RESOLVED |
| F-13 | `crates/ctx-contract/src/verify.rs`                 |   ~3 | (dead-code removal)                                                                                             | RESOLVED |

**Final test counts**:
- 19 unit (in-crate)
- 40 cross-impl parity
- 7 regression (Phase 5 pins)
- **66 / 66 passing**

A second improvement loop was not required; all CONFIRMED findings resolved
in one pass and the LIKELY-but-out-of-scope findings (F-06 cross-platform
errno, F-08 non-UTF-8 line handling) were intentionally deferred.

Report: `crates/ctx-contract/PHASE5_REPORT.md`.

---

## Phase 6 — Delivery (this phase)

**Output**:
- `.gitignore` updated to exclude `crates/**/target/`,
  `crates/**/*.rs.bk`, `tests/parity/goldens-tmp/`.
- `CHANGELOG.md` created with `[Unreleased]` entry covering Added / Changed.
- `tests/RELEASE_NOTES.md` documenting ships / does-not-ship / rollback.
- `tests/SUMMIT_EXECUTION_REPORT.md` (this document).
- Commit strategy recommended (5 conventional commits — see "Commit
  strategy" section below).

**No git commits were made by this phase** — by recipe constraint, Phase 6
only proposes the granularity and leaves execution to the human operator.

---

## Engine distribution audit

| engine | recipe target | actual contribution | reason for delta |
|---|---:|---:|---|
| codex | 55% | ~15% | Phase 3 hung 5h (zero output); Phase 4 skipped pre-emptively to avoid a second hang |
| claude | 45% | ~85% | Carried Phase 3 retry end-to-end + Phase 4 adversarial review + Phase 5 fix loop + Phase 6 delivery |
| agy | 0% (advisory) | 0% | Blocked at Claude Code safety classifier on `--dangerously-skip-permissions` for the entire run |

**Implication**: The Summit recipe's tri-engine *quality-maximization* thesis
was **not fully exercised**. The recipe effectively degraded to
"dual-engine intent, single-engine execution". This does not invalidate the
parity result (the Go side is the ground truth, and parity is byte-exact),
but it means the **engine-diversity hypothesis remains untested** for this
pioneer. The next Summit pass should either:

1. Resolve the agy safety-classifier block upstream (re-pin permissions
   model, or invoke agy through a different entry point), and
2. Diagnose the codex hang root cause (input-size limits? streaming-output
   buffer? watchdog timeout?) before re-launching codex on a
   comparable-scope task.

---

## Cost-and-time summary

| phase | wall-clock (approx) | notes |
|---|---:|---|
| Phase 0 charter | ~20 min | single-engine, low cost |
| Phase 1 dual-engine consensus | ~45 min | dual-engine reasoning, agy blocked |
| Phase 2 plan | ~30 min | single-engine planning |
| Phase 3 implementation | ~5h hang + ~90 min retry | codex hang dominated wall-clock |
| Phase 4 verification | ~60 min | parity + adversarial review |
| Phase 5 improvement | ~45 min | 1-loop, 7 fixes + 7 regression tests |
| Phase 6 delivery | ~10 min | this report + changelog + release notes |

**Total**: ~9 hours wall-clock, of which ~55% (5 of 9 hours) was the codex
Phase 3 hang. Effective working time (excluding the hang) was ~4 hours.

---

## Strategic conclusions

**What this pioneer proves**:

1. **Feasibility**: a leaf Go module *can* be ported to Rust with byte-exact
   JSON parity in a single Summit cycle once a working harness exists.
2. **Recipe robustness**: dual-engine consensus + golden parity + adversarial
   review + compressed fix loop converged in 1 improvement loop with 7
   CONFIRMED + LIKELY findings fully resolved. The protocol works.
3. **Cost of clock seams is negligible**: a single `var nowFn = time.Now`
   indirection plus a `SetNowFunc` opt-in is enough to make Go-side
   deterministic golden export practical without touching production
   call sites.
4. **Cross-compile reach is plausible**: the probe verifies darwin-arm64
   on the host; the same crate skeleton compiles cleanly on stable Rust
   and matches the Go target matrix shape.

**What this pioneer does NOT prove**:

1. **Tri-engine quality maximization** — agy was blocked, codex hung; only
   Claude executed at meaningful volume. The diversity hypothesis remains
   untested.
2. **Production wiring** — no FFI, no CLI integration, no end-user-visible
   behavior change. The crate is a library hanging off the side of the repo.
3. **Hardening** — D-3 deliberately left L-02 symlink mode and L-08 JSON
   parse-size cap at parity-only. Production hardening would require a
   second pass.
4. **Cross-platform parity** — F-06 (errno→string) and F-08 (non-UTF-8 line
   handling) deferred. Linux/Windows parity not exhaustively verified.
5. **Benchmark wins** — criterion suite deferred (T-25, P3). No quantitative
   "Rust is faster" claim is supportable from this delivery.

---

## Follow-up scope (deferred to next Summit / apex)

| id | description | priority |
|---|---|---|
| T-25 | Criterion benchmark suite for `ctx-contract` vs Go baseline | P3 |
| T-26 | FFI shim (cgo or extern-C) for Rust crate ↔ Go binary | P1 |
| T-27 | `ctx` CLI integration — flip pioneer codepath to Rust under feature flag | P1 (depends on T-26) |
| F-06 | Cross-platform errno→string parity | P2 |
| F-08 | Non-UTF-8 line handling parity | P2 |
| L-02 | Hardened mode (symlink follow=false) | P2 |
| L-08 | JSON parse size cap | P2 |
| ENG-1 | Resolve agy safety-classifier block (re-pin permissions / alt entry) | P1 |
| ENG-2 | Diagnose codex Phase 3 hang root cause before re-launching | P1 |
| EXP-1 | Pick next module up the dependency graph (likely `internal/pack` leaves) | P2 |

---

## Commit strategy (recommendation only — not executed)

Five conventional commits, ordered to keep each commit independently
buildable and reviewable:

1. **`feat(contract): add Go-side clock seam (SetNowFunc) for parity testing`**
   - Files: `internal/contract/build.go`
   - Body: 30-LoC opt-in `nowFn` indirection + `SetNowFunc(fn) func() time.Time`.
     Production unchanged; enables deterministic golden export.

2. **`feat(testdata): add empty/json/multi-lang pack fixtures for parity testing`**
   - Files: `internal/contract/testdata/{empty_pack.md,json_pack.json,multi_lang_pack.md}`
   - Body: three new pack fixtures exercising empty input, raw JSON input,
     and multi-codepoint input. Feeds both Go-side golden export and
     Rust-side parity tests.

3. **`feat: cross-compile probe + Go-side golden exporter (Summit Phase 3 infra)`**
   - Files: `ci/cross-compile-probe.sh`, `cmd/contract-golden-export/`,
     `crates/ctx-contract-probe/`, `tests/parity/goldens/`
   - Body: probe sweeps `{darwin,linux} x {amd64,arm64}`; Go CLI emits
     deterministic JSON goldens. 40 goldens generated.

4. **`feat(ctx-contract): port internal/contract from Go to Rust (Summit pioneer module)`**
   - Files: `crates/ctx-contract/{Cargo.toml,Cargo.lock,src/**,tests/**,PHASE*_REPORT.md,PARITY_REPORT.md}`
   - Body: full P1+P2 surface (Build / ExtractReferences / ParseFromPack /
     StripContractBlock / Verify / Render / Embed{Markdown,XML,Plain,JSON});
     19 unit + 40 parity + 7 regression tests passing.

5. **`docs(summit): add Summit execution report, changelog, release notes`**
   - Files: `.gitignore`, `CHANGELOG.md`, `tests/RELEASE_NOTES.md`,
     `tests/SUMMIT_EXECUTION_REPORT.md`, `tests/parity/PARITY_REPORT.md`,
     `tests/parity/PHASE3_CLAUDE_REPORT.md`
   - Body: gitignore Rust target dirs; CHANGELOG entry under
     `[Unreleased]`; release notes with rollback plan; master execution
     report aggregating Phase 0..6.

Subject lines ≤72 chars. Bodies kept to 1-3 lines explaining the "why".

---

## Files produced (this Summit)

**Go side**:
- `internal/contract/build.go` (modified, +30 LoC clock seam)
- `internal/contract/testdata/empty_pack.md`
- `internal/contract/testdata/json_pack.json`
- `internal/contract/testdata/multi_lang_pack.md`
- `cmd/contract-golden-export/main.go`

**Rust side**:
- `crates/ctx-contract/Cargo.toml`, `Cargo.lock`
- `crates/ctx-contract/src/{lib,types,format,hash,parse_refs,embed,verify,builder}.rs`
- `crates/ctx-contract/src/testing/{mod,parity_fixture_builder}.rs`
- `crates/ctx-contract/tests/{parity,regression}.rs`
- `crates/ctx-contract-probe/{Cargo.toml,Cargo.lock,src/lib.rs}`

**Infrastructure**:
- `ci/cross-compile-probe.sh`
- `.gitignore` (updated)

**Goldens**:
- `tests/parity/goldens/{sample_pack,empty_pack,json_pack,multi_lang_pack}/*.json` (40 files)

**Reports**:
- `crates/ctx-contract/PHASE3_REPORT.md`
- `crates/ctx-contract/PHASE4_REVIEW.md`
- `crates/ctx-contract/PHASE5_REPORT.md`
- `tests/parity/PARITY_REPORT.md`
- `tests/parity/PHASE3_CLAUDE_REPORT.md`
- `CHANGELOG.md`
- `tests/RELEASE_NOTES.md`
- `tests/SUMMIT_EXECUTION_REPORT.md` (this document)
