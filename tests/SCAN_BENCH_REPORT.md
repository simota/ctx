# ctx-scan — Rust vs Go Benchmark Report

Phase 1 deliverable. Measures the `ScanFileWithOptions` hot path on
byte-identical synthetic inputs to confirm the Phase 1 prediction that
the REGEX_HEAVY `internal/scan` port delivers ≥1.5× throughput.

## Methodology

### Input generation

A single Go generator at `tests/bench-inputs/scan-gen/main.go` produces
every benchmark fixture from a fixed `math/rand` seed so every byte is
deterministic and reproducible:

```
go run ./tests/bench-inputs/scan-gen
```

Fixtures written to `tests/bench-inputs/`:

| File              | Size     | Secret lines | Filler words |
|-------------------|----------|--------------|--------------|
| `scan_small.txt`  |  1.7 KB  | 5            | 200          |
| `scan_medium.txt` |   17 KB  | 50           | 2,000        |
| `scan_large.txt`  |  182 KB  | 500          | 20,000       |

Each fixture interleaves filler (English words) with secret-shaped
tokens at a fixed cadence so the regex hot path is exercised across
the full body. Seven kinds of token are round-robined (AWS access key,
GCP API key, GitHub PAT, Slack bot token, JWT, env-style assignment,
and a non-matching decoy line) so every regex in the secret-pattern
table runs.

Both harnesses read the exact same files from disk; no language-specific
generation is performed at bench time.

### Verify input shape

Default options (no allowlist, entropy disabled) for the three
`ScanFile` cases. A separate `ScanFileEntropy/medium` case enables
the entropy scan to confirm the entropy branch also clears the
≥1.5× bar.

## Measurement environment

- Apple M4 (10-core), macOS 26
- rustc 1.92.0
- go 1.25.0
- Single-platform — cross-platform behaviour is unverified (Phase 1
  exit criterion D — to be addressed in Phase 1 cross-compile probe
  before Phase 2 scope expansion).

## Raw numbers

### Go (`testing.B`, `-benchtime=2s`)

```
BenchmarkScanFile/small-10           9942      245,481 ns/op   7.09 MB/s    7,674 B/op    42 allocs/op
BenchmarkScanFile/medium-10          1054    2,288,151 ns/op   7.67 MB/s   36,683 B/op   365 allocs/op
BenchmarkScanFile/large-10            100   23,980,172 ns/op   7.58 MB/s  312,908 B/op 3,525 allocs/op
BenchmarkScanFileEntropy-10           987    2,428,297 ns/op   7.23 MB/s  151,943 B/op   975 allocs/op
```

### Rust (`criterion`, default profile)

```
ScanFile/small          time:   [16.306 µs 16.357 µs 16.410 µs]   thrpt: [101.12 MiB/s 101.45 MiB/s 101.77 MiB/s]
ScanFile/medium         time:   [94.007 µs 94.385 µs 94.842 µs]   thrpt: [176.46 MiB/s 177.32 MiB/s 178.03 MiB/s]
ScanFile/large          time:   [885.07 µs 891.17 µs 900.38 µs]   thrpt: [192.65 MiB/s 194.65 MiB/s 195.99 MiB/s]
ScanFileEntropy/medium  time:   [230.65 µs 231.30 µs 232.00 µs]   thrpt: [72.139 MiB/s 72.356 MiB/s 72.562 MiB/s]
```

## Speedup table

| Hot path                | Input             | Go ns/op   | Rust ns/op | **Speedup** |
|-------------------------|-------------------|------------|------------|-------------|
| ScanFile                | small (1.7 KB)    |    245,481 |     16,357 | **15.0×**   |
| ScanFile                | medium (17 KB)    |  2,288,151 |     94,385 | **24.2×**   |
| ScanFile                | large (182 KB)    | 23,980,172 |    891,170 | **26.9×**   |
| ScanFileEntropy         | medium (17 KB)    |  2,428,297 |    231,300 | **10.5×**   |

## Throughput table (MB/s)

| Hot path                | Input             | Go MB/s | Rust MB/s | **Speedup** |
|-------------------------|-------------------|---------|-----------|-------------|
| ScanFile                | small             |    7.09 |    101.45 | **14.3×**   |
| ScanFile                | medium            |    7.67 |    177.32 | **23.1×**   |
| ScanFile                | large             |    7.58 |    194.65 | **25.7×**   |
| ScanFileEntropy         | medium            |    7.23 |     72.36 | **10.0×**   |

## Verdict

**Phase 1 success criterion (≥1.5× on at least one hot path): EXCEEDED.**

All four measured paths clear the bar by an order of magnitude.

Speedup grows with input size (15× → 27× as the file grows from 1.7 KB
to 182 KB), which is the characteristic curve of a regex-bound workload
where Rust's `regex` crate amortises startup over more matched bytes.
The entropy branch is slower per-byte in both languages because the
inner loop also runs `shannon_entropy` over candidate tokens, but the
Rust impl still delivers a 10× win.

The empirical net speedup measured here is **higher than the 7-9× the
roadmap predicted from the contract pioneer's REGEX_HEAVY paths**.
Hypothesis: the contract pioneer's `ExtractReferences` only runs one
regex; the scan workload runs 15 regexes per line (with a `break` on
first match), so the Rust regex crate's DFA-batching advantage over
Go's RE2 amortises across more compiled state. **Phase 2 (`relations`,
also REGEX_HEAVY) should be re-estimated upward — expect 15-25× on
similarly multi-pattern paths.**

## Cgo cost

The cgo bridge was NOT included in these numbers — both harnesses
exercise the Rust crate directly via Rust callers, so the speedups
above are "intrinsic" and represent the upper bound. The contract
pioneer measured cgo at ~1-2 µs per call; on the smallest (small,
16 µs intrinsic) case that's a 6-12% tax, so the **net user-visible
speedup is still ≥13×**. On medium/large inputs the cgo tax becomes
negligible. End-to-end E2E parity confirmed: the same `ctx scan`
fixture run with `--scan-engine=go` vs `--scan-engine=rust` produces
byte-identical JSON output on all 4 parity fixtures.

## Memory

Go-side allocations scale linearly with secrets found (7-300 KB per
run); Rust criterion does not enable allocator counters by default
and we did not add `dhat-rs` instrumentation in Phase 1. The roadmap's
"≥30% memory reduction" alt-target remains unclaimed but the per-op
allocation in Go (42 → 3,525 allocs scaling with input) suggests there
is substantial headroom; Phase 1 close-out should add `dhat-rs` here
before deciding whether Phase 2 keeps memory as a tracked metric.

## Reproducibility

```
# Regenerate fixtures
go run ./tests/bench-inputs/scan-gen

# Go bench
go test -bench=. -benchmem -benchtime=2s -run='^$' ./internal/scan/

# Rust bench
cargo bench --manifest-path crates/ctx-scan/Cargo.toml
```
