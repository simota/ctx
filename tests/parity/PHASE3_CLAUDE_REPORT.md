# Phase 3 — Claude Branch Report

Branch: `summit/contract-rust-port`
Scope: Go-side parity infrastructure (T-24), cross-compile smoke probe (T-25b), and the Rust-side parity fixture builder helper (T-01b).

## Summary

| Task | Status | Notes |
| ---- | ------ | ----- |
| T-24 Go golden exporter | DELIVERED | 4 fixtures × 10 funcs = 40 deterministic JSON goldens. |
| T-25b Cross-compile smoke probe | DELIVERED | host triple builds; other targets skip cleanly when rustup target is absent. |
| T-01b parity_fixture_builder (Rust) | DELIVERED (ahead of Codex) | Lives at `crates/ctx-contract/src/testing/parity_fixture_builder.rs`, gated behind `#[cfg(any(test, feature = "testing"))]`. 3/3 unit tests pass. |
| T-25 criterion bench | DEFERRED | Out of scope per the prompt's "skip unless time" clause. |

## Files Created / Modified

### New files (Phase 3)

- `cmd/contract-golden-export/main.go` — Go golden exporter (CLI).
- `internal/contract/testdata/empty_pack.md` — synthetic fixture, zero-file pack.
- `internal/contract/testdata/multi_lang_pack.md` — synthetic fixture, 3-file mixed-language pack.
- `internal/contract/testdata/json_pack.json` — synthetic fixture, JSON-pack carrying `contract` field.
- `tests/parity/goldens/<fixture>/<func>.json` — 40 deterministic golden outputs.
- `crates/ctx-contract-probe/Cargo.toml` — minimal staticlib probe crate.
- `crates/ctx-contract-probe/src/lib.rs` — single `#[no_mangle] extern "C" fn ctx_contract_probe() -> i32 { 42 }`.
- `ci/cross-compile-probe.sh` — cross-target driver (`chmod +x`).
- `crates/ctx-contract/Cargo.toml` — Phase 3 stub manifest with `testing` feature.
- `crates/ctx-contract/src/lib.rs` — Phase 3 stub crate root declaring `pub mod testing;`.
- `crates/ctx-contract/src/testing/mod.rs` — module rollup.
- `crates/ctx-contract/src/testing/parity_fixture_builder.rs` — T-01b helper (FrozenClock, Instant, serialize_with_always_emit, fixture_dir, goldens_dir).
- `tests/parity/PHASE3_CLAUDE_REPORT.md` — this report.

### Modified files

- `internal/contract/build.go` — clock seam (see below).

## T-24 — Go golden exporter

### Fixture count
4 fixtures under `internal/contract/testdata/`:
1. `sample_pack.md` (existing) — single-language Go pack with two files.
2. `empty_pack.md` (new) — empty contract array, edge case.
3. `multi_lang_pack.md` (new) — TS + Python + Markdown, exercises symbol resolution across languages.
4. `json_pack.json` (new) — JSON pack with the contract as a top-level `contract` field.

### Go-side clock injection method
Added `var nowFn = time.Now` plus `func SetNowFunc(fn func() time.Time) func() time.Time` to `internal/contract/build.go`. `Build` now reads `nowFn()` instead of `time.Now()`. This is the **least invasive** seam — no build tags, no test-only symbols, no struct rewrite, ~+30 LoC.

The exporter calls `contract.SetNowFunc(func() time.Time { return frozen })` once at startup and never restores. Determinism verified by running the exporter twice and comparing `shasum` of all golden files — identical hash (`470823e0…`).

Diff:
```
internal/contract/build.go | 31 ++++++++++++++++++++++++++++++-
1 file changed, 30 insertions(+), 1 deletion(-)
```
The only behavioural change to `Build()` is the call site `time.Now()` → `nowFn()`. Production callers see no difference since `nowFn` defaults to `time.Now`.

### Run command (as specified)
```
go run ./cmd/contract-golden-export ./internal/contract/testdata ./tests/parity/goldens
```

### Determinism evidence
```
$ shasum tests/parity/goldens/*/*.json | shasum
470823e02f9c2bd6f27ac3aa6eeac406ba84dc73  -
$ go run ./cmd/contract-golden-export ./internal/contract/testdata ./tests/parity/goldens
$ shasum tests/parity/goldens/*/*.json | shasum
470823e02f9c2bd6f27ac3aa6eeac406ba84dc73  -
```

### Notes for Phase 4
- All 10 public functions of `internal/contract/` are exercised per fixture; the file naming `tests/parity/goldens/<fixture>/<func>.json` is stable.
- The exporter normalises nil slices to `[]` and sorts map keys (via Go's `encoding/json`) so the Rust parity test can use a direct byte-compare or a JSON-aware compare without further normalisation.
- `EmbedXML` is exercised independently of `EmbedMarkdown` even though it currently aliases — the goldens will catch any future drift.

## T-25b — Cross-compile smoke probe

### Targets attempted (local Darwin arm64 host)
| Target | Triple | Result | Reason |
| ------ | ------ | ------ | ------ |
| darwin-amd64 | x86_64-apple-darwin | skip | not-installed (rustup) |
| darwin-arm64 | aarch64-apple-darwin | **ok** | host triple, compiled clean |
| linux-amd64 | x86_64-unknown-linux-gnu | skip | not-installed (rustup) |
| linux-arm64 | aarch64-unknown-linux-gnu | skip | not-installed (rustup) |

Results recorded to `/tmp/cross-compile-probe-results.txt` per spec. Script exits 0 by design so a partially-installed dev host can still run it; CI should grep `^fail` on the results file to enforce.

### Design choices
- Dropped initial `#![no_std]` — first run failed with "no panic_handler". The probe is meant to mirror the real crate's std-linking shape, so falling back to std is more honest.
- `crate-type = ["staticlib"]` matches the eventual cgo link path. cdylib was not added (would require per-target system libs we don't want to chase here).
- Script is idempotent — truncates `/tmp/cross-compile-probe-results.txt` at start; safe to run multiple times.

## T-01b — parity_fixture_builder (Rust)

### Status
**Delivered** ahead of Codex. The Codex branch's `crates/ctx-contract/` did not exist when this phase began, so the Claude branch created a Phase 3 stub for `Cargo.toml`, `src/lib.rs`, and `src/testing/mod.rs`. The merge plan when Codex's branch lands:

1. Replace `crates/ctx-contract/Cargo.toml` with Codex's full manifest, but **preserve** the `testing` feature flag.
2. Replace `crates/ctx-contract/src/lib.rs` with Codex's crate root, but **preserve** `pub mod testing;`.
3. Leave `src/testing/mod.rs` and `src/testing/parity_fixture_builder.rs` untouched.

The file carries a `// PARITY-FIXTURE-BUILDER (T-01b)` top-line annotation as specified.

### Exported API (matches Phase 2 spec)
- `FROZEN_INSTANT: &str = "2026-05-29T00:00:00Z"`
- `trait FrozenClock { fn now_rfc3339(&self) -> &'static str; }`
- `struct Instant(...)` with `const fn frozen()` and `const fn at(rfc3339)`
- `fn serialize_with_always_emit<T: Serialize>(value: &T) -> Vec<u8>` — normalises six parity-critical collection fields (`files`, `violations`, `ok`, `stale_files`, `repack_suggestions`, `symbols`) to `[]` even when serde would emit `null`.
- `fn fixture_dir() -> PathBuf` and `fn goldens_dir() -> PathBuf` — resolve via `CARGO_MANIFEST_DIR`.

### Test evidence
```
running 3 tests
test testing::parity_fixture_builder::tests::frozen_clock_returns_canonical_instant ... ok
test testing::parity_fixture_builder::tests::fixture_dir_points_at_go_side_testdata ... ok
test testing::parity_fixture_builder::tests::always_emit_replaces_null_collections ... ok

test result: ok. 3 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Compile verified locally with `cargo build` and `cargo test --lib` from `crates/ctx-contract/`.

## Open items for Phase 4

1. Codex-side merge of `crates/ctx-contract/Cargo.toml` and `src/lib.rs` (Phase 3 stubs need to be reconciled with Codex's real port).
2. Rust-side parity test that loads `tests/parity/goldens/<fixture>/<func>.json` and asserts byte-equality against the Rust impl's output — not implemented in Phase 3.
3. `ci/cross-compile-probe.sh` to be wired into a GitHub Actions matrix where all four targets are pre-installed (Phase 4 task).
4. `T-25` criterion bench harness — deferred per prompt; pick up in Phase 4 if budget allows.

## Constraints honored

- `internal/contract/*.go` modifications limited to the documented clock seam (+30 LoC, no behavioural change to default callers).
- No `cargo test` / `go test` invocations to "validate the system" — only smoke checks (`go build`, `cargo build`, `cargo test --lib` on the isolated parity helper) needed to confirm the deliverables compile.
- `crates/ctx-contract/` files touched are limited to the Phase 3 stubs needed to reach `src/testing/`; all real port content remains Codex-owned.
- Test-only Rust code is `#[cfg(any(test, feature = "testing"))]`-gated.
