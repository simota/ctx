# ctx Git Log Viewer TUI Design Spec

## Metadata

- Slug: `git-log-viewer-tui-design`
- Feature title: Git Log Viewer TUI Design
- Status: `draft`
- Current phase: `SPECIFY`
- Date: 2026-06-30
- Owner: TBD
- Related spec: `docs/specs/ctx-cli-git-log-viewer.md`
- Related implementation: `crates/ctx-cli/src/commands/log.rs`
- Build-path decision: `feature`
- TUI framework decision: Ratatui `0.29` with the crossterm backend

## L0 - Vision

### Problem

`ctx log` needs to feel like a quick terminal inspection tool, not a dense clone of
`git log`. The user should always know which commit is selected, which detail mode
is active, whether the expensive diff body is loaded, and how to move through the
view without memorizing hidden controls.

The broader feature spec covers command scope and ctx-specific signals. This
document narrows the design to the terminal UI contract: layout, states,
navigation, feedback, color semantics, and testable acceptance criteria.

### Audience

- Terminal-first developers checking recent repository history.
- Reviewers scanning changes without opening a browser.
- Maintainers inspecting large commits and changed files.
- Agent-driven workflows that need predictable non-mutating UI behavior.

### Job To Be Done

When I run `ctx log`, I want to skim commits, inspect changed files, open diffs,
and jump between file boundaries while staying oriented in the terminal.

### Success Definition

A user can complete the critical loop in under one minute:

1. Open `ctx log`.
2. Identify the selected commit and source scope.
3. Move through commits.
4. Inspect changed files without loading full diffs.
5. Load a full diff on demand.
6. Jump between files in the diff.
7. Quit with terminal state restored.

## Scope

### In Scope

- TUI layout for wide and narrow terminals.
- Header, footer, commit pane, and detail/diff pane behavior.
- Keyboard navigation and discoverability.
- Files-only summary mode, loading mode, and full diff mode.
- Diff color semantics.
- Empty, truncated, loading, and error states.
- Read-only safety and terminal cleanup expectations.
- Test strategy for render-independent behavior and TTY smoke checks.

### Out Of Scope

- Mutating git operations.
- Mouse support.
- Commit graph visualization.
- Web Git Log UI.
- Full co-change graph UI.
- Persistent bookmarks, copy actions, or LLM handoff actions.
- Final design of token, symbol, query, and impact signal panels.

## Design Principles

1. Orientation first: every screen must answer "where am I?" in the header,
   pane titles, and footer.
2. Progressive disclosure: show changed files first; load full diffs only after
   explicit user action.
3. No silent work: any operation that can take more than 400 ms must show a
   visible status before blocking.
4. Dense but legible: optimize for terminal scanning, not decorative framing.
5. Read-only by design: no key may mutate git state or repository files.
6. Text-first surface: use stable text, spacing, borders, and color; avoid
   decorative symbols that break narrow terminals or logs.

## L1 - Requirements

### `REQ-TUI-001`: Layout adapts to terminal size

- The viewer MUST use a two-pane horizontal layout when width is at least 96 columns.
- The viewer MUST use a stacked layout when width is below 96 columns so diffs
  keep enough horizontal space for old/new line columns.
- Header and footer rows MUST remain reserved when height permits.
- The detail/diff pane SHOULD keep at least one visible content region in stacked mode.

### `REQ-TUI-002`: Header communicates source scope

- The first row MUST identify the command surface, source label, source kind, and commit count.
- Truncated history MUST be visible near the top when `--limit` cut the result.
- Errors MUST be visible without hiding the footer.

### `REQ-TUI-003`: Commit pane supports fast scanning

- The commit pane MUST show selected position as `current/total`.
- The selected row MUST be visibly distinct.
- Each row SHOULD include a marker, short hash, subject, and matched paths when applicable.
- When the default `HEAD` log has uncommitted worktree changes, the commit pane SHOULD show a synthetic `worktree` row before committed history.
- Empty history MUST render a clear empty state.

### `REQ-TUI-004`: Detail pane has explicit modes

- The right pane MUST expose one of these modes in its title: `files`, `loading`, or `diff`.
- The title MUST include short hash, visible line range, and changed-file count when a commit is selected.
- The focused pane MUST be visible in the pane title or frame.
- Files mode MUST state that full diff is not loaded and how to load it.
- Loading mode MUST replace stale detail content with explicit loading text.
- Diff mode MUST show full diff lines and keep the visible range accurate.
- A `worktree` row MUST use the same files/loading/diff modes as committed rows, comparing current working files to `HEAD`.

### `REQ-TUI-005`: Full diff loading is explicit and lazy

- Initial commit selection MUST load only summary data and changed-file stats.
- Full diff body MUST load only after `d` or `Enter`.
- Before full diff generation starts, the UI MUST render and flush a loading state.
- When loading completes, the pane MUST switch from `loading` to `diff`.

### `REQ-TUI-006`: Keyboard model is discoverable

- Footer MUST show the primary keymap for the current mode.
- The base keymap MUST include commit movement, detail scrolling, file jumping, diff loading, and quit.
- Narrow terminals SHOULD use a shortened footer that keeps focus, movement, diff loading, and quit visible.
- `Ctrl+C` and `q` MUST both exit and restore the terminal.
- Repeated keys MUST not panic at list boundaries.

### `REQ-TUI-007`: Diff colors follow git semantics

- Added lines and added files MUST render green when color is available.
- Deleted lines and deleted files MUST render red when color is available.
- Modified files and truncation warnings MUST render yellow when color is available.
- Diff file headers MUST render cyan when color is available.
- Changed-file stat rows SHOULD color the addition and deletion counts independently.
- Diff body rows SHOULD include old and new line-number columns before the text.
- Diff body sections SHOULD include an `old/new/code` column header before textual hunks.
- The content MUST remain understandable without color.

### `REQ-TUI-008`: Empty, error, and truncated states are designed

- No-commit results MUST show an empty state in the commit pane.
- Detail pane with no selected commit MUST show a "select a commit" style message.
- Errors from commit detail or diff loading MUST appear in the global status area.
- Binary files and no-text-change states SHOULD be labeled inline instead of appearing as generic gray text.
- Truncated history MUST be labeled as limit-driven, not data loss.

### `REQ-TUI-009`: Scroll behavior keeps information dense

- Detail scrolling MUST clamp to a valid line range.
- At the end of a long detail/diff, the start row SHOULD be adjusted so the
  final page is not mostly blank.
- The visible line range MUST match the actual rendered window.

### `REQ-TUI-010`: File boundary navigation is supported

- In files or diff content, `n` SHOULD jump to the next file boundary.
- `p` SHOULD jump to the previous file boundary.
- File boundaries are changed-file stat rows and `diff --` headers.
- Regular diff body additions/deletions MUST NOT be treated as file boundaries.

### `REQ-TUI-011`: Read-only safety

- No TUI key MUST run checkout, reset, rebase, commit, staging, patch apply, or file writes.
- Terminal raw mode and alternate screen MUST be restored on normal quit and `Ctrl+C`.
- Errors MUST be reported without printing sensitive repository content beyond the requested paths and git metadata.

### `REQ-TUI-012`: Future ctx signals have reserved presentation slots

- The design SHOULD reserve a compact summary region for token footprint, symbol hints,
  query bridge context, and impact signals.
- Unavailable signals MUST use reason categories instead of silent omission.
- These signals MUST not block initial commit list rendering.

## L2 - TUI Design

### Layout Model

Wide layout, width >= 96:

```text
row 1: ctx log | <source> (<kind>) | commits <n>[+]
row 2: optional warning/error

left pane: commits                    right pane: files/loading/diff
+ commits 3/100 +                    + diff a1b2c3d 1-20/104 | 12 files +
| > a1b2c3d subject |                 | file src/lib.rs                     |
|   b2c3d4e subject |                 | M    2+    1- src/lib.rs            |
                                      |    old  new | code                  |
|   c3d4e5f subject |                 | -   42      | old line             |
                                      | +        42 | new line             |
                                      |     43   43 | context              |

last row: commit 3/100 | focus diff | diff | d top | left/right focus | j/k move/scroll | f/b page | n/p file | q
```

Narrow layout, width < 96:

```text
row 1: ctx log | <source> (<kind>) | commits <n>[+]
top block: commits
bottom block: files/loading/diff
last row: compact footer
```

### Pane Responsibilities

| Pane | Responsibility | Must not do |
|------|----------------|-------------|
| Header | source, kind, count | keymap dump |
| Warning/error row | truncated and detail errors | hide content permanently |
| Commit pane | select commits quickly | load full diffs |
| Detail pane | show files, loading, or diff content | mutate git state |
| Footer | current position, mode, primary keys | duplicate long help text |

### Screen States

| State | Entry | Visible signal | Exit |
|-------|-------|----------------|------|
| `files` | initial load or commit move | `files <hash> <range> | <n> files | d diff` | `d`/`Enter` -> `loading`; commit move -> `files` |
| `loading` | user requests full diff | `loading <hash> ... | wait` and loading body text | success -> `diff`; error -> `files` with error row |
| `diff` | full diff loaded | `diff <hash> <range> | <n> files` | `d`/`Enter` -> top; commit move -> `files` |
| `worktree` | dirty default `HEAD` worktree | synthetic `worktree` row with uncommitted file stats | clean worktree or explicit `--ref` omits it |
| `empty` | no commits | `log [0/0]` and no-commit text | quit |
| `error` | detail load fails | error row in global status | commit move or retry action |

### Keyboard Contract

| Key | State | Action |
|-----|-------|--------|
| `Left` | all | focus commit pane |
| `Right` | all | focus detail/diff pane |
| `j`, `Down` | commit focus | move to next commit |
| `k`, `Up` | commit focus | move to previous commit |
| `j`, `Down` | detail/diff focus | scroll detail/diff down one line |
| `k`, `Up` | detail/diff focus | scroll detail/diff up one line |
| `f`, `Space`, `PageDown` | files/diff | scroll detail pane down |
| `b`, `PageUp` | files/diff | scroll detail pane up |
| `g`, `Home` | all | first commit |
| `G`, `End` | all | last commit |
| `d`, `Enter` | files | load full diff |
| `d`, `Enter` | diff | return detail scroll to top |
| `n` | files/diff | next file boundary |
| `p` | files/diff | previous file boundary |
| `q` | all | quit |
| `Ctrl+C` | all | quit with cleanup |

### Visual Language

- Headers use cyan.
- Truncation and loading messages use yellow.
- Errors use red.
- Selected commit row uses a high-contrast highlighted row.
- Focused pane uses a high-contrast frame/title plus `[focus]` text so focus is not color-only.
- Diff semantics:
  - Added line or added file: green.
  - Deleted line or deleted file: red.
  - Modified file or truncation marker: yellow.
  - `diff --` file header: cyan.
- Changed-file stat rows color `+` counts green and `-` counts red even when the row status is modified.
- Diff body line format uses marker + old line + new line + text:
  `+ <old> <new> | <text>`, `- <old> <new> | <text>`, or
  `  <old> <new> | <text>`.
- Textual diff sections include an `old/new/code` header so the line-number columns are readable without color.
- Color MUST be additive; text must still carry meaning without color.

### State Model

Current implementation can be represented as:

```rust
struct LogState {
    root: PathBuf,
    data: LogData,
    selected_commit: usize,
    diff_scroll: usize,
    detail: CommitDetail,
    diff_loading: bool,
    error: Option<String>,
}

struct CommitDetail {
    files: Vec<CommitFile>,
    lines: Vec<String>,
    diff_loaded: bool,
}
```

Design rules:

- Commit movement resets `diff_scroll` to `0`.
- Commit movement clears `diff_loading`.
- Summary loading sets `diff_loaded=false`.
- Full diff loading sets `diff_loading=true`, renders once, then computes.
- Full diff success sets `diff_loaded=true`, `diff_loading=false`, `diff_scroll=0`.
- Full diff failure sets `error`, clears `diff_loading`, and preserves safe terminal state.

### Performance Strategy

- Initial commit list and files view MUST avoid full diff generation.
- Full diff generation is user-triggered.
- Large commits MUST show loading feedback before synchronous work starts.
- Future expensive ctx signals MUST be lazy and scoped to the selected commit.
- A small session cache for loaded diffs MAY be added later, with an explicit entry cap.

### Accessibility And Terminal Robustness

- The UI MUST be keyboard-only usable.
- Essential instructions MUST be visible in footer, not hidden in a tooltip or separate help screen.
- The display MUST tolerate narrow widths via truncation, not wrapping that breaks layout.
- Terminal alternate screen, cursor visibility, and raw mode MUST be restored on exit.
- Non-TTY default invocation MUST fail clearly instead of attempting interactive rendering.

## L3 - Acceptance Criteria

### `AC-TUI-001`: wide layout renders two panes

Given an interactive terminal with width at least 96 columns  
When the user runs `ctx log`  
Then the view renders a commit pane and a detail pane as adjacent framed panes  
And the footer remains visible.

### `AC-TUI-002`: narrow layout stacks content

Given an interactive terminal with width below 96 columns  
When the user runs `ctx log`  
Then the commit list appears above the detail pane  
And the footer remains visible when height permits.

### `AC-TUI-003`: header shows source and count

Given loaded log data  
When the TUI renders  
Then the header shows `ctx log`, source label, source kind, and commit count  
And appends `+` when history is truncated by the limit.

### `AC-TUI-004`: commit selection is visible

Given at least one commit  
When a commit is selected  
Then the left pane title shows the selected position  
And the selected row is visually distinct.

### `AC-TUI-004A`: pane focus is visible and keyboard controlled

Given the TUI is open  
When the user presses `Left` or `Right`  
Then focus moves between the commit pane and detail/diff pane  
And the focused pane is visible without relying only on color.

### `AC-TUI-005`: files mode avoids full diff work

Given a selected commit  
When the viewer first renders the detail pane  
Then it shows changed-file summary lines  
And it shows a prompt to load the full diff  
And it does not compute full diff bodies.

### `AC-TUI-005a`: uncommitted worktree changes are inspectable

Given the default `HEAD` worktree has modified, deleted, or untracked files
When the user opens `ctx log`
Then a synthetic `worktree` row appears before committed history
And selecting it shows changed-file stats
And loading diff shows the working tree diff against `HEAD`
And `ctx log --plain` and `ctx log --json` keep returning committed history only.

### `AC-TUI-006`: diff loading gives immediate feedback

Given a selected commit whose full diff has not been loaded  
When the user presses `d` or `Enter`  
Then the detail pane renders `loading` state before full diff generation starts  
And the footer mode changes to `loading`.

### `AC-TUI-007`: diff mode shows range and file count

Given full diff generation succeeds  
When the detail pane renders  
Then its title starts with `diff`  
And includes the visible line range and changed-file count.

### `AC-TUI-008`: scrolling keeps the final page dense

Given a long detail or diff body  
When the user scrolls beyond the final page  
Then the rendered start line is clamped so the final page is not mostly blank  
And the range label matches the rendered lines.

### `AC-TUI-009`: file boundary jumps skip diff body lines

Given a loaded diff containing changed-file rows, `diff --` headers, and added/deleted body lines  
When the user presses `n` or `p`  
Then the viewer jumps only to changed-file rows or `diff --` headers  
And does not treat `+` or `-` body lines as file boundaries.

### `AC-TUI-010`: diff colors map to semantic classes

Given rendered diff content  
When color output is available  
Then added content is green, deleted content is red, modified/truncated content is yellow, and file headers are cyan  
And diff body rows expose old and new line-number columns before the text  
And textual hunks include an old/new/code header
And the same content remains understandable without color.

### `AC-TUI-011`: empty state is explicit

Given the selected source has no commits  
When the TUI renders  
Then the commit pane shows a no-commit empty state  
And the detail pane does not show stale data.

### `AC-TUI-012`: errors remain recoverable

Given changed-file or diff loading fails  
When the TUI renders the failure  
Then the error is visible near the top  
And the footer remains available for navigation or quit.

### `AC-TUI-013`: quit restores terminal

Given the TUI is open  
When the user presses `q` or `Ctrl+C`  
Then raw mode is disabled  
And cursor visibility and alternate screen state are restored.

### `AC-TUI-014`: read-only contract holds

Given any supported TUI key  
When the user navigates, scrolls, loads diff, or quits  
Then no git state or repository file is mutated.

## Traceability Matrix

| Requirement | Acceptance Criteria |
|-------------|---------------------|
| `REQ-TUI-001` | `AC-TUI-001`, `AC-TUI-002` |
| `REQ-TUI-002` | `AC-TUI-003`, `AC-TUI-012` |
| `REQ-TUI-003` | `AC-TUI-004`, `AC-TUI-011` |
| `REQ-TUI-004` | `AC-TUI-004A`, `AC-TUI-005`, `AC-TUI-006`, `AC-TUI-007` |
| `REQ-TUI-005` | `AC-TUI-005`, `AC-TUI-006`, `AC-TUI-007` |
| `REQ-TUI-006` | `AC-TUI-004`, `AC-TUI-004A`, `AC-TUI-006`, `AC-TUI-009`, `AC-TUI-013` |
| `REQ-TUI-007` | `AC-TUI-010` |
| `REQ-TUI-008` | `AC-TUI-011`, `AC-TUI-012` |
| `REQ-TUI-009` | `AC-TUI-008` |
| `REQ-TUI-010` | `AC-TUI-009` |
| `REQ-TUI-011` | `AC-TUI-013`, `AC-TUI-014` |
| `REQ-TUI-012` | Deferred to ctx signal implementation |

## Test Strategy

### Unit Tests

- `commit_position_label` clamps selection for empty and out-of-range lists.
- `visible_range_label` reports empty, first-page, middle, and end ranges.
- `visible_scroll_start` keeps the final page dense.
- `file_jump_target` moves between changed-file rows and diff headers.
- `is_file_jump_line` ignores regular diff body lines.
- `format_diff_line` emits old/new line-number columns.
- `classify_detail_line` maps diff text rows to semantic classes.
- `detail_line_style` maps semantic classes to high-contrast Ratatui styles.

### Integration Tests

- Non-TTY `ctx log` fails with the documented TTY error.
- `ctx log --plain` and `ctx log --json` remain deterministic.
- Invalid refs are rejected before git invocation.
- Path and query modes narrow commit results without changing the TUI contract.

### TTY Smoke Tests

- `ctx log --limit 1` opens the viewer.
- `d` shows `loading` before large diff generation.
- Completed diff title shows `diff` and line range.
- `n` and `p` update the visible range by file boundary.
- `q` and `Ctrl+C` restore terminal state.

## Considered But Rejected

- Always loading full diffs on commit selection: rejected because large commits block navigation.
- A fixed four-pane layout: rejected because narrow terminals become unreadable.
- Hidden help-only keymap: rejected because users need recognition over recall.
- Color-only diff semantics: rejected because color may be unavailable or inaccessible.
- Git mutation shortcuts: rejected because `ctx log` is an inspection surface.

## Open Questions / Deferred Decisions

- Should `?` open an in-TUI help overlay once the base footer is stable?
- Should loaded full diffs be cached for the last N commits?
- Should mouse wheel scrolling be supported, or should keyboard-only remain the contract?
- Should `Tab` move focus between commit and detail panes if future interactive subpanes are added?
- Should ctx-specific token/symbol/impact signals occupy a fixed summary band or inline file rows?

## Spec Quality Gate Draft

- Ambiguity: pass for layout, states, and keymap; deferred ctx signal placement is explicitly open.
- Completeness: every TUI requirement has at least one acceptance criterion except future signal slots, which are marked deferred.
- Consistency: the design preserves the broader feature spec's read-only and terminal-first constraints.
- Testability: core behavior is verifiable through pure unit tests, CLI integration tests, and TTY smoke tests.
- Scope coherence: this document covers TUI design only; git data retrieval and ctx signals remain in the broader feature spec.
