# Loop goal — Wave 3 cutover: make native Rust the default dispatch

## Objective
Flip the ctx-cli strangler dispatcher so the native Rust implementations become
the DEFAULT (no longer delegating to Go) for the two remaining delegated paths,
while `tui` stays a deliberate Go carve-out (ported in a later step, ratatui).

The two cutover changes (CLI commands are already native via `try_run_native`):
1. **MCP**: add `ctx-mcp` (path `../ctx-mcp`) as a ctx-cli dependency and route
   `ctx mcp serve --root <dir> [--allow-outside-root] [--log-file <f>]` through
   `try_run_native` to `ctx_mcp::serve(...)` (mirror `internal/cli/mcp.go`). Runs
   NATIVE, no delegate.
2. **WEB DEFAULT**: in `parse_browse_args`, change the web-engine default from the
   unset-empty (Go) to `rust`, so `ctx browse` with no flag/env uses the native
   axum server (already serves all ported routes incl. git). Explicit
   `--web-engine go` / `CTX_WEB_ENGINE=go` still delegates.

## Acceptance criteria (verify.sh-gated, count-driven)
1. **AC1 — cutover oracle GREEN.** `crates/ctx-cli/tests/cutover.rs` (5 pinned
   dispatch-assertion tests) passes 5/5. The 3 RED cases (mcp native, browse
   default rust ×2) flip green; the 2 locks stay green.
2. **AC2 — carve-out preserved.** `cutover_tui_still_delegates_to_go` STAYS green
   (tui must keep delegating to Go until its ratatui port).
3. **AC3 — no regression.** cli/web/symbols/mcp/git_parity suites stay fully green;
   counts monotonic.
4. **AC4 — Go untouched** (internal/**, cmd/**). **AC5 — go build clean.**
   **AC6 — no placeholders** in changed Rust src.

## OUT OF SCOPE
- Removing the `delegate_to_go` machinery or deleting Go — that is Wave 4.
- Porting `tui` — a later ratatui effort (Wave 4 prerequisite).
- Any Go change.

## Sign-off
This is the PRODUCTION default flip (ADR-0005 Wave 3). The loop's output is a PR
that REQUIRES human sign-off before merge — never auto-merge.

## Verification command
`bash loops/go-cutover/verify.sh`

NEXUS_LOOP_STATUS: READY
NEXUS_LOOP_SUMMARY: Wave 3 cutover (mcp wire + web default rust); 5-case dispatch oracle; tui stays Go carve-out
