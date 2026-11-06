# Changelog

All notable changes to this project will be documented in this file.

The format is loosely based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project does not yet follow strict semver — see release notes per tag.

## [Unreleased]

### Added
- Line-range excerpts: a selection entry `path:START-END`, `path:N` or
  `path#LSTART-LEND` packs only those lines — on `ctx pack --from-stdin`,
  in mixes (`ctx mix save` / `add`, `ctx pack --from-mix`), in MCP
  `ctx_pack` `files` and in `ctx browse` `POST /api/pack`. This answers the
  request an `--overflow signatures` digest invites ("show me
  `src/store.rs:120-180`") without paying for the whole file again. Ranges
  of one file merge into one entry with a `[ctx: ...]` header per segment;
  a whole-file entry wins; listings show `lines 120-180 of 412` (JSON:
  `lines`), and ranges past the end of the file are recorded as omitted.
  The digest header and the MCP next-step hint now say how to ask.
- Pack contracts record partially packed files — excerpts and
  signatures-only digests — as the lines actually shown, under their line
  numbers in the full file (`"partial": true`). `ctx contract verify`
  reports a citation of a line the pack never showed as `out-of-context`
  (`referenced lines were not in the pack`), and `--check-worktree` calls
  such a file stale only when a shown line changed.
- `ctx pack --base <REV>` (MCP `ctx_pack` `base`): a branch-review pack of
  every file the current branch changes relative to REV — changed since the
  merge base in commits, staged, unstaged or untracked — where `--changed`
  only saw uncommitted work. Implies `--changed`; combines with `--goal`,
  `--companions`, `--include`/`--exclude` and `--overflow`. Each file's
  reason carries its `+/-` line counts and whether the change is committed
  or not; files deleted on the branch are listed as omitted
  (`deleted vs <REV>`). REV is any revision git resolves (`origin/main`,
  `HEAD~3`, a hash); unknown or option-like values are errors.
- `ctx browse` Git Log → Review: "Pin N changed files" pins the range's
  changed files (deleted ones excluded), so a branch diff can be packed
  from the pinned board.
- `ctx pack` records what it left out. Files dropped for the budget, listed
  files that could not be read and files removed by `--include`/`--exclude`
  are listed with their reason in a Markdown `## Omitted files` section
  (plain: `-` lines; JSON: the `skipped` array, which was always empty;
  `--snapshot` manifests: `skipped`), with the `--budget` that would fit
  every budget-dropped file. Files below the relevance cut-off are counted.
  Output is unchanged when nothing was left out.
- `ctx pack --include <GLOB>` / `--exclude <GLOB>` (repeatable,
  comma-separated; `*`, `**`, `?`, `{a,b}`; a pattern without `/` matches a
  name at any depth, a directory pattern its subtree) scope a pack to part
  of the repository without a `.ctxignore` edit or a mix. MCP `ctx_pack`
  takes the same `include` / `exclude` arrays.
- `ctx pack --overflow signatures` (MCP `ctx_pack` `overflow`): a relevant
  file that does not fit the remaining budget is packed as its declaration
  lines with their line numbers, marked "signatures only", instead of being
  dropped — when the digest fits. Works for every language the symbol
  extractor knows (the existing `--api-only` covers Go/JS/TS/Python only).
- `ctx mix` (`save` / `list` / `show` / `add` / `drop` / `set` / `rm`):
  create and edit saved file selections ("mixes") — an ordered file list
  plus goal and budget. `ctx pack --from-mix` could read recipes, but
  nothing in the Rust build could create one. `save` and `add` accept
  files and directories (expanded like a pack walk), `ctx where --json`
  output (`--from-where`) or a path list (`--from-stdin`); paths outside
  the root or missing are rejected. Recipes live in the shared
  `ctx-mix` store, also used by `ctx browse` and MCP.
- `ctx pack --from-mix` accepts a recipe name as well as its id and warns
  about listed files that are missing from the tree.
- `ctx browse` pinned board: build a context pack from the pinned files in
  board order (goal, budget, Markdown/plain; copy or download, with a
  preview and the list of skipped files), save the pins as a mix, and pack,
  load into pins or delete saved mixes. Search results gain
  "Pin all N results", so search → pin → pack works without the CLI.
- `ctx browse` API: `POST /api/pack` (pack an explicit file list or a saved
  mix), `POST /api/mix` (create; `skip_missing` drops files absent from the
  served root) and `DELETE /api/mix/<id>` — the mix mutations that used to
  return `405`.
- MCP `ctx_pack`: `files` packs exactly the given files in order (e.g. the
  top `ctx_where` hits) and `mix` packs a saved recipe with its goal and
  budget; files that are missing, outside the root or over budget are
  reported as skipped.

### Fixed
- `ctx pack --overflow signatures`: the contract hashed the digest text as
  lines 1..N, so verifying a citation of a real declaration line compared
  it against the wrong line and reported stale content. It now records the
  declaration lines under their own numbers.
- `ctx pack | head`, `ctx where … | head` and every other one-shot command
  panicked with "failed printing to stdout: Broken pipe" and a backtrace
  when the reader closed the pipe early. They now end quietly like other
  Unix filters (`browse`, `mcp` and `tui` are unaffected).
- `ctx pack --from-stdin` / `--from-mix` with a `--goal` (or a recipe
  goal) silently dropped every listed file that did not score against the
  goal, often producing an empty pack. Explicitly selected files are now
  always packed; the goal only orders them.

### Security
- `ctx browse`: state-changing requests (`POST` / `DELETE`) must be
  `application/json` and are refused with `403 forbidden_origin` when the
  browser marks them as cross-site (`Origin` not matching `Host`, or
  `Sec-Fetch-Site: cross-site` / `same-site`), so another web page cannot
  create or delete mixes through the local server.
- Replay snapshot store (`ctx pack --snapshot`, `ctx replay`, `ctx browse`
  replay/evidence): saving a snapshot no longer writes through a fixed
  `<id>.tmp` path, so a repository that commits
  `.ctx/replay/<id>.tmp -> ~/somefile` can no longer make
  `ctx pack --snapshot <id> --shared` overwrite that file; the temporary file
  gets a fresh name and is created with `O_EXCL`. A dangling `<id>.json`
  symlink now counts as an existing snapshot. `--shared` refuses a `.ctx` or
  `.ctx/replay` that is a symlink. Listing and loading read only regular
  files, so a `*.json` FIFO or a link to `/dev/stdin` in the store no longer
  hangs `ctx replay list` / `show` / `diff` / `prune`, and a `*.json` link to
  a file outside the store is no longer read.
- `ctx pack`: a private key was redacted only on its
  `-----BEGIN … PRIVATE KEY-----` line; the key body and END line were
  packed in clear. The key-material lines after it, through the matching
  `-----END ` line, are now redacted too, one marker per line so line
  numbers are kept; code that merely mentions a header (a PEM parser, a
  test) keeps its following lines. `ENCRYPTED PRIVATE KEY`
  (PKCS#8) and `PGP PRIVATE KEY BLOCK` headers, which were never detected,
  are now caught as well.
- `ctx pack --api-only` skipped the secret scan, so an
  `export const TOKEN = "…"` line was emitted verbatim; the extracted text is
  now scanned and redacted.
- `ctx pack --from-where`, `ctx pack --diff` (both blobs and the patch),
  `ctx focus`, `ctx braid` (without `--dry-run`), the MCP `ctx_pack` /
  `ctx_focus` tools and the TUI `p` pack (`context.md`) now redact secrets
  like `ctx pack` (the CLI commands honour the ctx.toml `[security]`
  settings; MCP and the TUI use the defaults). They previously emitted file
  contents unscanned. The MCP `ctx://file/…` resource and `ctx_skim`, like
  `ctx browse` `/raw`, still return a file as stored.
- Secret scan: patterns use ASCII word boundaries as in the Go scanner, so
  a key written directly next to non-ASCII text (`キーはAKIA…です`) is no
  longer missed.
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
- MCP `ctx_digest` returned "native repository digest is not implemented"
  for every repository; it now reports commits, authors, file churn, token /
  symbol deltas and hot files (markdown, JSON or plain, with `page_size` /
  `cursor` paging of hot files). The digest logic moved to the new
  `ctx-digest` crate shared with `ctx digest`; MCP counts symbols with the
  tree-sitter extractor, so its symbol delta can differ from the CLI's.
- MCP `ctx_pack` with `changed: true` always returned an empty pack; it now
  packs the files `git status` reports as staged, unstaged or untracked.
- `ctx digest`: a renamed file is listed under its new path (as an add,
  with the old path as a delete) instead of only under the old path with
  its whole size as a negative token delta, and non-ASCII or
  quote-containing file names are reported verbatim instead of as C-quoted
  strings with a token/symbol delta of 0 (and unusable "next reads"
  anchors).
- `ctx browse` git log: picking a branch or worktree in "Show log for"
  reloaded `HEAD` over the branch request, so the list kept showing `HEAD`'s
  commits under the selected branch name; the branch's log is now shown.
- `ctx browse`: a slow response for the previously viewed item can no longer
  overwrite the current one — the working-tree diff and hex view of the
  file detail (switching files while a diff loads showed the previous
  file's diff), per-file diffs in commit detail and range review, and the
  Largest files list when "Respect .gitignore" is toggled quickly.
- `ctx browse`: an unknown `/api/*` path answers `404` with the JSON error
  envelope instead of the SPA's `index.html`, and the roots picker's
  register / remove / open actions, which this server does not implement
  yet, now explain which `ctx roots` command to run instead of failing with
  "Method Not Allowed" or "Invalid JSON".
- `ctx browse`: the embedded frontend is rebuilt from `web/src`; the shipped
  bundle predated the hash-route decoding fix, so a malformed `%` escape in
  the URL still broke the page and `%2C` in a file name split `open=`.
- `ctx log` run from a subdirectory of a repository printed `no commits`;
  it now reads the enclosing repository's history, and `--path` (or the
  positional path) is taken relative to the current directory, as with
  `git log -- <path>`.
- `ctx log` (TUI file list) and `ctx browse` `/api/git/commit-files`: a
  merge commit lists the files it changed against its first parent instead
  of nothing.
- `ctx browse` `/api/git/commit-files` and `/api/git/changed-files`: file
  names containing `"`, `\`, a tab or a control character are reported raw
  instead of C-quoted (which made them unresolvable in `commit-diff` and
  dropped their line counts), and a rename into a parent directory
  (`src/{sub => }/x`) keeps its `+/-` counts in the row and the summary.
- `ctx browse`: `/api/tests` no longer hangs when `path` or `profile` names
  a FIFO; a non-regular `path` gets `400 not_a_file` and a non-regular
  profile is treated as missing (`profile=.` no longer returns 500).
  Coverage totals saturate instead of overflowing (a panic in debug builds)
  on huge profile numbers.
- `ctx pack`: a `.ctxignore` / `[ignore]` directory pattern such as `build/`
  no longer also drops directories whose name merely ends in it
  (`src/rebuild/`).
- `ctx map`: `--width` / `--height` above 1000 are rejected instead of
  aborting on a failed canvas allocation (or, with `--format svg`,
  panicking on overflow).
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
- `ctx cochange [PATH]...` lists the files that historically change in the
  same commits as PATH (a file or directory), with how many times they moved
  together, a confidence (share of PATH's commits that also touched the
  file) and the last change date. Without PATH it ranks the most coupled
  file pairs. `--since`, `--commits` (default 500), `--min-weight`
  (default 2), `--top`, and `--format text|plain|json|paths`; `paths` feeds
  `ctx pack --from-stdin`. Commits touching more than 50 files are left out
  of the pairing as bulk edits.
- `ctx pack --companions[=N]` adds up to N (default 5) co-change partners of
  the selected files (`--changed`, `--from-where`, `--from-stdin`,
  `--from-mix`) after them, each with a `companion: changed together with …`
  reason, so a pack of a change also carries the tests, schemas and configs
  that usually change with it. Without a selection it is an error.
- MCP: new `ctx_cochange` tool (anchor partners or coupled pairs, markdown or
  JSON) and a `companions` argument for `ctx_pack` (with `changed: true`).
- `ctx digest [ROOT]` digests a repository other than the current directory;
  a subdirectory scopes the digest to the changes under it, with paths
  relative to it.
- `ctx pack|where|cochange|digest --help` list the command's options instead
  of a bare `[args]...`.
- `ctx where --repos-only` (aliases `--only-repos`, `--repos`) prints the
  repositories that hold the ranked matches instead of the matches: a nested
  repository by its root-relative path and the repository enclosing the root
  as `.`, one per line or as a JSON array with `--json`. It is derived from
  the top `--limit` results and rejects `--format vimgrep`. Matches outside
  any repository are listed by their top-level directory instead; files
  directly under such a root show as `.` only when nothing else matched.
- `ctx where --no-ignore` skips the root `.gitignore` and `.ctxignore`
  (`--no-gitignore` / `--no-ctxignore` skip one); the built-in exclusions
  (`.git/`, `node_modules/`, `dist/`, `coverage/`) still apply.
- `ctx where --no-deps` (aliases `--exclude-deps`, `--skip-deps`) also skips
  `target/`, `vendor/`, `pkg/`, `.venv/`, `venv/`, `__pycache__/`, `build/`
  and `.cargo/` directories.
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
