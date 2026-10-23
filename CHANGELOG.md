# Changelog

All notable changes to this project will be documented in this file.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project does not yet follow strict semver — see release notes per tag.

## [Unreleased]

### Security
- `ctx browse`: `/api/git/commit-files` no longer forwards a `hash` that
  starts with `-` to `git diff-tree`, which allowed a request such as
  `?hash=--output=<file>` to create or truncate an arbitrary file. Such
  hashes now get `400 bad_request`.
- `ctx browse`: `/api/git/commit-diff` (and `ctx-git` revision resolution in
  general) now validates ref names with `git check-ref-format` rules and
  requires ref contents to be an object id, so `to=refs/../../<file>` can no
  longer read files outside the served root and echo them in the error.
- `ctx browse`: `/raw/*` now applies the secret deny list to the resolved
  path as well as the request spelling, so `/raw/.env/`, `/raw/x/../.env` or
  an in-root symlink to `.env` / into `.ssh/` get `403 secret_deny` instead
  of serving the file.
- `ctx-mcp`: the `ctx_pack`, `ctx_budget`, `ctx_where` and `ctx_symbols`
  walkers no longer follow symlinks out of the server root and no longer
  descend into symlinked directories, so a link to an outside file cannot leak
  its contents and a link cycle cannot stall the server. Non-regular files
  (FIFOs, sockets) are skipped. Behaviour change: `ctx_where` / `ctx_symbols`
  used to descend into in-root symlinked directories (e.g. linked monorepo
  packages); their contents are now omitted, as in the CLI pack walker.
- `ctx browse`: a loopback-bound server now answers only requests whose
  `Host` is a loopback IP literal or `localhost` (any port); other hosts get
  `403 forbidden_host`. Previously a page on an attacker domain re-resolved to
  127.0.0.1 (DNS rebinding) could read the served repository through `/raw/*`
  and `/api/*`, which was practical with a fixed `--port`. Binds made with
  `--allow-nonlocal` to a non-loopback address are not filtered.

### Fixed
- `ctx browse`: `/api/file` reads at most 1 MiB + 1 byte of a file instead of
  buffering the whole file before truncating it to 256 KiB; the response is
  unchanged.
- `ctx-mcp`: a request line with invalid UTF-8 no longer terminates the stdio
  server (it gets a `-32700` reply and the session continues), and a prompt
  argument whose 1 KiB cap falls inside a multi-byte character is cut at a
  character boundary instead of panicking.

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
