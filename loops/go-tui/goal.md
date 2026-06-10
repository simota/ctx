# Loop goal — port the tui to native Rust (ratatui), snapshot-parity

## Objective
Port the ctx terminal UI (`ctx tui`, Go Bubble Tea, `internal/tui/app.go` 304 LOC)
to a native Rust `crates/ctx-tui` (ratatui) whose rendered frames match the Go
tui FRAME-FOR-FRAME (content + layout). This is the last Go-only command and a
**Wave 4 prerequisite** (Go can't be deleted while `ctx tui` delegates to Go).

## Verification model — snapshot goldens (NOT byte-parity-vs-live-Go)
The tui is out of the HTTP/CLI byte-parity model. Its oracle is
`crates/ctx-tui/tests/snapshot.rs`: for each fixed scripted key session, it drives
the Rust tui, renders to an 80x24 buffer, extracts the cell TEXT grid, and asserts
it byte-equals a golden frame captured from the FROZEN Go tui
(`tests/goldens/<session>.txt`, exported via `cmd/tui-golden-export`).
**Carve-out**: ANSI styling (colors/bold) is NOT verified — only content/layout
(cross-library ANSI parity is impossible). The goldens are pinned/immutable.

## Acceptance criteria (verify.sh-gated, count-driven)
1. **AC1 — snapshot GREEN.** All `BASE_TUI_TOTAL` (2) snapshot sessions pass:
   `nav_toggle_open`, `expand_all_scroll` (nav, toggle, open/collapse, g/G jump,
   viewport scroll). Frame text grids byte-equal the goldens.
2. **AC2 — no regression.** cli/web/symbols/mcp/git_parity/cutover stay fully green.
3. **AC3 — Go untouched** (internal/**, cmd/**). **AC4 — go build clean.**
   **AC5 — no placeholders** in changed Rust src OUTSIDE ctx-tui (ctx-tui is the
   in-progress crate; the snapshot oracle is its anti-stub gate).

## Recipe
ratatui crate. Match `internal/tui/app.go`: Model state, Update key handling
(↑↓ nav, Space toggle, Enter open, left collapse, g/G jump, p pack, q quit),
View() layout (header `ctx tokens: N / M`, body rows via renderRow
`%s%s%s %s %s%s` with `│  ` indent, `├─`/`└─` connectors, `▾`/`▸` markers,
`[ ]`/`[x]`, name, `N tokens`, help line), the viewport scroll window
(start/end vs cursor/height), and the root `.` rendering with `└─` at depth 0.

## OUT OF SCOPE
- Verifying ANSI styling/colors. - Any Go change. If a session is not
  reproducible, append to `crates/ctx-tui/TUI_DEFERRED.md` (do not fake).

## Verification command
`bash loops/go-tui/verify.sh`

NEXUS_LOOP_STATUS: READY
NEXUS_LOOP_SUMMARY: tui Go(Bubble Tea)->Rust(ratatui) snapshot-parity loop; 2 frozen golden sessions; content/layout only
