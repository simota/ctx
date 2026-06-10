# go-cutover loop — Wave 3 cutover (native Rust as default dispatch)

Count-driven accumulation runner (same hardened template as `loops/go-mcp` /
`loops/go-git`). Drives the PINNED oracle `crates/ctx-cli/tests/cutover.rs` (5
dispatch-assertion tests) by passing-case COUNT until the cutover is done.
DONE = cutover 5/5.

## What the cutover changes (small — CLI is already native)
1. wire `ctx mcp serve` → native `ctx_mcp::serve` (add ctx-mcp dep)
2. flip web-engine default → `rust` (`ctx browse` uses native axum by default)
`tui` STAYS delegating to Go (carve-out; ratatui port is a Wave 4 prerequisite)
— `cutover_tui_still_delegates_to_go` regression-locks this.

## Why a dispatch oracle (not byte-parity)
Native and Go are byte-identical by design, so a black-box output test can't tell
which ran. The oracle probes the DISPATCH DECISION: it points `CTX_GO_BIN` at a
sentinel stub — a delegating command hits the stub, a native command doesn't.

## Hardening (inherited; see loops/go-mcp/README for the full B1–B7 history)
B1 pinned sandbox; B2 worktree isolation + PID cleanup, stdin `</dev/null`,
per-iter log; B5 no model-critic (pinned oracle = judge) + accumulation;
B6 stall guard; B7 gate files restored-from-HEAD + `chmod 0444` before each
codex attempt.

## Gate model
Per iteration: cli/web/symbols/mcp/git_parity stay fully green + cutover count
`>= BASE_CUTOVER` (floor, 2 locks) + Go-untouched + go-build + placeholder grep.
DONE when cutover count `== BASE_CUTOVER_TOTAL` (5).

## Sign-off
PRODUCTION default flip (ADR-0005 Wave 3). Loop output → PR → **human sign-off**;
never auto-merge.

## Order: bootstrap → merge oracle+loop to main → run-loop.sh → PR.
## Recovery: `recover.sh --reset-circuit | --reset-state | --drop-worktree`
