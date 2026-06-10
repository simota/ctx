# CONFIG_SCREENING

Tier 2 #7 candidate: `internal/config` (479 LOC source: `config.go` 221 +
`roots.go` 258, 0 internal deps, ~20 caller files across braid/web/pack/mcp/
security/cli).
Date: 2026-05-30.
Branch: `phase4/config-screen` (cut from main).
Recipe applied: **Step 0 → Step 1 → Step 2 (post-PR #76 screen-before-port)** —
second application after `digest` (Tier 2 #6).

## Verdict

**SKIPPED — small + I/O-dominated + zero deps. The canonical "do not port" shape.**

The hot path is dominated by **filesystem syscalls** — `LoadRoots` spends
**80.7% of CPU in `syscall.syscall`** (mostly `os.File.Close` in the
`toml.DecodeFile` flow), `SaveRoots` spends **97.4% in `syscall.syscall`**
(`syscall.write` for the temp file, plus rename/mkdir). The `BurntSushi/toml`
encoder/decoder is a Go-only library and the parse/marshal compute itself is
≤7-49% of total time, all of it gated by file I/O. The in-memory mutation
helpers (`AddRoot` / `RemoveRoot` / `Find` / `RootsPath`) are all sub-cgo-floor
(8.7 μs / 22 ns / 30 ns / 84 ns). No `--config-engine` flag is introduced.
The module remains 100% Go.

Per-function L4 verdict:
- `RootsPath` — 84 ns/op → sub-cgo-floor by ~150×. SKIP.
- `Find` — 30 ns/op, 1 alloc → sub-cgo-floor. SKIP.
- `RemoveRoot` — 22 ns/op, 0 allocs → sub-cgo-floor. SKIP.
- `AddRoot` — 8.7 μs/op (98% in `canonicalize` → `EvalSymlinks` syscall). SKIP.
- `LoadRoots` n=10 — 56 μs/op (boundary), n=100 — 421 μs/op. Above L1, but
  L3 says no: 80%+ of that is the syscall-bound TOML decode of a tiny file.
  SKIP.
- `SaveRoots` n=10 — 163 μs/op, n=100 — 487 μs/op. Same story: 97% syscall.
  SKIP.

No part of the module passes the new recipe. We do not ship even as
evidence-only.

---

## Step 0 — Source-read findings

### Files

| File | LOC | Role |
|---|---|---|
| `config.go` | 221 | `Config` struct + sub-configs + `Load(dir)` / `LoadWithPath(dir)` + `Default()` + `AuditConfig.Validate/CompiledMaskPatterns` + `ExpandPath` |
| `roots.go` | 258 | `Root` + `RootsFile` + `RootsPath` + `LoadRoots`/`LoadRootsFrom` + `SaveRoots`/`SaveRootsTo` + `AddRoot` / `RemoveRoot` / `Find` / `MarkOpened` / `Sorted` |
| `roots_test.go` | 200 | 9 unit tests + `withRootsFile` env-override helper |

### Public API surface

```
config.Load(dir) → Config                       # one-shot ctx.toml decode
config.LoadWithPath(dir) → (Config, path, err)
config.Default() → Config
config.ExpandPath(path) → string                # ~ expansion
(AuditConfig).Validate() / .CompiledMaskPatterns()

config.RootsPath() → (path, err)
config.LoadRoots() / .LoadRootsFrom(path) → (RootsFile, err)
config.SaveRoots(rf) / .SaveRootsTo(path, rf) → err
(*RootsFile).AddRoot(name, path) → (added bool, err)
(*RootsFile).RemoveRoot(nameOrPath) → bool
(*RootsFile).Find(nameOrPath) → *Root
(*RootsFile).MarkOpened(name)
(RootsFile).Sorted() → []Root
```

### Hot-path inventory

```
LoadRoots
  RootsPath()                              # env lookup + UserHomeDir (84 ns)
  os.Stat(path)                            # 1 syscall
  toml.DecodeFile(path, &rf)               # ← OPEN + READ + CLOSE syscalls
    ↳ BurntSushi/toml.parse                # Go-only TOML parse on ≤a few KB
    ↳ reflect-based field assignment       # tiny: ~5 fields × N roots

SaveRoots
  RootsPath()
  os.MkdirAll(dir, 0o700)                  # 1-2 syscalls
  os.CreateTemp(dir, …)                    # syscall
  toml.NewEncoder(tmp).Encode(rf)          # Go-only marshal → write syscalls
  tmp.Close() + os.Chmod() + os.Rename()   # 3 syscalls (atomic replace)

AddRoot
  canonicalize(path) [×~2]                 # filepath.Abs + EvalSymlinks (Stat syscall)
  in-place mutation of rf.Roots            # cheap

RemoveRoot / Find / MarkOpened
  findIndex (linear scan over rf.Roots)    # 22-30 ns on 10 entries
```

### Internal deps

**Zero.** `internal/config` only imports stdlib + `github.com/BurntSushi/toml`.
No `internal/tokens`, no `internal/scan`, no graph chain — config is a leaf.

### Callers (all single-shot)

20 caller files across the repo:

| Surface | Pattern |
|---|---|
| `internal/cli/{root,roots,pack,where,focus,map,replay,doctor,browse,audit,audit_verify,mcp}.go` | One `config.Load(...)` or `config.LoadRoots()` per CLI invocation. |
| `internal/mcp/server.go::runRootsList` | One `LoadRoots` per `ctx_roots_list` MCP tool call. |
| `internal/web/roots_api.go` | `LoadRoots` + (occasionally) `SaveRoots` per **HTTP request** — the only multi-call surface in process lifetime. |
| `internal/pack/pack.go`, `internal/scan/secret.go`, `internal/security/offline.go`, `internal/braid/{braid,config}.go`, `cmd/braid-golden-export/main.go` | One `config.Load(dir)` per invocation. |

**Web is the only multi-call surface, but each request loads the same tiny
file (`~/.ctx/roots.toml`, typically 5-30 entries) afresh — and the disk
page is already in the OS cache after the first read.**

No daemon-resident config state. No long-lived `*RootsFile` shared across
requests. The pattern is verify-stale-on-each-call (idiomatic for a small
mutable registry that operators edit by hand or via CLI).

---

## Step 1 — 5-minute Go bench

Bench file: `internal/config/config_screen_bench_test.go` (this PR).
Command: `go test -bench=. -benchmem -benchtime=3s -run='^$' ./internal/config/`
Host: Apple M4, darwin/arm64, go 1.25.0.

| Bench | ns/op | B/op | allocs/op |
|---|---:|---:|---:|
| `LoadRoots` n=10 | **56,422** (56 μs) | 32,592 | 443 |
| `LoadRoots` n=100 | **420,534** (421 μs) | 317,186 | 4,056 |
| `SaveRoots` n=10 | **162,699** (163 μs) | 17,244 | 566 |
| `SaveRoots` n=100 | **487,134** (487 μs) | 120,275 | 5,426 |
| `AddRoot` | **8,756** (8.8 μs) | 3,329 | 41 |
| `Canonicalize` (isolated) | **8,556** (8.6 μs) | 3,232 | 38 |
| `RemoveRoot` | **22** (22 ns) | 0 | 0 |
| `Find` | **30** (30 ns) | 80 | 1 |
| `RootsPath` | **85** (85 ns) | 32 | 1 |

**Key reading: `AddRoot` (8,756 ns) ≈ `Canonicalize` (8,556 ns) — i.e. 98% of
AddRoot's cost is in `filepath.EvalSymlinks`, a single Go stdlib syscall.
Nothing portable lives between those two numbers.**

### CPU pprof — `LoadRoots/n=100` (5s benchtime, 14k iters)

```
   flat  flat%   sum%        cum   cum%
  8.57s 80.70% 80.70%      8.57s 80.70%  syscall.syscall
      0     0% 80.70%      8.19s 77.12%  os.(*File).Close
      0     0% 80.70%      9.09s 85.59%  toml.DecodeFile
      0     0% 80.70%      0.67s  6.31%  toml.(*Decoder).Decode      ← Go-only parser
      0     0% 80.70%      0.47s  4.43%  toml.parse                  ← Go-only parser
      0     0% 80.70%      0.39s  3.67%  toml.(*parser).topLevel
      0     0% 80.70%      0.37s  3.48%  toml.(*parser).next
      0     0% 80.70%      0.35s  3.30%  toml.(*lexer).nextItem
```

**The TOML parser is <7% of total time on n=100. Even a 5-10× Rust BurntSushi
replacement would shave ~5% of LoadRoots. The cgo+JSON shuttle floor (~50 μs)
is ~10× that potential saving on n=10.**

### CPU pprof — `SaveRoots/n=100` (5s benchtime, 13k iters)

```
   flat  flat%   sum%        cum   cum%
 10.44s 97.39% 97.39%     10.44s 97.39%  syscall.syscall
      0     0% 97.39%      6.66s 62.13%  syscall.Write       ← temp file write
      0     0% 97.39%      6.69s 62.41%  toml.(*Encoder).Encode
      0     0% 97.39%      5.25s 48.97%  toml.(*Encoder).eArrayOfTables
      0     0% 97.39%      5.25s 48.97%  toml.(*Encoder).eStruct
```

Encoder appears at 62% cum but that's because it's the call-tree ancestor of
the `syscall.Write` it triggers; the encoder's own marshal compute is
included in `bufio.(*Writer).Flush → syscall.Write`. The actual flat CPU is
all in syscall.

---

## Step 2 — L1/L2/L3/L4 application

### L1 — heatmap criterion (per-call ≥ 50 μs + ≥1 caller per cmd)

| Function | per-call | L1 verdict |
|---|---:|---|
| `RootsPath` | 85 ns | **FAIL** (~590× below 50 μs floor) |
| `Find` | 30 ns | **FAIL** |
| `RemoveRoot` | 22 ns | **FAIL** |
| `AddRoot` | 8.8 μs | **FAIL** (5.7× below) |
| `LoadRoots` n=10 | 56 μs | borderline PASS (just clears) |
| `LoadRoots` n=100 | 421 μs | PASS (raw) |
| `SaveRoots` n=10 | 163 μs | PASS (raw) |
| `SaveRoots` n=100 | 487 μs | PASS (raw) |

`LoadRoots`/`SaveRoots` raw-pass at n=100 but L1 alone is not the criterion —
see L3. n=100 is also an upper bound for synthetic stress; real registries
hold 5-30 entries (n=10 is the realistic operating point).

### L2 — pack/sticky-handle: amortisation surface?

- All 20 callers are **one-shot per CLI invocation** or **one-shot per MCP
  tool call**. The CLI examples (`ctx roots add`, `ctx browse`, `ctx pack`)
  load config exactly once.
- The web server (`/api/roots`) is the only multi-call surface, but:
  - The roots file is **tiny** (5-30 entries, <2 KB on disk) — the disk page
    is in OS cache after the first read.
  - Each request must observe operator edits made via CLI between requests
    (verify-stale-on-each-call is the correct semantics).
  - A session-resident cache would need invalidation hooks for CLI mutations
    — added complexity for a sub-millisecond saving that the OS page cache
    already delivers.
- A theoretical "cache RootsFile in-memory across HTTP requests with TTL"
  optimisation could be done **in Go** without any Rust involvement — and
  would still shave only the 56-163 μs the syscalls cost. Not a port story.

**L2: FAIL.**

### L3 — echo "what is the actual hot operation?"

Pprof tells the story unambiguously:

| Sub-op | Share of CPU (Load n=100) | Share (Save n=100) | Rust-portable? |
|---|---:|---:|---|
| `syscall.syscall` (open/read/close/write/rename/stat) | **80.7%** | **97.4%** | NO — syscall is OS, unchanged under Rust |
| `BurntSushi/toml` parse / marshal | <7% / <49%* | — | YES (`toml-rs`) — but slice too small or downstream of syscall |
| reflect-based struct fill / type checking | <3% | <2% | YES — irrelevant |
| `EvalSymlinks` (in AddRoot) | 98% of 8.8 μs | n/a | NO — syscall.Stat |

*`Encoder.Encode` shows 62% cum because it owns the call-tree that drives
`syscall.Write`. The encoder's own compute is in the few-% range; the
visible cost is the write it triggers.

**The hot operation is non-portable filesystem I/O. The Rust-portable slice
(TOML parse/marshal of a <2 KB file) is <10% of total runtime in Load and
buried below syscall.Write in Save.**

This is the **same shape as `digest` (Tier 2 #6)** — high raw latency
dominated by a Go-only library boundary (here: `toml.DecodeFile` /
`toml.Encoder` + the kernel; in digest: go-git's loose-object walk).
The cgo floor would swallow any portable improvement.

**L3: FAIL.**

### L4 — replay per-function verdict (per-function ship?)

- `RootsPath`: SKIP. 85 ns. Below cgo floor by ~590×.
- `Find` / `RemoveRoot`: SKIP. 22-30 ns, in-memory linear scans on ≤30
  elements; nothing to port.
- `AddRoot`: SKIP. 8.8 μs, 98% is `EvalSymlinks` (syscall). The remaining
  ~170 ns is the dup-check linear scan + slice append — sub-cgo-floor.
- `LoadRoots`: SKIP. Raw latency clears L1 but L3 says no — 80.7% syscall,
  <7% portable parse. Cgo floor (~50 μs round-trip per FFI call) eats the
  potential 5-15 μs Rust speedup on the parse step.
- `SaveRoots`: SKIP. 97.4% syscall. Even if Rust marshal were 10× faster
  than BurntSushi, the savings (~30 μs out of 487) wouldn't clear the cgo
  floor.

**L4: no per-function ship candidate.**

---

## Why this skip matters (lessons)

1. **"Small + I/O-dominated + zero deps" is the canonical SKIP shape.**
   This is the first explicit confirmation of the rule (the digest skip
   was "medium + I/O via Go-only dep + 1 internal dep"). Anytime a Tier
   2/3 candidate matches **all three** of {<500 LOC source, hot path is
   `os.Stat`/`os.Open`/`os.Read`/`os.Write`/`syscall.*`, zero internal
   deps}, screen-out without writing a Rust crate.
2. **TOML parse/marshal alone is not enough to clear cgo.** BurntSushi/toml
   is fast (a few μs on a <2 KB file). Even a 10× Rust replacement is
   sub-cgo-floor per call. Port a TOML library only if it's called
   thousands of times per request on the hot path — none of this repo's
   callers do that.
3. **`os.UserHomeDir` + `os.Stat` + `filepath.EvalSymlinks` are syscall-
   gated.** Any module whose hot helper is "resolve a path on disk" is
   syscall-bound by definition. Skip without measuring the wrapper —
   measure only to confirm the proportion (here, 98% of AddRoot).
4. **The web caller looks multi-call but operates on a cache-warm file.**
   Don't over-index on caller-count when each call is sub-ms against a
   page-cached file. The "multi-call surface" test should be "does the
   same expensive computation repeat across calls?" — for config, no:
   each call is a fresh, cheap read.
5. **Step 0 source-read + Step 1 pprof catches this in 15 minutes.** The
   recipe is now validated on two consecutive skips (`digest`, `config`).
   Future Tier 2 candidates with the same shape should be skipped via the
   same path without a full port template.

---

## Recommended alternatives

| Option | Worth? |
|---|---|
| Port `BurntSushi/toml` decode/encode → Rust | **No** — covers <7% (Load) / <49% encoder-tree (Save, but downstream of syscalls) of total time; cgo floor swamps the saving. |
| Cache `RootsFile` in-memory across HTTP requests with TTL | **Defer** — would shave 56-163 μs per request but the disk page is already OS-cached, and CLI/MCP mutations would need invalidation. **Pure-Go optimisation if it ever matters; not a port story.** |
| Replace TOML with a faster format (JSON/CBOR) | **No** — TOML is user-facing (operators edit `roots.toml` by hand). Format change is a config-stability regression with no perf upside. |
| Daemon mode (Tier 3) | **Defer** — if a future `ctx serve` keeps a long-lived RootsFile, in-process cache is the answer (Go-side), not a Rust port. |
| Wire config into existing pack/replay sessions | **No** — config is a leaf module loaded once at process startup; sessioning a startup-cost module is a non-sequitur. |

---

## Telemetry

- `internal/config/config_screen_bench_test.go` retained as the screening
  evidence and as a regression guard if the hot-path composition ever
  changes (e.g., if registries grow to thousands of entries — but the CLI
  workflow caps practical sizes at the operator-tolerable threshold).
- No `--config-engine` flag is introduced. The module remains 100% Go.

---

## Cross-references

- `tests/MIGRATION_ROADMAP.md` — Tier 2 #7 row updated to **SCREENED-SKIPPED**.
- `tests/RELEASE_NOTES.md` — no new flag.
- Recipe origin: PR #76 (Step 0 + Step 1 + Step 2 codification).
- Sibling precedents:
  - `digest` (Tier 2 #6) — first skip via this recipe; shape was
    "medium + I/O via Go-only dep + 1 internal dep" (go-git syscall storm).
  - `config` (Tier 2 #7, this doc) — second skip; shape is
    "small + I/O via stdlib + zero deps" (TOML over filesystem syscalls).
  - Together these establish that **any module whose pprof shows >75%
    `syscall.syscall` is SKIP regardless of raw latency or LOC.**
