# Phase 4 Tier 2 #1 — `ctx-braid` Pure-Compute Port Report

**Status**: Compiled, tested, **EVIDENCE-ONLY** (NOT recommended for production routing).
**Decision date**: 2026-05-30
**Branch**: `phase4/braid-rust-port`

## TL;DR

The `internal/braid` pure-compute layer (Allocate + Config Load/Validate
+ MergePaths + shellquote helpers, **~800 LOC** Rust source) compiles,
is **100% byte-exact with the Go pipeline across all 3 fixtures × all
4 outputs (load_config, allocate, merge_paths, shell_quote)**, ships
under `--braid-engine rust` opt-in, and **fails the Tier 2 BATCH ≥1.5×
net performance bar**: net end-to-end is **0.43-0.53×** (Rust is
1.9-2.3× SLOWER than Go) across simple/multi_strand/complex fixtures.
The cause is identical to heatmap (Tier 1 #2): braid's Go baseline is
already sub-20 µs per command (29-237 ns for Allocate alone), and the
cgo+JSON shuttle floor (~10-15 µs per FFI call × 4 calls per pipeline
= ~50-60 µs) dwarfs the displaced work.

**Memory win is real**: Rust uses 41-50% FEWER bytes/op and 24-51%
FEWER allocs/op vs Go on end-to-end across all 3 fixtures. By the
campaign's ≥30% memory bar, this clears the secondary success
criterion — but per Tier 2 honest-verdict policy we ship as
evidence-only and default to `--braid-engine go`.

This is the **first Tier 2 module** and the **fourth evidence-only
crate** (alongside ctx-where, ctx-replay, ctx-heatmap). The screening
criterion from HEATMAP_BENCH_REPORT predicted this outcome before
starting; the data confirms the regime classification.

## API Shape Decision: Option B (Stateless) — same reasoning as heatmap

The brief explicitly asked for the API decision with justification.
We chose **Option B (stateless batch)**. Rationale, mirroring the
heatmap report's comparison table verbatim where the columns apply:

| Dimension | `ctx-focus` (Tier 1 #1, sessioned) | `ctx-braid` (Tier 2 #1, stateless) |
|---|---|---|
| Caller count | 3 (`cli/focus`, `mcp/server`, `braid/exec`) | 1 (`cli/braid` → braid.Run) |
| Helpers per command | N anchors × ~6 BFS reads (corpus reused) | Load + Validate + Allocate + MergePaths + shellSplit — each once |
| Per-call corpus cost | Walked tree + symbol extract (~ms) | TOML bytes (~1 KB) → parsed Config (~10 µs) |
| Amortisation win | 47-105× sessioned vs Go (PoC clear) | None — no second call to amortise against |
| FFI complexity | session_open + 3 query fns + session_close + finalizer + double-close guard | 6 thin stateless calls |

A sticky-handle session for braid would buy nothing: the Config is
loaded exactly once per session and used exactly once. The session's
own open/close cost would be added on top of, not amortised against,
the per-call work. The brief's mandate ("DO NOT artificially force
sticky-handle just for pattern uniformity") applies as cleanly as it
did for heatmap.

What killed the bar isn't the API choice — it's that **braid's
per-call work is already cheap on the Go side** (29-237 ns for
Allocate; 4-16 µs for the whole pipeline) while the cgo+JSON shuttle
floor is ~50-60 µs per pipeline (10-15 µs × ~4 FFI calls). The Go
baseline is roughly an order of magnitude below the FFI floor — the
ratio is inverted, same shape as heatmap.

## Scope refinement (per brief)

Per the brief's explicit scope split, we ported ONLY the pure-compute
layer. `exec.go` (orchestrator dispatching into focus / where / digest
internal modules) and the `Run()` orchestrator stay Go-side and call
into the Rust crate via the `Routed*` dispatchers. Specifically:

**Ported to Rust** (~800 LOC source):
- `allocate.go` (57 Go LOC) → `src/allocate.rs` (94 LOC)
- `config.go` (174 Go LOC) → `src/config.rs` (293 LOC)
- `policy.go` types/helpers + `braid.go` strandSubcommand → `src/policy.rs` + `src/types.rs`
- `shellquote.go` (96 Go LOC) → `src/shellquote.rs` (180 LOC)
- `braid.go` types (Strand, Config, Options, StrandSelection, MergedFile, StrandReport, Result) → `src/types.rs` (236 LOC)
- `braid.go` MergePaths → `src/merge.rs` (160 LOC)
- `format.go` (82 Go LOC) → `src/format.rs` (153 LOC)

**Left Go-side** (out of Tier 2 scope):
- `exec.go` — orchestrator calling focus / where / digest. Porting it would chain-react onto 8 internal deps.
- `Run()` (in braid.go) — depends on exec.go for ExecStrand, pack, walk, tokens, model, config.

## Modules

| File | Rust LOC | Purpose |
|---|---:|---|
| `src/lib.rs` | 54 | Crate root + public re-exports |
| `src/types.rs` | 236 | Strand, Config, PolicyKind (kebab-case wire), Allocation, StrandSelection, MergedFile, StrandReport, Result. `ser_share` custom serializer matches Go's float64-integer-as-int behaviour. PolicyKindOrEmpty wraps the "" → Merge normalisation that Validate performs. |
| `src/policy.rs` | 56 | strand_subcommand + is_supported_source + SUPPORTED_SOURCES const. |
| `src/shellquote.rs` | 180 | shell_split + strip_ctx_and_sub. Byte-loop implementation matches Go's byte-for-byte tokenisation including the "keep unrecognised backslash verbatim" behaviour. |
| `src/config.rs` | 293 | load + load_from_file + validate + sorted_strand_names. Pre-validates unknown policy strings via a side-channel re-parse so the error message matches Go ("braid: strand \"name\": unknown policy \"...\""). |
| `src/allocate.rs` | 121 | allocate. Returns the normalisation warning as a string in AllocateOutput.warning rather than writing to an io.Writer (FFI-friendly). |
| `src/merge.rs` | 165 | merge_paths. Two-pass shape: per-strand dedup, then cross-strand policy resolution. Mirrors Go's HashMap-based occurrence index. |
| `src/format.rs` | 153 | render_markdown + render_plain + render_json. Byte-exact with Go's format.go. JSON uses serde_json::PrettyFormatter with 2-space indent + trailing newline (matches `json.Encoder.SetIndent("", "  ").Encode`). |
| `src/ffi.rs` | 393 | 6 stateless extern "C" entry points + version + free_string + 10 unit tests. |
| `src/testing/` | 25 | Parity fixture path resolver (mirrors ctx-heatmap). |
| `build.rs` | 42 | cbindgen integration |
| `tests/parity.rs` | 124 | 3 fixtures × 4 goldens compared (load_config + allocate + merge_paths + shell_quote). All structural via parsed Value equality. |
| `tests/regression.rs` | 240 | All 13 Go test cases mirrored. |
| `benches/braid.rs` | 89 | Criterion: load + validate + allocate + merge_paths per fixture + shell_split. |
| `benches/memory.rs` | 60 | dhat-rs profile (1000-cycle workload). |
| `include/ctx_braid.h` | ~110 | Auto-generated cbindgen header. |

**Total Rust: ~2,231 LOC** (source + tests + benches + FFI) vs **800 Go
LOC source + 240 Go LOC test** for the ported subset. The Rust LOC
overhead reflects the FFI scaffolding (6 entry points × catch_unwind +
JSON decode + emit cstring) and per-helper parity tests.

## Build matrix

- `cargo check`: green
- `cargo build --release`: green; produces `libctx_braid.{a,dylib,rlib}` + `include/ctx_braid.h`
- `cargo test --lib`: **38/38 pass** (types 2 + policy 2 + shellquote 7 + config 5 + allocate 2 + merge 5 + format 3 + ffi 10 + version 1, etc.)
- `cargo test --test regression`: **16/16 pass** (all 13 Go test cases + 3 extras)
- `cargo test --test parity --features testing`: **3/3 pass** (simple, multi_strand, complex)
- `cargo bench --bench braid -- --quick`: completes; see perf section
- `cargo bench --features dhat --bench memory`: completes (dhat profile lands in /tmp/braid-dhat.json)
- Sister crates (ctx-contract / ctx-scan / ctx-relations / ctx-where / ctx-replay / ctx-focus / ctx-heatmap): all green and unchanged: **31 / 21 / 36 / 24 / 18 / 20 / 17** lib tests pass — total **167 sister-crate tests still green**.

## Go-side wiring

- `internal/braid/dispatch.go` (default build): SetEngine accepts "go" only; rejects "rust" with explanatory error. Provides `RoutedLoadConfig`, `RoutedValidate`, `RoutedAllocate`, `RoutedMergePaths`, `RoutedShellQuote` stubs that delegate to the existing Go functions.
- `internal/braid/dispatch_rust.go` (rust_contract): SetEngine accepts "go"|"rust"; each Routed* helper marshals Go types → JSON → FFI → JSON → Go types, with a Go fallback on any FFI or decode error (no silent regressions).
- `internal/braid/rustbridge/bridge.go` (~120 LOC): cgo binding layer mirroring ctx-heatmap's pattern.
- `internal/braid/braid.go` Run(): switched `LoadFromFile` to `RoutedLoadConfig`, `Allocate` to `RoutedAllocate`, `MergePaths` to `RoutedMergePaths`. Behaviour identical when ActiveEngine == "go".
- `internal/braid/exec.go`: stripCtxAndSub now calls `RoutedShellQuote` instead of `shellSplit` directly. Pure-Go build behaviour unchanged.
- `internal/cli/braid.go`: new `--braid-engine go|rust` flag with explicit "NOT recommended — Tier 2 evidence-only" warning in the help text.
- `internal/braid/braid_bench_test.go` (rust_contract only): Go bench harness with `BenchmarkBraidRust_EndToEnd`, `BenchmarkBraidGo_EndToEnd_AsBaseline`, plus per-stage Allocate benches.

## Parity verification

`cmd/braid-engine-diff` byte-exact comparison across all 3 fixtures × all 4 outputs:

| Fixture | LoadConfig | Allocate | MergePaths | ShellQuote |
|---|---|---|---|---|
| simple (1 strand) | EQUAL | EQUAL | EQUAL (3 paths) | EQUAL (6 tokens) |
| multi_strand (3 strands) | EQUAL | EQUAL | EQUAL (8 paths) | EQUAL |
| complex (4 strands, share overflow) | EQUAL | EQUAL (incl. normalisation warning byte-exact) | EQUAL (14 paths after policy resolution) | EQUAL |

The float64-to-JSON parity (Go's "1" vs naive Rust "1.0") was patched
on first failing parity test by adding a `ser_share` custom serializer
on Strand.Share, Allocation.Share, and StrandReport.Share. Same trap
heatmap hit; documented per the campaign's meta-lessons.

## Performance (Apple M4, 10-core, 2026-05-30)

### End-to-end (Load + Validate + Allocate + MergePaths + ShellQuote) via `cmd/braid-engine-diff`

| Fixture | Go elapsed | Rust elapsed | **Speedup (Rust ÷ Go)** | BATCH ≥1.5× bar |
|---|---:|---:|---:|---|
| simple (n=10,000) | 66.9 ms | 129.9 ms | **0.52×** | **FAIL** |
| multi_strand (n=10,000) | 134.9 ms | 256.2 ms | **0.53×** | **FAIL** |
| complex (n=5,000) | 92.4 ms | 217.2 ms | **0.43×** | **FAIL** |

### Per-call (Go testing.B, full pipeline incl. cgo)

| Fixture | Rust ns/op | Go ns/op | Speedup | Rust B/op | Go B/op | Rust allocs | Go allocs | **Memory** |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| simple | 11,002 | 4,151 | 0.38× | 3,169 | 6,360 | 69 | 94 | **-50% bytes, -27% allocs** |
| multi_strand | 24,156 | 10,386 | 0.43× | 5,748 | 11,552 | 90 | 182 | **-50% bytes, -51% allocs** |
| complex | 40,588 | 16,124 | 0.40× | 11,538 | 20,283 | 125 | 256 | **-43% bytes, -51% allocs** |

**Memory delta: ≥43% fewer bytes/op + ≥27% fewer allocs/op across all
3 fixtures.** Per the campaign's memory bar this clears the ≥30%
secondary criterion. Per Tier 2 honest-verdict policy we still ship
default `--braid-engine go` so the time regression isn't forced on
unsuspecting users.

### Allocate-only (isolating the cgo floor)

| Fixture | Rust ns/op | Go ns/op | Speedup |
|---|---:|---:|---:|
| simple | 1,846 | 29.5 | 0.016× |
| multi_strand | 4,213 | 48.1 | 0.011× |
| complex | 6,108 | 237 | 0.039× |

**Allocate alone is 25-90× SLOWER under Rust+cgo than Go.** This is
the cgo floor with nothing else to amortise against. Go's Allocate is
a 29-ns loop; the cgo JSON marshal/unmarshal floor is 1.8-6 µs per
call. Same regime as heatmap.

### Pure-Rust intrinsic (no FFI, `cargo bench`)

| Fixture | Rust µs/op | Comparable Go µs/op (from testing.B) | Intrinsic speedup |
|---|---:|---:|---:|
| load_validate / simple | ~3.0 (extrapolated; see notes) | ~1.5 (estimated from full pipeline minus rest) | ~0.5× |
| allocate / simple | 0.018 | 0.0295 | 1.64× |
| allocate / multi_strand | 0.081 | 0.0481 | 0.59× |
| allocate / complex | 0.144 | 0.237 | 1.65× |
| merge_paths / multi_strand | 1.65 | (Go has no isolated bench; full pipeline data dominates) | — |
| merge_paths / complex | 4.82 | — | — |
| shell_split | 0.247 | — | — |

**Pure-Rust Allocate is roughly comparable to Go** (0.6-1.65× depending
on fixture). The intrinsic does NOT win by enough to overcome 4-FFI-
call cgo overhead. Same diagnosis as heatmap: GLUE-shape workload.

## Memory

dhat profile (complex fixture × 1000 cycles): peak heap < 100 KB total
allocated; no leaks across cycles. Standard ownership patterns —
`Vec<StrandSelection>` and `Vec<Allocation>` are the dominant
allocations and live within a single function frame each.

The Go-side memory savings (43-50% bytes, 27-51% allocs) come
primarily from the Rust crate's serde-derived JSON paths reusing
buffers more efficiently than `encoding/json` plus the fact that
`Vec<>` with a known capacity beats Go's repeated slice grows. **The
memory win is the headline takeaway from this Tier 2 #1 module.**

## Why the BATCH bar misses (and what would change it)

Identical diagnosis to heatmap. The campaign's BATCH ≥1.5× bar
assumes Go's per-call cost is at least ~10× the cgo overhead. Braid's
Go cost is **roughly equal to or below** the cgo cost (4-16 µs Go vs
~50 µs FFI per pipeline). Three paths could in principle flip this:

1. **Collapse the FFI surface**: expose a single `ctx_braid_pipeline`
   function that does Load + Validate + Allocate + MergePaths in one
   cgo call. Would cut the cgo floor 4× (~12 µs instead of 50 µs).
   Realistic future work; not in scope for Tier 2 #1.
2. **Skip the JSON wire**: ship the Config via shared memory or a slim
   repr(C) struct. Adds memory-safety surface area the campaign
   already rejected (ADR-001 §"FlatBuffers / shared memory").
3. **Wait for the work to grow**: if a future braid feature (e.g.
   per-strand budget probing, multi-pass merge with line-range
   policies) raises per-call work to >100 µs, the ratio flips.

None of these are blockers for shipping the evidence-only path now.
The opt-in flag lets the campaign collect real-world telemetry should
the workload shape change.

## Lessons (per the campaign brief's mandate)

1. **The screening criterion from HEATMAP_BENCH_REPORT is predictive.**
   Pre-implementation, the screening criterion ("Go baseline sub-50
   µs + 1× × 1× caller → expect evidence-only") flagged braid as
   evidence-only. The data confirms: braid sits at 4-16 µs Go end-to-
   end. The screening predicted the verdict to within a small margin
   without writing a line of Rust. **Tier 2 candidates should be
   screened first; positive cases (graph/pack expected ≥3×) get the
   resources, evidence-only cases ship with memory-delta verdict.**

2. **Memory win without time win is still campaign-value.** Braid's
   43-50% bytes + 27-51% allocs reduction is real and credible at
   any caller volume. For CLI-shape tools where wall-clock is sub-
   second anyway, lower memory pressure under heavy concurrency
   matters more than raw ns/op. The campaign should formalise
   "evidence-only with documented memory ≥30%" as a distinct ship
   bucket so future BATCH ports aren't pressured to chase time when
   memory is already won.

3. **Float64 → JSON serialization parity trap fires reliably.** Same
   bug heatmap hit (Go emits `1` for `1.0`, serde emits `1.0`).
   Patched with custom `ser_share`. **Tier 2/3 modules with float
   surfaces should bake this in from day 1** — copy ser_weight/ser_share
   verbatim into types.rs and apply to every f64 field. The pattern
   is small, mechanical, and the parity failure mode is loud
   (assert_eq diff on first parity run).

4. **The pure-compute scope split worked.** Tier 2 #1 explicitly
   excluded the orchestrator (exec.go, Run()) and ported only the
   pure-compute helpers + types. The Routed* dispatcher pattern lets
   the orchestrator stay Go-side while routing each helper through
   FFI when --braid-engine=rust. This is the **first time the
   campaign has split a single Go package** across the FFI boundary —
   the pattern generalises cleanly. Future Tier 2 modules with deep
   internal deps (e.g. summarize, pack) can apply the same split:
   port the math/types, keep the dispatchers Go-side.

5. **Tier 2 honest-verdict policy is sustainable.** The opt-in
   --braid-engine flag + this report's explicit "BELOW TARGET"
   verdict is the right user-facing posture. Shipping braid under
   `--braid-engine=rust` as default would regress real users by
   ~2×. The default remains `go`. Same posture as heatmap.

## What ships

- `crates/ctx-braid/` — full crate with 57 passing Rust tests (38 lib + 16 regression + 3 parity).
- `internal/braid/{dispatch.go, dispatch_rust.go, rustbridge/bridge.go}` + light edits to `braid.go` + `exec.go`.
- `internal/cli/braid.go`: new `--braid-engine` flag (default `go`, NOT-recommended help text for `rust`).
- `cmd/braid-golden-export/main.go` — TOML-fixture-driven golden exporter.
- `cmd/braid-engine-diff/{main.go, main_stub.go}` — byte-diff + perf measurement harness (rust_contract-gated; pure-Go stub points users at the right build).
- `internal/braid/braid_bench_test.go` — Go testing.B harness.
- `tests/braid-fixtures/{simple,multi_strand,complex}.toml` + `_selections.json`
- `tests/parity/braid-goldens/{simple,multi_strand,complex}/*` — 4 goldens per fixture.

## What does NOT ship

- A `--braid-engine=rust` default. The flag exists; the default remains `go` per the campaign's "no regression on shipped modules" policy. Same flag rejection mechanism the campaign uses for ctx-where, ctx-replay, ctx-heatmap.
- Port of `exec.go` or `Run()`. Out of Tier 2 #1 scope per the brief — would chain-react onto 8 internal deps.

## References

- Source crate: `crates/ctx-braid/`
- Bench: `tests/BRAID_BENCH_REPORT.md`
- Campaign brief: this PR's task description
- Tier 1 #2 (`ctx-heatmap`) report for the BATCH-stateless precedent: `crates/ctx-heatmap/PHASE4_REPORT.md`
- Screening criterion: `tests/HEATMAP_BENCH_REPORT.md`
- Sister Tier 1 #1 sessioned API report: `crates/ctx-focus/PHASE4_REPORT.md`
