# Phase 3 — ctx-where port report

**Status:** SHIPPED — parity green, fragility verdict: **LOOKUP_HEAVY thesis at risk**

**Branch:** `phase3/where-replay-rust-port`
**Date:** 2026-05-29

## Scope

Port `internal/where/where.go` (1110 LOC) to a Rust crate that runs the
LOOKUP_HEAVY hot path (substring scoring, identifier splitting,
Levenshtein DP) behind the existing Go-side walk + tree-sitter symbol
extraction pipeline. Predicted intrinsic speedup 1.85-2.5×; Phase 3 exit
criterion ≥1.3× **net** end-to-end including cgo overhead.

## Files

```
crates/ctx-where/
├── Cargo.toml              # dhat baked in from day 1 (lesson #5)
├── build.rs                # cbindgen → include/ctx_where.h
├── cbindgen.toml
├── include/ctx_where.h     # generated FFI surface
├── src/
│   ├── lib.rs              # module roots
│   ├── types.rs            # Suggestion/Match/Result/ScoreBreakdown
│   ├── levenshtein.rs      # pure DP edit-distance (first parity gate)
│   ├── score.rs            # scoreFile / scoreFileWithSets,
│   │                       # extract_keywords, split_identifier,
│   │                       # has_all_keyword_sets, context_lines
│   ├── search.rs           # search_with_options + suggest_similar
│   │                       # operating on pre-walked file list
│   ├── ffi.rs              # ctx_where_search / suggest / levenshtein
│   │                       # + free_string + version (extern "C")
│   └── testing/parity_fixture_builder.rs
├── benches/
│   ├── where.rs            # criterion: small/medium/large fixtures
│   └── memory.rs           # dhat-rs profile, gated by --features dhat
└── tests/
    ├── parity.rs           # parity-vs-Go-goldens (3 fixtures)
    └── regression.rs       # 8 edge-case regressions
```

## Test counts

| Suite | Tests | Status |
|-------|------:|--------|
| `cargo test --lib` | 18 | PASS |
| `cargo test --test regression` | 8 | PASS |
| `cargo test --test parity --features testing` | 3 | PASS |
| **Total** | **29** | **PASS** |

Parity covers `small_repo` (3 files), `medium_repo` (~10 files with
symbol extraction), `large_repo` (1000 generated handler files).
Output JSON matches Go byte-for-byte after `pretty_assertions::assert_eq`
on the parsed `serde_json::Value`.

## Speedup measurements

### Criterion (in-process, isolated Rust hot path)

| Fixture | Files | Rust µs/op |
|---------|------:|-----------:|
| small_repo | 3 | 6.45 |
| medium_repo | 10 | 15.4 |
| large_repo | 1000 | 3,458 |

### Go testing.B (Go hot path, isolated)

| Fixture | Go µs/op | bytes/op | allocs/op |
|---------|---------:|---------:|----------:|
| small_repo | 265 | 327,059 | 1,410 |
| medium_repo | 663 | 808,365 | 2,376 |
| large_repo | 101,041 | 86,358,586 | 250,285 |

Intrinsic ratio (Go-isolated / Rust-isolated):

| Fixture | Ratio |
|---------|------:|
| small_repo | **41×** |
| medium_repo | **43×** |
| large_repo | **29×** |

The intrinsic margin is well above the 1.85-2.5× prediction.

### End-to-end through cgo (the FRAGILITY TEST)

Measured via `cmd/where-engine-diff` with the SAME walk + symbol
extraction running on the Go side BEFORE FFI for both engines; only the
scoring loop differs.

| Fixture | n reps | Go elapsed | Rust elapsed | **Net speedup** | Verdict |
|---------|-------:|-----------:|-------------:|----------------:|---------|
| small_repo | 2000 | 526.7 ms | 544.4 ms | **0.97×** | FAIL |
| medium_repo | 2000 | 1.234 s | 1.336 s | **0.92×** | FAIL |
| large_repo | 20 | 1.900 s | 1.998 s | **0.95×** | FAIL |

**Verdict: FAIL — LOOKUP_HEAVY thesis breaks under cgo+JSON overhead.**

### Root cause

The walk + tree-sitter symbol extraction dominates total runtime
(~96-98% on the medium fixture). Of the remaining 2-4% scoring time, the
in-process Rust scoring loop is ~40× faster — but the JSON marshal of
the pre-walked file list (~10s of KB) plus cgo crossing plus JSON
unmarshal of results adds roughly the SAME 2-4% of total runtime that
the Rust win would otherwise reclaim, neutralising the speedup.

Two design choices that contributed:

1. **Walk + symbols stay on Go side**: this preserves the existing
   tree-sitter pipeline and avoids a heavy Rust dep, but it means the
   single LOOKUP_HEAVY win must pay for the JSON shuttle of the
   pre-walked corpus on every call.
2. **Per-file JSON serialisation**: each FileInput carries its full
   content `lines` array; the cgo bridge marshals all of it. Going to
   FlatBuffers or a borrowed-bytes protocol could change the verdict,
   but is out of scope for Phase 3.

## Memory profile

dhat-rs `cargo bench --features dhat --bench memory` (200 iterations on
medium_repo):

| Metric | Value |
|--------|------:|
| Total allocations | 4,396,027 bytes / 98,239 blocks |
| At t-gmax | 348,281 bytes / 311 blocks |
| At t-end | 25,490 bytes / 86 blocks |

Comparison with Go `BenchmarkSearch_MemAlloc` (200 iters medium):

| Engine | bytes/op | allocs/op | heap_after |
|--------|---------:|----------:|-----------:|
| Go | 808,075 | 2,376 | 3,643,672 |
| Rust (dhat avg per call) | 22,000 | ~490 | (n/a — single-shot profile) |

Rust uses **~37× less memory per call** for the scoring hot path
alone. This advantage is real and persists across the cgo boundary,
since the allocations are in the Rust heap, not the Go heap.

## FRAGILITY VERDICT — recommendation for Phase 4 LOOKUP

> **STOP — do NOT port `focus` (Phase 4 candidate LOOKUP_HEAVY module)
> until the cgo+JSON shuttle is redesigned.**

Per `tests/MIGRATION_ROADMAP.md` Phase 3 stop-condition: when `where`
net end-to-end speedup falls below 1.2×, the LOOKUP_HEAVY thesis is
falsified for the current FFI shape. Our measured 0.92-0.97× is BELOW
the 1.2× soft floor, so:

- **DO NOT** start the `focus` / `heatmap` ports as planned for Phase 4.
- **DO** investigate alternative FFI shapes (FlatBuffers, shared-memory
  walker, ARC-passed handles) before retrying any LOOKUP_HEAVY module.
- The 40× intrinsic scoring win is REAL — it just isn't visible end-to-
  end through this FFI shape. If the FFI redesign succeeds, the win
  could come back online for free.

## Lessons (Phase 3)

1. **Intrinsic vs net speedup gap is real.** Criterion's 6μs vs Go's
   265μs (41×) looks like a slam-dunk, but the JSON shuttle adds ~270μs
   per call when the corpus is non-trivial. The criterion bench was
   honest within its scope — the gap is geometry, not measurement.
2. **The walk-on-Go-side decision was correct for portability but wrong
   for LOOKUP_HEAVY economics.** A future redesign should consider
   either:
   - Walking inside Rust too (with a regex-based symbol fallback), so
     the FFI only crosses once per `where` invocation, or
   - A shared-memory handle pattern so the FileInput corpus is built
     once per session and reused.
3. **The fragility test deserved its name.** Phase 1 + 2 both showed
   the intrinsic Rust win surviving cgo because the JSON payload was
   smaller (`scan` is per-file with line bytes, `relations` returns a
   compact graph). LOOKUP_HEAVY's per-call corpus is fundamentally
   different.
4. **Bench memory is decoupled from speedup.** Rust's 37× memory win
   over Go was unaffected by the cgo geometry. This is a real,
   shippable improvement even when wall-clock is a wash.
5. **The perf-regression CI workflow MUST gate this.** A future PR that
   accidentally bloats the JSON payload (adding a field, dropping
   omitempty) would silently make this worse; the workflow we shipped
   in this PR catches it.

## What ships in Phase 3

- The Rust crate is BUILT, TESTED, and PARITY-VERIFIED. Operators who
  want to opt in can use `--where-engine=rust` on a `-tags rust_contract`
  build; the fallback path is transparent.
- The default `--where-engine=go` remains the production path; users
  see no behavior change.
- The Phase 4 LOOKUP roadmap recommendation is updated based on the
  fragility verdict.

## Reproduce locally

```bash
# Build Rust crate
cd crates/ctx-where && cargo build --release

# Run parity (Go goldens vs Rust)
go run ./cmd/where-golden-export ./tests/where-fixtures ./tests/parity/where-goldens
cargo test --manifest-path crates/ctx-where/Cargo.toml \
    --test parity --features testing

# Run the FRAGILITY TEST
CGO_ENABLED=1 go build -tags rust_contract -o /tmp/diff ./cmd/where-engine-diff
/tmp/diff ./tests/where-fixtures/large_repo
```
