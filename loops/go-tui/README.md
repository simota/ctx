# go-tui loop — port ctx tui to ratatui (snapshot-parity)

Count-driven accumulation runner (same hardened template as go-mcp/go-git/
go-cutover). Drives the PINNED oracle `crates/ctx-tui/tests/snapshot.rs` (2
frame-snapshot sessions) by passing-case COUNT until the ratatui port renders
frame-identical to the frozen Go tui. DONE = tui 2/2.

## Verification model (differs from the other loops)
The tui is NOT byte-parity-vs-live-Go. The oracle compares the Rust tui's
rendered 80x24 cell TEXT grid against goldens captured ONCE from the frozen Go
Bubble Tea tui (`tests/goldens/*.txt`, via `cmd/tui-golden-export`).
**Carve-out**: ANSI styling/colors NOT verified — content/layout only (cross-
library ANSI parity is impossible). Goldens are pinned/immutable (B7 locks them).

## Hardening (inherited; see loops/go-mcp/README for B1–B7)
B1 sandbox; B2 worktree + PID cleanup + per-iter log; B5 no model-critic (the
pinned goldens are the judge) + accumulation; B6 stall guard; B7 snapshot.rs +
goldens restored-from-HEAD + chmod 0444 before each codex attempt.

## Gate model
Per iteration: cli/web/symbols/mcp/git_parity/cutover stay fully green + tui
snapshot count `>= BASE_TUI` (floor) + Go-untouched + go-build + placeholder grep
(EXCLUDING ctx-tui — it's the in-progress crate; the snapshot oracle is its
anti-stub gate). DONE when tui count `== BASE_TUI_TOTAL` (2).

## Why a snapshot is all-or-nothing
Each session asserts EVERY frame in the scripted run byte-equals its golden, so a
session only flips green once the ratatui render+update is complete enough to
reproduce the whole session — codex accumulates the impl across iterations.

## Order: bootstrap → merge oracle+loop to main → run-loop.sh → PR (Wave 4 prereq).
## Recovery: recover.sh --reset-circuit | --reset-state | --drop-worktree
