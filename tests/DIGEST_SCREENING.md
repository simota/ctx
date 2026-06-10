# DIGEST_SCREENING

Tier 2 #6 candidate: `internal/digest` (579 LOC source, 1 internal dep, 4 callers).
Date: 2026-05-30.
Branch: `phase4/digest-rust-port` (cut from main).
Recipe applied: **Step 0 → Step 1 → Step 2 (post-PR #76 screen-before-port)**.

## Verdict

**SKIPPED — evidence-only is not justified either; do not port.**

The hot path is dominated by **filesystem I/O via go-git plumbing (~83% of CPU
in `syscall.syscall`)**, not by anything in the Rust-portable slice. The
Rust-portable sub-operations (tiktoken BPE + sort + format) account for **<5%
of total runtime** and do not register in the top 60 pprof nodes.

Per-function L4 verdict:
- `ParseSince` — 16 ns/op, 0 allocs → trivially sub-cgo-floor. SKIP.
- `WriteMarkdown` / `WriteJSON` — 2.8 μs / 5.6 μs per op → sub-cgo-floor. SKIP.
- `Generate` — 3-83 ms per op (looks portable!) → SKIP because the portable
  slice is <5% of that time; the rest is non-portable go-git filesystem walk.

No part of the module passes the new recipe. We do not ship.

---

## Step 0 — Source-read findings

### Files

| File | LOC | Role |
|---|---|---|
| `brief.go` | 390 | `Generate` (main entry point) + delta computation + symbol counting |
| `format.go` | 116 | `WriteMarkdown` / `WriteJSON` / `WritePlain` |
| `duration.go` | 73 | `ParseSince` — string → time.Duration |
| `brief_test.go` | 176 | Fixture repo builder + 5 tests for Generate |

### Hot-path inventory (`Generate`)

```
PlainOpen(root)                                # cheap: file open
  ↓
repo.Head() / CommitObject(head)               # cheap: object fetch
  ↓
repo.Log({Since: …})                           # iter ctor
  ↓
iter.ForEach(commit):                          # ←← LOOP (N commits)
  commit.Parent(0).Patch(commit)               # ←← go-git tree diff (HEAVY)
    → DiffTreeContext → merkletrie walk        # syscall storm: open every loose object
  → patch.FilePatches()                        # patches list
  → fileMap[path] update                       # cheap map ops
  ↓
for path in sorted(fileMap):                   # ←← LOOP (F files, capped at 100)
  blobContent(headTree, path)                  # ←← go-git object fetch
  blobContent(sinceTree, path)                 # ←← go-git object fetch
  tc.CountString(content)                      # tiktoken BPE (portable to Rust)
  countSymbols(path, content)                  # tree-sitter parse (CGO via go-tree-sitter)
  ↓
sort.SliceStable(hotFiles)                     # tiny: ≤ 100 elements
```

### Internal dep

- `internal/tokens.TiktokenCounter.CountString` → pkoukk/tiktoken-go (cl100k_base).
  Shared encoder (built once). BPE is portable to Rust via `tiktoken-rs`.

### Callers (all single-shot)

| Caller | Pattern |
|---|---|
| `internal/cli/digest.go` | One Generate per `ctx digest` CLI invocation. |
| `internal/braid/exec.go` | One Generate per braid strand using digest as source. |
| `internal/mcp/server.go` | One Generate per `ctx_digest` MCP tool call. |
| (`server_hints_test.go`) | Test only. |

**No daemon / no repeat-call surface. No corpus state survives between calls.**

---

## Step 1 — 5-minute Go bench

Bench file: `internal/digest/digest_screen_bench_test.go` (this PR).
Command: `go test -bench=. -benchmem -benchtime=3s -run='^$' ./internal/digest/`
Host: Apple M4, darwin/arm64.

| Bench | ns/op | B/op | allocs/op |
|---|---|---|---|
| `ParseSince` | **15.88** | 0 | 0 |
| `Generate small` (5 files, 5 commits, 7d) | **3,032,105** (3.0 ms) | 903,760 | 8,520 |
| `Generate medium` (20 files, 30 commits, 30d) | **18,750,503** (18.7 ms) | 5,592,885 | 59,187 |
| `Generate large` (80 files, 120 commits, 90d) | **82,539,564** (82.5 ms) | 30,859,628 | 379,178 |
| `WriteMarkdown` (medium brief) | **2,849** (2.8 μs) | 784 | 71 |
| `WriteJSON` (medium brief) | **5,645** (5.6 μs) | 5,641 | 13 |

### CPU pprof — `Generate medium` (cumulative top)

```
   flat  flat%   sum%        cum   cum%
  4.25s 83.17% 83.17%      4.25s 83.17%  syscall.syscall
      0     0% 83.17%      4.16s 81.41%  go-git/storage/filesystem.(*ObjectStorage).EncodedObject
      0     0% 83.17%      4.16s 81.41%  go-git/storage/filesystem.(*ObjectStorage).getFromUnpacked
      0     0% 83.17%      3.97s 77.69%  os.ignoringEINTR
      0     0% 83.17%      3.89s 76.13%  go-billy/helper/chroot.(*ChrootHelper).Open
      0     0% 83.17%      3.87s 75.73%  go-git/storage/filesystem/dotgit.(*DotGit).Object
      0     0% 83.17%      3.73s 72.99%  go-git/plumbing/object.(*commitLimitIter).ForEach
      0     0% 83.17%      3.19s 62.43%  go-git/plumbing/object.(*Commit).Patch
      0     0% 83.17%      2.84s 55.58%  go-git/plumbing/object.(*Tree).PatchContext
      0     0% 83.17%      2.65s 51.86%  go-git/plumbing/object.GetTree
      0     0% 83.17%      2.26s 44.23%  go-git/plumbing/object.DiffTreeWithOptions
```

**tiktoken and tree-sitter do not appear in the top 60 nodes (each < 0.59% cum).**

The 30 MB allocation on the large bench is go-git's `Patch` / `Tree.Children`
internals; not user-portable work.

---

## Step 2 — L1/L2/L3/L4 application

### L1 — heatmap criterion (per-call ≥ 50 μs)

| Function | per-call | L1 verdict |
|---|---|---|
| `ParseSince` | 16 ns | **FAIL** (deep below cgo floor) |
| `WriteMarkdown` | 2.8 μs | **FAIL** |
| `WriteJSON` | 5.6 μs | **FAIL** |
| `Generate small` | 3 ms | PASS (raw) |
| `Generate medium` | 19 ms | PASS (raw) |
| `Generate large` | 83 ms | PASS (raw) |

`Generate` raw-passes L1 but L1 alone is not the criterion — see L3.

### L2 — pack/sticky-handle: amortisation surface?

- CLI / braid / MCP are all **one-shot per invocation**.
- No daemon, no repeated calls on the same (root, since) inside a single
  process lifetime.
- A theoretical "memo by (HeadSha, SinceSha)" cache could be useful if MCP
  saw repeated identical calls, but **the dominant cost is go-git's tree
  walk between two commits, not the blob-level tiktoken work** — caching
  blob-token counts would shave <5%, the syscall storm is per-commit-range
  and re-runs on every distinct period.
- The `replay` data-access-amortisation lane does not apply here because
  the data-access **is** the cost, not a sub-step we can host in Rust —
  go-git is the data-access library and is Go-only.

**L2: FAIL.**

### L3 — echo "what is the actual hot operation?"

The brief labelled digest a "report generator". The truth:

| Sub-op | Share of CPU | Rust-portable? |
|---|---|---|
| go-git loose-object filesystem reads (`syscall.syscall`) | ~83% | NO (would need rewriting git plumbing in Rust — out of scope for a port) |
| go-git Tree.PatchContext / DiffTreeWithOptions | (subset of above) | NO |
| tiktoken BPE (`tokens.CountString`) | <5% (not in top 60) | YES (tiktoken-rs) — but the slice is too small |
| tree-sitter `countSymbols` | <1% (not in top 60) | NO — already CGO, swapping Go binding for Rust binding does not change the underlying lib |
| `sort.SliceStable` + format | <0.1% | YES — irrelevant |

**The hot operation is non-portable filesystem I/O via go-git.** Porting
tiktoken alone would deliver an unmeasurable speedup. Reimplementing
go-git in Rust to ship a 5% improvement is wildly out of scope.

This is the **mirror image of `symbols`**: symbols looked CGO-bound (tree-
sitter) so we feared it would skip, but its post-walk pure-compute slice
turned out to be amortisable via lookup sessions → shipped 121-161×.
Digest looks portable (Generate is 3-83 ms!) but its raw runtime is
go-git filesystem I/O, with the Rust-portable slice being a rounding error.

**L3: FAIL.**

### L4 — replay per-function verdict

- `ParseSince`: SKIP. 16 ns. Below cgo floor by 3 orders of magnitude.
- `WriteMarkdown` / `WriteJSON` / `WritePlain`: SKIP. 2-6 μs, sub-cgo-floor,
  trivial fmt.Fprintf chains over <30 fields.
- `Generate`: SKIP. Raw latency is high but no portable sub-slice clears the
  bar with a real root cause — see L3.
- `computeDeltas` / `blobContent` / `countSymbols`: these are sub-helpers of
  `Generate` and depend on go-git Tree handles or tree-sitter — neither is
  hostable in Rust as currently scoped.

**L4: no per-function ship candidate.**

---

## Why this skip matters (lessons)

1. **High raw latency is not sufficient — composition matters.** `Generate`
   spends 19 ms per medium-corpus call, but 83% of that is in go-git's
   filesystem object walk via `syscall.syscall`. Naive ROI math ("19 ms × 4
   callers = potentially big wins") would lead us to port, then ship a 5%
   improvement and consider it a loss.
2. **Step 0 source-read + Step 1 pprof is what catches this.** Without the
   pprof we would have seen the 19 ms number and trusted the L1 heatmap.
   The pprof showed where the time actually is — and the answer was
   "not in the slice we can port".
3. **L3 echo-rule generalises**: when the dominant cost is a Go-only
   library boundary (here go-git; in echo it was String/HashMap), Rust's
   slot is too small to clear the cgo floor.
4. **L4 per-function still says no.** The `replay` precedent allowed a
   per-query ship even when the module didn't — that path doesn't open
   here because the high-latency function (Generate) is itself the one
   with the wrong root cause, not a low-latency function with a high-
   latency sub-slice.

---

## Recommended alternatives

| Option | Worth? |
|---|---|
| Port only `tiktoken` BPE → `internal/tokens` | **No** — already covered by the existing Rust tokens-bridge consideration; digest's tiktoken cost is <5% of Generate. Tokens-bridge is its own decision. |
| Cache per-blob (HeadSha, path) → tokenCount / symbolCount | **Defer** — would shave <5%. Only worth it if a daemon mode emerges that repeats Generate. |
| Replace go-git with libgit2/Rust git plumbing | **No** — month-long rewrite for a CLI/MCP feature that runs once per invocation; no user-visible regression on the current 3-83 ms numbers. |
| Daemon mode (Tier 3) | **Possible future**: if `ctx serve` ever exposes a long-running digest endpoint that the same client hits repeatedly with the same since-window, then a memo-by-HeadSha+SinceSha cache may unlock a sessioned ship. Park for Tier 3. |
| Wire digest into existing pack/replay sessions | **No** — different shape; digest's window changes per request. |

---

## Telemetry

- `internal/digest/digest_screen_bench_test.go` retained as the screening
  evidence and as a regression guard if Generate's hot-path composition
  ever changes (e.g., if go-git is swapped for go-git/v6 with packfile
  optimisations that change the cost balance).
- No `--digest-engine` flag is introduced. The module remains 100% Go.

---

## Cross-references

- `tests/MIGRATION_ROADMAP.md` — Tier 2 #6 row updated to **SCREENED-SKIPPED**.
- `tests/RELEASE_NOTES.md` — no new flag.
- Recipe origin: PR #76 (Step 0 + Step 1 + Step 2 codification).
- Sibling precedents:
  - `echo` (Tier 2 #3) — shipped evidence-only because hot path was
    String/HashMap allocation; digest is one step further (hot path is
    filesystem I/O via Go-only dep, not even portable).
  - `symbols` (Tier 2 #5) — mirror of digest. Looked unportable, was
    actually session-shippable on the lookup hot path.
