# Changelog

All notable changes to this project will be documented in this file.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project does not yet follow strict semver — see release notes per tag.

## [Unreleased]

### Added
- Rust crate `ctx-contract` (`crates/ctx-contract/`) — pioneer port of the Go
  `internal/contract` module (969 LOC) to Rust as the Summit calibration shot
  for a broader Go→Rust migration. Supplies byte-exact JSON parity verified
  across 40 golden fixtures (4 packs x 10 functions). Surface mirrors the Go
  package: `Build`, `ExtractReferences`, `ParseFromPack`, `StripContractBlock`,
  `Verify`, `Render`, `Embed{Markdown,XML,Plain,JSON}`. Build via
  `cargo build --manifest-path crates/ctx-contract/Cargo.toml`. Test via
  `cargo test --manifest-path crates/ctx-contract/Cargo.toml --features testing`.
- `cmd/contract-golden-export` — Go CLI emitting deterministic golden JSON
  for the cross-implementation parity harness. Uses a frozen clock seam so
  output bytes are reproducible across runs.
- `crates/ctx-contract-probe` + `ci/cross-compile-probe.sh` — cross-compile
  smoke probe sweeping `{darwin,linux} x {amd64,arm64}` to confirm the Rust
  toolchain can ship the pioneer crate on the same target matrix as the Go
  binary. Targets without an installed toolchain are logged as skips, not
  failures.
- Parity golden corpus under `tests/parity/goldens/` — 40 JSON fixtures
  (10 functions x 4 packs: `sample_pack`, `empty_pack`, `json_pack`,
  `multi_lang_pack`) used by the Rust parity harness.
- Pack fixtures under `internal/contract/testdata/` — `empty_pack.md`,
  `json_pack.json`, `multi_lang_pack.md` — feeding both Go-side golden export
  and Rust-side parity tests.

### Changed
- `internal/contract`: added `SetNowFunc(fn func() time.Time) func() time.Time`
  clock seam (+30 LoC, non-breaking, opt-in). Production code paths continue
  to use `time.Now`; the seam exists solely so the golden exporter and tests
  can stamp `Contract.Created` deterministically. Calling cost is one
  function-pointer indirection per `Build` invocation.

### Notes
- The Rust crate is built, tested, and parity-verified but is **not yet wired
  into the `ctx` CLI** — the Go implementation remains the production code
  path. FFI shim + binary integration are deferred to a follow-up Summit pass.
  See `tests/RELEASE_NOTES.md` for the full deferred-scope list and rollback
  plan.
