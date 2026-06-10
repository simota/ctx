# WALK_SCREENING

Tier 2 #8 candidate: `internal/walk` (728 LOC source: `walk.go` 553 +
`secure.go` 96 + `timefilter.go` 79; 2 internal deps = `model` + `git`;
8-10 callers across braid/relations/pack/web/mcp/tui).
Date: 2026-05-30.
Branch: `phase4/walk-screen` (cut from main).
Recipe applied: **Step 0 → Step 1 → Step 2 (post-PR #76 screen-before-port)** —
third application after `digest` (Tier 2 #6) and `config` (Tier 2 #7).

## Verdict

**SKIPPED — pprof shows 97.1% in `syscall.syscall`, well above the 75% SKIP
threshold codified after `digest` + `config`.**

The hot path is dominated by **per-file filesystem syscalls via
`countTextStats` (79.7% cum: `os.ReadFile` → `os.Open` → `syscall.Open`)**
plus **`os.Lstat` (13.3%)** plus **`os.ReadDir` (4.9%)**. The Rust-portable
slice that motivated the screen — the `gitignore.MatchesPath` regex matcher —
clocks in at **0.08% of total** (10 ms cum out of 12.36 s). Even a free
pattern-matching kernel would shave <0.1% of walk's runtime.

`ParseTimeFilter` is a 56 ns/op pure-CPU helper: ~900× below the cgo floor.
Cannot ship per L4 either.

No part of the module passes the new recipe. We do not ship.

Per-function L4 verdict:
- `ParseTimeFilter` — 56 ns/op, 2 allocs → sub-cgo-floor by ~900×. SKIP.
- `Walk` (small, 50 files) — 804 μs/op → raw passes L1 but L3 says no.
- `Walk` (medium, 500 files + gitignore) — 12.2 ms/op → raw passes L1 but
  97.1% syscall per pprof; ignore matcher is 0.08%. SKIP.
- `Walk` (large, 5000 files + 2 ignore files + time filter) — 143 ms/op → same
  shape, same verdict.

---

## Step 0 — Source-read findings

### Files

| File | LOC | Role |
|---|---:|---|
| `walk.go` | 553 | `Walker` + `New` + `Walk` + `visit` + `pruneEmptyDirs` + `Flatten` + `DefaultOptions` + `CtxignoreRuleCount` + `fileTime` + `buildCommitTimeIndex{Git,GoGit}` + `buildHeadPaths` + `countTextStats` + `inferRole` + helpers |
| `secure.go` | 96 | `SecretDenyPatterns` + `SecureDefaults` + `SecretDenyMatcher` (gitignore matcher wrapper for ~30 deny patterns) |
| `timefilter.go` | 79 | `ParseTimeFilter` — string → `time.Time` (absolute date / calendar suffix / Go duration) |
| `walk_test.go` | 426 | 12 tests for walk + ignore + time-filter |
| `ctxignore_test.go` | ~200 | .ctxignore-specific tests |
| `timefilter_test.go` | ~80 | ParseTimeFilter tests |

### Public API surface

```
walk.New(root, Options) → (*Walker, error)       # constructor; opens git repo when time-filter active
(*Walker).Walk(root) → (*model.FileInfo, error)  # tree walk
walk.Flatten(*FileInfo) → []*FileInfo            # DFS flatten
walk.DefaultOptions() / walk.SecureDefaults()    # presets
walk.CtxignoreRuleCount(root) → int              # rule counter
walk.ParseTimeFilter(s, now) → (time.Time, error)
walk.MatchesSecretDeny(relPath) → bool
walk.NewSecretDenyMatcher() → *SecretDenyMatcher
(SecretDenyMatcher).Matches(p) → bool
walk.Options{RespectGitignore, RespectCtxignore, ExtraIgnore, MaxDepth, Since, Until, UseMTime, GitRoot}
```

### Hot-path inventory (`Walk`)

```
Walker.Walk(root)
  visit(root, root, 0)                          ← recursive
    filepath.Rel + os.Lstat                     ← 1 syscall per node
    [if ignorer]    gitignore.MatchesPath × ≤2  ← regex match (Rust-portable)
    [if ctxIgnorer] gitignore.MatchesPath × ≤2  ← regex match (Rust-portable)
    [if file]
      fileTime(...)                             ← map lookup (cheap) or mtime
      countTextStats(path)                      ← os.ReadFile + utf8.Valid + bytes.Count
        ↳ os.ReadFile → syscall.Open + syscall.Read + syscall.Close
      inferRole(relSlash, isDir)                ← string ops only (cheap)
    [if dir]
      os.ReadDir(path)                          ← 1 syscall per dir
      for entry: visit(...)                     ← recurse
  [if time-filter] pruneEmptyDirs(tree)         ← in-memory tree walk
```

### Time-filter cost (separate from base walk)

When `Since`/`Until` non-zero **and** `UseMTime=false`:
- `ctxgit.OpenRepo(GitRoot)` — once
- `buildCommitTimeIndex(repo, since, until)` — once, shells out to
  `git log --all --name-only` (or falls back to go-git iteration)
- `buildHeadPaths(repo)` — once, iterates HEAD tree
- `fileTime()` per file — single map lookup

The time-filter setup cost lives in go-git/`exec.Command` territory and is
itself >75% syscall.

### Internal deps

- `internal/model` — `FileInfo` struct (data shape only, no logic to port).
- `internal/git` — `OpenRepo` thin wrapper around `go-git.PlainOpen`.

Walk depends on the go-git stack transitively (same Go-only library that
forced the `digest` skip).

### Callers (all single-shot per request)

- `internal/braid/exec.go` — one walk per braid strand.
- `internal/relations/*` — one walk per relations build.
- `internal/pack/pack.go` — one walk per `ctx pack`.
- `internal/web/*` — one walk per HTTP request (treeview, mixdown, etc.).
- `internal/mcp/server.go` — one walk per MCP tool call.
- `internal/cli/*` — one walk per CLI invocation (`browse`, `map`, etc.).
- `internal/tui/*` — one walk per TUI panel refresh.

Every caller is a **fresh one-shot tree walk per request**. No daemon-resident
walker state. No corpus shared across walks of the same root inside a single
process lifetime.

---

## Step 1 — 5-minute Go bench

Bench file: `internal/walk/walk_screen_bench_test.go` (this PR).
Command: `go test -bench=. -benchmem -benchtime=3s -run='^$' ./internal/walk/`
Host: Apple M4, darwin/arm64, go 1.25.0.

| Bench | ns/op | B/op | allocs/op |
|---|---:|---:|---:|
| `Walk_SmallTree` (50 files, no ignore) | **803,938** (804 μs) | 100,833 | 682 |
| `Walk_MediumTree` (500 files + .gitignore 10 rules) | **12,227,368** (12.2 ms) | 932,169 | 6,012 |
| `Walk_LargeTree` (5000 files + .gitignore + .ctxignore + Since mtime filter) | **143,371,835** (143 ms) | 9,028,386 | 56,442 |
| `ParseTimeFilter` (mixed cases) | **56.34** (56 ns) | 70 | 2 |

### CPU pprof — `Walk_MediumTree` (10 s benchtime, 1014 iters, 12.36 s sampled)

```
   flat  flat%   sum%        cum   cum%
     0     0%     0%     12.10s 97.90%  walk.(*Walker).Walk
     0     0%     0%     12.10s 97.90%  walk.(*Walker).visit
   12s 97.09% 97.09%        12s 97.09%  syscall.syscall
     0     0% 97.09%     11.17s 90.37%  os.ignoringEINTR
     0     0% 97.09%      9.85s 79.69%  walk.countTextStats
     0     0% 97.09%      9.85s 79.69%  os.ReadFile
     0     0% 97.09%      9.46s 76.54%  os.open / syscall.Open
     0     0% 97.09%      1.64s 13.27%  os.Lstat / syscall.Lstat
     0     0% 97.09%      0.60s  4.85%  os.ReadDir
     0     0% 97.09%      0.57s  4.61%  internal/poll.(*FD).Read / syscall.read
     0     0% 97.09%      0.24s  1.94%  os.(*File).ReadDir / syscall.fdopendir
```

### Rust-portable slice in the medium fixture

```
ROUTINE go-gitignore.(*GitIgnore).MatchesPath
         0       10ms (flat, cum) 0.081% of Total
ROUTINE go-gitignore.(*GitIgnore).MatchesPathHow
         0       10ms (flat, cum) 0.081% of Total
```

**Total gitignore matcher CPU: 0.08%. Replacing the matcher with a free
Rust kernel saves 10 ms out of 12 360 ms.**

`inferRole` (string-ops, theoretically Rust-portable) does not appear in
the top 100 nodes; its share is <0.05%.

---

## Step 2 — L1/L2/L3/L4 application

### L1 — heatmap criterion (per-call ≥ 50 μs)

| Function | per-call | L1 verdict |
|---|---:|---|
| `ParseTimeFilter` | 56 ns | **FAIL** (~900× below 50 μs floor) |
| `Walk_SmallTree` | 804 μs | PASS (raw) |
| `Walk_MediumTree` | 12.2 ms | PASS (raw) |
| `Walk_LargeTree` | 143 ms | PASS (raw) |

`Walk` raw-passes L1 but L1 alone is not the criterion — see L3.

### L2 — pack/sticky-handle: amortisation surface?

- All callers are **one-shot per CLI invocation / MCP tool call / HTTP
  request**. No long-lived `*Walker` shared across walks.
- A theoretical "cache `(root, mtime)`→FileInfo subtree" optimisation would
  need filesystem-event invalidation hooks — added complexity for a
  saving that the OS dirent cache already delivers on a warm filesystem.
- Web is the only multi-call surface, but each HTTP request must observe
  user edits made in the file tree → verify-stale-on-each-call is the
  correct semantics. Sessioning a tree walk that must reflect real-time
  filesystem state is a non-sequitur.
- The replay precedent (per-query session over a sticky corpus) does not
  apply: there is no corpus — the filesystem **is** the input.

**L2: FAIL.**

### L3 — echo "what is the actual hot operation?"

Pprof tells the story unambiguously:

| Sub-op | Share of CPU (Medium) | Rust-portable? |
|---|---:|---|
| `syscall.syscall` (open/read/close/stat/readdir) | **97.09%** | NO — kernel boundary, unchanged under Rust |
| `os.ReadFile` (in `countTextStats`) | 79.69% | NO — wrapper around `syscall.read` |
| `os.Lstat` (per node) | 13.27% | NO — `syscall.Lstat` |
| `os.ReadDir` (per dir) | 4.85% | NO — `syscall.fdopendir` |
| `gitignore.MatchesPath` regex matcher | **0.081%** | YES (regex-rs / globset) — but slice is too small |
| `inferRole` string ops | <0.05% | YES — irrelevant |
| `ParseTimeFilter` calendar parsing | n/a in Walk hot path | YES — irrelevant |

**The hot operation is non-portable filesystem I/O.** The portable
slice (`MatchesPath` + `inferRole`) is **<0.15% of total runtime**. Even
a 100× Rust replacement would shave ≤0.15% of Walk.

This is the **same shape as `config` (Tier 2 #7)** — high raw latency
dominated by the kernel boundary. The cgo floor (~50 μs per FFI round-trip)
exceeds the entire potential Rust saving (10 ms / 12 200 ms ≈ <1% Walk
speedup at infinite Rust speed-up factor).

**L3: FAIL.**

### L4 — replay per-function verdict (per-function ship?)

- `ParseTimeFilter`: **SKIP**. 56 ns per call; ~900× below the cgo floor.
  Even if every CLI/MCP/web request called it, the wall time is
  unmeasurable. Calendar-suffix parsing in Rust would shuttle in/out via
  cgo for a value that takes longer to marshal than to compute.
- `inferRole` / `isConfigFile` / `isDottedTestName`: **SKIP**. Pure
  string-prefix checks; <0.05% of Walk; sub-ns per call.
- `Walk` itself: **SKIP**. Raw latency clears L1 but L3 says no — 97%
  syscall, 0.08% portable slice.
- `countTextStats`: **SKIP**. 80% of Walk's CPU is here, but it is
  `os.ReadFile` + UTF-8 valid + line count. The `os.ReadFile` is syscall;
  the UTF-8 + line-count compute on the bytes after read is itself <1%
  of `countTextStats`. Porting only the post-read compute is sub-cgo-floor.
- `buildCommitTimeIndex{Git,GoGit}`: **SKIP**. Either shells out to `git`
  (process-fork) or walks go-git's loose-object store (syscall storm).
  Same shape as `digest`.
- `SecretDenyMatcher.Matches`: **SKIP**. Same regex matcher as gitignore;
  called rarely (MCP-only); per-call <100 ns; sub-cgo-floor.

**L4: no per-function ship candidate.**

---

## Why this skip matters (lessons)

1. **A "pattern-matching" module hot path is not necessarily a "pattern-
   matching" CPU profile.** Walk *does* run gitignore regex matches on
   every node, but the cost is dwarfed by the syscall it makes to learn
   that node exists in the first place. Pattern matching looks portable
   only on a static path-list input — once you have to discover the paths
   from disk, syscall dominates.
2. **`countTextStats` is the surprise.** The screening brief named
   gitignore matching as the primary port candidate, but pprof shows
   the file-content read (used to count lines + UTF-8 detect binaries)
   takes 80% of Walk's CPU. This per-file `os.ReadFile` is what makes
   Walk syscall-bound, not the directory walk itself.
3. **The 75% syscall-share rule from `digest` + `config` holds.** Walk's
   97.09% syscall reading is the cleanest skip case of the three. The
   rule is now applied to four modules (digest, config, walk explicitly;
   `tokens` implicitly via digest's analysis).
4. **L4 per-function still says no for ParseTimeFilter.** Even a small
   pure-CPU helper does not ship when its absolute cost is 56 ns — the
   cgo round-trip (~50 μs) is ~900× more expensive than the computation
   itself. The replay precedent (session-amortised per-query ship)
   requires the per-call cost to be material *and* the corpus to be
   sticky; ParseTimeFilter has neither.
5. **Step 0 source-read + Step 1 pprof catches this in 12 minutes.** The
   recipe is validated on three consecutive skips (`digest`, `config`,
   `walk`). Future candidates with this shape can be skipped without a
   full port template.

---

## Recommended alternatives

| Option | Worth? |
|---|---|
| Port `gitignore.MatchesPath` to Rust (`globset`/`ignore` crate) | **No** — 0.08% of Walk; even a free kernel saves <0.1%. cgo floor swamps the saving by 4-5 orders of magnitude. |
| Port `countTextStats` post-read compute (UTF-8 + line count) | **No** — the read syscall is 80% of `countTextStats`; the byte compute after read is <1%. cgo round-trip exceeds saving. |
| Cache `(absPath, mtime)` → `FileInfo` across Walk calls | **Defer** — would benefit the web caller but adds invalidation complexity (operator file edits between requests). **Pure-Go optimisation if it ever matters; not a port story.** |
| Parallelise `visit` recursion via goroutines | **Defer** — would reduce wall time on large trees by 2-4× on the M4 (10 cores), but is orthogonal to a Rust port and is a pure-Go change. Worth considering if `ctx browse` UX latency becomes a complaint. |
| Skip `countTextStats` on files matching size/extension heuristics | **Defer** — could halve `countTextStats` cost by short-circuiting on `.png`/`.jpg`/large binaries before opening. Pure-Go change; not a port story. |
| Replace `os.ReadFile` with `os.Stat`+size threshold + lazy read | **Defer** — would convert most line counts from `read+count` to `stat-only` for files where we only need the size. Pure-Go change. |
| Daemon mode (Tier 3) with in-memory file-info cache | **Defer** — would unlock real wins, but the gain comes from cache hits not from Rust translation. **Not a port story.** |
| Wire Walk into pack/replay sessions | **No** — sessioning a tree walk that must reflect real-time filesystem state is incompatible with the sticky-handle invariant (the corpus changes between calls). |

---

## Telemetry

- `internal/walk/walk_screen_bench_test.go` retained as the screening
  evidence and as a regression guard if Walk's hot-path composition ever
  changes (e.g., if a future change adds expensive pre-stat work, or if
  `countTextStats` is replaced by a lazy reader, the bench numbers shift
  and the screen should be re-run).
- No `--walk-engine` flag is introduced. The module remains 100% Go.

## Notes for next maintainer

- `TestWalkSince_NoMatches` previously failed on dates after 2026-05-22
  because fixture files used real `t.TempDir()` mtimes while the test
  compared them against a hard-coded logical `now`. Follow-up commit
  `4528c96` fixed this by assigning controlled mtimes with `os.Chtimes`.

---

## Cross-references

- `tests/MIGRATION_ROADMAP.md` — Tier 2 #8 row updated to **SCREENED-SKIPPED**.
- `tests/RELEASE_NOTES.md` — no new flag.
- Recipe origin: PR #76 (Step 0 + Step 1 + Step 2 codification).
- Sibling precedents:
  - `digest` (Tier 2 #6) — first skip via this recipe; shape was
    "medium + I/O via Go-only dep + 1 internal dep" (go-git syscall storm).
  - `config` (Tier 2 #7) — second skip; shape was "small + I/O via stdlib
    + zero deps" (TOML over filesystem syscalls).
  - `walk` (Tier 2 #8, this doc) — third skip; shape is "medium + per-file
    `os.ReadFile` over a filesystem tree + 2 internal deps". The pattern-
    matching surface (gitignore regex) looked portable but is <0.1% of CPU.
  - Together these establish the rule: **any module whose pprof shows >75%
    `syscall.syscall` is SKIP regardless of raw latency, LOC, or apparent
    pattern-matching workload.**
