# go-git loop — git web routes Go→Rust byte-parity (Codex executor)

Count-driven accumulation runner (same hardened template as `loops/go-mcp`,
which drove the MCP port to 22/22). Drives the PINNED oracle
`crates/ctx-web/tests/git_parity.rs` (13 per-case HTTP differential tests) by
its passing-case COUNT until all 3 git routes are byte-parity. DONE = git 13/13.

## Hardening (inherited from go-mcp; see its README for the full B1–B7 history)
- B1 pinned `-s workspace-write` sandbox (no bypass); B2 worktree isolation +
  PID/children cleanup, codex stdin `</dev/null`, stdout → per-iter log;
  B5 no model-critic (the pinned oracle is the judge) + work accumulation
  (commit each verify-passing iter, revert only on count regression);
  B6 stall guard (no worktree change for `NOWORK_LIMIT` iters → BLOCK);
  B7 gate files restored-from-HEAD + `chmod 0444` before every codex attempt.

## Gate model
Per iteration: `verify.sh` requires cli/web/symbols/mcp fully green + the
git_parity count `>= BASE_GIT` (floor) + Go-untouched + go-build + placeholder
grep. The runner then commits if non-regressing; `> prev` is a byte-green flip.
DONE when git_parity count `== BASE_GIT_TOTAL` (13).

## Recipe handed to codex
gix (blob/commit reads) + `dmp` 0.2.3 (diffmatchpatch, checklines=false, no
cleanup) + ported `renderDiffLines`; shared logic in a new `crates/ctx-git`.
Proven byte-exact by the `spike/git-dmp-parity` spike (15/15).

## Order of operations
1. Oracle authored + RED (done). 2. `bash loops/go-git/bootstrap.sh` (pin +
baseline). 3. Merge oracle + loop infra to main. 4. `bash loops/go-git/run-loop.sh`.

## Recovery
`recover.sh --reset-circuit | --reset-state | --drop-worktree`
