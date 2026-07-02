# ctx browse Pinned Files Board Spec

## Metadata

- Slug: `ctx-browse-pinned-files-board`
- Feature title: ctx browse Pinned Files Board
- Status: `locked`
- Current phase: `APEX_IMPLEMENTED`
- Date: 2026-07-02
- Owner: TBD
- Build-path decision: `apex`

## L0 - Vision

### Problem

`ctx browse` ではファイルツリー、タブ、split pane、検索、Largest などから複数ファイルを行き来できるが、ユーザーが「この調査・レビューで重要なファイル」を明示的に残す場所がない。

現在のタブは開いているファイル一覧であり、session-first な作業状態として URL `?open=` から seed される。調査対象やレビュー対象を reload 後も維持する「選んだファイル集合」としては意味が違う。

そのため、ユーザーは重要ファイルを再検索したり、タブを閉じられなかったり、レビュー・実装・LLM handoff 用の作業対象リストを外部メモで管理する必要がある。

### Audience

- 大きなリポジトリを調査する開発者
- 複数ファイルを横断してレビューする保守者
- 実装前に参照ファイルを集めるユーザー
- LLM に渡す調査対象を整理するユーザー
- local-first/offline な作業状態を重視するチーム

### Job To Be Done

リポジトリを見ている最中に重要なファイルをピン留めし、あとで専用ボードから開き直したり、整理したり、作業対象として確認したい。

### Success Definition

ユーザーは Tree、File detail、Tab からファイルを pin/unpin できる。ピン留めしたファイルは repo root ごとに browser-local に保存され、`Pins` board で一覧表示、open、open to side、copy path、unpin、reorder ができる。

## Reuse And Constraints

### Existing reusable assets

- `web/src/lib/router.svelte.ts` has hash route parsing and helpers for top-level views, but no `pins` route yet.
- `web/src/lib/tabs.svelte.ts` already models an ordered, duplicate-free `string[]` of repo-relative file paths. It is session-only and intentionally not `localStorage`-backed.
- `web/src/lib/view.svelte.ts`, `web/src/lib/theme.svelte.ts`, `web/src/lib/panes.svelte.ts`, and `web/src/components/FileDetail.svelte` already show localStorage preference patterns.
- `web/src/App.svelte` owns top-level route rendering, top navigation, tab seeding from `?open=`, and URL write-back for tabs/right pane.
- `web/src/components/TreeNode.svelte`, `web/src/components/FileDetail.svelte`, and `web/src/components/TabBar.svelte` already expose context menus for file-level actions.
- `web/src/lib/context-menu.svelte.ts` and `web/src/components/ContextMenu.svelte` provide a generic menu item contract.
- `web/src/lib/commands.ts` provides command-palette registration for Navigation, View, and Tabs actions.
- `web/src/components/LargestFiles.svelte` is the closest existing board-style file list surface.
- Existing file identity across web UI and API is repo-relative `path`.
- `crates/ctx-web/src/handlers/mix.rs` has a server-side saved file collection concept, but mutation is deferred and frontend wrappers are absent.

### Constraints

- MVP should not require backend or Rust changes.
- Existing tab semantics must remain "open files", not "pinned files".
- Existing split-pane state should not be repurposed for pins.
- Existing `?open=` URL behavior should not become the source of truth for persistent pins.
- The pin store must be scoped to the served repo root to avoid mixing files from different projects in the same browser.
- Browser-local persistence may fail in private mode, locked-down browsers, or quota errors; failures must degrade without corrupting current session state.
- File existence can drift after a file is pinned. Missing or renamed files should be visible as stale entries and removable, not silently deleted.

## Scope

### In Scope

- Pin and unpin files from Tree row context menu, File detail toolbar/context menu, Tab context menu, and Command Palette.
- A new `Pins` top-level board route in `ctx browse`.
- Browser-local persistence of pinned repo-relative file paths scoped by repo root.
- Duplicate prevention, stable ordering, manual reorder, and clear empty state.
- Board actions: open, open to side, copy path, unpin, move up/down, clear all with confirmation.
- Keyboard and screen-reader access for board rows and pin actions.
- Explicit local-only privacy behavior.

### Out Of Scope

- Directory pinning.
- Notes, labels, groups, comments, ownership, review status, or kanban columns.
- Server-side `/api/pins`.
- Writing to or mutating `mix` records.
- Cross-browser or cross-machine sync.
- Cloud sharing.
- Automatic pack generation from pins.
- Pinning line ranges, symbols, diffs, commits, or evidence snippets.
- Persistent browser tabs.

## Candidate Directions

### Candidate A - Browser-Local Pinned File Board

- Shape: Add `web/src/lib/pins.svelte.ts` with a per-root localStorage-backed ordered list, plus `#/pins` board UI.
- Rationale: Smallest useful feature. It matches current local-first browse behavior and avoids backend mutation.
- Tradeoff: Pins are browser-local and not shared across machines.
- Defers: server sync, mix integration, shareable boards, notes, and groups.

### Candidate B - Shareable URL Board

- Shape: Encode pinned paths in URL query, similar to `?open=`.
- Rationale: Easy to send a one-off board to another user.
- Tradeoff: Long URLs, no durable personal persistence, and collisions with existing open-tab URL semantics.
- Defers: persistent board ownership and large file sets.

### Candidate C - Server-Side Mix-Backed Pins

- Shape: Reuse `mix` as the persisted collection model and expose create/update/delete from the web UI.
- Rationale: Aligns with `pack --from-mix` and future CLI/web handoff.
- Tradeoff: Requires backend mutation design, file store lifecycle, validation, and migration of deferred routes.
- Defers: frontend-only MVP.

### Candidate D - Browser-Style Pinned Tabs

- Shape: Split `tabs.paths` into pinned and unpinned sections in the TabBar.
- Rationale: Familiar browser metaphor and minimal new route surface.
- Tradeoff: Conflates "currently open" with "important for later"; the existing tab design explicitly treats tabs as session state.
- Defers: optional future visual marker for pinned open files.

### Candidate E - Task Board With Notes And Groups

- Shape: A workspace board with pinned files, notes, groups, and review/progress states.
- Rationale: More useful for long reviews and agent handoffs.
- Tradeoff: Larger product surface before validating the core pin action.
- Defers: non-MVP work.

## Challenge Decision

Chosen direction: Candidate A, with limited affordances from Candidate D.

Decision: conditional GO to SHAPE.

MVP contract:

- Pins are browser-local, scoped by repo root, and stored separately from tabs.
- Pins are files only, represented by repo-relative path plus metadata needed for ordering.
- Board route is `#/pins`.
- Board is path-first and may show lightweight derived metadata, but it must not require loading file contents.
- Existing `tabs.paths`, `?open=`, split-pane state, and `mix` are not source-of-truth for pins.
- `mix` integration is a future bridge, not part of the first implementation.

## Proposal

### Problem

During repository exploration, users need a durable list of important files. Tabs help immediate navigation, but tabs are not a stable review set. Search and tree navigation can rediscover files, but do not preserve user intent.

### Proposed Solution

Add a `Pins` board to `ctx browse`. Users can pin a file from common file-action surfaces, then open a dedicated board that shows all pinned files for the current repo root.

The first implementation is frontend-only. It stores pins in localStorage under a versioned key scoped by the absolute repo root. Each pin stores the repo-relative path, pinned timestamp, and optional last seen display fields. The board renders from this local state and uses existing navigation helpers to open files or open them to the side.

### In Scope

- New `pins` route and top-nav entry.
- New `PinnedFilesBoard.svelte` component.
- New `pins.svelte.ts` store with add, remove, toggle, move, clear, and load/save behavior.
- Pin/unpin actions in Tree row context menu, File detail toolbar/context menu, Tab context menu, and command palette.
- Per-root localStorage namespace using `repo.root` when available.
- Stale entry handling for paths that no longer load.
- Board keyboard navigation and accessible controls.

### Out Of Scope

- Backend persistence.
- `mix` mutation or `pack --from-pins`.
- Pin sharing by URL.
- Direct editing of pinned file contents.
- Metadata batch API.
- Directory pins.
- Groups, labels, notes, status columns, or review assignments.

### User Flow

1. The user opens `ctx browse` and navigates through files normally.
2. The user chooses `Pin File` from a file row, tab, file toolbar, file context menu, or command palette.
3. The UI stores the file path in the current root's pin list if not already pinned.
4. The user opens `Pins` from top navigation or command palette.
5. The board shows pinned files in user-defined order.
6. The user opens a pinned file, opens it to the side on desktop, copies its path, moves it, or unpins it.
7. After reload, the board restores the same pinned file list for the same repo root.
8. If a pinned path no longer exists, the board marks it stale and still allows unpin/copy path.

### Data Shape

```ts
type PinnedFile = {
  path: string;        // repo-relative file path
  pinnedAt: number;    // epoch ms
  lastOpenedAt?: number;
  label?: string;      // optional cached basename, derived from path
};

type PinsState = {
  rootKey: string;
  files: PinnedFile[];
  loaded: boolean;
  persistence: 'ok' | 'unavailable' | 'error';
  error: string | null;
};
```

Storage key:

```text
ctx-pins:v1:<repo-root>
```

`<repo-root>` is the absolute root from the served project. If it is not available yet, the store may keep an in-memory pending list, but it must save only after the root key is known.

### Board Layout

- Header: `Pinned files`, count, path filter, and `Clear all`.
- Empty state: tells the user to pin files from the tree, file view, or tabs.
- Row/card fields: basename, repo-relative path, pinned time if available, stale marker if the file cannot be opened.
- Row/card actions: Open, Open to Side, Copy Path, Move Up, Move Down, Unpin.
- Desktop can use compact rows; mobile should remain a single-column list.

### Assumptions

- Pins are personal browser state, not team state.
- Repo-relative `path` is the stable identity within a served root.
- File existence validation can be lazy. A stale pin is acceptable as long as it is visible and removable.
- Users will commonly pin fewer than 100 files. The MVP cap can be 200 pins per root.
- localStorage persistence is acceptable for first version because existing view preferences already use it.

### Validation Hypothesis

If users can pin files and see them on a board, they will keep fewer "parking lot" tabs open and spend less time rediscovering files during review, implementation planning, and LLM handoff.

### Fail Condition

- Pinning changes tab, split-pane, or file route behavior unexpectedly.
- Pins from different repo roots appear together.
- localStorage failure silently loses an active pin action without feedback.
- Board requires backend changes to deliver MVP value.
- Missing files disappear automatically without user control.

## L1 - Requirements

Scope mode: `Standard`. MVP is frontend-only, local-first, file-only, and repo-root scoped.

| ID | Type | Priority | Requirement |
|---|---|---:|---|
| REQ-PIN-001 | Functional | MVP | The UI shall let users pin and unpin individual files by repo-relative path. |
| REQ-PIN-002 | Functional | MVP | Pin actions shall be available from Tree file row context menu, File detail toolbar/context menu, Tab context menu, and Command Palette. |
| REQ-PIN-003 | Functional | MVP | The app shall provide a top-level `Pins` board route at `#/pins`. |
| REQ-PIN-004 | Functional | MVP | The board shall list pinned files for the current repo root in stable user-defined order. |
| REQ-PIN-005 | Functional | MVP | The board shall support Open, Open to Side, Copy Path, Move Up, Move Down, Unpin, and Clear All actions. |
| REQ-PIN-006 | Functional | MVP | The pin store shall persist across reloads using localStorage scoped by repo root. |
| REQ-PIN-007 | Functional | MVP | Duplicate pin attempts shall be idempotent and must not create duplicate board rows. |
| REQ-PIN-008 | Functional | MVP | Missing or renamed pinned files shall remain visible as stale entries until the user unpins them. |
| REQ-PIN-009 | Cross-functional | MVP | The MVP shall not require backend APIs, Rust handler changes, or `mix` mutation. |
| REQ-PIN-010 | Cross-functional | MVP | The MVP shall not change existing tab, split-pane, file route, `?open=`, or context-menu behavior except adding pin actions. |
| REQ-PIN-011 | Cross-functional | MVP | Pin controls and board rows shall be keyboard accessible and screen-reader understandable. |
| REQ-PIN-012 | Cross-functional | MVP | Persistence failure shall be communicated and shall keep in-memory state usable for the current session. |
| REQ-PIN-013 | Cross-functional | MVP | The board shall avoid loading full file contents just to render the pinned list. |
| REQ-PIN-014 | Cross-functional | MVP | Pin state shall remain local to the browser and shall not be uploaded or shared by default. |
| REQ-PIN-015 | Functional | Should | The board should provide a path filter for large pin lists. |
| REQ-PIN-016 | Functional | Should | The command palette should expose `Open Pins Board`, `Pin Active File`, and `Unpin Active File` commands. |

## L2 - Detail

### L2-Biz

- The board represents "files I intentionally marked as important", not "files currently open".
- The first version optimizes for individual review and exploration, not team sharing.
- Pins help users return to a working set without keeping all files open as tabs.
- Pins may become a later bridge to `mix` and `pack`, but the MVP should validate the core workflow before adding server persistence.

### L2-Dev

- Add route support in `web/src/lib/router.svelte.ts`:
  - Extend `RouteName` with `pins`.
  - Parse `#/pins`.
  - Add `toPinsHash()`.
- Add `web/src/lib/pins.svelte.ts`:
  - Maintain `PinsState`.
  - Read/write localStorage with a versioned per-root key.
  - Normalize paths by rejecting empty path, `.` path, directory paths when known, and duplicate paths.
  - Preserve insertion order unless the user reorders.
  - Enforce `PIN_LIMIT = 200` per root.
  - Keep in-memory state if localStorage throws.
- Add `web/src/components/PinnedFilesBoard.svelte`:
  - Render the current root's pinned paths.
  - Provide path filter and row actions.
  - Use `navigate(toFileHash(path))` for Open.
  - Use `openRight(path)` for Open to Side when non-mobile.
  - Use `navigator.clipboard.writeText(path)` for Copy Path.
  - Avoid fetching file content for each pin.
- Update `web/src/App.svelte`:
  - Add top nav link.
  - Add right pane branch for `route.name === 'pins'`.
  - Ensure direct `#/pins` load can initialize repo-root scoped state. If repo root is not known through the tree pane, the board may request enough project metadata to learn it, or wait until `repo.root` is available and show a loading state.
- Update action surfaces:
  - `TreeNode.svelte`: add `Pin File` or `Unpin File` for files.
  - `FileDetail.svelte`: add toolbar pin state and context-menu pin action.
  - `TabBar.svelte`: add `Pin File` or `Unpin File` to tab context menu.
  - `commands.ts`: add pin-related commands gated by active file or route.
- Stale handling:
  - A pin can be stale if opening it fails or if optional metadata lookup cannot find it.
  - Stale rows must keep Copy Path and Unpin available.
  - Stale rows must not be auto-deleted.

### L2-Design

- Use existing quiet, dense `ctx browse` styling. Avoid a marketing-style board.
- The board should read as a work surface, not a decorative card gallery.
- Keep rows compact and scannable:
  - basename first
  - full repo-relative path second
  - actions grouped on the right on desktop, below on narrow screens
- Use existing button/context-menu conventions.
- Do not use only color to indicate pinned/stale state.
- The active pin affordance should be visible in File detail and Tab context menus.
- The board empty state should be actionable and short.

### L2-Test

- Unit-test the pins store:
  - duplicate prevention
  - add/remove/toggle
  - reorder bounds
  - per-root storage separation
  - localStorage unavailable/error fallback
  - max pin cap
- Component-test or E2E-test board flows:
  - pin from file surface
  - open board
  - reorder
  - unpin
  - reload persistence
  - no regression to tab route URL write-back

## L3 - Acceptance Criteria

### AC-PIN-001 - Pin from tree row

**Linked:** REQ-PIN-001, REQ-PIN-002, REQ-PIN-007, REQ-PIN-011

Given a file row is visible in the file tree
When the user opens the row context menu and chooses `Pin File`
Then the file is added to the current root's pin list
And repeating the same action does not create a duplicate.

### AC-PIN-002 - Unpin from an already pinned tree row

**Linked:** REQ-PIN-001, REQ-PIN-002, REQ-PIN-007

Given a file row is already pinned
When the user opens the row context menu
Then the menu offers `Unpin File`
And choosing it removes that path from the current root's pin list.

### AC-PIN-003 - Pin active file from file detail

**Linked:** REQ-PIN-001, REQ-PIN-002, REQ-PIN-010, REQ-PIN-011

Given a file is loaded in File detail
When the user activates the pin control in the toolbar or context menu
Then the active file is pinned or unpinned
And the existing Copy, Hex, View, Diff, History, Wrap, Find, and Split actions keep their existing behavior.

### AC-PIN-004 - Pin from tab context menu

**Linked:** REQ-PIN-001, REQ-PIN-002, REQ-PIN-010

Given a file path is visible in the TabBar
When the user opens its context menu and chooses `Pin File`
Then that path is pinned
And the tab remains open or closed only according to the user's existing tab action.

### AC-PIN-005 - Open Pins route

**Linked:** REQ-PIN-003, REQ-PIN-004, REQ-PIN-011

Given the app is loaded
When the user navigates to `#/pins` through the top nav or command palette
Then the Pins board is shown
And the board announces its pinned file count to assistive technology.

### AC-PIN-006 - Per-root persistence

**Linked:** REQ-PIN-004, REQ-PIN-006, REQ-PIN-014

Given a user pins files in repo root A
When the browser reloads the same served root
Then the same pins are restored
And pins from a different repo root B are not shown.

### AC-PIN-007 - Board row actions

**Linked:** REQ-PIN-005, REQ-PIN-011, REQ-PIN-013

Given the board contains at least one pinned file
When the user activates a row's Open, Open to Side, Copy Path, or Unpin action
Then the action performs the named behavior using existing navigation, pane, clipboard, or pin-store helpers
And the board does not fetch full file content just to render the row.

### AC-PIN-008 - Reorder pinned files

**Linked:** REQ-PIN-004, REQ-PIN-005, REQ-PIN-006, REQ-PIN-011

Given the board contains multiple pinned files
When the user moves a pinned file up or down
Then the board order changes
And the changed order persists after reload for the same repo root.

### AC-PIN-009 - Clear all requires confirmation

**Linked:** REQ-PIN-005, REQ-PIN-012

Given the board contains pinned files
When the user chooses Clear All
Then the UI requires an explicit confirmation
And only after confirmation are all pins for the current root removed.

### AC-PIN-010 - Stale pins stay removable

**Linked:** REQ-PIN-008, REQ-PIN-005

Given a pinned file was deleted or renamed after it was pinned
When the board cannot open or validate that path
Then the row is marked stale
And Copy Path and Unpin remain available
And the stale pin is not automatically deleted.

### AC-PIN-011 - Persistence failure degrades explicitly

**Linked:** REQ-PIN-006, REQ-PIN-012

Given localStorage is unavailable or throws
When the user pins or unpins a file
Then the current in-memory pin list updates for the session
And the UI communicates that pins may not persist after reload.

### AC-PIN-012 - Existing tabs remain independent

**Linked:** REQ-PIN-004, REQ-PIN-010

Given a file is pinned but not open as a tab
When the user opens and closes unrelated tabs
Then the pinned file remains pinned
And closing a tab does not unpin its path.

### AC-PIN-013 - Existing open URL remains tab-only

**Linked:** REQ-PIN-006, REQ-PIN-010

Given a file route contains `?open=A,B`
When the app seeds tabs from the URL
Then the app does not add A or B to the pin list unless the user explicitly pins them.

### AC-PIN-014 - Pin cap fails closed

**Linked:** REQ-PIN-012, REQ-PIN-014

Given the current root already has 200 pinned files
When the user tries to pin another file
Then the new file is not added
And the user receives feedback explaining that the pin limit was reached.

### AC-PIN-015 - Board filter is path-based

**Linked:** REQ-PIN-015, REQ-PIN-011

Given the board has multiple pinned files
When the user enters text in the board filter
Then the visible rows are filtered by case-insensitive repo-relative path substring
And clearing the filter restores the full pin list.

### AC-PIN-016 - Local-only privacy

**Linked:** REQ-PIN-009, REQ-PIN-014

Given the user pins or unpins files
When the action completes
Then no `/api/pins`, `/api/mix`, or other backend mutation is required by the MVP
And no pin list is uploaded or shared by default.

## Traceability Matrix

| REQ | Type | Acceptance Criteria | Status |
|---|---|---|---|
| REQ-PIN-001 | Functional | AC-PIN-001, AC-PIN-002, AC-PIN-003, AC-PIN-004 | Locked |
| REQ-PIN-002 | Functional | AC-PIN-001, AC-PIN-002, AC-PIN-003, AC-PIN-004 | Locked |
| REQ-PIN-003 | Functional | AC-PIN-005 | Locked |
| REQ-PIN-004 | Functional | AC-PIN-005, AC-PIN-006, AC-PIN-008, AC-PIN-012 | Locked |
| REQ-PIN-005 | Functional | AC-PIN-007, AC-PIN-008, AC-PIN-009, AC-PIN-010 | Locked |
| REQ-PIN-006 | Functional | AC-PIN-006, AC-PIN-008, AC-PIN-011, AC-PIN-013 | Locked |
| REQ-PIN-007 | Functional | AC-PIN-001, AC-PIN-002 | Locked |
| REQ-PIN-008 | Functional | AC-PIN-010 | Locked |
| REQ-PIN-009 | Cross-functional | AC-PIN-016 | Locked |
| REQ-PIN-010 | Cross-functional | AC-PIN-003, AC-PIN-004, AC-PIN-012, AC-PIN-013 | Locked |
| REQ-PIN-011 | Cross-functional | AC-PIN-001, AC-PIN-003, AC-PIN-005, AC-PIN-007, AC-PIN-008, AC-PIN-015 | Locked |
| REQ-PIN-012 | Cross-functional | AC-PIN-009, AC-PIN-011, AC-PIN-014 | Locked |
| REQ-PIN-013 | Cross-functional | AC-PIN-007 | Locked |
| REQ-PIN-014 | Cross-functional | AC-PIN-014, AC-PIN-016 | Locked |
| REQ-PIN-015 | Functional | AC-PIN-015 | Locked |
| REQ-PIN-016 | Functional | AC-PIN-005 | Locked |

Traceability completeness: 100% REQ -> AC, 100% AC -> REQ.

## Spec Quality Gate

Status: PASS. Locked on explicit user sign-off (`lock`) on 2026-07-02.

- Ambiguity: PASS for MVP defaults. Browser-local, file-only, repo-root scoped behavior is explicit.
- Completeness: PASS. Every in-scope requirement has at least one acceptance criterion.
- Consistency: PASS. Tabs, split panes, and pins remain separate concepts.
- Testability: PASS. Acceptance criteria are observable through UI state, localStorage behavior, route behavior, and board actions.
- Scope coherence: PASS. Server persistence, mix mutation, shareable boards, directories, notes, and groups are deferred.
- Lock status: signed off and promoted from draft to locked spec.

## Open Questions / Deferred Decisions

MVP defaults locked by user sign-off:

- Persistence model: browser-local localStorage scoped by repo root.
- Board route: `#/pins`.
- Pin target: files only, no directories.
- Pin limit: 200 files per repo root.
- Board metadata: path-first rendering without full file-content fetch.
- Missing file behavior: mark stale, do not auto-delete.

Deferred decisions:

- Whether pins should later export to or import from `mix`.
- Whether a shareable `#/pins?files=` URL should exist.
- Whether pins should support notes, labels, groups, or review states.
- Whether pinning open tabs should visually pin them inside `TabBar`.
- Whether a backend batch metadata endpoint is worthwhile after MVP usage is proven.
- Whether `ctx pack` should accept a browser pin board as input.

## Build-path Decision

Status: `apex` selected on 2026-07-02.

Selected path: `/nexus apex` for bounded one-shot implementation, verification, and delivery.

Use `orbit` only if the implementation later expands into a longer unattended loop with machine-checkable AC completion.
