# ctx-echo Phase 4 Report — Tier 2 #3

**Status**: EVIDENCE ONLY (memory bucket FAIL on medium/large)
**Module**: `internal/echo/` → `crates/ctx-echo/`
**Date**: 2026-05-30
**Engineer**: Tier 2 #3 (Claude execution agent)
**Branch**: `phase4/echo-rust-port`

---

## TL;DR

`ctx-echo` is a clean, byte-parity-correct port of the BM25 evaluator with
4 parity tests, 8 regression tests, and 21 unit tests all green. Engine-diff
confirms structural equality with ULP-tolerant float comparison across small/
medium/large fixtures.

**Performance verdict**: BELOW PERF-SHIPPED BAR.
**Memory verdict**: BELOW MEMORY-BUCKET BAR on medium + large fixtures.

The shape — REGEX_HEAVY stateless batch on a hot tokenize-+-BM25 path —
sits in the regime where prior wins (`ctx-contract` 7-9×, `ctx-scan`
15-27×) had clear advantages from byte-scanning and regex compile-once
patterns. Echo's hot path is **String + small-HashMap allocation**, not
regex matching, and Rust's stdlib HashMap + small String allocations
are competitive with Go's GC at this size — without the FFI shuttle's
~10 µs floor.

This is the brief's "examine why" outcome. See *Why it didn't ship* below.

---

## API decision

**STATELESS BATCH**, mirroring `ctx-contract` and `ctx-scan`. Justification:

* Single entry point `Evaluate(pack_path, pack_body, opts) → Result`.
* No corpus reuse — each call is independent over its own pack body.
* Substantial per-call work (chunk → tokenize → BM25 → format) suggested
  the call work would dominate the ~10-15 µs cgo shuttle floor.
* No external state besides the input bytes.

FFI surface:

```c
int ctx_echo_evaluate(
    const uint8_t* pack_path_ptr, uintptr_t pack_path_len,
    const uint8_t* pack_body_ptr, uintptr_t pack_body_len,
    const uint8_t* opts_json_ptr, uintptr_t opts_json_len,
    char** out_result_ptr);
void ctx_echo_free_string(char* s);
const char* ctx_echo_version(void);
```

Memory protocol: borrowed inputs, heap-owned CString output via `into_raw`
+ `ctx_echo_free_string`. Identical conventions to ctx-contract / ctx-scan.

---

## Per-component benchmark (Apple M4)

### Rust native (criterion, release build)

| Fixture | Bytes | Time | Throughput |
|---|---|---|---|
| small | 1,349 | **35.6 µs** | 36.1 MiB/s |
| medium | 51,282 | **1.47 ms** | 33.2 MiB/s |
| large | 524,542 | **15.0 ms** | 33.3 MiB/s |

### Go native (`testing.B`, `internal/echo/echo_bench_test.go`)

| Fixture | ns/op | B/op | allocs/op |
|---|---|---|---|
| small | **72.7 µs** | 1,077,873 | 550 |
| medium | **1.25 ms** | 2,347,065 | 23,769 |
| large | **14.1 ms** | 15,128,124 | 242,529 |

### Engine-diff (Rust via cgo+JSON shuttle vs Go in-process)

| Fixture | Go elapsed | Rust elapsed | Speedup | Verdict |
|---|---|---|---|---|
| small | 160.2 ms (2k reps) | 100.7 ms | **1.59×** | MARGINAL |
| medium | 252.7 ms (200 reps) | 303.3 ms | **0.83×** | BELOW TARGET |
| large | 680.7 ms (50 reps) | 790.0 ms | **0.86×** | BELOW TARGET |

### Memory (dhat heap profiler, 50 reps/fixture)

| Fixture | Rust bytes/call | Rust blocks/call | Go B/op | Go allocs/op | Δbytes | Δallocs |
|---|---|---|---|---|---|---|
| small | 73,510 | 1,479 | 1,077,873 | 550 | **−93%** | +169% |
| medium | 2,938,987 | 59,320 | 2,347,065 | 23,769 | **+25%** | +149% |
| large | 30,595,022 | 605,380 | 15,128,124 | 242,529 | **+102%** | +149% |

---

## Verdict

**PERF SHIPPED bar (≥5×)**: NO
**PERF MARGINAL bar (≥2×)**: small only
**MEMORY BUCKET bar (≥30% bytes saved)**: small only (−93% bytes); medium and
large both REGRESS (+25% / +102% bytes, +149% allocs each)

Net classification: **EVIDENCE ONLY — DEFAULT REMAINS GO**.
Rust path ships under `--echo-engine rust` for telemetry & future
re-evaluation, but operators should not opt in for production.

---

## Why it didn't ship

The brief flagged this scenario: "if echo lands sub-5× even though it's
REGEX_HEAVY, examine why — would indicate the per-call work is smaller
than the contract/scan precedent suggested." Diagnosis:

1. **Hot path is String + HashMap, not regex/byte-scan.**
   * `ctx-contract` and `ctx-scan` shipped 7-27× because their hot paths
     were `Regex::find` over raw `&[u8]` — Rust's regex crate is
     dramatically faster than Go's `regexp` (DFA vs NFA, no allocation per
     match). Echo's hot path is `String::to_lowercase()`, `Vec::push`,
     `HashMap::entry`, and `lines.join("\n")` — none of which Rust does
     fundamentally better than Go's GC at small sizes.

2. **`Chunk.body = lines.join("\n")` reallocates per chunk.**
   * On the large fixture we produce 6,069 chunks. Each chunk's body is
     a freshly-allocated `String` ~150 bytes long. Each chunk's tokens
     vector is a freshly-allocated `Vec<String>` averaging ~25 tokens,
     each token a fresh lowercased `String`. That's ~6k × ~30 small
     String allocs ≈ 180k allocs just for chunking — matching the
     dhat blocks/call figure (605k for large).
   * Go's slice append amortises better here than Rust's `Vec<String>`
     because the Go GC reclaims unreferenced intermediate slices in
     bulk, while Rust's `Drop` runs deterministically per chunk-clone in
     `score_chunks`.

3. **BM25 score function clones each Chunk into a ScoredChunk.**
   * Mirroring the Go `[]ScoredChunk` shape required a full deep clone
     of every chunk into the output vector. On the large fixture that's
     6,069 × (Vec<String> + String) clones at ~25 tokens × ~150 bytes
     each — ~30 MiB of duplicated heap data per call. This dominates the
     +102% bytes/call regression on large.
   * Refactor opportunity: `ScoredChunk` could carry a `Chunk` reference
     instead of an owned clone, but that requires sweeping API changes
     and the perf upside is unclear given map iteration is already
     cache-unfriendly.

4. **JSON shuttle cost ≈ 6 µs on small, ≈ 10 µs on medium+large.**
   * For small, Rust 35.6 µs raw vs 50 µs through the shuttle. Shuttle
     overhead is ~14 µs which is the dominant cost component when raw
     work is ~36 µs.
   * For large, Rust 15 ms raw vs ~15.8 ms through the shuttle — shuttle
     is now negligible (~5%) but Rust raw is already slower than Go raw.

5. **f64 parity divergence (informational, not a failure).**
   * BM25 sums map-iterated f64s. Go's map iteration order is random
     each run; Rust's HashMap is order-defined but differs from Go.
     The result is ~3 ULP divergence in the BM25 score's last decimal
     places. Parity tests pass with 1e-9 relative tolerance; engine-diff
     tolerates numerically-equal byte-shape divergence.

---

## What this confirms about the screening rule

The Tier 2 #2 (pack) meta-lesson said: "sessioned can't beat sub-50 µs Go
baselines via cgo+JSON shuttle." Echo at small (72 µs Go baseline) is
right at that boundary — and the cgo route lands only 1.59× faster.

The **new** lesson from echo is that **REGEX_HEAVY classification is
necessary but not sufficient**. The shape must also be:

* **byte-scan dominated** (contract, scan), not String/HashMap dominated
  (echo). The latter has no Rust super-power.
* **Linear over input size**, not super-linear (echo's chunking +
  per-chunk tokenization is O(chunks × tokens), which scales as the
  pack body grows).

Proposed update to screening criterion in `tests/MIGRATION_ROADMAP.md`:

> REGEX_HEAVY ship candidates must additionally have:
>   (a) hot path expressible as `regex::find_iter` over `&[u8]`, AND
>   (b) Go baseline > 100 µs/op for the smallest representative
>       fixture, AND
>   (c) per-call structured output < 10 KB (so JSON shuttle stays < 10%
>       of total).

Echo fails (a) and (c) on medium/large.

---

## Parity & test status

* **21/21 unit tests pass** (tokenize, chunk, score, evaluate, ffi).
* **8/8 regression tests pass** (one per `echo_test.go` case + 3 extras).
* **4/4 parity tests pass** (3 fixtures × canonical goal + 1 threshold).
* **Engine-diff agrees on 3/3 fixtures** modulo ULP-level BM25 sum
  divergence (1e-9 relative tolerance).
* **9 prior crates' tests** (contract: 31 unit + 7 regression, scan: 7+,
  others): SPOT-CHECKED green. No changes to those crates.
* `go test ./...` (default Go build): all packages pass.
* `CGO_ENABLED=1 go test -tags rust_contract ./internal/echo/...`: pass.

---

## Files added (this PR)

### Rust crate
```
crates/ctx-echo/
  Cargo.toml
  cbindgen.toml
  build.rs
  src/
    lib.rs
    types.rs
    tokenize.rs
    chunk.rs
    score.rs
    format.rs
    evaluate.rs
    ffi.rs
  tests/
    parity.rs
    regression.rs
  benches/
    echo.rs
    memory.rs
  include/
    ctx_echo.h          (cbindgen-generated)
```

### Go-side
```
internal/echo/
  dispatch.go           (Go-only build tag)
  dispatch_rust.go      (rust_contract build tag)
  rustbridge/
    bridge.go           (cgo binding)
  echo_bench_test.go    (Go benchmarks)
internal/cli/
  echo.go               (added --echo-engine flag)
```

### Fixtures + tooling
```
tests/echo-fixtures/{small,medium,large}_pack.md
tests/parity/echo-goldens/{small,medium,large}_pack/evaluate.json
cmd/echo-golden-export/main.go
cmd/echo-engine-diff/main.go        (rust_contract build tag)
```

### Reports
```
crates/ctx-echo/PHASE4_REPORT.md    (this file)
tests/ECHO_BENCH_REPORT.md
```

(Modified: `internal/echo/echo.go` — `Run` now calls `RoutedEvaluate`;
`internal/cli/echo.go` — added `--echo-engine` flag.
`tests/MIGRATION_ROADMAP.md` — Tier 2 #3 row updated.
`tests/RELEASE_NOTES.md` — `--echo-engine` documented.)

---

## Recommendation

* **Default Go build remains pure Go** — no behaviour change for end users.
* **`-tags rust_contract` build accepts `--echo-engine rust`** but the
  rust path is documented as evidence-only.
* **Roadmap classification**: ctx-echo → "Tier 2 evidence-only,
  memory regression". Do NOT promote to production routing.
* **Possible future work**: refactor `ScoredChunk` to borrow `&Chunk`
  rather than own a clone. Estimated 30% bytes/call reduction on large.
  Not pursued in this PR because the speedup ceiling is still bounded
  by the Go baseline at sub-15 ms — a 30% memory win plus 1.5× speed
  would still not clear the memory-bucket bar on medium where Rust
  already loses by 25%.

End of Phase 4 report.
