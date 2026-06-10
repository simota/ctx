# Batch Screen — 17 remaining `internal/*` modules

**Document type**: Desk-quality L1-L4 screen (per ADR-004 campaign close)
**Date**: 2026-05-30
**Author**: ADR-004 batch-screen pass
**Method**: source-read + caller-count + L1-L4 recipe prediction. **No bench
execution, no pprof.** Confidence labelled per module so future amendments
can target the LOW-confidence rows first if anyone re-opens the campaign.

Recipe applied (from
[ADR-004](../docs/adr/0004-campaign-close.md) §"Empirical foundation"):

- **L1 (per-call cost)**: Go baseline ≥50 µs → PASS (cgo+JSON shuttle floor
  is ~50-60 µs per round trip).
- **L2 (session fit)**: corpus / state amortises across ≥2 calls per
  process → PASS.
- **L3 (hot path operation type)**: regex/byte-scan or pure-compute
  arithmetic → PASS. String/HashMap/JSON allocation churn → FAIL
  (per `ctx-echo` precedent). Tree-sitter / go-git / `os.ReadFile`
  syscall-bound → FAIL (per digest/config/walk precedent).
- **L4 (per-function ship)**: any function ≥50 µs Go baseline with a
  Rust-portable hot path that isn't syscall-bound → SHIP candidate.

Verdict legend:

- **SKIP** — no port even as evidence-only. L1 OR L3 fails, AND no
  data-access amortisation lane exists.
- **EVIDENCE-ONLY-MEM** — likely to memory-bucket-ship (-30%+ allocs)
  but perf would regress. Worth a port only if a future memory-budget
  target gates it (none currently).
- **SHIP-CANDIDATE-FOR-FOLLOWUP** — predicted ≥1.5× net or sessioned
  win. Worth a dedicated future-ADR amendment to schedule.

Confidence levels:

- **HIGH** — pattern matches a screened-skipped sibling (digest /
  config / walk) or a shipped pattern (focus / contract) directly.
- **MEDIUM** — source-read is consistent with the prediction but the
  exact pprof shape is unknown.
- **LOW** — module shape doesn't fit any precedent cleanly; would
  benefit from a 30-min pprof + Go-bench probe before any port
  decision.

---

## Summary table

| # | Module | LOC | Inbound | Verdict | Confidence | Bucket |
|---|--------|----:|--------:|---------|------------|--------|
| 1 | audit | 277 | 9 | SKIP | HIGH | I/O + sub-µs hot path |
| 2 | budget | 135 | 7 | EVIDENCE-ONLY-MEM | MEDIUM | sub-50 µs sort+greedy |
| 3 | cli | 4329 | 1 | SKIP | HIGH | GLUE — Cobra wiring |
| 4 | git | 596 | 13 | SKIP | HIGH | go-git syscall floor |
| 5 | hooks | 97 | 1 | SKIP | HIGH | `os/exec` process IO |
| 6 | mcp | 2926 | 1 | SKIP | HIGH | JSON-RPC stdio server |
| 7 | mix | 271 | 4 | SKIP | HIGH | Filesystem CRUD ≤500 records |
| 8 | model | 89 | 65 | SKIP | HIGH | Pure type defs, no logic |
| 9 | noise | 230 | 2 | SKIP | MEDIUM | Walks via `internal/walk` (already SKIP) |
| 10 | onboarding | 504 | 1 | SKIP | MEDIUM | go-git churn + walk + scoring |
| 11 | render | 556 | 1 | EVIDENCE-ONLY-MEM | MEDIUM | sub-50 µs tree print |
| 12 | security | 17 | 2 | SKIP | HIGH | Env-var lookup |
| 13 | skim | 327 | 2 | SHIP-CANDIDATE-FOR-FOLLOWUP | LOW | per-file >50 µs; depends on tree-sitter scope-split |
| 14 | testinsights | 519 | 1 | SKIP | MEDIUM | Walk + `go/parser` + coverprofile parse |
| 15 | tokens | 90 | 13 | SKIP | HIGH | tiktoken-go is Go-bound CGO-equivalent |
| 16 | tui | 304 | 2 | SKIP | HIGH | Bubble Tea UI loop |
| 17 | web | 3411 | 4 | SKIP | HIGH | HTTP I/O + handler dispatch |

**Bucket totals**:

- SKIP: **14** modules (audit, cli, git, hooks, mcp, mix, model,
  noise, onboarding, security, testinsights, tokens, tui, web).
- EVIDENCE-ONLY-MEM: **2** modules (budget, render).
- SHIP-CANDIDATE-FOR-FOLLOWUP: **1** module (skim).

If the 14 SKIPs are accepted, the remaining campaign surface is 3
modules (budget, render, skim) of which only one (skim) projects a
perf win and that win is gated on a tree-sitter scope-split decision
that ADR-004's "no further ports under current FFI" stance explicitly
defers.

---

## Per-module entries

### internal/audit
- **LOC**: 277 (audit 99 + mask 32 + verify 117 + lock_unix 16 + lock_other 13)
- **Internal deps**: 0
- **Inbound callers**: 9 (cli wrappers + mcp + web audit endpoint)
- **Source-read summary**: appends JSONL hash-chained records under a
  file lock (`flock` on unix). `Append` opens the file, locks, reads
  last line to compute prev-hash, JSON-marshals the entry, writes, and
  unlocks. `VerifyChain` streams the file line-by-line, recomputing
  sha256. `MaskQuery` applies precompiled regex replacements.
- **L1**: FAIL — `MaskQuery` is sub-µs; `Append` is dominated by
  `flock` + `os.OpenFile` + `os.Sync` (filesystem syscalls); the only
  computation is one `json.Marshal` + one `sha256.Sum256` per call.
- **L2**: FAIL — each call is independent; no corpus state to cache.
  Audit log is append-only by design; a session would re-introduce
  the file lock contention.
- **L3**: FAIL — hot path is syscall (file lock + write + sync),
  matching the `walk` / `config` precedent.
- **L4**: FAIL — `MaskQuery` is the only pure-compute function and
  sits below the cgo floor (sub-µs).
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (matches the post-config "small + I/O-dominated
  + zero deps" SKIP shape exactly)
- **Recommendation**: skip; do not introduce `--audit-engine`.

### internal/budget
- **LOC**: 135 (single file)
- **Internal deps**: 1 (model)
- **Inbound callers**: 7 (pack planner, render, cli, mcp, web)
- **Source-read summary**: `Plan(files, tokenBudget)` builds a
  candidate slice (one struct per file), sorts by role priority + path,
  greedy-fills until budget exceeded. Pure-compute over file
  metadata; no I/O.
- **L1**: BORDERLINE — likely <50 µs for typical pack sizes (≤2000
  files); could clear the bar on very large monorepos but the per-call
  shape is one-shot per `ctx pack` invocation.
- **L2**: FAIL — single call per pack; no second call to amortise
  against in production.
- **L3**: PASS-LITE — sort + greedy fill is Rust-friendly (Vec sort +
  scan), but per-call work is small.
- **L4**: FAIL for time bar; could PASS memory bar (the `[]Item`
  candidate slice is allocated per call and discarded).
- **Predicted verdict**: EVIDENCE-ONLY-MEM (memory bucket only, like
  ctx-braid)
- **Confidence**: MEDIUM (need a 5-min bench to confirm the sub-50 µs
  baseline; otherwise mirror the braid precedent)
- **Recommendation**: do not port. If a future memory-budget gate
  fires, port with explicit memory-bucket label.

### internal/cli
- **LOC**: 4329 across 28 files
- **Internal deps**: ~28 (depends on almost every other internal
  package — it's the Cobra command tree)
- **Inbound callers**: 1 (cmd/ctx main)
- **Source-read summary**: Cobra command definitions, flag parsing,
  argument validation, dispatching to internal packages. Pure glue.
  Hot path is the user's invocation, not anything cli does itself.
- **L1**: FAIL — flag parsing + dispatch is sub-µs per command.
- **L2**: FAIL — one-shot per process.
- **L3**: FAIL — GLUE shape (the original anti-recommendation in
  NEXT_MODULES_ANALYSIS.md still stands).
- **L4**: FAIL — no portable per-function unit.
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (matches the original ADR-001 / NEXT_MODULES
  anti-recommendation verbatim)
- **Recommendation**: skip permanently. Porting cli loses Cobra
  ergonomics with no possible speedup.

### internal/git
- **LOC**: 596 (diff 408 + file_log 110 + last_commit 11 + status 67)
- **Internal deps**: 1 (model)
- **Inbound callers**: 13 (cli, web, mcp, onboarding, walk, etc.)
- **Source-read summary**: thin wrapper over `go-git/v5`. `Status`
  opens a repo and walks the worktree; `Diff` resolves two revs and
  computes file-level patches via `Tree.PatchContext`;
  `FileLog` walks commit history filtered by path; `LastCommit`
  resolves HEAD.
- **L1**: PASS on raw latency (diff/log over real repos clears 50 µs
  easily on medium repos).
- **L2**: BORDERLINE — same repo handle could be reused, but go-git's
  `PlainOpen` is cheap relative to the per-call work.
- **L3**: FAIL — go-git is the same Go-only dependency that drove
  digest's SKIP verdict. >75% syscall.syscall expected on any pprof
  here (loose-object walks, packfile reads).
- **L4**: FAIL — no portable per-function slice; all hot paths bottom
  out in go-git, which we cannot host in Rust without reimplementing
  the dep (libgit2 binding cost dwarfs any speedup per ADR-001).
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (digest precedent applies directly — same
  go-git dependency, same syscall composition expected)
- **Recommendation**: skip. Reopen only if libgit2-binding work is
  funded as a separate program.

### internal/hooks
- **LOC**: 97 (single file)
- **Internal deps**: 0
- **Inbound callers**: 1 (cli pack pipeline)
- **Source-read summary**: runs configured shell commands at lifecycle
  events (pre_pack / post_pack / on_secret) via `os/exec.CommandContext`.
  Captures stdout/stderr, enforces timeout.
- **L1**: FAIL — work-per-call is dominated by external process startup
  (forks the configured shell command). Hooks itself does sub-µs flag
  evaluation.
- **L2**: FAIL — each event fires once per pack run; no amortisation.
- **L3**: FAIL — `os/exec` is process-IO; cgo crossing on top adds
  pure overhead.
- **L4**: FAIL — nothing portable above the process boundary.
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (matches the hooks anti-recommendation in
  NEXT_MODULES_ANALYSIS.md and the audit/walk SKIP pattern)
- **Recommendation**: skip permanently.

### internal/mcp
- **LOC**: 2926 across 6 files (server 2000+, prompts, resources,
  safepath, progress, transcript)
- **Internal deps**: 14 (audit, budget, config, digest, focus, git,
  model, pack, scan, skim, symbols, tokens, walk, where)
- **Inbound callers**: 1 (cli `mcp serve`)
- **Source-read summary**: long-lived JSON-RPC server over stdio.
  Decodes JSON-RPC requests, dispatches to tool handlers, encodes
  responses + progress notifications. Most per-request cost is
  downstream of the dispatch (the called tools do the heavy lifting).
- **L1**: FAIL — per-request mcp work (JSON-RPC decode + dispatch +
  encode) is sub-50 µs; the heavy work happens in the called
  internal/* tools.
- **L2**: PASS-LITE — server is long-lived, but its own state is
  request-routing tables and transcript buffers, not corpus state.
- **L3**: FAIL — hot path is stdio + JSON-RPC plumbing, not
  regex/byte-scan/arithmetic.
- **L4**: FAIL — every per-function unit is plumbing.
- **Predicted verdict**: SKIP (or PURE-RESEARCH-ONLY — could be its
  own ADR if rewriting in Rust+Tokio for protocol-fidelity reasons,
  but that's not a perf decision)
- **Confidence**: HIGH (mcp anti-recommendation in NEXT_MODULES_ANALYSIS
  still stands; per-request JSON cost dwarfed by IO/dispatch)
- **Recommendation**: skip under current FFI. Treat any future MCP
  rewrite as a separate strategic decision (own ADR), not a port.

### internal/mix
- **LOC**: 271 (mix 62 + store 209)
- **Internal deps**: 0
- **Inbound callers**: 4 (cli, web, mcp)
- **Source-read summary**: filesystem-backed CRUD for `.mix.json`
  recipe artifacts. `Save` / `Load` / `List` / `Delete` operate on
  files under a configured store dir. `Validate` enforces field
  limits (Files ≤500, Goal ≤1024 bytes, Name ≤128 bytes).
- **L1**: FAIL — file open + decode of a ≤8 KB JSON file is dominated
  by `os.File.Open/Close` syscalls (same shape as config).
- **L2**: BORDERLINE — web could cache a recently-listed mix index,
  but each list-then-load flow already hits the page cache.
- **L3**: FAIL — syscall-bound (config precedent).
- **L4**: FAIL — `Validate` is sub-µs; `Save`/`Load` are syscall-floor.
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (matches config "small + I/O-dominated + zero
  deps" SKIP shape)
- **Recommendation**: skip.

### internal/model
- **LOC**: 89 (single file)
- **Internal deps**: 0
- **Inbound callers**: 65 (used everywhere)
- **Source-read summary**: type definitions only — `FileInfo`,
  `Symbol`, `GitStatus`, `FileRole` enums. No functions, no logic.
- **L1**: N/A — no code to bench.
- **L2**: N/A.
- **L3**: N/A.
- **L4**: N/A.
- **Predicted verdict**: SKIP (mirror as `serde` structs inside
  other ports, never standalone — this is the original ADR-001
  guidance and remains correct)
- **Confidence**: HIGH (verbatim from NEXT_MODULES_ANALYSIS)
- **Recommendation**: skip. Already done correctly — Rust crates
  mirror the types they need via `serde::{Serialize, Deserialize}`.

### internal/noise
- **LOC**: 230 (single file)
- **Internal deps**: 4 (model, symbols, tokens, walk)
- **Inbound callers**: 2 (cli noise, mcp)
- **Source-read summary**: `Inspect(root)` walks the repo via
  `walk.New`, classifies each file (`generated` / `lockfile` /
  `testdata` / `binary` / `huge-json` / `low-density`), counts
  tokens, sorts by token count, returns top-N candidates.
- **L1**: PASS-LITE — total Inspect call is multi-millisecond on
  medium repos, but >90% of that is `walk` (already SKIPped) + tokens
  (tiktoken Go-bound, already SKIP-recommended) + symbols (tree-sitter
  scope-split rules apply).
- **L2**: FAIL — one-shot per CLI/MCP invocation.
- **L3**: FAIL — the classify function itself is pure-compute (string
  prefix/suffix checks) and sub-µs per file; the bulk of runtime is
  delegated to already-SKIP modules.
- **L4**: FAIL — `classify` clears L3 syntactically but sub-cgo-floor.
- **Predicted verdict**: SKIP
- **Confidence**: MEDIUM (the per-file `classify` could in principle
  be sessioned alongside a Rust walk + symbols + tokens stack, but
  all three dependencies are SKIPped, so this transitively SKIPs)
- **Recommendation**: skip. Would become a candidate only if walk +
  tokens + symbols all moved to Rust under a future FFI redesign.

### internal/onboarding
- **LOC**: 504 (single file)
- **Internal deps**: 3 (model, symbols, walk)
- **Inbound callers**: 1 (cli onboarding)
- **Source-read summary**: ranks files for new-contributor reading
  order. `Rank()` walks the repo, builds a churn map from
  `go-git`'s 60-day log, computes ref-counts from symbol graphs,
  scores each file (entry-role + churn + ref-count + size + role).
  Like onboarding, dominated by data-source costs.
- **L1**: PASS on raw latency (Rank over a 500-file medium repo is
  10s of ms) — but composition is the same as digest/onboarding-like
  modules: go-git churn + walk + symbols, all already SKIP.
- **L2**: FAIL — one-shot per invocation.
- **L3**: FAIL — the score arithmetic itself is sub-µs per file; the
  hot path is `gogit.Log` (matching digest's 83% syscall pprof shape).
- **L4**: FAIL — `Rank` clears L1 but no portable slice.
- **Predicted verdict**: SKIP
- **Confidence**: MEDIUM (high-confidence on the go-git-dominates
  argument; medium because we haven't run pprof on Rank specifically)
- **Recommendation**: skip. Reopen only under a libgit2-binding
  redesign.

### internal/render
- **LOC**: 556 (budget 175 + json 134 + plain 68 + tree 179)
- **Internal deps**: 2 (budget, model)
- **Inbound callers**: 1 (cli)
- **Source-read summary**: writes tree / budget / JSON output to an
  `io.Writer`. `Tree` recursively walks `*model.FileInfo` printing
  box-drawing connectors; `Budget` formats a progress bar + included
  / excluded item lists; `JSON` marshals a tree DTO via
  `encoding/json`.
- **L1**: FAIL — per-call work is dominated by `io.Writer` calls
  (Stdout / file) and `encoding/json` on small structs; per-call is
  sub-50 µs on typical pack sizes.
- **L2**: FAIL — one-shot per command.
- **L3**: PASS-LITE — pure-compute string formatting + JSON marshal
  is Rust-friendly, but the cgo+JSON shuttle floor exceeds the work.
- **L4**: FAIL for time bar; could PASS memory bar (per-render
  allocations are non-trivial — `fmt.Fprintf` + `strings.Builder` +
  intermediate slices).
- **Predicted verdict**: EVIDENCE-ONLY-MEM
- **Confidence**: MEDIUM (BATCH shape, sub-50 µs Go baseline expected,
  matches braid/heatmap memory-bucket precedent)
- **Recommendation**: do not port. If a future memory-budget gate
  fires, port as memory bucket only.

### internal/security
- **LOC**: 17 (single function)
- **Internal deps**: 1 (config)
- **Inbound callers**: 2 (cli, mcp)
- **Source-read summary**: `IsStrictOffline(flag, cfg) bool` —
  returns true if any of the flag / config field / env var indicates
  strict-offline mode. Pure boolean logic.
- **L1**: FAIL — sub-100 ns per call (env var lookup + 3 string
  comparisons).
- **L2**: FAIL.
- **L3**: FAIL.
- **L4**: FAIL.
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (trivially below any conceivable cgo floor)
- **Recommendation**: skip permanently.

### internal/skim
- **LOC**: 327 (single file)
- **Internal deps**: 2 (symbols, tokens)
- **Inbound callers**: 2 (cli skim, mcp)
- **Source-read summary**: compresses a single file into a token
  budget via tiered fallback (full → api+doc → signatures → outline).
  `Skim(opts)` reads the file, calls `symbols.Extract`, formats one
  tier at a time until output fits the budget. `stripDocComments`
  parses comment lines; `renderOutline` formats anchor-form lines.
- **L1**: PASS on per-file work — `symbols.Extract` is a tree-sitter
  parse and `tokens.Count` is BPE encode, both already known to be
  in the hundreds of microseconds to milliseconds range per medium
  file.
- **L2**: BORDERLINE — same file is sometimes skimmed at multiple
  tiers; could cache the symbols extraction per-file across the
  tier-fallback loop. Sticky-handle pattern applies if a multi-file
  skim becomes a use case (none currently).
- **L3**: MIXED — the Rust-portable surface is `stripDocComments`
  (regex/byte-scan) + tier rendering (string formatting) + budget
  comparison (arithmetic). The hot path is dominated by tree-sitter
  (Go-bound) per the symbols precedent (Tier 2 #5 scope-split).
- **L4**: PASS for the post-extract rendering + tier-fallback loop
  IF scope-split with tree-sitter staying Go-side (the symbols Tier 2
  #5 pattern). Per-file work would clear L1 only if the file is
  >50 µs to skim — true for medium / large files; not for small.
- **Predicted verdict**: SHIP-CANDIDATE-FOR-FOLLOWUP
- **Confidence**: LOW (the per-tier render work is small; the only
  reason this isn't SKIP is the symbols Tier 2 #5 scope-split shape
  could apply. Real verdict requires a Go bench + pprof on a multi-
  file skim workload that doesn't currently exist in production.)
- **Recommendation**: dedicated future-ADR port only if a multi-file
  skim caller emerges (e.g., a `ctx skim --all` batch mode for
  onboarding kits). Otherwise skip; the single-file caller shape
  doesn't justify FFI complexity even with positive numbers.

### internal/testinsights
- **LOC**: 519 (single file)
- **Internal deps**: 1 (walk)
- **Inbound callers**: 1 (web `/api/tests` handler)
- **Source-read summary**: `Analyze(root, relPath, profile)` finds
  tests + coverage data related to a Go source file. Walks
  `_test.go` files, parses each via `go/parser` (Go stdlib AST),
  matches function names against the target's symbols, optionally
  reads a Go coverprofile and parses cover lines.
- **L1**: PASS on raw latency (per-file `go/parser` on test files is
  a few ms each).
- **L2**: BORDERLINE — web handler could cache the parsed AST index
  per repo, but that requires reimplementing `go/parser` AST
  inspection in Rust (out of scope; Rust has no `go/parser`
  equivalent for Go-source AST inspection).
- **L3**: FAIL — hot path is Go stdlib `go/parser` + `go/ast`, which
  is Go-only. The string-matching post-AST is sub-µs per call.
- **L4**: FAIL — every per-function unit either calls go/parser (not
  portable) or is sub-µs.
- **Predicted verdict**: SKIP
- **Confidence**: MEDIUM (high on the "go/parser is Go-only"
  argument; medium because we haven't profiled the call
  distribution — `parseCoverLine` and `symbolsMentioned` are
  text-string ops that could in principle be sessioned, but they're
  downstream of the parse cost)
- **Recommendation**: skip. The `go/parser` dependency is structural;
  porting requires reimplementing Go-source AST in Rust, which is
  out of scope under any conceivable FFI redesign.

### internal/tokens
- **LOC**: 90 (counter 69 + plans 21)
- **Internal deps**: 0
- **Inbound callers**: 13 (every module that needs token counts)
- **Source-read summary**: thin wrapper over `tiktoken-go`'s
  cl100k_base BPE encoder. `CountString` calls `enc.Encode(text)` and
  returns `len(tokens)`. `CountFile` reads a file then encodes its
  content. Encoder is shared (sync.Once) across all callers.
- **L1**: BORDERLINE — `Encode` on a medium chunk (≤10 KB) takes
  hundreds of microseconds to a few ms; clears the cgo floor for
  larger inputs but not for small.
- **L2**: PASS-LITE — encoder state is already process-shared
  (sync.Once). Sessioning would only re-amortise across calls within
  one Rust process — which is what tiktoken-go already does.
- **L3**: FAIL — `tiktoken-go` is the Go BPE implementation. The
  Rust equivalent (`tiktoken-rs`) exists but the call site is exactly
  the cgo+JSON shuttle floor problem from echo (Tier 2 #3):
  per-token work is small, per-call result is small, the dominant
  cost shifts to FFI marshalling.
- **L4**: FAIL — `CountString` and `CountFile` both bottom out in the
  BPE encoder; no portable slice above that.
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (matches NEXT_MODULES_ANALYSIS anti-rec
  verbatim: "per-call work too small; cgo overhead dominates BPE
  encode")
- **Recommendation**: skip. Only reopen if a future FFI redesign
  reduces the per-call shuttle floor below tiktoken-rs's intrinsic
  Encode cost.

### internal/tui
- **LOC**: 304 (single file)
- **Internal deps**: 4 (model, pack, tokens, walk)
- **Inbound callers**: 2 (cli `--tui`, cli browse)
- **Source-read summary**: Bubble Tea TUI for interactive file
  selection. `Model.Update` handles key events (cursor up/down,
  toggle include, render); `Model.View` renders the visible window
  with lipgloss styling.
- **L1**: FAIL — per-keystroke work is sub-ms; user-perceived latency
  is dominated by terminal redraw.
- **L2**: FAIL — TUI state is the corpus state (already in-process).
- **L3**: FAIL — terminal control codes + lipgloss styling are
  string concatenation; cgo shuttle would invert the latency.
- **L4**: FAIL — every per-function unit is sub-ms UI plumbing.
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (UI loops are GLUE-shape; cgo per keystroke
  is the worst possible application)
- **Recommendation**: skip permanently.

### internal/web
- **LOC**: 3411 across 12 source files (api, audit, browser, embed,
  handlers, mix, roots_api, routes, safepath, server, etc.)
- **Internal deps**: 13 (budget, config, contract, git, mix, model,
  relations, replay, symbols, testinsights, tokens, walk, where)
- **Inbound callers**: 4 (cli browse, cmd entry points)
- **Source-read summary**: embedded HTTP UI server. `handlers.go`
  dispatches to per-resource handlers (`/api/files`, `/api/symbols`,
  `/api/relations`, `/api/replay/*`, `/api/contract`, `/api/tests`,
  etc.). Each handler reads a small request body, validates, calls
  into an internal/* package, writes JSON to the response. Most of
  the heavy lifting is already done by Rust crates via the
  sticky-handle pools (`API.RelationsPool`, `API.ReplayPool`,
  `API.SymbolsPool`).
- **L1**: FAIL — per-handler web work (HTTP parse + dispatch + JSON
  encode) is sub-50 µs; the heavy work is downstream.
- **L2**: PASS — server is long-lived, but its own state is the
  sticky-handle pools (already Rust) plus an embedded asset map
  (sub-µs lookups). Web doesn't have its own corpus to amortise.
- **L3**: FAIL — hot path is HTTP I/O (`net/http`) + JSON-encode +
  per-handler dispatch, all of which are stdlib + already-Rust calls.
- **L4**: FAIL — every per-function unit is a thin dispatch shell
  around an already-Rust or already-SKIP dependency.
- **Predicted verdict**: SKIP
- **Confidence**: HIGH (matches the original NEXT_MODULES_ANALYSIS
  anti-rec: "54-type API surface; months of Tokio/Axum porting for
  a network-IO wash")
- **Recommendation**: skip. The right architecture for web is what
  it already is — Go-side HTTP + dispatch into the sticky-handle
  Rust pools for the work that actually amortises. Porting web
  itself would only relocate the I/O code without changing the
  economics.

---

## Cross-cutting observations

1. **The 14 SKIP modules cluster into three failure modes**:
   - **I/O / syscall dominated** (audit, mix, git, hooks, web,
     onboarding, noise, testinsights, walk-via-dep): hot path is
     `syscall.syscall` regardless of language. The post-digest /
     post-config / post-walk SKIP rule applies directly.
   - **Sub-cgo-floor per-call work** (security, tokens, render,
     budget, tui): work is too small for the shuttle round-trip cost
     to amortise.
   - **GLUE / orchestration** (cli, mcp, model): no portable
     per-function unit; the dispatcher itself is the module.

2. **One MEDIUM-confidence row would benefit from a future probe**
   (skim) — its scope-split sketch (Rust owns post-AST tier rendering;
   tree-sitter stays Go) is technically feasible but the production
   caller shape doesn't currently justify the dual-language tax.

3. **No row in the 17 projects ≥3× net wall-clock under sticky-handle**
   — the only row that projects positively (skim) does so against a
   workload that doesn't yet exist in production. ADR-004's "close
   the campaign" verdict is well-supported by this batch screen.

4. **Three modules (mcp, web, cli) are PURE-RESEARCH-ONLY for a
   future Rust-stack project** — if leadership ever decides to rewrite
   the entire program in Rust (not the same as "port internal/*
   modules under cgo+JSON"), those three would be the architectural
   anchors. That decision is explicitly out of scope under ADR-004
   and would need its own ADR.

## Where pprof would change a verdict

For audit trail: the rows that LOW or MEDIUM confidence would benefit
most from a one-hour pprof+bench probe before any future amendment
acts on them:

- **skim** (LOW): the only positive-projection row. A 5-min Go bench
  of `Skim` on small/medium/large files would calibrate whether the
  post-extract rendering is >50 µs.
- **testinsights** (MEDIUM): if `Analyze` ever moves to a multi-file
  batch mode, the AST-parse dominance argument needs re-verification
  (currently assumed structurally identical to the go/parser
  precedent).
- **onboarding** (MEDIUM): if `Rank` ever loses its go-git churn
  dependency (e.g., switches to a precomputed index), the verdict
  could shift toward EVIDENCE-ONLY-MEM.

All other 14 rows are HIGH-confidence SKIPs and a pprof probe would
not change their classification.

## References

- [`docs/adr/0004-campaign-close.md`](../docs/adr/0004-campaign-close.md) — the ADR this batch screen supports.
- [`tests/DIGEST_SCREENING.md`](./DIGEST_SCREENING.md) — first L1-L4 recipe application; defines the syscall-bound SKIP rule.
- [`tests/CONFIG_SCREENING.md`](./CONFIG_SCREENING.md) — second L1-L4 recipe application; "small + I/O + zero deps" SKIP shape.
- [`tests/WALK_SCREENING.md`](./WALK_SCREENING.md) — third L1-L4 recipe application; pprof-vs-named-surface lesson.
- [`tests/ECHO_BENCH_REPORT.md`](./ECHO_BENCH_REPORT.md) — String+HashMap REGEX_HEAVY failure precedent.
- [`tests/SYMBOLS_BENCH_REPORT.md`](./SYMBOLS_BENCH_REPORT.md) — scope-split with tree-sitter Go-side, post-AST Rust-side (skim's reference pattern).
- [`tests/NEXT_MODULES_ANALYSIS.md`](./NEXT_MODULES_ANALYSIS.md) — original anti-recommendations for cli/mcp/web/tokens/git etc., still standing.
