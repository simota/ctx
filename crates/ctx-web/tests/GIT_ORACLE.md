# git×3 byte-parity oracle (`git_parity.rs`)

Immutable differential HTTP parity oracle for the three git routes being ported
from Go (`internal/web`) to Rust (`crates/ctx-web`) under ADR-0005 Wave 2:

| Route | Go handler (`internal/web/handlers.go`) | Producer (`internal/git`) |
|---|---|---|
| `GET /api/git/diff?path=` | `handleGitDiff` | `WorktreeDiff` (diff.go) |
| `GET /api/git/file-log?path=&limit=` | `handleFileLog` | `FileLog` (file_log.go) |
| `GET /api/git/commit-diff?path=&from=&to=` | `handleCommitDiff` | `CommitDiff` (diff.go) |

The Go web server is the **frozen oracle**. The Rust server does NOT yet serve
`/api/git/*`, so every case is **RED by construction**: Go returns real git
JSON (`application/json`); Rust falls through its SPA catch-all
(`router.rs::spa_fallback`) and returns `index.html` (`text/html`). A later
migration loop ports each Rust handler and flips the cases GREEN. If any case
passes before the Rust route exists, the oracle is wrong.

## Why a separate test file (not added to `parity.rs`)

`parity.rs` serves the static, **non-git** `tests/fixtures` dir. The git routes
require a real git repository, so `git_parity.rs` builds its own deterministic
git repo at runtime and boots BOTH servers against THAT root. The Go/Rust
boot + minimal HTTP client + de-chunk + header-compare machinery is copied
verbatim from `parity.rs`.

Each case is its **own `#[test] fn gitparity_<case>()`** (not one monolith) so a
migration loop can count progress (N passing / 13 total) per route×case.

## Deterministic fixture (the crux)

Commit SHA-1s depend on content + author/committer identity + dates.
`GitFixture::build` (`git_parity.rs`) `git init`s a unique temp dir and creates
a fixed commit sequence with **frozen** `GIT_AUTHOR_*` / `GIT_COMMITTER_*` name,
email, and per-commit date, plus `GIT_CONFIG_GLOBAL=/dev/null` /
`GIT_CONFIG_SYSTEM=/dev/null` to neutralize ambient git config (signing,
templates, autocrlf). Identical inputs ⇒ identical hashes on every run/machine.

Frozen commit graph (oldest → newest), identity `Parity Bot <parity@example.com>`:

| short | full | subject | author date (epoch) |
|---|---|---|---|
| `e494c66` | `e494c667fa44faadf3b928887f209801b189de7a` | Add greeting | 1577934245 |
| `1e6958a` | `1e6958a716f0169dd7d56499cc36e48685adf6cd` | Modify greeting and add notes | 1580702706 |
| `5623208` | `5623208c15831d8ebd5593d5bf189425e5d18165` | Append epsilon | 1583298367 |
| `70170c7` | `70170c72aa25f170e5895629addcdf655212ef9e` | Add binary | — |
| `be1e194` | `be1e1944f755758736c033d2567e3b3e8195fc70` | Add big file | — |

After the commits, the **worktree is dirtied (uncommitted)**: `greeting.txt`
gets a worktree edit (drives the worktree-vs-HEAD diff) and `big.txt` is
rewritten so every one of its 6000 lines differs (drives the >5000-line cap).

**Stability verified**: the fixture builder run twice (independent temp dirs)
produced byte-identical hashes, and the full suite run twice produced zero
`GUARD FAILED` — the baked hashes in `expect_contains` matched the Go output on
both runs. If hashes ever drift, the `expect_contains` guard fires
`GUARD FAILED` (not `PARITY MISMATCH`), pinpointing the regression.

## Cases (route × case → guard)

Each test asserts byte-identical (status, body, content-type) between Go and
Rust AND an `expect_contains` guard on the **Go** body, so a both-empty /
both-error false PASS is impossible.

| Test | Route case | Guard asserts (real shape) |
|---|---|---|
| `gitparity_diff_worktree_modified` | (a) worktree vs HEAD | eq/del/add lines with old_num/new_num for greeting.txt |
| `gitparity_diff_binary` | (d) binary file | `"binary":true`, `"lines":[]` |
| `gitparity_diff_truncated` | (e) >5000-line cap | `"truncated":true` + first del line |
| `gitparity_diff_no_change` | identical file | `"no_change":true`, `"lines":[]` |
| `gitparity_diff_missing_path` | (f) missing param | 400 `bad_request` / "path is required" |
| `gitparity_diff_traversal` | (f) traversal | 400 `path_traversal` |
| `gitparity_file_log_multi` | (c) multi-commit log | short+full hashes, author, subjects, epoch dates |
| `gitparity_file_log_limit` | (c) limit clamp | newest commit only + `"truncated":true` |
| `gitparity_file_log_no_history` | (f) uncommitted path | `"commits":[]`, `"truncated":false` |
| `gitparity_file_log_missing_path` | (f) missing param | 400 `bad_request` |
| `gitparity_commit_diff` | (b) commit→commit | eq/del/add line sequence between e494c66 and 1e6958a |
| `gitparity_commit_diff_missing_revs` | (f) missing from/to | 400 `bad_request` / "from and to are required" |
| `gitparity_commit_diff_bad_rev` | (f) unresolvable rev | 500 `git_commit_diff` / "reference not found" |

## Normalization (`Norm`)

Mirrors `parity.rs`. All 13 git cases use **`Norm::Exact`** (byte-exact) —
none of the git route bodies embed a machine-specific absolute path:

- diff / file-log / commit-diff success bodies contain only the slash-relative
  `path`, line text, and hashes — all fixture-stable, no abs path.
- error bodies are stable too: `path_traversal` and `bad_request` messages are
  fixed strings; the `commit_diff` bad-rev 500 echoes the **revision string**
  (`deadbeef`), not a filesystem path.

`Norm::AbsPath` is retained in the harness (same semantics as `parity.rs`) for
any future git case whose error message would embed the resolved absolute path
(e.g. a future not-found variant that surfaces an OS errno + abs path). None of
the current cases need it. No raw-float fields are emitted, so no float
tolerance is required.

## Running

```sh
# Build the Go oracle once (or set CTX_GO_BIN):
go build -o target/ctx-go-oracle ./cmd/ctx
CTX_GO_BIN=$PWD/target/ctx-go-oracle \
  cargo test --manifest-path crates/ctx-web/Cargo.toml --test git_parity
```

Expected today: `0 passed; 13 failed` — every failure a `PARITY MISMATCH`
(Go JSON vs Rust SPA HTML), zero `GUARD FAILED`. The suite goes GREEN
incrementally as the Rust git handlers land.
