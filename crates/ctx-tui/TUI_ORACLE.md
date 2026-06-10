# ctx-tui Oracle — Frame-Snapshot Goldens

ADR-0005 Wave 4 prerequisite #1. This document is the **immutable oracle**
for porting `internal/tui` (Go, Bubble Tea) to native Rust (ratatui).

## Why this is not byte-parity-vs-Go

Every other ctx crate is verified byte-for-byte against the Go reference
(HTTP/CLI). The tui is **out of that model**: the Go tui renders through
Bubble Tea + lipgloss, which emit ANSI escape/SGR codes for colour and
styling. ratatui produces different ANSI. **Cross-library ANSI/styling
parity is impossible and explicitly out of scope.**

Instead the port is verified by **frame-snapshot goldens**:

- Frames are captured from the **frozen Go tui**, driven headlessly.
- ANSI escape codes are **stripped** before capture.
- We assert **CONTENT + LAYOUT** — the cell text grid — and **NOT** visual
  styling/colours (bold, reverse-video cursor, foreground colours).

If a future change needs to verify styling, that is a separate,
human-reviewed effort; this oracle deliberately does not cover it.

## Headless capture (Go side)

`cmd/tui-golden-export/main.go` (additive tooling; imports `internal/tui`
**read-only**, modifies no `internal/**` behaviour).

It drives the Go `tui.Model` purely through its exported Bubble Tea
interface — `tui.New(root)` then `Update(msg)` / `View() string` — with no
real terminal and no `tea.Program`. This makes capture headless and fully
deterministic.

Per session it:

1. builds a Model from the fixed in-memory fixture (below),
2. sends `tea.WindowSizeMsg{Width: 80, Height: 24}`,
3. applies the fixed scripted `tea.KeyMsg` sequence, calling `View()`
   after each step,
4. strips ANSI (regex `\x1b\[[0-9;]*m`) and writes framed output.

```
go run ./cmd/tui-golden-export -out crates/ctx-tui/tests/goldens
```

Determinism is verified by running twice and diffing — output is
byte-identical (same sha256). The fixture has no filesystem, clock, or git
dependence, and every file node carries an explicit `Tokens` count so the
Go `tokenCount()` never falls through to a size-based estimate.

## Fixed fixture

Defined identically in `cmd/tui-golden-export/main.go` (`fixture()`) and
`crates/ctx-tui/src/lib.rs` (`golden_fixture()`):

```
.                       (dir)
├─ cmd/                 (dir)
│  └─ cmd/main.go       120 tokens
├─ internal/            (dir)
│  ├─ internal/app.go   340 tokens
│  └─ internal/util.go  80 tokens
└─ README.md            45 tokens
```

Total = 585 tokens; budget = 50000 (the tui default).

## Scripted sessions

One golden file per session: `tests/goldens/<session>.txt`. STEP 0 is the
initial frame (after the window-size message, before any key); step *i*
renders after applying `script[i-1]`. Script tokens map to keys:
`down up enter left right space` plus single chars (e.g. `g`, `G`, `p`,
`q`).

| Session              | Script                                                        | Covers                                  |
| -------------------- | ------------------------------------------------------------- | --------------------------------------- |
| `nav_toggle_open`    | down, enter, down, space, up, left, down, enter, down, space  | nav (up/down), open (enter), collapse (left), toggle (space → header token total changes 585→465→…) |
| `expand_all_scroll`  | down, enter, down, down, enter, G, g, down, down              | expand multiple dirs, jump-to-last (G) / jump-to-first (g), re-navigation; exercises viewport math |

## Frame delimiter format

Within a golden file, frames are separated by a delimiter line:

```
===== FRAME <n> =====
```

`<n>` is the step index (0 = initial). Everything between one delimiter and
the next (or EOF) is the ANSI-stripped frame body.

## Rust oracle (RED)

`crates/ctx-tui` — ratatui crate, `src/lib.rs` + `tests/snapshot.rs`.

The test replays the identical scripted sequence against the Rust `Model`,
renders to an 80×24 ratatui `Buffer` via `TestBackend`, extracts the cell
text grid (ANSI-free by construction), normalises (trim trailing spaces per
line, drop trailing blank lines — same normalisation applied to the
goldens), and asserts byte-equality with each golden frame. One
`#[test]` per session so a port loop can count green/red.

### Current status: RED (scaffold)

`ctx_tui::render()` and `Model::update()` are explicit **STUBS** (render
emits an empty buffer). The snapshot tests therefore fail with a real
content mismatch (blank grid vs golden content), **not** a compile error.

The Wave 4 ratatui port loop implements `render()` / `update()` (and any
model behaviour the goldens require — inclusion set, open/collapse, cursor,
viewport scrolling, token totals) until every session goes green. **Do not
edit the goldens to match a partial port** — the goldens are the frozen Go
reference.

```
cargo test --manifest-path crates/ctx-tui/Cargo.toml --test snapshot
```
