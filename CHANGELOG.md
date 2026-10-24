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
  `--allow-nonlocal` to a non-loopback address are not filtered. Behaviour
  change: opening a loopback-bound server through a forwarder that sends its
  own hostname (Codespaces/Gitpod port forwarding, a reverse proxy) now gets
  403; SSH `-L` forwarding to `localhost` keeps working.
- `ctx browse`: `/api/dir` resolves the README preview through the root jail,
  so a `README.md` symlink to a file outside the served root is no longer
  returned in `readme`; in-root symlinked READMEs are still shown.
- `ctx pack` (including `--why`, `--from-stdin`, `--from-mix`,
  `--from-where`, and a single-file root such as `ctx pack README.md`) and
  `ctx braid` no longer read a symlinked file whose
  target is outside the repository (the git work tree holding the root, or
  the root itself outside git), so a repository containing
  `notes.rs -> ~/.ssh/id_rsa` (or, for `braid`, such a link committed and
  picked up by a `digest` strand) cannot leak that file into the bundle.
  `--from-where` also drops absolute and `../` paths that leave the
  repository. Symlinks to files elsewhere in the same repository are still
  packed, also when packing a subdirectory. `ctx where` and the commands
  built on its walk (`focus`, `braid`, `noise`, `onboarding`, `map`, `log`)
  drop such links the same way.

### Fixed
- `ctx where` / `noise` / `braid` / `focus` / `log` and the `ctx browse` tree: a root
  `.gitignore` or `.ctxignore` that is not valid UTF-8 (e.g. a Shift_JIS
  comment) no longer fails the walk with "stream did not contain valid
  UTF-8" (CLI) or silently drops every ignore rule (browse). As in the Go
  `go-gitignore` library, only the undecodable lines are dropped.
- `ctx pack`: a `.ctxignore` containing a non-UTF-8 line no longer discards
  all of its patterns, which let files it excludes (e.g. `secrets/`) into the
  pack; only the undecodable lines are skipped.
- `ctx browse`, `ctx tree --json`, `ctx-mcp` and every other `ctx-symbols`
  caller: symbol extraction walks the syntax tree iteratively, so a deeply
  nested (generated or minified) source file no longer overflows the stack
  and aborts the whole process. Symbol order is unchanged.
- `ctx-tui` pack (`p`): a text file with a non-UTF-8 byte after its first
  512 bytes is skipped, as in `ctx pack`, instead of failing the whole pack.
- `ctx pack --from-where`: the `null` that `ctx where --format json` prints
  when nothing matches is now the "got 0 paths" error instead of a lookup
  for (and, if present, a pack of) a file named `null`; a non-UTF-8 line in
  a newline path list now skips only that line instead of reporting 0 paths.
- `ctx contract verify`: a reference to `package.json`, `*.tsx` or `*.jsx`
  is no longer read as `package.js` / `*.ts` / `*.js` (losing any `:L10-L12`
  range), which reported in-contract files as out-of-context violations.
  This intentionally diverges from the frozen Go oracle, which shares the bug.
- `ctx contract verify` / `ctx browse` evidence: a pack whose file content
  contains the `<!-- ctx:contract v1` marker (e.g. a pack of
  `crates/ctx-contract`) now reads the appended contract block instead of
  failing with "no contract", and `strip_contract_block` removes only that
  block rather than also cutting the matching span out of the file content.
- `ctx focus`: a path anchor spelled `./internal/x.go`, `internal/./x.go` or
  `internal/y/../x.go` now resolves like `internal/x.go`, as Go's
  `filepath.Clean` did, instead of "anchor not found".
- `ctx pack <subdir>`: `--since` / `--until` no longer drop every tracked
  file, `--changed` no longer packs nothing, and `--diff` no longer lists
  changes outside the root with empty patches. Git names these paths from
  the top of the work tree; they are now re-expressed relative to the pack
  root. `--since` / `--until` also match non-ASCII file names to their
  commit time instead of silently falling back to the file's mtime.
- `ctx pack --diff` runs `git diff` with `--no-ext-diff --no-textconv
  --no-color` (as `ctx diff` does), so a `diff.external` tool, textconv
  driver or `color.ui=always` in the user's git config no longer leaks
  tool output or ANSI escapes into the bundle.
- `ctx browse <subdir>`: the file tree and `/api/file` show git status for
  files under a served subdirectory (porcelain paths were looked up by the
  wrong, work-tree-relative key), so the changed-only filter no longer hides
  every changed file.
- `ctx map` no longer panics when a heatmap cell is narrower than a
  non-ASCII directory name; labels are clipped on character boundaries.
- `ctx impact` / `ctx deps`, the `ctx browse` relations endpoint and the
  test-insights walker no longer hang on a FIFO (or socket/device) in the
  tree; only regular files are read.
- `ctx where` (and `focus`, `braid`, `noise`, `onboarding`, `map`, `log`) no
  longer descends into symlinked directories: a `loop -> .` link produced
  `loop/loop/…/file` duplicates up to the ELOOP depth, and two such links made
  the walk exponential. Behaviour change: in-root symlinked directories (e.g.
  linked monorepo packages) are now omitted, as in the `ctx pack` and MCP
  walkers.
- `ctx pack` (including `--from-where`), `ctx where` (and the commands
  above), the default tree view and `--json` no longer hang on a FIFO, socket
  or device file in the tree (or a link to one): pack/where walkers and
  `braid` strand token estimates skip them, and the tree lists them with a
  size-based token estimate instead of reading them.
- `ctx browse`: `/api/dir` and `/api/tree` no longer read through FIFOs,
  devices or (for `/api/dir`) symlinks when counting lines and tokens, and the
  README preview reads at most 64 KiB of a regular file; a `README` FIFO or a
  link to `/dev/zero` used to hang the request or grow memory without bound.
  Symlinked files in `/api/dir` totals are no longer read through: like
  `/api/tree`, they contribute an estimate from the link's own lstat size
  (the length of its target path), not the target file's tokens.
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
