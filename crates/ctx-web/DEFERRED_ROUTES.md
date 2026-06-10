# Deferred Web Routes (not yet ported to Rust)

These `/api/*` routes are intentionally **unregistered** in
`crates/ctx-web/src/router.rs`. Under `--web-engine rust` they fall through to
the SPA catch-all (404 for `/api/*`) — they do NOT return wrong/under-reported
data. The Go server continues to serve them. Each is deferred because it
depends on a Go module that has no Rust crate equivalent yet.

| Route | Go handler | Blocking dependency |
|-------|-----------|--------------------|
| `GET /api/git/diff` | `handleGitDiff` | `internal/git` (go-git + diffmatchpatch custom renderer) |
| `GET /api/git/file-log` | `handleFileLog` | `internal/git` (go-git committer-time log traversal) |
| `GET /api/git/commit-diff` | `handleCommitDiff` | `internal/git` (go-git + diffmatchpatch custom renderer) |

> **Note:** `/api/mix` is a DIFFERENT shape of deferral. Its GET routes (list +
> get-by-id) ARE ported and byte-parity. Its **mutations** (`POST` create,
> `DELETE` delete) are deferred but the route IS registered — Rust returns a
> deliberate **405 sentinel** for those methods, which **diverges** from Go
> (Go performs the create/delete: 201/204). See the `/api/mix` mutations
> section below — this divergence blocks cutover.

---

## `GET /api/tests` — PORTED (Wave 2)

Go handler: `handleTests` in `internal/web/handlers.go` (~line 860)
Rust crate: `crates/ctx-testinsights/src/lib.rs`
Rust handler: `crates/ctx-web/src/handlers/tests.rs`

**Status: PORTED — byte-parity GREEN ×2, 10 parity cases, real test fixtures.**

Approach: `ctx-testinsights` uses `tree-sitter-go` (ABI-14 pinned, same as
`ctx-symbols`) to replicate Go's `go/parser`+`go/ast` test-detection rules:
- `function_declaration` at source_file level with name starting `Test`,
  `Benchmark`, or `Fuzz` → `isGoTestFunc` (no `Example`, matching Go source).
- Symbol extraction mirrors `ast.FuncDecl`, `ast.TypeSpec`, `ast.ValueSpec`
  — **TOP-LEVEL declarations ONLY** (`for _, decl := range f.Decls`). The
  walk iterates `source_file`'s direct children and does NOT descend into
  function bodies, parameter lists, struct fields, or short-var-decls.
- Grouped declaration blocks are flattened to match `GenDecl.Specs`. NOTE the
  tree-sitter-go grammar asymmetry: grouped `var ( … )` wraps its `var_spec`
  nodes in an intermediate **`var_spec_list`**, whereas grouped `const ( … )`
  and `type ( … )` keep their specs as DIRECT children. The walk descends
  through any `{const,var,type}_spec_list` wrapper (`for_each_spec`). Each spec
  contributes ALL its direct `identifier` children (single + multi-name
  `a, b = 1, 2`).
- Walker mirrors `walk.DefaultOptions()`: skips `.git`, `node_modules`,
  `dist`, `coverage`; sorted entries; no symlinks.

### Regression #1 locked: `out` OVER-extraction (top-level-only fix)

A draft recursed into ALL named children, so a function PARAMETER / local like
`out` in `walkNode(..., out *[]model.Symbol, ...)` (and the test local `out`
used 7× in `extractor_test.go`) leaked into the target symbol set. This
polluted `matched_symbols` (→ `["Extract","New","out"]`), inflated scores, and
changed which test files matched (`total_tests` 60 vs Go's 12 on the full
repo). Go's `go/ast` only surfaces `f.Decls`. Fixed by restricting extraction
to top-level declarations.

Locked by `tests_extractor_complex` (`symbols/extractor.go` + real
`extractor_test.go`): asserts `matched_symbols":["Extract","New"]` (NO `out`),
`test_count":5`.

### Regression #2 locked: grouped `var ( … )` UNDER-extraction

The top-level-only fix initially OVER-corrected: it iterated `var_declaration`
direct children for `var_spec`, but grouped `var ( … )` nests them under
`var_spec_list`, so grouped-block names (`sharedEncoder`, `sharedEncoderErr`,
`sharedEncoderOnce` in `internal/tokens/counter.go`) were silently dropped.
This made `counter_test.go`'s `matched_symbols` miss `sharedEncoder` (score 95
vs Go's 100) and the `sharedEncoder`-only cross-file match (`handlers_perf_test.go`
in the real repo) disappear → `total_tests` 2 vs Go's 3. Fixed by descending
through `*_spec_list` wrappers (`for_each_spec`).

Locked by `tests_counter_grouped_var` (`tokens/counter.go` + real
`counter_test.go` + a `perf_test.go` referencing only `sharedEncoder`):
asserts `matched_symbols":["CountString","NewTiktokenCounter","sharedEncoder"]`
for counter_test.go AND `matched_symbols":["sharedEncoder"]` for perf_test.go,
`total_tests":2`.

### Verified against the Go oracle (go/ast) on real packages

`parse_go_file`'s symbol set was diffed byte-for-byte against Go's actual
`go/ast` top-level extraction (a throwaway dumper, since removed) on
`internal/symbols/extractor.go`, `internal/tokens/counter.go`, and
`internal/where/where.go` — all three IDENTICAL (no `out`; all grouped
`sharedEncoder*` present; where.go's 29 symbols match).

Locked by unit tests in `lib.rs`: `extracts_only_top_level_declarations`
(params/locals must not leak), `extracts_grouped_declaration_blocks` (grouped
var/const/type names all surface), `counts_only_top_level_test_funcs`.

> **Walker scope note (shared, pre-existing):** the Rust walker honours only
> `ExtraIgnore` (`.git`/`node_modules`/`dist`/`coverage`), NOT `.gitignore` /
> `.ctxignore`. This matches every other ported ctx-web walker (tree, dir,
> where, relations) and is sound for the parity fixtures (no `.gitignore`).
> On a REAL repo root with a `.gitignore` (e.g. excluding `target/`), Go's
> `walk.DefaultOptions()` would skip more paths than Rust → divergence on the
> full-repo file SET. This is a project-wide walker limitation tracked
> separately, not specific to testinsights. See TODO below.

Coverage (profile arg): no-profile case is parity target. Coverage parsing
is fully ported (Analyze returns Coverage when profile file exists) but no
deterministic coverprofile fixture was added; coverage parity is deferred —
see below.

#TODO(agent): wire `.gitignore`/`.ctxignore` matching into the remaining Rust walkers (ctx-web testinsights/tree/dir/where/relations, ctx-cli tree/pack, ctx-mcp) so they byte-match Go on real repos with a .gitignore (not just gitignore-less fixtures). The sabhiram/go-gitignore port now exists at `crates/ctx-cli/src/gitignore.rs` and is already wired into the where/map CLI walker (`crates/ctx-cli/src/commands/where_cmd.rs` `where_files`/`WalkIgnore`, mirroring Go `walk.New`/`visit`) — lift it to a shared crate and reuse.

---

## `GET /api/tests` — Coverage parity — DEFERRED

Coverage (`?profile=...`) is ported in the `ctx-testinsights` library (full
`readCoverage` / `parseCoverLine` / `mergeRanges` implementation). However,
no deterministic coverprofile fixture is included in the parity harness, so
coverage byte-parity is not asserted.

**Why not tested:** A coverprofile embeds module-qualified paths (e.g.
`github.com/simota/ctx/internal/testinsights/insights.go:…`) that cannot be
generated deterministically from the static fixture directory. Any fixture
coverprofile would need to be pinned to the specific module path and line
numbers of the test fixture, making it brittle to code changes.

**Unblock when:** A pinned miniature coverprofile fixture is added under
`crates/ctx-web/tests/` with absolute-path normalization in the harness
(`Norm::CoverProfile` variant) and a parity case that asserts `coverage`
fields are byte-identical.

#TODO(agent): add deterministic coverprofile fixture + Norm::CoverProfile case to assert coverage parity for /api/tests?profile=....

---

## Git Routes — DEFERRED

Routes: `GET /api/git/diff`, `GET /api/git/file-log`, `GET /api/git/commit-diff`
Go handlers: `handleGitDiff`, `handleFileLog`, `handleCommitDiff` in `internal/web/handlers.go`
Go internals: `internal/git/diff.go`, `internal/git/file_log.go`
Assessed: Wave 2 / 2026-06

**Verdict: DEFERRED — byte-parity not achievable without non-trivial renderer work.**
All three routes are currently unregistered in `crates/ctx-web/src/router.rs`.

---

## Key Finding: Custom JSON Renderer, Not `git diff` Format

The output format for `diff` and `commit-diff` is **not** standard unified diff (`git diff`). It
is a project-specific JSON envelope:

```json
{
  "path": "internal/foo.go",
  "added": false, "deleted": false, "binary": false, "no_change": false, "truncated": false,
  "lines": [
    {"type": "eq",  "text": "package foo", "old_num": 1, "new_num": 1},
    {"type": "del", "text": "old line",   "old_num": 2},
    {"type": "add", "text": "new line",                  "new_num": 2}
  ]
}
```

The renderer (`renderDiffLines` in `internal/git/diff.go`) is driven by
`github.com/sergi/go-diff/diffmatchpatch`. Specifically:

```go
// internal/git/diff.go
diffs := utildiff.Do(beforeContent, afterContent)   // line-oriented Myers diff
result.Lines, result.Truncated = renderDiffLines(diffs, maxWorktreeDiffLines)
```

`utildiff.Do` (from `go-git/v5/utils/diff`) calls `dmp.DiffLinesToRunes` +
`dmp.DiffMainRunes` — a **line-level Myers diff** where each line is mapped to a
Unicode rune. This IS algorithmically equivalent to LCS-based line diff.

## Why Byte-Parity Is Hard

### 1. Diff algorithm tie-breaking

Myers diff is deterministic given the same tie-breaking policy. `diffmatchpatch` uses a
specific heuristic (edit-distance bias, then sequence order). Rust's `similar` crate
also implements Myers diff but uses a different tie-breaking path. For any input where
the shortest edit script is **unique**, both produce identical results. For inputs with
multiple equally-short edit scripts, they may diverge.

**Empirical verification** requires a fixture of real diffs against a git repo — the
parity test fixture is not a git repo, so both servers currently return the same error
(`gogit.PlainOpen` fails → `{"error": {"code": "git_diff", ...}}`). This makes the
parity test trivially equal but does NOT prove byte-identity for real diffs.

### 2. go-git `FileLog` traversal order

`FileLog` uses `gogit.LogOptions{FileName: &path, Order: gogit.LogOrderCommitterTime}`.
go-git's committer-time traversal is implemented differently from `gix`'s topological
walk and `git log --follow`. Commit ordering may diverge when two commits share an
identical committer timestamp.

### 3. No git fixture in parity harness

The parity harness serves a static fixture directory that is not a git repository.
To properly test git route byte-parity, a pinned git fixture (bare repo or
bundled `.git/`) would be needed. This is a non-trivial addition to the harness.

## Options

| Option | Effort | Fidelity |
|--------|--------|---------|
| **A. Port `renderDiffLines` + Myers to Rust** | Medium | High if Myers tie-breaking matches; needs git fixture to verify |
| **B. Shell out to `git diff` / `git log` and map to JSON** | Low | NOT byte-identical — `git diff` produces unified diff, not the custom JSON |
| **C. Accept a format-parity carve-out** | Low | Structurally equivalent but NOT byte-identical; requires ADR amendment |
| **D. Add pinned git fixture + verify Myers parity empirically** | High | Required before any of A/B/C can be validated |

## Recommendation

Option D first, then Option A if Myers tie-breaking matches on real inputs.
- Create a small pinned git bundle under `crates/ctx-web/tests/git-fixture/`
- Add `Norm::GitDiff` case to normalize commit hashes / timestamps
- Run empirical comparison of Rust `similar::TextDiff` vs `diffmatchpatch.Do` output
- If they match for the fixture → implement handlers; if not → consider Option C with ADR

Until then, the three routes remain **unregistered** in Rust. The Go server continues
to serve them; `--web-engine rust` will 404 on these paths (SPA fallback), which is
the correct behavior (no stub returning wrong data).

---

## `/api/mix` mutations — DEFERRED

Routes: `POST /api/mix` (create), `DELETE /api/mix/<id>` (delete)
Go handlers: `handleMixCreate`, `handleMixDelete` in `internal/web/mix.go`
Go internal: `internal/mix/store.go` (`GenerateID`, `Save`, `Delete`)
Assessed: Wave 2 / 2026-06

**Verdict: DEFERRED — Go PERFORMS these mutations (201/204); Rust returns a
deliberate 405 sentinel. This is a KNOWN DIVERGENCE that BLOCKS cutover.**

The READ side (GET /api/mix list + GET /api/mix/<id> get-by-id) is fully
ported and byte-identical in Rust (73 parity cases GREEN ×2).

### ⚠️ Correction — Go does NOT 405 on POST/DELETE

`internal/web/mix.go` dispatches:

```
handleMixCollection:  GET → list,  POST   → handleMixCreate (CREATES, 201),  default → 405 "GET or POST only"
handleMixRoute:       GET → get,   DELETE → handleMixDelete (DELETES, 204),  default → 405 "GET or DELETE only"
```

So Go **supports** POST (create) and DELETE (delete) — verified empirically:

- `POST /api/mix` (Go) → `201 Created`, body is the saved Mix with a fresh
  `crypto/rand` id (e.g. `7910af99a31931fd`) and a wall-clock `created`
  timestamp (e.g. `2026-06-02T16:10:54.076479+09:00`).
- `DELETE /api/mix/<id>` (Go) → `204 No Content`, empty body.

The 405 path in Go is ONLY for genuinely-unsupported methods (PUT/PATCH/...).

### Why these two are not byte-parity-able

1. **GenerateID non-determinism** — `mix.GenerateID` uses `crypto/rand` for a
   16-char lowercase hex id; the POST-create response embeds that id plus a
   wall-clock `created`. Comparing the response body byte-for-byte is
   infeasible without injecting a deterministic id+clock source into BOTH
   servers.
2. **Write side-effects** — POST/DELETE mutate the on-disk store. The read
   harness shares ONE pinned fixture store (`tests/replay-store/ctx/mixes/`,
   two `.mix.json` files with deterministic ids + zero-nanosecond timestamps)
   between both servers, so a real create/delete would pollute the read cases.

### Current Rust behavior — deliberate 405 sentinel (DIVERGES from Go)

- `POST /api/mix` → **Rust 405** `{"error":{"code":"method_not_allowed",
  "message":"GET or POST only"}}` + `Allow: GET, POST`.
  **Go would 201-create.** ⚠️ NOT byte-parity — known divergence.
- `DELETE /api/mix/<id>` → **Rust 405** `{"error":{"code":"method_not_allowed",
  "message":"GET or DELETE only"}}` + `Allow: GET, DELETE`.
  **Go would 204-delete.** ⚠️ NOT byte-parity — known divergence.

The 405 is a deliberate "rust engine: mix mutations not yet supported"
sentinel — chosen over a wrong-output stub so `--web-engine rust` never
returns subtly-incorrect created/deleted data. It is honestly a DIVERGENCE,
not parity, and these two methods **BLOCK cutover** until mix mutations are
ported.

### What IS byte-parity (the genuine default-405 path)

`PUT /api/mix` and `PUT /api/mix/<id>` (and any other non-CRUD method) hit the
default-405 branch on BOTH servers with the identical envelope + `Allow`
header. These ARE true byte-parity and are asserted by the
`mix_collection_put_rejected` and `mix_item_put_rejected` parity cases. The
POST-create / DELETE-delete operations are deliberately NOT tested for parity.

**Unblock cutover when:**
1. The parity harness gains per-test server isolation (separate fixture dirs
   per case) so write operations don't pollute read tests.
2. A deterministic id + clock source is injected (e.g. through `AppState`) so
   POST-create responses can be compared byte-for-byte.
3. POST `/api/mix` (create) and DELETE `/api/mix/<id>` (delete) are ported in
   Rust; the inlined mix store in `handlers/mix.rs` gains `Save`/`Delete`.
4. Optionally, the Rust mix store (currently inlined in `ctx-web`) is promoted
   to a standalone `ctx-mix` crate if CLI commands need it.

#TODO(agent): port POST /api/mix (create) + DELETE /api/mix/<id> (delete) — Go performs them (201/204); Rust currently 405-diverges. Needs isolated write fixtures + deterministic id/clock injection before cutover.

---

## Files Changed

- `crates/ctx-testinsights/src/lib.rs` — new crate: port of `internal/testinsights` (Wave 2)
- `crates/ctx-testinsights/Cargo.toml` — new crate manifest (tree-sitter-go pinned)
- `crates/ctx-web/src/handlers/tests.rs` — new handler: `GET /api/tests` (Wave 2)
- `crates/ctx-web/src/router.rs` — `/api/tests` now registered; git routes absent; mix GET routes registered
- `crates/ctx-web/src/handlers/mix.rs` — mix read-side handler (GET list + GET by id); POST/DELETE return a deliberate 405 sentinel that DIVERGES from Go (Go=201/204) and blocks cutover
- `crates/ctx-web/tests/fixtures/gocode/add.go` + `add_test.go` — simple Go source+test fixture (`GocodeUniqueSum`, unique name to avoid cross-fixture identifier collisions)
- `crates/ctx-web/tests/fixtures/symbols/extractor.go` + `extractor_test.go` + `lookup.go` — REAL `internal/symbols` files copied verbatim; regression #1 lock (test uses local `out`)
- `crates/ctx-web/tests/fixtures/tokens/counter.go` + `counter_test.go` + `perf_test.go` — REAL `internal/tokens/counter.go` (grouped `var ( … )` block) + its real test + a synthetic `perf_test.go` referencing only `sharedEncoder`; regression #2 lock (grouped-var under-extraction)
- `crates/ctx-web/tests/replay-store/ctx/mixes/` — pinned fixture mix store (2 entries)
- `crates/ctx-web/tests/parity.rs` — 10 /api/tests cases incl. `tests_extractor_complex` (regression #1) + `tests_counter_grouped_var` (regression #2) with expect_contains guards on real matched_symbols/test_count/total_tests; added HTTP chunked-transfer de-chunking in the test client (Go chunks large responses, Rust uses Content-Length — transport detail the harness ignores); mix READ cases; 2 genuine default-405 PUT cases
- `crates/ctx-web/DEFERRED_ROUTES.md` — this document (strategy for orchestrator)
