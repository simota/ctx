# ctx CLI Git Log Viewer Spec

## Metadata

- Slug: `ctx-cli-git-log-viewer`
- Feature title: Terminal Git Log Viewer
- Status: `locked`
- Current phase: `LOCK`
- Date: 2026-06-30
- Owner: TBD
- Build-path decision: `feature` (`ctx log` MVP)
- Current implementation slice: simple `git log` + `git diff` viewer first; ctx-specific signals are deferred until the base viewer is readable.

## L0 - Vision

### Problem

`ctx browse` already provides a web Git Log view, and plain `git log` provides chronological history in the terminal. What is missing is a fast terminal-native viewer that lets a developer inspect repository history while staying in the shell, with enough `ctx` context to understand why a commit matters.

The user wants to quickly check git history inside the terminal, not open a browser. A plain git log viewer is not enough; the viewer should differentiate by connecting commits to ctx-specific context such as touched files, symbols, token footprint, co-change/relations signals, and handoff-ready evidence.

### Audience

- Terminal-first developers
- Reviewers checking recent changes
- Maintainers doing code archaeology
- New contributors trying to understand why files changed
- Agent/LLM users who need compact, citable history context

### Job To Be Done

When working in a repository from the terminal, inspect recent commits, changed files, and relevant diffs quickly, while using `ctx` metadata to understand the code impact and extract useful context without switching tools.

### Success Definition

A user can run `ctx log` from the terminal, skim recent commits interactively, open changed-file and diff details, and see `ctx`-specific context that a standard `git log` viewer does not provide: token footprint, symbol hints, query bridge, and impact signals.

## Reuse And Constraints

Existing reusable assets:

- `crates/ctx-git/src/lib.rs` provides `repo_log`, `commit_files`, `file_log`, `commit_diff`, `worktree_diff`, and co-change graph primitives.
- `web/src/components/GitLogList.svelte`, `GitCommitDetail.svelte`, and `GitCoChangeGraph.svelte` prove the desired git data model and user flows already exist in the web surface.
- `crates/ctx-cli/src/main.rs` dispatches native subcommands via manual `run_*_command(args)` handlers.
- `crates/ctx-tui` is ratatui-based but currently focused on file-tree selection and pack workflows, not git history.
- `crates/ctx-cli` already depends on `ctx-git`, so a terminal history command can reuse git data without adding a new backend boundary.

Constraints:

- Keep the MVP terminal-native; do not require `ctx browse`.
- Do not reimplement `git` history parsing when `ctx-git` already provides safe wrappers.
- Validate refs conservatively to avoid git option injection.
- Bound expensive history reads with explicit limits.
- Handle unborn branches, non-git directories, and non-TTY invocations deliberately.
- Preserve the existing web Git Log behavior; this feature should add a CLI/TUI surface, not replace web.

## Scope

### In Scope

- Terminal-first git log viewer concept.
- Differentiation from standard `git log` / `tig` / generic log viewers.
- Reuse of existing `ctx-git` history APIs.
- `ctx log` command shape, context signals, and traceable acceptance criteria.

### Out Of Scope

- Browser-only Git Log shortcut.
- Full IDE replacement.
- Network or remote hosting features.
- Mutating git state.
- Commit editing, rebasing, staging, or checkout workflows.
- Full co-change graph UI and handoff actions.

## Candidate Directions

### Candidate A - Context Log

- Shape: A terminal-native recent commit viewer that shows commit list, changed files, and diff detail, with `ctx` metadata alongside the git facts.
- Differentiation: Each commit can surface context that plain `git log` does not know: file token footprint, file roles, symbols touched, and compact changed-file summaries.
- Fit: Strong MVP fit because it reuses `ctx_git::repo_log`, `commit_files`, and `commit_diff`, and can stay read-only.

### Candidate B - Impact Log

- Shape: A history viewer organized around possible blast radius, using dependency impact, relations, or co-change data to show what a commit may affect.
- Differentiation: Strong code-review value: the user sees likely impact, not only files changed.
- Tradeoff: Heavier MVP because impact and co-change can be expensive, noisy, and need careful explanation to avoid false certainty.

### Candidate C - Archaeology Log

- Shape: A terminal-native history explorer that starts from a path, symbol, or query and narrows the commit list to the history relevant to that code.
- Differentiation: Standard git can filter by path, but `ctx` can present this as a code-understanding workflow: query -> matching files/symbols -> relevant commits -> changed files/diffs.
- Fit: Good MVP fit if the first version supports path filtering and leaves full semantic query/symbol filtering as incremental follow-up.

### Candidate D - Handoff Log

- Shape: A viewer that lets the user turn a commit, file diff, or selected range into a pack/evidence handoff for review, issue comments, or LLM prompts.
- Differentiation: Strong `ctx` workflow integration.
- Tradeoff: It is more of a downstream action layer than the core viewer; best deferred until the browsing loop is proven.

## Challenge Decision

Chosen direction: Candidate A + Candidate C integrated MVP.

Decision: conditional GO to SHAPE.

MVP contract:

- Terminal-native, read-only viewer.
- Primary command opens recent commits quickly.
- Commit detail exposes changed files and per-file diff inspection.
- Path filtering is in MVP.
- Token footprint, symbol hints, query bridge, and impact signals are in the target MVP.
- `ctx` differentiation must be visible in the default view, not hidden behind an advanced mode.

Pressure test:

- Necessity: Pass. The web Git Log exists, but the user explicitly wants terminal-native quick inspection.
- Differentiation: Pass because the selected-commit detail includes token footprint, symbol hints, query bridge context when used, and impact signals. A plain curses wrapper around `git log` fails this spec.
- Scope: Pass with guardrails. The MVP includes lightweight impact signals from static imports/importers, while full co-change graph UI and evidence handoff stay out.
- Feasibility: Pass. `ctx-git` already owns repo log, commit files, and diff data; `ctx-cli` already has native command wiring.
- Risk: Medium. A full ratatui app can grow quickly; MVP must define a narrow keymap and non-interactive fallback behavior.

## Proposal

### Problem

Terminal-first users can already run `git log`, and `ctx browse` already has a web Git Log. The missing workflow is a quick terminal-native log viewer that preserves the speed of staying in the shell while adding `ctx` context for code understanding.

### Proposed Solution

Add a read-only terminal git log viewer to `ctx` that opens recent commits in an interactive terminal surface. The MVP combines:

- Context Log: recent commit list with changed files, diff inspection, and visible `ctx` metadata.
- Archaeology Log: path-first history filtering, with a designed path toward symbol/query filtering.

The viewer should make the default experience meaningfully different from `git log`: the commit list and detail pane should expose code-understanding context, such as touched symbols, file token footprint, changed-file roles, or compact `ctx pack`-style context hints.

### In Scope

- A terminal-native read-only viewer command.
- Recent commit list with selected commit detail.
- Changed-file list for the selected commit.
- Diff inspection for a selected changed file.
- Path filtering in MVP.
- Token footprint, symbol hints, query bridge, and impact signals.
- Non-interactive error behavior for non-TTY use.
- Conservative ref validation and bounded history limits.

### Out Of Scope

- Mutating git state: checkout, reset, rebase, commit, staging, or patch apply.
- Browser-only implementation.
- Full web feature parity.
- Full co-change graph as default MVP.
- Persistent notes, issue creation, or LLM chat.
- Remote repository hosting integrations.

### User Flow

1. User runs the viewer from a repository, for example `ctx log`.
2. The terminal opens a two-pane history view: commits on the left/top, details on the right/bottom depending on terminal size.
3. The user moves through commits with keyboard navigation.
4. The detail surface shows subject, author, date, full hash, changed files, and a compact `ctx` context summary.
5. The user opens a changed file to inspect a diff.
6. The user can start focused: `ctx log --path web/src/App.svelte`.
7. If the terminal is not interactive, the command fails fast with a clear message unless a later `--plain`/`--json` mode is explicitly requested.

### Command Shape

Preferred MVP command:

```sh
ctx log [--limit N] [--ref REF] [--path PATH] [--query QUERY]
```

Command name decision: use `ctx log`.

### Viewer Model

MVP layout:

- Commit list: short hash, subject, relative date, author, changed-file count.
- Commit detail: full hash, parents, author/email, timestamp, subject, changed files.
- Changed-file row: status, path, additions/deletions, binary marker, optional token/file-role signal.
- Diff view: selected file diff from `ctx_git::commit_diff`.

Default `ctx` differentiation signals:

- File token footprint for changed files.
- Touched symbol hints when symbol extraction is available.
- Path/query archaeology filters that bridge `ctx where` results to commit history.
- Static impact signals from imports/importers.

User decision: all four context signals are in the target MVP:

- Token footprint.
- Symbol hints.
- Query bridge.
- Impact signals.

To keep this bounded, the MVP computes these signals lazily and labels best-effort results clearly.

## L1 - Requirements

### `REQ-LOG-001`: `ctx log` command

- Input: `ctx log [--limit N] [--ref REF] [--path PATH] [--query QUERY]`.
- Processing: open an interactive terminal viewer when stdin/stdout are TTYs.
- Output: read-only commit history view.
- Error conditions:
  - non-git repository -> non-zero exit with actionable error.
  - non-TTY without explicit plain/json mode -> non-zero exit with `ctx log: requires an interactive terminal (TTY)`.
  - invalid ref -> non-zero exit before invoking git.

### `REQ-LOG-002`: Commit list

- Input: repository root, optional ref, optional path/query narrowing, limit.
- Processing: load recent commits using existing `ctx-git` history functions.
- Output: bounded commit list with short hash, subject, author, relative/absolute date, and changed-file count when available.
- Constraint: default limit is `100`; accepted range is `1..=500`.

### `REQ-LOG-003`: Commit detail and diff inspection

- Input: selected commit and selected changed file.
- Processing: fetch changed files via `ctx_git::commit_files`; fetch file diff via `ctx_git::commit_diff(parent, commit, path)`.
- Output: changed-file list and line-level diff with added/deleted/context lines.
- Error conditions:
  - binary diff -> show binary marker, not raw bytes.
  - truncated diff -> show truncation marker.
  - root commit -> diff against empty parent.

### `REQ-LOG-004`: Token footprint signal

- Input: selected commit's changed file paths.
- Processing: estimate current-worktree token count for each changed file when it exists and is readable.
- Output: per-file token footprint and commit-level total for available files.
- Constraint: deleted/missing/binary/unreadable files must show `tokens unavailable`, not `0`.

### `REQ-LOG-005`: Symbol hints

- Input: selected changed file and its diff lines.
- Processing: use `ctx_symbols::extract` on the current worktree file when supported. Map changed line numbers to nearest preceding symbol as a best-effort "near symbol" hint.
- Output: compact symbol hints such as `near function load_config` or `symbols unavailable`.
- Constraint: symbol hints are current-worktree context, not guaranteed commit-time symbols.

### `REQ-LOG-006`: Query bridge

- Input: `ctx log --query QUERY`.
- Processing: run the existing `ctx_where` search over the current worktree to find relevant paths, then show commits touching those paths.
- Output: viewer starts in a narrowed archaeology mode with matched paths, reasons, and deduplicated relevant commits.
- Constraint: default query path fan-out is capped at top `10` paths unless overridden by a later flag.

### `REQ-LOG-007`: Path archaeology

- Input: `ctx log --path PATH` or positional path if accepted in final parser design.
- Processing: use file history for the given path.
- Output: commits that touched the path, newest first, with the same detail/diff viewer.

### `REQ-LOG-008`: Impact signals

- Input: selected changed file.
- Processing: use `ctx_relations` import graph for supported file types.
- Output: imports/importers counts and top importers as a blast-radius hint.
- Constraint: unsupported file types show `impact unavailable`; importers are hints, not proof of runtime impact.

### `REQ-LOG-009`: Read-only safety

- The viewer must not mutate git state.
- No checkout, reset, rebase, staging, commit, patch apply, or branch creation actions are in scope.
- All git refs must be validated before being passed to git.

### `REQ-LOG-010`: Degraded-mode honesty

- Every unavailable context signal must display a reason category: unsupported, missing, binary, unreadable, too large, or not computed yet.
- The UI must not silently omit a requested signal in the selected commit detail.

## L2 - Detail

### Command Interface

```sh
ctx log [--limit N] [--ref REF] [--path PATH] [--query QUERY]
```

Reserved for later:

```sh
ctx log --plain [--limit N] [--ref REF] [--path PATH] [--query QUERY]
ctx log --json  [--limit N] [--ref REF] [--path PATH] [--query QUERY]
```

Parser integration:

- Add `log` to `COMMANDS` in `crates/ctx-cli/src/main.rs`.
- Dispatch to a new native command module under `crates/ctx-cli/src/commands/`.
- Follow the existing manual parser pattern used by `where`, `relations`, and `replay`.

### Viewer Layout

The MVP can use a simple ratatui-style layout:

- Commit list pane.
- Commit summary/detail pane.
- Changed files pane.
- Diff/context pane.

Small terminals may stack panes vertically. The viewer must keep text readable rather than forcing a fixed four-pane layout.

### Keyboard Contract

Minimum keymap:

- `j` / `Down`: next commit or next row.
- `k` / `Up`: previous commit or previous row.
- `Enter`: open selected commit/file.
- `Esc` / `Backspace`: go back from diff to file list or detail.
- `/`: filter within currently loaded rows.
- `q`: quit.

### Data Flow

Default `ctx log`:

1. Resolve repo root from current working directory.
2. Validate optional `--ref`.
3. Load commits with `ctx_git::repo_log(root, limit, ref)`.
4. Render commit list immediately.
5. When a commit is selected, load `commit_files`.
6. Compute context signals for selected commit lazily.
7. When a file is selected, load `commit_diff`.

Path mode:

1. Validate and normalize `--path`.
2. Use `ctx_git::file_log(root, path, limit)`.
3. Reuse the same detail/diff viewer.

Query mode:

1. Build `ctx_where::FileInput` corpus using existing ignore behavior.
2. Run `ctx_where::search_with_options`.
3. Take top matched file paths.
4. Load each matched path's file history.
5. Deduplicate commits by full hash.
6. Show the matched paths and reasons beside the commit list.

### Context Signal Semantics

Token footprint:

- Source: current worktree file content via `ctx_tokens`.
- Meaning: approximate LLM context cost of the current file, not historical blob size.
- Display: `12.4k tok` per file and commit subtotal for available files.

Symbol hints:

- Source: current worktree symbols via `ctx_symbols::extract`.
- Mapping: changed line -> nearest preceding symbol line in the same file.
- Display: at most `3` hints per changed file in summary; full list can appear in file detail.
- Honesty label: `current symbols`.

Query bridge:

- Source: `ctx_where` result paths and reasons.
- Display: `query: routing -> 7 paths -> 18 commits`, with top matched paths.
- Commit relevance: a commit is relevant if it appears in at least one matched path's file history.

Impact signals:

- Source: `ctx_relations` import graph.
- Display: `imports 4 / importers 9` plus top `3` importers.
- Meaning: static import blast-radius hint. It does not claim runtime reachability.

### Data Shapes

Internal view model sketch:

```rust
struct LogCommitView {
    hash: String,
    hash_full: String,
    subject: String,
    author: String,
    author_email: String,
    date: i64,
    parents: Vec<String>,
    matched_paths: Vec<String>,
    context: CommitContextSummary,
}

struct CommitContextSummary {
    changed_files: usize,
    available_token_total: i64,
    unavailable_token_files: usize,
    symbol_hints: Vec<SymbolHint>,
    impact_hints: Vec<ImpactHint>,
}

struct SymbolHint {
    path: String,
    line: i32,
    kind: String,
    name: String,
    confidence: &'static str, // "near-line" | "file-level"
}

struct ImpactHint {
    path: String,
    imports: usize,
    importers: usize,
    top_importers: Vec<String>,
}
```

### Performance Strategy

- Initial list must not compute symbols, tokens, relations, or diffs.
- Context signals are computed for the selected commit only.
- Query mode may do a repository walk; it must show a loading state before entering the viewer.
- Relation graph building is cached by `ctx_relations` where available; still treat it as lazy.
- Large changed-file commits may show summarized context only.

### Testing Plan

- `ctx-git` tests: no new producer behavior required unless new helper functions are added.
- `ctx-cli` tests: parser, non-TTY behavior, invalid ref, path mode, query mode, read-only command behavior.
- `ctx-tui` tests: snapshot tests for list/detail/diff states if the implementation uses ratatui rendering.
- Fixture tests should include:
  - normal commit with text diff.
  - root commit.
  - binary file.
  - deleted file.
  - unsupported symbol language.
  - query matching multiple paths with duplicated commits.

## L3 - Acceptance Criteria

### `AC-LOG-001`: command opens terminal viewer

Given a git repository with commits  
When the user runs `ctx log` in an interactive terminal  
Then a read-only terminal viewer opens  
And the commit list contains at most the default limit of `100` commits.

### `AC-LOG-002`: non-TTY fails honestly

Given stdout or stdin is not a TTY  
When the user runs `ctx log` without `--plain` or `--json`  
Then the command exits non-zero  
And stderr contains `ctx log: requires an interactive terminal (TTY)`.

### `AC-LOG-003`: commit detail loads changed files

Given a selected commit with changed files  
When the user opens commit detail  
Then the viewer shows each changed file path  
And shows status, additions, deletions, and binary marker where applicable.

### `AC-LOG-004`: file diff inspection

Given a selected changed text file  
When the user opens the file diff  
Then the viewer shows added, deleted, and context lines with line numbers  
And indicates when the diff is truncated.

### `AC-LOG-005`: token footprint visible

Given a selected commit with readable changed files in the current worktree  
When commit detail context is loaded  
Then each readable changed file shows a token footprint  
And deleted, missing, binary, or unreadable files show `tokens unavailable`.

### `AC-LOG-006`: symbol hints visible

Given a selected changed file in a supported language with extractable symbols  
When the viewer loads context hints  
Then the file detail shows at least one best-effort symbol hint near changed lines  
And the UI labels the hint as current-worktree context.

### `AC-LOG-007`: query bridge narrows commits

Given a repository where `ctx_where` returns matching paths for `QUERY`  
When the user runs `ctx log --query QUERY`  
Then the viewer shows matched paths and match reasons  
And the commit list is limited to commits touching those matched paths.

### `AC-LOG-008`: path archaeology narrows commits

Given a valid file path  
When the user runs `ctx log --path PATH`  
Then the commit list shows commits touching that path  
And commit detail/diff inspection still works.

### `AC-LOG-009`: impact signal visible

Given a selected changed file supported by `ctx_relations`  
When context hints are loaded  
Then the viewer shows imports/importers counts  
And shows up to `3` top importer paths when present.

### `AC-LOG-010`: invalid ref rejected

Given a ref value starting with `-` or containing characters outside the allowed ref set  
When the user runs `ctx log --ref REF`  
Then the command rejects the ref before invoking git  
And exits non-zero with an invalid-ref error.

### `AC-LOG-011`: read-only guarantee

Given any viewer navigation or inspection action  
When the user uses the MVP keymap  
Then no git state changes are performed  
And no files are modified.

### `AC-LOG-012`: degraded signals are explicit

Given a selected commit where a context signal cannot be computed  
When the detail view renders  
Then the view shows the signal as unavailable with a reason category  
And does not silently hide the signal.

## Traceability Matrix

| Requirement | Acceptance Criteria |
|-------------|---------------------|
| `REQ-LOG-001` | `AC-LOG-001`, `AC-LOG-002` |
| `REQ-LOG-002` | `AC-LOG-001`, `AC-LOG-010` |
| `REQ-LOG-003` | `AC-LOG-003`, `AC-LOG-004` |
| `REQ-LOG-004` | `AC-LOG-005`, `AC-LOG-012` |
| `REQ-LOG-005` | `AC-LOG-006`, `AC-LOG-012` |
| `REQ-LOG-006` | `AC-LOG-007` |
| `REQ-LOG-007` | `AC-LOG-008` |
| `REQ-LOG-008` | `AC-LOG-009`, `AC-LOG-012` |
| `REQ-LOG-009` | `AC-LOG-011` |
| `REQ-LOG-010` | `AC-LOG-012` |

## Spec Quality Gate Draft

- Ambiguity: pending review after command name and signal scope decisions. `ctx log` and all four signals are now decided.
- Completeness: every L1 requirement has at least one L3 acceptance criterion.
- Consistency: MVP is terminal-native and read-only; browser and git mutation workflows are out of scope.
- Testability: all ACs are observable via CLI/TUI behavior, fixtures, or snapshot tests.
- Scope coherence: impact/query/symbol/token are included, but computed lazily to keep MVP bounded.

## Considered But Rejected

- Full graph-based Impact Log in MVP: deferred because co-change graph UI is heavier and can be noisy. Lightweight static import/importer impact signals are included.
- Handoff Log in MVP: deferred because handoff actions depend on a stable browsing model.
- Browser shortcut only: rejected because the user wants terminal-native quick inspection.
- Generic `git log` clone: rejected because it would not differentiate from existing terminal git viewers.

## Open Questions / Deferred Decisions

- Whether `ctx log --plain` and `ctx log --json` ship in the same MVP or remain reserved flags.
- Whether positional `ctx log PATH` should alias `ctx log --path PATH`.
- Whether symbol hints should remain nearest-preceding-line only, or whether a later helper should expose symbol ranges.
- Whether impact signals should include co-change in addition to static import/importer counts.
- Whether the implementation extends `ctx-tui` or creates a focused `commands/log.rs` terminal surface first.
