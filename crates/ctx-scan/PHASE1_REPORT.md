# ctx-scan — Phase 1 Port Report

Branch: `phase1/scan-rust-port`
Goal: port `internal/scan` (REGEX_HEAVY, 218 LOC + 76 LOC test, 2 internal
deps used) to Rust as the calibration confirmation per the Phase 1 plan
in `tests/MIGRATION_ROADMAP.md`.
Status: **RESOLVED, ready to merge.**

## Modules ported

| Go source                              | Rust target                          | Notes |
|----------------------------------------|--------------------------------------|-------|
| `internal/scan/entropy.go`             | `crates/ctx-scan/src/entropy.rs`     | Shannon entropy over runes. |
| `internal/scan/env_patterns.go`        | `crates/ctx-scan/src/patterns.rs`    | env_assignment regex appended to the secret-pattern table. |
| `internal/scan/secret.go` (patterns)   | `crates/ctx-scan/src/patterns.rs`    | All 14 patterns from the Go table, in the same order so first-match-wins matches Go. |
| `internal/scan/secret.go` (scan logic) | `crates/ctx-scan/src/scan.rs`        | `scan_file{,_with_options}`, `scan_files{,_with_options}`, allowlist helpers, preview. |
| (FFI surface)                          | `crates/ctx-scan/src/ffi.rs`         | 5 extern "C" entry points. |

### Minimal Rust mirrors of internal deps (per Phase 1 charter)

- `internal/model.Warning` → `crates/ctx-scan/src/types.rs::Warning` (snake_case JSON).
- `scan.Options` → `crates/ctx-scan/src/types.rs::Options` (Deserialize-able from `opts_json`).
- `config.SecurityConfig` was NOT ported — `OptionsFromConfig` is a
  pure Go-side adapter that doesn't cross the FFI boundary, so the
  Rust side never sees `config.Config`. The dispatcher's
  `ScanFileDispatched` keeps `OptionsFromConfig` as the public Go API.

## FFI surface

Five extern "C" entry points mirror the pioneer's pattern:

```c
int32_t ctx_scan_file(const uint8_t *path_ptr, uintptr_t path_len,
                      const uint8_t *opts_json_ptr, uintptr_t opts_json_len,
                      char **out_result_ptr);

int32_t ctx_scan_text(const uint8_t *text_ptr, uintptr_t text_len,
                      const uint8_t *virtual_path_ptr, uintptr_t virtual_path_len,
                      const uint8_t *opts_json_ptr, uintptr_t opts_json_len,
                      char **out_result_ptr);

int32_t ctx_scan_files(const uint8_t *paths_json_ptr, uintptr_t paths_json_len,
                       const uint8_t *opts_json_ptr, uintptr_t opts_json_len,
                       char **out_result_ptr);

void           ctx_scan_free_string(char *s);
const char *   ctx_scan_version(void);
```

Error codes:
- `0` OK
- `-1` null pointer in required argument
- `-2` input length exceeds `MAX_INPUT_BYTES` (100 MiB)
- `-3` input bytes were not valid UTF-8 / JSON
- `-4` internal serialization failure
- `-5` IO error opening the path
- `-99` Rust panic caught by `catch_unwind`

## Build tag decision

We **reuse the contract crate's build tag `rust_contract`** rather
than introducing a new `rust_scan` tag.

Rationale:

1. **Single CGO matrix.** The cross-compile probe in `ci/` is keyed to
   `rust_contract`; splitting tags would mean two probes per platform
   for two crates, doubling matrix cost in CI.
2. **One opt-in flag for operators.** A user that has decided "give me
   the fast Rust path" expects one switch, not one switch per module.
3. **Reviewer cognitive load.** Phase 1's mission is calibration —
   adding an orthogonal tag would dilute the comparison to the pioneer.

The per-crate engine selectors (`contract.SetEngine`, `scan.SetEngine`)
remain independent, so a binary built with `rust_contract` can still
mix-and-match: `--engine=rust` on `ctx contract verify` and
`--scan-engine=go` on `ctx pack` is valid (and vice-versa).

Trade-off accepted: pulling in `crates/ctx-scan` requires `crates/
ctx-contract` to also be built, even if only the scan path is used.
On a 10-core M4 the two crates link in ~10 s release; the contract
artifact is already required by the pioneer's tests so this is a
no-op for any developer who has run the pioneer suite.

## Test counts

| Suite                                       | Count |
|---------------------------------------------|-------|
| ctx-scan unit (lib)                         | 21    |
| ctx-scan parity (`--features testing`)      | 4     |
| ctx-scan regression                         | 7     |
| **ctx-scan total**                          | **32** |
| ctx-contract unit (still passing, no change)| 31    |
| ctx-contract parity (still passing)         | 40    |
| ctx-contract regression (still passing)     | 7     |
| **ctx-contract total (unchanged)**          | **78** |

The 4 parity fixtures (`api_keys`, `clean_code`, `env_config`,
`high_entropy`) cover every regex kind in the secret-pattern table
plus the entropy branch. All four pass byte-exact against
Go-generated goldens on the first port attempt — see "Lessons" below.

The 7 regression tests pin the edge cases the pioneer Phase 4 review
proved to be expensive when discovered late:

- R-01: empty file → no warnings, no error
- R-02: multi-secret line → first-match-wins (mirrors Go `break`)
- R-03: very long line (>2 MiB) → no panic, no truncation
- R-04: embedded NUL byte → does not terminate scanning early
- R-05: Unicode line with secret → preview stays valid UTF-8
- R-06: allowlist_files glob short-circuits before touching disk
- R-07: `scan_files` skips missing paths (Go's `continue` policy)

## Build matrix

| Command                                                           | Result |
|-------------------------------------------------------------------|--------|
| `cargo check --manifest-path crates/ctx-scan/Cargo.toml`          | OK     |
| `cargo build --release --manifest-path crates/ctx-scan/Cargo.toml`| OK (libctx_scan.{a,dylib,rlib} + ctx_scan.h) |
| `cargo test --manifest-path crates/ctx-scan/Cargo.toml --features testing` | 32/32 pass |
| `cargo test --manifest-path crates/ctx-contract/Cargo.toml --features testing` | 78/78 pass (no regression) |
| `go build ./...` (pure Go, no Rust toolchain)                     | OK     |
| `CGO_ENABLED=1 go build -tags rust_contract ./...`                | OK     |
| `go test ./internal/scan/... ./internal/contract/... ./internal/pack/...` | All pass |

## End-to-end parity

Built a CGO `ctx` binary with `-tags rust_contract` and verified the
same fixture run through both engines produces byte-identical
JSON output:

```
for fx in clean_code.go api_keys.txt high_entropy.txt env_config.txt; do
  scan-e2e go   tests/scan-fixtures/$fx > /tmp/go.json
  scan-e2e rust tests/scan-fixtures/$fx > /tmp/rust.json
  diff /tmp/go.json /tmp/rust.json   # empty
done
# OK: clean_code.go / api_keys.txt / high_entropy.txt / env_config.txt
```

## Bench speedups

See `tests/SCAN_BENCH_REPORT.md` for full numbers. Summary:

| Path                  | Go ns/op   | Rust ns/op | Speedup  |
|-----------------------|------------|------------|----------|
| ScanFile/small        |    245,481 |     16,357 | **15.0×**|
| ScanFile/medium       |  2,288,151 |     94,385 | **24.2×**|
| ScanFile/large        | 23,980,172 |    891,170 | **26.9×**|
| ScanFileEntropy/medium|  2,428,297 |    231,300 | **10.5×**|

**Exceeds the Phase 1 target (≥1.5×) by an order of magnitude.**

## Lessons from the second port

### What the pioneer infra made easy

1. **Pattern table — copy/paste with regex notes.** The contract
   pioneer's `parse_refs.rs` had already documented the Go RE2 vs Rust
   `regex` semantic differences (`\s` Unicode default, `\b` Unicode
   default, the `(?-u:...)` escape hatch). I wrote the entire 15-row
   secret-pattern table in ~15 minutes and it passed parity on first
   run. Without those notes I would have hit at least the `\s` trap.

2. **FFI scaffolding — direct port.** Copying `crates/ctx-contract/
   src/ffi.rs` to `crates/ctx-scan/src/ffi.rs` and renaming
   `ctx_contract_*` → `ctx_scan_*` plus swapping in the new function
   bodies took ~10 minutes. The error-code enum, `catch_unwind`
   wrapper, and `slice_from_raw` helper are all reusable verbatim.

3. **Cargo / cbindgen / build.rs — verbatim copy.** Zero changes
   required beyond crate name and the bench section.

4. **Go-side dispatcher pattern — direct mirror.** The
   `dispatch.go` / `dispatch_rust.go` build-tag pair was a 30-minute
   copy with `contract.` → `scan.` and signature changes for
   `model.Warning`.

5. **Bench harness pattern — verbatim copy.** The fixture-root
   walk-up helper, `testing.B` shape, and criterion `bench_with_input`
   pattern transferred without changes.

### What was hard despite the pioneer

1. **cgo string-vs-bytes lifetime trap.** My first cut of
   `internal/scan/rustbridge/bridge.go` did:
   ```go
   func stringToCPtr(s string) (*C.uint8_t, C.uintptr_t) {
       b := []byte(s)
       return (*C.uint8_t)(unsafe.Pointer(&b[0])), C.uintptr_t(len(b))
   }
   ```
   The pointer dangled because `b` could be GC'd before the cgo call
   returned. The Rust side surfaced this as `ERR_BAD_JSON` (-3) on
   `decode_utf8`. The pioneer's bridge.go takes `[]byte` directly so
   it never hit this; scan's API takes `string` paths so I had to add
   `runtime.KeepAlive` on caller-held byte slices. **Phase 2 mitigation:**
   the architect skill should pre-write the convention that bridge
   functions accept `[]byte` not `string` whenever the underlying FFI
   takes (ptr, len) — saves 10 minutes of GC debugging per crate.

2. **`Lazy<Regex>` cannot live in `static` slices.** Rust's E0492
   forbids `&[T]` static initialisers where T contains interior
   mutability (which `Lazy<Regex>` does). The contract pioneer
   side-stepped this by holding individual `Lazy<Regex>` constants
   rather than a table; scan's pattern table is semantically a slice,
   so I had to switch to `Lazy<Vec<SecretPattern>>` with a
   `secret_patterns()` accessor. Cost: 20 minutes the first time;
   trivial once you know.

3. **Parity for the entropy branch.** Go's `EnableEntropy=true`
   emits warnings AFTER the regex warning on the same line (the Go
   loop does both checks per line in order). My initial Rust port
   correctly mirrored that, but to make the parity goldens cover the
   entropy path I had to remember to enable it in the Go-side
   exporter. **Mistake corrected before merge:** my first exporter
   draft had `Options{}` (entropy off) and the parity tests would
   have silently missed the entropy branch.

### Effort comparison

| Phase           | Effort budget    | Actual effort    |
|-----------------|------------------|------------------|
| Pioneer (T-01..T-27, contract) | 5-6 weeks (per Summit logs) | 5-6 weeks |
| Phase 1 (scan, this port)      | 7-12 days (roadmap estimate) | **~1 day** |

The 5-7× efficiency gain over the original estimate is almost entirely
attributable to copy-paste-able infrastructure (Cargo.toml, build.rs,
cbindgen.toml, FFI scaffold, dispatcher pattern, bench scaffolding).
**Phase 2 (`relations`) is likely to be similar: 1-2 days if the
module size is comparable.**

### Phase 2 (`relations`) implications

Re-estimate for Phase 2:

| Roadmap estimate | Updated estimate (after Phase 1 evidence) |
|------------------|-------------------------------------------|
| 7-10 days        | 2-4 days                                  |

Caveats:
- Relations is bigger than scan (likely ~500 LOC vs 218). Triple
  effort for triple LOC: ~3 days.
- The `runtime.KeepAlive` cgo trap is now documented and avoidable.
- If relations has stateful regex compilation (e.g. caches indexed
  by repo root), the dispatcher pattern needs a stronger lifetime
  story than scan's stateless approach. Budget 1 day for that.

### Speedup re-prediction for Phase 2

Phase 1 scan delivered **15-27× intrinsic** (vs the 7-9× roadmap
prediction for REGEX_HEAVY). The hypothesis is that scan runs 15
regexes per line (with first-match-wins break), so the Rust regex
crate's DFA-batching amortises across more compiled state than
contract's single-regex `ExtractReferences`. **If `relations` is also
multi-regex per line, expect 15-25×. If it's single-regex per line,
expect 7-9× as originally predicted.**

## What did NOT change (per Phase 1 charter)

- `internal/contract/*` — untouched.
- `crates/ctx-contract/*` — untouched.
- Default `go build ./...` behaviour — pure Go, no Rust required,
  same binaries, same default code path.
- `ctx contract verify --engine=*` — unchanged surface; verified by
  running the contract crate's full test suite (78/78 pass).

## Proposals for Phase 2 (relations)

1. **Skip the `stringToCPtr` mistake** — make `internal/relations/
   rustbridge` accept `[]byte` everywhere; document the cgo-lifetime
   rationale once in a package-level comment.
2. **Cap regex compilation up-front** — Phase 1 used `once_cell::sync::
   Lazy<Vec<SecretPattern>>` which compiles all 15 regexes on first
   call. The first call is therefore ~3 ms slower than steady-state.
   Phase 2 should either (a) pre-warm in `Builder::new()` or (b)
   document the warmup cost in BENCH_REPORT.
3. **Add `dhat-rs` instrumentation** — Phase 1's memory claim is
   unverified. Phase 2 should land a dhat-instrumented bench so we
   can finally close the roadmap's "≥30% memory" alt-target as either
   PROVEN or DROPPED.
4. **Promote the cross-compile probe to a CI workflow before Phase 2
   merges** — the pioneer's probe is one-shot. With two crates in
   tree, a Phase 2 PR that breaks linux-musl will not be caught
   without explicit CI coverage.

## Files produced / modified

### New (Phase 1 deliverables)

- `crates/ctx-scan/Cargo.toml` + `Cargo.lock`
- `crates/ctx-scan/build.rs`
- `crates/ctx-scan/cbindgen.toml`
- `crates/ctx-scan/src/lib.rs`
- `crates/ctx-scan/src/types.rs`
- `crates/ctx-scan/src/entropy.rs`
- `crates/ctx-scan/src/patterns.rs`
- `crates/ctx-scan/src/scan.rs`
- `crates/ctx-scan/src/ffi.rs`
- `crates/ctx-scan/src/testing/mod.rs`
- `crates/ctx-scan/src/testing/parity_fixture_builder.rs`
- `crates/ctx-scan/include/ctx_scan.h` (cbindgen-generated)
- `crates/ctx-scan/tests/parity.rs`
- `crates/ctx-scan/tests/regression.rs`
- `crates/ctx-scan/benches/scan.rs`
- `crates/ctx-scan/PHASE1_REPORT.md` (this file)
- `internal/scan/rustbridge/bridge.go`
- `internal/scan/dispatch.go`
- `internal/scan/dispatch_rust.go`
- `internal/scan/scan_bench_test.go`
- `cmd/scan-golden-export/main.go`
- `tests/scan-fixtures/clean_code.go`
- `tests/scan-fixtures/api_keys.txt`
- `tests/scan-fixtures/high_entropy.txt`
- `tests/scan-fixtures/env_config.txt`
- `tests/parity/scan-goldens/{clean_code,api_keys,high_entropy,env_config}/scan.json`
- `tests/bench-inputs/scan-gen/main.go`
- `tests/bench-inputs/scan_{small,medium,large}.txt`
- `tests/SCAN_BENCH_REPORT.md`

### Modified

- `internal/cli/pack.go` — added `--scan-engine` flag and
  `scan.SetEngine` call in `RunE`.
- `internal/pack/pack.go` — `ScanFilesWithOptions` →
  `ScanFilesDispatched`.
- `internal/pack/redact.go` — `ScanFileWithOptions` →
  `ScanFileDispatched`.
- `tests/MIGRATION_ROADMAP.md` — Phase 1 status update.
- `tests/RELEASE_NOTES.md` — opt-in scan path documented.
