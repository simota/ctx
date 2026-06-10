# Next-Module Port Analysis (post-pioneer)

T-Summit deliverable. Ranks the next 5-7 `internal/*` modules for Go→Rust
port, anchored to empirical numbers from `tests/BENCH_REPORT.md` (the
`internal/contract` pioneer). Read-only static analysis — no source files
modified.

## Methodology

### Inputs

For every package under `internal/`, we collected via `find` / `grep` / `wc`:

- **LOC** (non-test `.go` lines, `wc -l` excluding `*_test.go`)
- **File count**
- **Internal deps**: distinct `github.com/simota/ctx/internal/*` imports
- **Inbound callers**: distinct packages (under `internal/` or `cmd/`)
  that import this module (proxy for hot-path centrality)
- **Regex intensity**: occurrences of `regexp.` / `MustCompile`
- **JSON intensity**: occurrences of `json.Marshal` / `Unmarshal` / `encoding/json`
- **I/O intensity**: occurrences of `os.ReadFile|Open|Create|WriteFile`,
  `ioutil.*`, `filepath.Walk`
- **Subprocess intensity**: `exec.Command|Cmd`
- **Concurrency intensity**: `go func`, `sync.Mutex|RWMutex|WaitGroup`, `chan `
- **Public API surface**: count of `^func [A-Z]` + `^type [A-Z]`
- **Stateful indicators**: `init()` count + package-level `var` blocks

### Workload classification rubric

A module is tagged with the dominant shape that explains its tight loop:

| Tag                | Trigger                                                    |
|--------------------|------------------------------------------------------------|
| `REGEX_HEAVY`      | ≥3 compiled regexes used in `FindAll*` / per-line loops    |
| `JSON_HEAVY`       | ≥5 `Marshal`/`Unmarshal` calls in a critical path          |
| `LOOKUP_HEAVY`     | Map-driven set membership / scoring (verify-orchestrator shape) |
| `IO_HEAVY`         | File traversal / disk reads dominate the wall-clock        |
| `CONCURRENCY_HEAVY`| Goroutines / channels are the workload, not glue           |
| `GLUE`             | Mostly orchestration; little intrinsic compute             |

A module can carry one primary tag and one secondary; we choose the shape
that the BENCH_REPORT model predicts the highest speedup for.

### Expected-speedup model (calibrated to pioneer benches)

Derived from `tests/BENCH_REPORT.md`:

| Workload shape       | Expected speedup     | Pioneer evidence                                            |
|----------------------|----------------------|-------------------------------------------------------------|
| `REGEX_HEAVY`        | **~7-9×**            | ExtractReferences 7.07-7.50×; ParseFromPack markdown 9.27×  |
| `JSON_HEAVY`         | **~7×**              | ParseFromPack json 7.03× (`serde_json` vs `encoding/json`)  |
| `LOOKUP_HEAVY`       | **~1.85×**           | Verify 1.85× (map/bookkeeping)                              |
| `IO_HEAVY`           | <1.5× (disk-bound)   | I/O bottleneck dominates; cgo overhead taints small calls   |
| `CONCURRENCY_HEAVY`  | unclear, often wash  | Tokio vs goroutines roughly comparable on CPU-bound work    |
| `GLUE`               | NEGATIVE             | cgo overhead (~1-2 µs/call) > intrinsic work                |

The pioneer also taught us that **batch-style APIs** (one cgo call per
collection of inputs) keep the ~1-2 µs FFI cost amortised. Fine-grained
APIs (per-line / per-symbol / per-token cgo crossings) bleed the speedup
away on small inputs.

## Module inventory table

| Module          | LOC  | Files | Internal deps | Inbound | Regex | JSON | I/O | Exec | Conc | API | Workload shape                |
|-----------------|------|-------|---------------|---------|-------|------|-----|------|------|-----|-------------------------------|
| audit           |  277 |  5    | 0             |  2      |  1    |  4   |  3  |  0   |  0   |  6  | GLUE                          |
| braid           | 1059 |  7    | 8             |  1      |  2    |  2   |  2  |  0   |  0   | 19  | GLUE                          |
| budget          |  135 |  1    | 1             |  4      |  0    |  0   |  0  |  0   |  0   |  3  | GLUE                          |
| cli             | 4140 | 26    | 26            |  1      |  3    | 12   |  6  |  4   |  4   |  3  | GLUE                          |
| config          |  479 |  2    | 0             |  7      |  4    |  0   |  1  |  0   |  0   | 21  | GLUE (regex is compile-only)  |
| **contract**    | 1229 |  8    | 1             |  4      |  5    | 18   |  1  |  0   |  0   | 32  | **PIONEER (already ported)**  |
| digest          |  579 |  3    | 1             |  3      |  0    |  2   |  0  |  0   |  0   |  9  | GLUE (tree-sitter wrapper)    |
| echo            |  800 |  5    | 0             |  1      |  1    |  2   |  1  |  0   |  0   | 12  | GLUE                          |
| **focus**       |  387 |  1    | 3             |  3      |  4    |  0   |  1  |  0   |  0   |  8  | LOOKUP_HEAVY + light regex    |
| git             |  596 |  4    | 1             |  5      |  0    |  0   |  1  |  0   |  0   | 12  | IO_HEAVY (go-git wrapper)     |
| heatmap         |  914 |  5    | 1             |  1      |  0    |  2   |  0  |  0   |  0   | 13  | LOOKUP_HEAVY (aggregation)    |
| hooks           |   97 |  1    | 0             |  1      |  0    |  4   |  0  |  4   |  0   |  4  | GLUE                          |
| mcp             | 2918 |  6    | 14            |  1      |  1    | 21   |  3  |  0   |  2   |  3  | JSON_HEAVY *but* CONCURRENCY+IO bound (JSON-RPC server) |
| mix             |  271 |  2    | 0             |  2      |  0    |  3   |  2  |  0   |  0   |  7  | GLUE                          |
| model           |   89 |  1    | 0             | 18      |  0    |  0   |  0  |  0   |  0   |  7  | GLUE (data types only)        |
| noise           |  230 |  1    | 4             |  1      |  0    |  0   |  0  |  0   |  0   |  5  | GLUE                          |
| onboarding      |  504 |  1    | 3             |  1      |  3    |  2   |  3  |  0   |  0   |  7  | GLUE                          |
| pack            | 2182 |  9    | 10            |  4      |  1    |  4   |  5  |  0   |  1   | 18  | IO_HEAVY + JSON_HEAVY (mixed) |
| **relations**   | 1318 |  5    | 2             |  1      | 12    |  4   |  9  |  0   |  1   |  5  | **REGEX_HEAVY** + IO          |
| render          |  556 |  4    | 2             |  1      |  0    |  4   |  0  |  0   |  0   | 14  | JSON_HEAVY (small payloads)   |
| replay          |  950 |  4    | 0             |  2      |  0    |  5   |  3  |  0   |  0   | 28  | JSON_HEAVY + IO               |
| **scan**        |  218 |  3    | 2             |  2      | 16    |  0   |  1  |  0   |  0   |  6  | **REGEX_HEAVY** (16 patterns/file) |
| security        |   17 |  1    | 1             |  1      |  0    |  0   |  0  |  0   |  0   |  1  | GLUE (constants)              |
| skim            |  327 |  1    | 2             |  2      |  0    |  0   |  1  |  0   |  0   |  5  | GLUE                          |
| symbols         |  566 |  3    | 2             |  9      |  0    |  0   |  2  |  0   |  0   |  9  | (tree-sitter; see note)       |
| testinsights    |  519 |  1    | 1             |  1      |  0    |  0   |  3  |  0   |  0   |  6  | GLUE                          |
| tokens          |   90 |  2    | 0             |  9      |  0    |  0   |  1  |  0   |  0   |  6  | GLUE (tiktoken wrapper)       |
| tui             |  304 |  1    | 4             |  1      |  0    |  0   |  1  |  0   |  0   |  3  | GLUE                          |
| walk            |  728 |  3    | 2             | 13      |  0    |  0   |  2  |  1   |  0   | 11  | IO_HEAVY (gitignore traversal)|
| web             | 3394 | 12    | 13            |  1      |  1    |  9   |  3  |  4   |  7   | 54  | CONCURRENCY_HEAVY (HTTP server) |
| **where**       | 1110 |  1    | 3             |  5      |  3    |  0   |  1  |  0   |  0   | 10  | **LOOKUP_HEAVY** + regex      |

Bold rows = top porting candidates. The pioneer row is bolded for
reference, not re-port.

## Notes on borderline modules

- **`symbols`** uses `go-tree-sitter` cgo bindings to C grammars. A Rust
  port would swap to the same C grammars via `tree-sitter` crate — the
  intrinsic parsing work is in C either way, so the per-call delta is
  small. **Anti-recommend** unless we also rewrite the dispatch logic
  that loops over file batches. Inbound=9 is misleading: the inbound
  callers consume small per-file results, not bulk scans.
- **`mcp`** has the highest JSON count (21) but it's a long-lived
  JSON-RPC stdio server. Per-message marshalling is dwarfed by the
  process-IO model and the request handlers themselves. **Anti-recommend**.
- **`web`** has goroutines (7) and a 54-type API surface — porting cost
  would dwarf the gain. **Anti-recommend**.
- **`tokens`** wraps `tiktoken-go`. A Rust port would re-use `tiktoken-rs`
  (same BPE merge table format) — the encoder is already a fast loop. The
  inbound count (9) is high but each call is small; pioneer FFI cost
  (~1-2 µs) is comparable to a single short Encode call. **Defer until
  pack-level batching is in place.**

## Ranked candidates (top 7)

### #1: `internal/scan`

- **Workload shape**: REGEX_HEAVY (16 compiled patterns, every line of every file)
- **Expected speedup**: **~7-9×** (matches pioneer's ExtractReferences profile)
- **LOC**: 218, **Inbound callers**: 2, **Internal deps**: 2
- **Why port**: Smallest LOC of any top candidate, 16 regex patterns run
  over every file scanned for secret detection — exactly the shape the
  Rust `regex` crate beat Go's by 7-9× in the pioneer bench. Already has
  a clean batch entry point (`ScanFiles(paths []string)`) so cgo overhead
  is amortised across the whole walk. The patterns are static (defined
  in `secret.go` / `env_patterns.go`) — no user-supplied regex to
  re-compile per call, which lets the Rust side `lazy_static` the
  compiled set.
- **FFI fit**: GOOD — `ScanFiles` already takes `[]string`, returns
  `[]model.Warning` (cleanly JSON-encodable for parity). `ScanFile` is
  also available for single-file calls.
- **Risks**: (1) `entropy.go` adds Shannon-entropy gating that needs a
  byte-for-byte reproduction in Rust to keep parity stable; (2) allowlist
  semantics from `Options` must round-trip exactly.
- **Estimated effort**: **SMALL** (1.5-2 days). Smaller than the pioneer
  (218 vs 1229 LOC), no contract semantics to preserve.
- **Strategic value**: **HIGH** — runs on every `ctx pack`, `ctx scan`,
  and MCP `pack` call. Direct user-visible latency win.

### #2: `internal/relations`

- **Workload shape**: REGEX_HEAVY (12 patterns across JS/TS/Svelte/Python/PHP/Java/Kotlin/Swift) + light IO
- **Expected speedup**: **~5-7×** on parse hot path; **~2-3×** end-to-end (file I/O floor)
- **LOC**: 1318, **Inbound callers**: 1 (`web` server), **Internal deps**: 2
- **Why port**: 12 regex patterns scan every JS/TS/Svelte/Python/PHP/Java/
  Kotlin/Swift file to extract imports. Same shape as ExtractReferences
  (regex-over-multi-KB-string). The Go side parses every supported file
  in the repo on `web` server load — measurable user-facing latency.
- **FFI fit**: MEDIUM — `Build(root string) (*Index, error)` is a single
  call returning a graph, but the function itself does directory walks,
  TOML/JSON probe for `composer.json`, and FQN index assembly. Cleanest
  factoring is to port the **per-file extractors** (regex + resolution)
  as batch calls and keep the Go orchestration. Saves the language probe
  logic and SPM detection in Go where filesystem assumptions belong.
- **Risks**: (1) PSR-4 prefix matching and SPM layout detection are
  fiddly — full parity needs exhaustive test fixtures; (2) the Go AST
  parser path (`go/parser` for `.go` files) cannot be replicated — must
  stay in Go and remain the dispatch point.
- **Estimated effort**: **MEDIUM** (4-5 days)
- **Strategic value**: **MEDIUM-HIGH** — `web` is the API surface most
  exposed to interactive use; faster graph builds = faster page loads.

### #3: `internal/where`

- **Workload shape**: LOOKUP_HEAVY (token maps, scoring) + light regex
- **Expected speedup**: **~1.85× - 2.5×** (Verify-shape baseline + small regex bonus)
- **LOC**: 1110, **Inbound callers**: 5, **Internal deps**: 3
- **Why port**: 1110 LOC single-file scoring engine with heavy map use
  (`tokenMap`, `baseHit`, `reasons` per-call). This is the closest
  analogue to the pioneer's Verify path — same lookup-heavy shape that
  hit 1.85× there. Bonus: query token regex (`queryTokenRE`) and per-line
  match regex contribute a small additional regex-over-string win.
  Inbound=5 — used by `web`, MCP, CLI, focus, and more.
- **FFI fit**: GOOD — `Search(root, query, limit)` / `SearchWithOptions`
  are clean batch calls returning slices. Inputs and outputs are
  JSON-encodable today (already exposed by web/MCP).
- **Risks**: (1) Levenshtein implementation must be byte-for-byte for
  consistent `SuggestSimilar` ranking; (2) synonym expansion currently
  loads from `config.WhereConfig` — port needs synonym table to be
  passed in at call time, not read from Go-side config.
- **Estimated effort**: **MEDIUM-LARGE** (5-7 days) — large single file
  to translate, but no external deps and no FS walking inside scoring.
- **Strategic value**: **HIGH** — `ctx where` is the primary lookup
  surface for interactive use; 1.85× there is a perceptible latency win.

### #4: `internal/replay`

- **Workload shape**: JSON_HEAVY (5 marshal/unmarshal sites, 28-type API)
- **Expected speedup**: **~5-7×** on serialisation; **~3-4×** end-to-end (disk read floor)
- **LOC**: 950, **Inbound callers**: 2, **Internal deps**: 0
- **Why port**: Zero internal deps (highly self-contained), 28
  public types means a well-formed record/event model that `serde` will
  trivialise. JSON throughput is the pioneer's second-best win shape
  (7×). Web verification path (recently merged in #61 area) calls into
  replay snapshot loading on every contract verify.
- **FFI fit**: GOOD — replay records are pure data, no external state.
  Batch read of a snapshot file → batch decode → return records.
- **Risks**: (1) The 28-type API surface is large; each `type` must
  round-trip via `serde` with field-name parity; (2) some records may
  embed timestamps with a specific format — need explicit format pinning.
- **Estimated effort**: **MEDIUM** (4 days) — straightforward port; the
  effort is mostly in the type-by-type `serde` derivations and tests.
- **Strategic value**: **MEDIUM** — feeds the web verification path,
  which is a recently expanded user-visible surface.

### #5: `internal/focus`

- **Workload shape**: LOOKUP_HEAVY (graph expansion BFS over relations) + light regex
- **Expected speedup**: **~1.85× - 2.2×** (Verify-shape with map/set bookkeeping)
- **LOC**: 387, **Inbound callers**: 3, **Internal deps**: 3
- **Why port**: Per-anchor neighbourhood expansion (BFS over the
  relations graph) with map-driven scoring. Hot path for both `ctx focus`
  CLI and MCP `focus` tool. Small LOC = low porting cost.
- **FFI fit**: GOOD — `Expand(root, anchor, opts)` is a clean batch call.
  The anchor model and `FileInfo` results JSON-encode cleanly.
- **Risks**: (1) Depends on `relations.Index` shape — if relations is
  ported in #2 first, focus can consume the Rust-side graph directly,
  skipping a serialisation round-trip. **Order #2 before #5.**
- **Estimated effort**: **SMALL-MEDIUM** (2-3 days)
- **Strategic value**: **MEDIUM** — composes nicely with #2 (relations);
  the combined `relations + focus` Rust path keeps the graph in Rust
  memory for the whole expansion.

### #6: `internal/render`

- **Workload shape**: JSON_HEAVY (4 marshal sites, small payloads)
- **Expected speedup**: **~3-5×** intrinsic; **<2×** end-to-end (small payload, cgo dominates)
- **LOC**: 556, **Inbound callers**: 1, **Internal deps**: 2
- **Why port**: JSON writers for tree/budget/plain render formats. Used
  on every CLI invocation that returns structured output.
- **FFI fit**: MEDIUM — payloads are usually small (treemaps, budgets),
  so the fixed cgo overhead is a meaningful fraction. Best if grouped
  with caller so cgo is invoked once.
- **Risks**: (1) Output format parity across CLI consumers — character-
  for-character regression risk for users who pipe into other tools;
  (2) Small payloads = cgo cost matters; the speedup may not be visible
  until payload sizes grow.
- **Estimated effort**: **SMALL** (2 days)
- **Strategic value**: **LOW-MEDIUM** — keep as a Phase 3 cleanup item
  rather than a strategic win.

### #7: `internal/heatmap`

- **Workload shape**: LOOKUP_HEAVY (squarified treemap layout + per-directory aggregation)
- **Expected speedup**: **~1.85× - 2×**
- **LOC**: 914, **Inbound callers**: 1, **Internal deps**: 1
- **Why port**: Squarified treemap is a recursive aggregation with map
  bookkeeping — Verify-shape work. Currently invoked from `ctx map`
  CLI, but the algorithm is naturally CPU-bound on large repos.
- **FFI fit**: GOOD — `Aggregate` + layout are pure functions over
  pre-walked file info.
- **Risks**: (1) Floating-point determinism for layout — must match Go's
  ordering exactly to avoid visual diff in ASCII renderer; (2) Inbound=1
  means the strategic value is bounded.
- **Estimated effort**: **MEDIUM** (3-4 days)
- **Strategic value**: **LOW** — `ctx map` is not a hot path. Include
  only after the higher-impact ports.

## Anti-recommendations (modules NOT to port)

| Module          | Reason                                                                                              |
|-----------------|-----------------------------------------------------------------------------------------------------|
| `model`         | Pure data types (89 LOC). Mirror in Rust as `serde`-derived structs *as part of other ports*, never a standalone port. Inbound=18 is a structural artifact, not a hot path. |
| `walk`          | IO-bound (`os.ReadDir` syscalls dominate). Go's `filepath.Walk` is already a thin syscall wrapper; Rust would offer ≤1.5×. Plus go-git and ctxgit deps add porting cost without runtime benefit. |
| `git`           | Wraps `go-git` (a complete Go-side git library). Porting requires either re-implementing on libgit2 or building Rust bindings against go-git's wire protocol — both vastly more work than the speedup justifies. |
| `tokens`        | Per-call work is small (BPE encode of a single string); cgo overhead would dominate. Defer until pack-level batching exists. The Go `tiktoken-go` library is already competitive. |
| `symbols`       | Tree-sitter parsing happens in C either way; both bindings (Go and Rust) marshal across cgo. Inbound=9 hides that each call is small. No intrinsic speedup. |
| `mcp`           | Long-lived stdio JSON-RPC server. The JSON-marshalling cost is dwarfed by process-IO and handler dispatch; the cgo bridge from Go would *add* latency to every request. |
| `web`           | HTTP server with goroutines and a 54-type API surface. Porting cost (Tokio/Axum re-impl, route-by-route parity, auth, websocket) is months of work for a wash on perf (network IO floor). |
| `cli`           | 4140 LOC of orchestration. Pure GLUE — porting would *remove* the Go ergonomics (Cobra, flag parsing, terminal detection) without any speedup. |
| `audit`, `digest`, `noise`, `skim`, `mix`, `echo`, `braid`, `budget`, `hooks`, `onboarding`, `tui`, `testinsights`, `config`, `security` | All GLUE-shape or low inbound. Negative ROI on cgo overhead. |

## Migration sequence proposal

```
Phase 1 (next 1-3 months) — high-confidence regex/lookup wins:
  #1 scan        SMALL    HIGH       (~7-9× confirm on real secret scans)
  #2 relations   MEDIUM   MEDIUM-HIGH (~5-7× confirm on JS/TS-heavy repos)
  #3 where       MEDIUM-L HIGH       (~1.85-2.5× confirm; user-visible)

Phase 2 (months 4-6) — composable / data-heavy:
  #5 focus       SMALL-M  MEDIUM     (chain w/ #2 — Rust graph stays in memory)
  #4 replay      MEDIUM   MEDIUM     (~5-7× JSON; web verify path)

Phase 3 (months 7+) — re-evaluate before committing:
  #6 render      SMALL    LOW-M
  #7 heatmap     MEDIUM   LOW
  re-check tokens / symbols once pack-level batching exists
```

**Sequencing rationale:**

1. `scan` first → smallest LOC, biggest expected speedup, validates the
   pioneer's pattern at a new module.
2. `relations` second → biggest regex-pattern surface, opens the door
   for `focus` to consume Rust-side graphs in Phase 2.
3. `where` third → only LOOKUP_HEAVY candidate with high inbound; ships
   the user-visible search latency win.
4. After Phase 1, **re-baseline the bench report**. If the model's
   ~1.85× LOOKUP_HEAVY prediction holds, commit Phase 2; if not, pull
   `#5 focus` out and re-evaluate.

## Open questions for product/eng leadership

1. **Target platform matrix for cgo**: pioneer ships cgo-on-by-default
   for darwin-arm64 and linux-amd64. Do we extend to windows-amd64 and
   linux-arm64 before Phase 2? Each new platform multiplies the CI cost
   and adds Rust toolchain provisioning to the release pipeline.

2. **Pure-Go fallback policy**: pioneer keeps a pure-Go path behind a
   build tag (`embed.go` / `dispatch.go`). Should subsequent ports do
   the same, or can we deprecate pure-Go fallback once cgo coverage is
   complete? Pure-Go fallback adds maintenance burden but is a
   compelling story for `go install` users.

3. **Dev experience for non-Rust contributors**: Phase 1 puts 3 more
   modules behind a Rust crate. What's the contribution policy for
   contributors who don't have a Rust toolchain installed? Pioneer has
   not yet been stress-tested by an external PR — first one will
   set precedent.

4. **Rust hiring / training**: who owns the Rust crates long-term?
   Currently one author. Phase 2 expansion implies at least 2-3 engineers
   need to be comfortable in the Rust codebase. Plan training/onboarding
   alongside the schedule.

5. **Benchmark CI**: pioneer's BENCH_REPORT is a one-time manual run.
   Should subsequent ports require a per-PR regression check (criterion
   + benchstat) to prevent silent perf regressions? This is cheap to add
   now and expensive to add later.

## Caveats and methodology limits

- **Static grep is a proxy, not a profile.** The regex / JSON / IO
  counts are *occurrences in source*, not runtime calls. We haven't
  attached pprof to a representative workload yet; that's a separate
  T-task. The pioneer's bench-based win predictions hold *if* the
  regex/JSON occurrences correspond to real hot paths — which the source
  reading above corroborates for `scan`, `relations`, `where`, `replay`,
  `focus`, but should be confirmed with pprof before committing Phase 1.
- **The 1-2 µs cgo cost is amortised over batch calls.** Speedups
  on tiny inputs will be smaller than the intrinsic Rust numbers; the
  pioneer's BENCH_REPORT deliberately excluded the cgo bridge for the
  same reason. Real `ctx pack` end-to-end measurements should follow
  Phase 1 to validate.
- **Memory reduction is not quantified.** The pioneer report explicitly
  did not measure Rust memory; we should add `dhat-rs` or `jemalloc-stats`
  to the next port's bench so we can claim the charter's 30%-memory
  alternative target rigorously.
