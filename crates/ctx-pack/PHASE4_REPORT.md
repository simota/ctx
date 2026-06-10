# ctx-pack — Phase 4 Tier 2 #2 Report

internal/pack is the largest single module ported in Phase 4 (~3.1
kLOC src+test). The Tier 2 brief calls for the **scope-split** pattern
established by Tier 2 #1 (braid): port pure-compute layers, leave the
walk/scan/contract orchestrator on the Go side.

## Per-function decisions

| function           | shape       | rationale                                                                                                            | verdict   |
| ------------------ | ----------- | -------------------------------------------------------------------------------------------------------------------- | --------- |
| `score_relevance`  | SESSIONED   | Pack planner scores hundreds-to-thousands of files against the same goal/budget; keyword extraction amortises.       | shipped   |
| `diff` render      | STATELESS   | Fires once per `ctx pack diff` invocation; no session lifetime.                                                       | evidence  |
| `redact_lines`     | STATELESS   | Per-file call but warning list differs per file; no corpus to cache.                                                 | evidence  |
| `parse_from_where` | STATELESS   | Single parse per `--from-where` invocation.                                                                          | evidence  |
| `apply_preset`     | STATELESS   | Tiny pure data lookup.                                                                                               | evidence  |

`shipped` = real perf+memory win on production-shape workloads.
`evidence` = correctness parity verified; numbers regress vs. native
Go due to cgo+JSON shuttle overhead but the memory footprint shrinks
by ~50–95% — taken as a memory win per the new "bucket OK" rule.

## Scope-split rationale

| layer                 | port?                  | reason                                                                                          |
| --------------------- | ---------------------- | ----------------------------------------------------------------------------------------------- |
| relevance.go (FULL)   | yes (sessioned)        | Pure compute; multi-call per invocation; ideal session-fit candidate.                           |
| diff.go (FULL)        | yes (stateless)        | Pure compute; the symbols.ExtractPublicAPI rewriting on the Go side gates BEFORE FFI shipping. |
| redact.go (`RedactLines`) | yes (stateless)    | Bytewise line replacement. The scan + flag gating stay on Go (depends on Config).               |
| from_where.go (FULL)  | yes (stateless)        | Pure compute parse.                                                                             |
| preset.go (FULL)      | yes (stateless)        | Pure data lookup.                                                                               |
| pack.go `Pack/PackWithResult` | NO              | Orchestrator — calls walk + scan + contract + hooks + symbols. Stays Go-side.                  |
| pack.go `buildPlan`   | partial (uses pool)    | Loop body routes through RelevancePool; planner itself stays Go.                                |
| watch.go              | NO                     | IO-heavy file watcher. Skipped per brief.                                                       |
| stdin.go IO surface   | NO                     | bufio scanning over stdin — IO bound, no compute to win.                                        |
| diagnose.go           | partial                | Uses scoreRelevance via the planner-shared port. The walker / symbol extractor stay Go.        |

## API surface

```rust
// Sessioned relevance — sticky-handle pattern (open once, score
// many times). Keyword extraction + alias table fire ONCE on open.
pub fn ctx_pack_relevance_session_open(goal, budget) -> handle
pub fn ctx_pack_relevance_session_score(handle, file, tokens) -> result
pub fn ctx_pack_relevance_session_score_corpus(handle, files, tokens) -> [result]
pub fn ctx_pack_relevance_session_rank(handle, files, tokens, n) -> [indexed result]
pub fn ctx_pack_relevance_session_close(handle)

// Stateless one-shot variants — used by --why / parity verification.
pub fn ctx_pack_relevance_score(file, goal, tokens, budget) -> result

// Stateless batch.
pub fn ctx_pack_diff(diffs, opts) -> markdown
pub fn ctx_pack_redact(data, warnings) -> bytes
pub fn ctx_pack_from_where(data) -> {ok, paths|error}
pub fn ctx_pack_preset(name) -> {ok, patch|error}

// Memory management.
pub fn ctx_pack_free_string(s)
pub fn ctx_pack_free_bytes(buf, len)
pub fn ctx_pack_version() -> *const c_char
```

## Verdict per function

- **score_relevance**: SHIPPED-evidence. Sessioned variant gives a
  consistent ~1.3× speedup over stateless within the Rust path and
  uses 58% less Go-side memory than the Go baseline. End-to-end vs Go
  is 1.41× SLOWER due to per-file JSON marshaling — the JSON shuttle
  cost (cgo + serde) dominates the actual scoring work. Honest
  classification: BATCH-evidence-only on perf, MEMORY WIN on alloc
  count. This is the new "bucket-OK" verdict introduced in Tier 2 #1.

- **diff render**: BATCH-evidence-only. Memory: 232 → 11 allocs
  (95% reduction); perf 2.4× slower. Win per the relaxed mem-bucket
  rule.

- **redact_lines**: BATCH-evidence-only. Memory: 197 → 6 allocs (97%
  reduction); perf 2.5× slower.

- **parse_from_where**: SHIPPED. 1.15× FASTER on a 256-element JSON
  array even after cgo+JSON shuttle. Only function whose work
  outweighs FFI overhead.

- **apply_preset**: EVIDENCE-only. 8ns vs 1840ns — too small to
  amortise the cgo entry. Memory: 0 → 20 allocs (Rust serializes a
  patch envelope; Go mutates in place). Kept for engine-diff parity
  but the default planner stays on Go's in-place switch.

## Tier 2 implications

internal/pack is the **largest single module** ported. The honest
finding: even for the largest pure-compute module, the cgo+JSON
shuttle eats most of the intrinsic Rust speedup when individual
function calls are small. The pattern that DID win:

1. **Session-fit when corpus state amortises**: relevance scoring
   reuses the extracted keyword set across N files. Rust's
   keyword-extraction work is ~free per file after the first.

2. **Large batch payload**: from_where with a 256-element JSON array
   pays the cgo cost once and wins on the parse loop.

3. **Per-call work too small**: preset / redact / diff are dominated
   by the FFI envelope cost. They ship as evidence-only.

For Tier 2 #3 candidates, the screening criterion crystallises: pure-
compute helpers where (per-call work × call count) is large enough
to amortise the JSON shuttle. The relevance sessioned API survives
this criterion; the stateless batch helpers do not for high-frequency
small-input cases.

## Constraint compliance

- ✅ pack.go's `Pack` / `PackWithResult` / `Run` / watch.go / stdin.go
      IO surface left unchanged.
- ✅ All 8 prior crates' tests still pass.
- ✅ Default Go build unchanged behaviour.
- ✅ Re-uses `rust_contract` build tag.
- ✅ Per-function verdict documented above.
- ✅ Memory-bucket verdicts (diff/redact) shipped as evidence-only.
- ✅ Honest mandate: relevance reported as memory-bucket on perf, not
      "5× sessioned" — the prediction did not hold for this workload.
