# go-mcp loop — MCP server Go→Rust byte-parity (Codex executor)

Hardened nexus-autoloop runner for the largest remaining Wave 2 unit: the MCP
server (`internal/mcp` ~5333 LOC → native `crates/ctx-mcp`). Drives the PINNED
differential oracle by its passing-case COUNT via Codex CLI — codex accumulates
work in a worktree until all 22 cases are byte-green. Terminates only on
external limits.

## Why this runner is hardened (failure modes found live)
The earlier `go-elimination` loop failed structurally, and the first launches of
THIS loop surfaced more. Each fix is baked in and labelled in `run-loop.sh`:

| ID | Failure | Fix here |
|----|---------|----------|
| B1 | `codex exec --full-auto` → sandbox not pinned | `codex exec -s workspace-write` hard-coded; `--dangerously-bypass-*` never used |
| B2 | ran on `$PWD` (=main); orphan codex mutated main | dedicated `git worktree`; codex PID+children killed on exit (no `setsid` — Linux-only); main never touched. codex stdin `</dev/null`; stdout → per-iter log |
| B5 | model-critic gave unstable verdicts + its revert WIPED partial work on large all-or-nothing cases (tools/list = 9 schemas) every iteration → never converged | NO model-critic: the **pinned Go oracle is the judge** (green = genuine byte-parity, codex can't fake a sha256-pinned test). Work is ACCUMULATED (committed each verify-passing iter, reverted only on regression) so large cases build across iters |
| B6 | a no-progress counter on mcp-count would false-BLOCK legitimate multi-iter accumulation | stall guard fires only when codex produces NO change for `NOWORK_LIMIT` iters |

## Order of operations (do NOT launch out of order)
1. **PHASE A (orchestrated, human-supervised):** author
   `crates/ctx-mcp/tests/parity.rs` — the differential JSON-RPC oracle (boots
   both `ctx-go mcp serve` and Rust `ctx-mcp` over stdio, byte-compares a fixed
   request corpus). Confirm it is **RED** against the current draft (proves it
   discriminates). This file is the immutable oracle — the loop may never edit it.
2. `bash loops/go-mcp/bootstrap.sh` — pin gate files + baseline counts.
3. Merge Phase A (scripts + oracle) to `main` so the worktree (off `origin/main`)
   contains them.
4. `bash loops/go-mcp/run-loop.sh` — launch. Watch `runner.log`.

## Gate model (count-driven accumulation)
The MCP oracle (`tests/parity.rs`) is 22 per-case `#[test]`s, pinned and RED
(3/22) at launch. Each iteration codex is handed the live failing-case list and
picks one to complete, building on prior committed work. Per iteration:
- `verify.sh` (safety): cli/web/symbols stay fully green; mcp passing-count `>=`
  the pinned floor; Go-untouched + go-build + placeholder grep.
- the RUNNER then compares the mcp passing-count before/after:
  - `< prev` (regression) → revert this iteration, retry;
  - no worktree change → stall counter (BLOCK at `NOWORK_LIMIT`);
  - else → commit (accumulate); `> prev` is logged as a byte-green flip.
- DONE when mcp passing-count `== BASE_MCP_TOTAL` (all 22). The pinned oracle
  makes a green case ungameable, so the count IS the parity proof — no false-DONE.

## Safety envelope
`MAX_ITERATIONS=20`, `CIRCUIT_THRESHOLD=3` (verify FAILs), `NOWORK_LIMIT=3`
(no-change iters), `USD_PER_RUN_CAP=40` (PAUSE, no auto-resume — note: nominal,
`.usd_spent` is not yet wired; real bound is the iteration cap + monitoring),
`RETRY_LIMIT=2` exponential backoff, gate files sha256-pinned.

## Recovery
`recover.sh --reset-circuit | --reset-state | --drop-worktree`
