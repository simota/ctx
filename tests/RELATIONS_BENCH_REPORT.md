# Phase 2 — relations Rust Port Bench Report

Branch: `phase2/relations-rust-port`
Module: `internal/relations` → `crates/ctx-relations`
Workload class: REGEX_HEAVY + IO (per `tests/NEXT_MODULES_ANALYSIS.md`)
Date: 2026-05-29

This report compares the Go and Rust implementations of the relations
port across **time**, **memory bytes**, and **dhat-rs peak heap**. Both
the criterion suite and Go testing.B harness were run on Apple M4
(10-core) under macOS 26 with rustc 1.92 and go 1.25.

---

## 1. Time speedup

### Go benchmarks (`go test -bench=BenchmarkBuild_PerFixture -benchmem`)

```
goos: darwin   goarch: arm64   cpu: Apple M4
BenchmarkBuild_PerFixture/go_project-10       7137   169167 ns/op   112555 B/op   1158 allocs/op
BenchmarkBuild_PerFixture/jsts_project-10     6316   188829 ns/op   108852 B/op   1165 allocs/op
BenchmarkBuild_PerFixture/jvm_project-10      4953   244054 ns/op   375594 B/op   1151 allocs/op
BenchmarkBuild_PerFixture/mixed_project-10    5590   212413 ns/op   116840 B/op   1175 allocs/op
```

### Rust benchmarks (`cargo bench --bench relations`)

| Fixture          | Rust time (µs) | Notes                              |
|------------------|----------------|------------------------------------|
| go_project       | 84.30          | 5 files, single-language          |
| jsts_project     | 90.95          | 6 files inc. .vue scripted source |
| jvm_project      | 135.65         | 4 files, .java + .kt              |
| mixed_project    | 107.77         | go + ts                            |

### Speedup table

| Fixture          | Go ns/op   | Rust ns/op | **Speedup** |
|------------------|------------|------------|-------------|
| go_project       | 169,167    | 84,300     | **2.01×**   |
| jsts_project     | 188,829    | 90,950     | **2.08×**   |
| jvm_project      | 244,054    | 135,650    | **1.80×**   |
| mixed_project    | 212,413    | 107,770    | **1.97×**   |
| **Average**      | —          | —          | **~1.97×**  |

### BuildCached hit path

| Path                                    | Rust time (µs) |
|-----------------------------------------|----------------|
| build_cached/rust_first/mixed_project   | 188.13         |
| build_cached/rust_hit/mixed_project     | **70.16**      |

Cache-hit is **~3× faster** than the first-pass cold build on the same
fixture, confirming the Rust cache invalidation matches the Go
semantics (size+mtime signature).

---

## 2. Memory bytes per Build()

Method: `go test -bench=BenchmarkBuild_MemAlloc -benchmem -benchtime=1x`
runs `Build(mixed_project)` 200× and reads
`runtime.MemStats.TotalAlloc` deltas. dhat-rs reports total bytes
allocated across the same 200-iteration loop in
`crates/ctx-relations/benches/memory.rs`.

| Metric              | Go (200 iters)       | Rust (200 iters)     | Reduction |
|---------------------|----------------------|----------------------|-----------|
| Total bytes allocated | 23,317,704           | 6,257,857            | **−73.2%** |
| Total allocations   | 235,000 (≈1175/iter) | 38,103 (≈190/iter)   | **−83.8%** |
| Bytes per Build     | ~116,589             | ~31,289              | **−73.2%** |
| Allocs per Build    | 1,175                | ~190                 | **−83.8%** |

Both metrics clear the Phase 2 ≥30% memory reduction target by a wide
margin.

---

## 3. dhat-rs detailed profile

dhat output from `cargo bench --features dhat --bench memory`
(/tmp/relations-dhat.json):

```
dhat: Total:     6,257,857 bytes in 38,103 blocks
dhat: At t-gmax: 389,509   bytes in    570 blocks
dhat: At t-end:    84,327  bytes in    250 blocks
```

- **At t-gmax (peak)**: 389.5 KiB live across 570 blocks. This is the
  high-water mark across all 200 Build calls — dominated by the
  per-build BTreeMap + Vec growth that drops at the end of each call.
- **At t-end**: only 84.3 KiB / 250 blocks remain (the lazy-built
  regex pattern table). All per-build state is released.

The Go side does not have an equivalent peak-heap measurement
(runtime.MemStats reports cumulative and current bytes, not high-water
within a window). The 73% reduction in total bytes allocated combined
with the bounded 389.5 KiB peak suggests the Rust implementation does
not retain transient state between calls — the regex table is the only
long-lived allocation.

---

## 4. End-to-end verification

The `cmd/relations-engine-diff` harness drives `BuildDispatched(root)`
under both engines on every parity fixture and compares the
normalised-JSON output byte-for-byte:

```
ok    engines agree on ./tests/relations-fixtures/mixed_project (297 bytes)
ok    engines agree on ./tests/relations-fixtures/jvm_project   (505 bytes)
ok    engines agree on ./tests/relations-fixtures/php_project   (229 bytes)
ok    engines agree on ./tests/relations-fixtures/jsts_project  (429 bytes)
```

All four mixed-language fixtures verify clean. The seven goldens
(`tests/parity/relations-goldens/<fixture>/{build,build_cached}.json`)
plus seven cargo-test parity entries are also clean (see Section 5).

---

## 5. Test counts

| Suite                                   | Count |
|-----------------------------------------|-------|
| ctx-relations unit (lib)                | 29    |
| ctx-relations parity (`--features testing`) | 7 |
| ctx-relations regression                | 7     |
| **ctx-relations total**                 | **43** |
| ctx-scan total (unchanged)              | 32    |
| ctx-contract total (unchanged)          | 78    |
| **All Rust crates total**               | **153** |

Go side: `go test ./internal/relations/...` reports `ok` (16 tests in
the existing relations_test.go suite plus the bench harness).

---

## 6. Verdict against Phase 2 targets

| Target                                              | Result | Status |
|-----------------------------------------------------|--------|--------|
| ≥1.5× intrinsic speedup on Build                    | 1.80–2.08× | **PASS** |
| ≥30% memory reduction                               | −73% bytes, −84% allocs | **PASS (by a wide margin)** |
| dhat-rs instrumentation lands                       | yes (`benches/memory.rs`) | **PASS** |
| Cross-compile CI promoted from probe to workflow    | yes (`.github/workflows/cross-compile.yml`) | **PASS** |
| Byte-exact parity across 7 fixtures × 2 functions  | yes (14/14 goldens green) | **PASS** |
| End-to-end Go↔Rust engine diff clean                | yes (4/4 fixtures) | **PASS** |

Phase 1's mandatory lessons all applied:

1. **cgo `[]byte` lifetime + `runtime.KeepAlive`** — applied in
   `internal/relations/rustbridge/bridge.go` for all 3 entry points.
2. **`Lazy<Vec<…>>` accessor pattern** — applied throughout
   `crates/ctx-relations/src/patterns.rs`.
3. **Goldens exercise every option branch** — Build + BuildCached
   covered per-fixture (14 goldens for 7 fixtures × 2 entry points).
4. **Cross-compile probe → production CI** — workflow added at
   `.github/workflows/cross-compile.yml`. Required-status flip is a
   reviewer step (recorded in PHASE2_REPORT.md).
5. **dhat-rs memory instrumentation** — `[features] dhat` +
   `benches/memory.rs` land in this PR with the comparison above.

---

## 7. Observations / Phase 3 input

- The relations speedup (≈2×) is in line with the contract crate's
  Verify result (1.85×) — both modules mix regex-light dispatch logic
  with heavier in-loop work. The contract `ExtractReferences` path
  was 7-9× and the scan path was 5-7×; relations falls in between
  because its Build loop is dominated by per-file IO (`fs::read_to_string`)
  and the dedup_sorted sweep across each adjacency list, not pure
  regex throughput.
- The memory win is much larger than the time win, suggesting the Go
  implementation produces a lot of short-lived map/slice churn during
  the BTreeMap → importers reversal. Phase 3's `where` port may see
  a similar split (LOOKUP_HEAVY tends to be allocation-heavy in Go).
- The dhat profile's t-end is ~84 KiB. That's the steady-state
  cost of the lazy regex table (about 12 regexes × ~7 KiB each). It
  does not grow under repeated Build calls, so a long-running web
  process holding the relations crate in memory pays a fixed,
  predictable cost regardless of how many requests it serves.

The Phase 2 wins justify continuing to Phase 3 (`where` + `replay`)
with the same opt-in build-tag pattern. See
`crates/ctx-relations/PHASE2_REPORT.md` for the deliverable summary.
