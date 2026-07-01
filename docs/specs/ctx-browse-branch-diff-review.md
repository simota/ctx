# ctx Browse Branch Diff Review Spec

## Metadata

- Slug: `ctx-browse-branch-diff-review`
- Feature title: Branch Diff Review in `ctx browse`
- Status: `locked`
- Current phase: `LOCK`
- Date: 2026-07-01
- Owner: ctx maintainers
- Build-path decision: `apex`

## L0 - Vision

### Problem

`ctx browse` has commit-level Git Log and per-file diff viewing, but it does not provide a review surface that starts from "only the files changed between a base branch/ref and a compare branch/ref."

Reviewers need to inspect a branch or PR-like change set by first seeing the changed-file list, status, and additions/deletions, then opening only the file diffs that matter. Walking commit history is a different workflow and adds noise when the review target is the branch delta.

Existing branch/worktree metadata and per-file revision diff APIs can be reused, but the product is missing a base/head changed-files API, URL state for a ref range, and a two-ref selection UI.

### Audience

- Maintainers reviewing a local branch before merge
- Developers self-reviewing branch changes before opening a PR
- Agents or reviewers that need a compact, citable changed-file surface
- Contributors comparing linked worktrees or local refs

### Job To Be Done

When reviewing local branch work in `ctx browse`, compare two refs and inspect only the files changed between them, with lazy per-file diffs for code review.

### Success Definition

A user can open a deep-linkable `ctx browse` review view, select or encode `base` and `head`, see only files changed between those refs, and expand individual files to inspect diffs without mutating git state.

## Reuse And Constraints

### Existing Reusable Assets

- `ctx-git` exposes local branch and worktree metadata via `branches` and `worktrees`.
- `ctx-web` already serves `/api/git/branches`, `/api/git/worktrees`, `/api/git/commit-files`, and `/api/git/commit-diff`.
- `web/src/components/GitCommitDetail.svelte` already implements changed-file rows, expand/collapse behavior, lazy diff loading, wrap toggle, and jump between visible diff targets.
- `web/src/components/GitLogList.svelte` already has a compact git-area header and a segmented control pattern for Git Log subviews.
- `web/src/lib/router.svelte.ts` already owns hash-route parsing and hash helpers for deep links.

### Constraints

- The feature is read-only. It must not checkout, merge, rebase, stage, apply patches, or mutate git state.
- Refs must be validated before invoking git so a malicious ref cannot become a git option.
- Expensive diff bodies should remain lazy. Initial range load should fetch changed-file metadata only.
- The URL must preserve enough state to reopen the same review target.
- The first implementation should target local refs/branches/worktrees. Remote-provider PR integrations are out of scope.

## Candidate Directions

### Candidate A - Compare Refs Review View

- Shape: Add a dedicated branch/ref comparison surface under the Git area, with two ref inputs (`base`, `head`), a changed-file manifest, status/stats rows, and lazy per-file diff expansion.
- Backend: Add a range changed-files API such as `/api/git/changed-files?base=&head=&mode=`.
- Route: Encode comparison state in the hash route, for example `#/gitlog/review?base=main&head=feature&mode=merge-base`.
- Rationale: This directly matches the reviewer's mental model: choose what to compare, then inspect only the files in that delta.
- Resolution: use PR-style `merge-base(base, head)..head` as the default, while supporting direct comparison as an explicit mode.

### Candidate B - Changed-Only Tree / Filter View

- Shape: Reuse the file-tree browsing model, but filter the tree to the range manifest so reviewers can navigate changed files in their existing spatial context.
- Backend: Same range manifest API as Candidate A.
- Route: Extend tree/file routes with range query state, or provide a review route that renders a changed-only tree in the left pane.
- Rationale: It avoids making review feel like a separate mini-app and preserves directory context that a flat list loses.
- Deferred decision: whole browse-tree filtering is out of scope for v1; review-screen filtering remains possible after the range manifest contract is proven.

### Candidate C - Resolved Review Session

- Shape: When a user requests a branch/ref comparison, resolve `base` and `head` to stable object IDs and return both requested refs and resolved OIDs in the manifest response.
- Backend: The manifest response becomes a review-session contract: requested refs, effective base/head OIDs, comparison mode, file list, and truncation flags.
- Route: The user-facing URL may keep ref names, while copy/share affordances can include resolved OIDs for reproducibility.
- Rationale: Branch names move. Review links and cache keys need stable OIDs if the same review must be reproducible.
- Resolution: preserve user-entered refs in the active route for v1, and expose resolved OIDs in the manifest so a stable permalink can be added without changing the API contract.

### Candidate D - Git Log Integrated Range Review

- Shape: Keep the feature inside the existing Git Log route with an additional `Review` segment next to `Commits` and `Relations`; reuse `GitCommitDetail` file-row and lazy diff UI patterns.
- Backend: Add range manifest API; keep `/api/git/commit-diff` for per-file diff by passing selected `base`/`head`.
- Route: Extend `#/gitlog` parsing to recognize `#/gitlog/review?...`.
- Rationale: Smallest navigational change, fits the current Git surface, and reduces the amount of new layout code.
- Resolution: keep the first version under `#/gitlog/review`; the UI label should present it as a Git-area Review subview, not a commit-history-only workflow.

### Candidate E - Minimal API First

- Shape: Implement only the backend range manifest contract first, then wire the UI as a thin consumer of that contract.
- Backend: Add a `ctx_git::changed_files_between(base, head, mode)`-style primitive and expose it through `ctx-web`.
- Response sketch: `{ requested_base, requested_head, effective_base, effective_head, mode, files: [{ status, path, old_path?, additions, deletions, binary }] }`.
- Rationale: The missing domain primitive is the hard boundary; once stable, multiple UI variants can consume it.
- Resolution: represent added, modified, deleted, and renamed explicitly; preserve `raw_status` for statuses not modeled in the first implementation.

## Challenge Decision

Chosen direction: Candidate A + Candidate C + Candidate D.

Decision: conditional GO to SHAPE.

The feature should be a file-first range review view inside the existing Git area:

- Use `#/gitlog/review?...` as the first navigational home, alongside the existing `Commits` and `Relations` subviews.
- Add a range changed-file manifest API rather than trying to aggregate single-commit `commit_files` results.
- Resolve requested refs to effective base/head object IDs and return those IDs in the manifest response.
- Keep the initial page load metadata-only; load per-file diffs lazily with the selected effective base/head.
- Default the review-oriented comparison mode to `merge-base`, equivalent to `merge-base(base, head)..head`.

### Decision Rationale

- Logos: A net range manifest is the correct git primitive. Summing commit-file lists can misrepresent the final branch delta.
- Pathos: Reviewers think in changed files first, not commit chronology first. The UI should make the file set primary.
- Sophia: Keeping the view under the existing Git surface reduces navigation churn and implementation scope while preserving a clear path to a future standalone review route if needed.

### Scope Pressure

- Keep: range manifest, ref selectors, lazy file diffs, stable effective OIDs, deep-linkable route.
- Defer: hosted PR integrations, review comments, approvals, full tree filtering, and mutation workflows.
- Watch: rename/deleted/binary handling, moving branch refs, and large range performance.

## Proposal

### Problem

`ctx browse` can show commit history and a selected commit's changed files, but branch review often starts from a different question: "what is the final file delta between this branch and its base?" The current Git Log flow makes reviewers infer that delta from commits, which is noisy and can be wrong for amended, reverted, or rebased work.

### Proposed Solution

Add a read-only `Review` subview to the existing Git area. The view compares a `base` ref and `head` ref, produces a range manifest of changed files, and lets the user expand individual files to inspect the diff for that exact effective range.

Default route shape:

```text
#/gitlog/review?base=main&head=feature&mode=merge-base
```

Default backend shape:

```text
GET /api/git/changed-files?base=main&head=feature&mode=merge-base
```

The response is a lightweight review manifest, not a full diff bundle. It includes requested refs, effective resolved OIDs, comparison mode, changed-file entries, summary counts, and truncation/error flags. The UI then calls existing per-file diff machinery with the effective base/head when a file row is expanded.

### User Flow

1. User opens Git Log and switches to `Review`.
2. The view preselects a sensible `base` and `head` when possible, while allowing manual branch/ref selection.
3. The user can also open a deep link with `base`, `head`, and `mode` already encoded.
4. The page loads only the changed-file manifest.
5. The user scans status, path, additions/deletions, binary marker, and optional old path for renames.
6. The user expands one file or all files to fetch diffs lazily.
7. The user can copy or reopen a stable review link that includes, or can reveal, the effective resolved OIDs.

### Comparison Modes

- `merge-base` (default): compare `merge-base(base, head)` to `head`, matching the common PR-review mental model.
- `direct`: compare `base` directly to `head`, matching explicit `base..head` range inspection.

The UI must label the active mode clearly. The API must return the effective base/head OIDs so users can understand what was actually compared.

### Implementation Fit

- Reuse existing branch/worktree/tag metadata fetchers where possible.
- Reuse the `GitCommitDetail` changed-file row and lazy diff interaction patterns, but generalize them away from a single `hash`.
- Reuse `fetchFileCommitDiff(path, from, to)` for per-file diff once the manifest provides effective refs.
- Add new `ctx_git` and `ctx-web` range-manifest functions for the missing net diff primitive.

### Alternative Framings Considered

- "Extend commit history review": rejected as the primary framing because commit chronology is useful context but not the user's main review object.
- "Filter the whole file tree to changed files": deferred because it adds global browse state and directory-tree semantics before the range manifest contract is proven.
- "Create a top-level Review app": deferred because the existing Git area already owns branches, worktrees, commits, and relation views.

### Risk Notes

- Moving refs can make a ref-name URL show different results later. Mitigation: expose effective OIDs and provide a stable permalink path or action.
- Rename/copy/submodule handling can complicate path-based diff loading. Mitigation: include `old_path` and status metadata in the manifest, and handle unsupported statuses explicitly.
- Large ranges can be expensive if full diffs load eagerly. Mitigation: manifest first, lazy diff bodies, server-side caps, and clear truncation state.

## L1 - Requirements

Scope mode: `Standard` — the feature spans frontend routing/UI, Rust web handlers, and `ctx-git` primitives, but the behavior remains one bounded read-only workflow.

### Functional Requirements

#### `REQ-BDR-001`: Review route and navigation

- Priority: `Must`
- The Git area must expose a `Review` subview alongside `Commits` and `Relations`.
- The review subview must be reachable by URL state containing `base`, `head`, and `mode`.
- The default route shape is `#/gitlog/review?base=<ref>&head=<ref>&mode=merge-base`.

#### `REQ-BDR-002`: Ref selection controls

- Priority: `Should`
- The review view should provide base and head selectors populated from available local branches and worktrees.
- Users should be able to type or paste a valid ref/hash when it is not present in the selector list.
- The UI should preserve selected refs in the route after the review target changes.

#### `REQ-BDR-003`: Range changed-file manifest

- Priority: `Must`
- The backend must expose a read-only endpoint that returns the net changed-file manifest between a requested base and head.
- The manifest must include requested refs, effective resolved OIDs, comparison mode, summary counts, and file entries.
- File entries must include status, path, additions, deletions, and binary marker; rename metadata should include `old_path` when available.

#### `REQ-BDR-004`: Lazy per-file diff loading

- Priority: `Must`
- The initial review view must not fetch full diff bodies for every changed file.
- Expanding a file row must fetch that file's diff for the selected effective base/head range.
- Binary files and unsupported textual diffs must render safe explanatory states rather than failing the whole review.

#### `REQ-BDR-005`: Review file-list interactions

- Priority: `Should`
- The file list should support expand/collapse per file, expand/collapse all, line wrapping, and jump-to-next/previous visible diff target.
- File rows should show status, path, additions/deletions, binary marker, and old path when relevant.
- Opening the current working-tree file should remain available for files that exist at `head` and resolve inside the served root.

#### `REQ-BDR-006`: Comparison mode clarity

- Priority: `Should`
- The UI must label the active comparison mode as `merge-base` or `direct`.
- `merge-base` means compare `merge-base(base, head)` to `head`.
- `direct` means compare `base` directly to `head`.

#### `REQ-BDR-007`: Empty and error states

- Priority: `Should`
- The review view must distinguish loading, no changes, invalid ref, git failure, truncated manifest, and per-file diff failure states.
- Invalid refs must not clear the user's selected refs.
- Error messages must be actionable and must not expose sensitive local data beyond the current repository context.

### Cross-Functional Requirements

#### `CFR-BDR-001`: Read-only safety

- Priority: `Must`
- The feature must never run git mutations such as checkout, merge, rebase, reset, stage, apply, or commit.
- Ref inputs must be validated before they reach git command execution.
- Refs beginning with `-` or containing unsupported characters must be rejected.

#### `CFR-BDR-002`: Performance and bounded work

- Priority: `Must`
- The changed-file manifest request must avoid full diff body generation.
- Full diff bodies must be lazy and bounded by existing or explicit server-side caps.
- Large or truncated responses must be marked clearly in the manifest or diff response.

#### `CFR-BDR-003`: Reproducibility

- Priority: `Must`
- The manifest response must include effective base and head OIDs.
- The UI must expose enough information for the user to understand when a branch-name comparison has resolved to specific commits.
- A future stable permalink path must remain possible without changing the manifest contract.

#### `CFR-BDR-004`: Accessibility and responsive behavior

- Priority: `Should`
- Controls and file rows must have accessible names.
- Keyboard users must be able to move through the review flow without relying on pointer-only interactions.
- The review view must remain usable in the existing mobile/narrow layout.

## L2 - Detail

### L2-Biz

- Outcome: reduce review setup time for local branch changes by making the changed-file set the first visible artifact.
- Primary user value: reviewers can evaluate final branch delta without mentally reconstructing it from commit history.
- Non-goal: this is not an approval system, PR comment system, or git mutation workflow.
- Success signal: users can open a local review route and inspect branch changes without leaving `ctx browse`.

### L2-Dev

#### Backend API Contract

```text
GET /api/git/changed-files?base=<ref>&head=<ref>&mode=merge-base|direct
```

- `base`: required ref/hash input.
- `head`: required ref/hash input.
- `mode`: optional; defaults to `merge-base`.
- Success: `200 OK` with a changed-file manifest.
- Invalid request: `400` with an existing `response::error`-style JSON error such as `bad_request`, `invalid_ref`, or `invalid_revision`.
- Git/runtime failure: `500` with a non-sensitive error code and message.

Response sketch:

```json
{
  "requested_base": "main",
  "requested_head": "feature",
  "mode": "merge-base",
  "effective_base": "0123456789abcdef...",
  "effective_head": "fedcba9876543210...",
  "merge_base": "0123456789abcdef...",
  "summary": {
    "files": 3,
    "additions": 18,
    "deletions": 4,
    "binary_files": 1
  },
  "limit": 1000,
  "truncated": false,
  "files": [
    {
      "status": "modified",
      "path": "web/src/App.svelte",
      "additions": 10,
      "deletions": 2,
      "binary": false
    }
  ]
}
```

Validation contract:

- `base` and `head` must be non-empty.
- `mode` must be omitted, `merge-base`, or `direct`; omitted mode resolves to `merge-base`.
- Ref/hash input must use a conservative allowlist:
  - accepted characters: ASCII letters, ASCII digits, `.`, `_`, `/`, `@`, and `-`
  - maximum length: 255 characters
  - must not start with `-`
  - must not contain `..`, `@{`, `//`, whitespace, NUL, backslash, quotes, semicolon, pipe, ampersand, redirection characters, or colon
  - must not end with `/` or `.`
- Commit hashes of 7 to 40 hex characters are valid if they resolve to commits.
- The backend must pass git arguments as discrete command arguments after validation and must not interpolate refs into shell strings.
- Path-based git commands must continue to use the existing safe path resolution and pathspec separation behavior.

File status contract:

- `added`, `modified`, and `deleted` are mandatory.
- `renamed` should be represented with `path` and `old_path` when available.
- Other git statuses may be represented with `raw_status` and a safe `modified` fallback if the first implementation cannot model them precisely.

#### `ctx-git` Contract

- Add a net range primitive, conceptually `changed_files_between(repo_root, base, head, mode)`.
- Use git's net diff output for the effective range, not an aggregation of commit-level file changes.
- Resolve refs to OIDs before building the manifest.
- For `merge-base` mode, compute the merge base and use it as the effective base.
- For `direct` mode, use the requested base as the effective base.
- Keep branch/tag/hash resolution conservative and reject option-like refs before command execution.
- Apply a named server-side manifest limit, initially `1000` changed-file entries.
- If the manifest limit is exceeded, return only the bounded file list, set `truncated: true`, and include the applied `limit`.
- The range manifest must not contain diff hunks, per-line diff bodies, or unified diff text.

#### Frontend Contract

- Extend hash routing to recognize `#/gitlog/review` and parse `base`, `head`, and `mode`.
- Add a route helper such as `toGitReviewHash({ base, head, mode })`.
- Add a client API helper such as `fetchChangedFiles(base, head, mode)`.
- Add a review component, conceptually `GitRangeReview.svelte`, that consumes the manifest and lazy-loads diffs.
- Generalize or extract reusable file-row/diff rendering behavior from `GitCommitDetail.svelte` where practical.
- Per-file diff loading should call the existing commit-diff endpoint with the manifest's effective base/head OIDs.

#### Data Flow

1. Router parses `base`, `head`, and `mode`.
2. Review component requests `/api/git/changed-files`.
3. Server validates refs, resolves effective OIDs, computes the net changed-file manifest, and returns metadata only.
4. UI renders summary and changed-file rows.
5. User expands a file.
6. UI requests `/api/git/commit-diff?path=<path>&from=<effective_base>&to=<effective_head>`.
7. UI renders diff, binary state, no-change state, or per-file error.

### L2-Design

- The existing Git segmented control should become `Commits | Review | Relations`.
- Review controls should be compact and work-focused: base selector, head selector, comparison mode control, and refresh/apply action when needed.
- The changed-file list is the primary content. Commit history may remain available through the `Commits` tab, not inline as the dominant view.
- File rows should be dense and scannable, matching existing Git detail styling.
- Diff rendering should preserve existing semantic colors, line-number gutters, wrap toggle, and truncation notes.
- Empty state should explicitly say that the selected range has no file changes.
- Invalid-ref state should keep the controls visible so users can correct the range.

## L3 - Acceptance Criteria

### `AC-BDR-001`: Review route opens the review subview

- Priority: `CRITICAL`
- Linked requirements: `REQ-BDR-001`, `REQ-BDR-002`

```gherkin
Given `ctx browse` is open on a git repository
When the user opens `#/gitlog/review?base=main&head=feature&mode=merge-base`
Then the Git area shows the `Review` subview
And the review controls show base `main`, head `feature`, and mode `merge-base`
```

### `AC-BDR-011`: Ref controls support listed and typed refs

- Priority: `HIGH`
- Linked requirements: `REQ-BDR-002`, `REQ-BDR-001`

```gherkin
Given the review view is open
When local branch `feature` is available from the branches API
Then `feature` is selectable as the head ref
When the user enters a valid commit hash that is not in the selector list and applies the range
Then the review route preserves that typed head value in the URL
And the review manifest is requested with the typed head value
```

### `AC-BDR-002`: Manifest API returns net changed files

- Priority: `CRITICAL`
- Linked requirements: `REQ-BDR-003`, `CFR-BDR-003`

```gherkin
Given branch `feature` adds `new.txt`, modifies `keep.txt`, deletes `gone.txt`, and renames `old.txt` to `renamed.txt` relative to `main`
When the review manifest is requested for base `main` and head `feature`
Then the response lists exactly `new.txt`, `keep.txt`, `gone.txt`, and `renamed.txt`
And the file metadata matches:
  | path        | status   | old_path | additions | deletions | binary |
  | new.txt     | added    |          | 2         | 0         | false  |
  | keep.txt    | modified |          | 1         | 1         | false  |
  | gone.txt    | deleted  |          | 0         | 3         | false  |
  | renamed.txt | renamed  | old.txt  | 0         | 0         | false  |
And the response includes effective base and head OIDs
```

### `AC-BDR-003`: Merge-base mode uses the merge base as effective base

- Priority: `HIGH`
- Linked requirements: `REQ-BDR-006`, `CFR-BDR-003`

```gherkin
Given `main` has advanced after `feature` branched
When the review manifest is requested with mode `merge-base`
Then the response effective base is the merge base of `main` and `feature`
And the changed-file list reflects the final delta from that merge base to `feature`
```

### `AC-BDR-004`: Direct mode compares the requested refs directly

- Priority: `HIGH`
- Linked requirements: `REQ-BDR-006`, `REQ-BDR-003`

```gherkin
Given `main` has advanced after `feature` branched
When the review manifest is requested with mode `direct`
Then the response effective base is the resolved OID for `main`
And the changed-file list reflects the direct delta from `main` to `feature`
```

### `AC-BDR-005`: Initial review load avoids full diff bodies

- Priority: `HIGH`
- Linked requirements: `REQ-BDR-004`, `CFR-BDR-002`

```gherkin
Given a selected range contains multiple changed files
When the review view first loads the range
Then the view displays the changed-file manifest
And the manifest response does not contain diff hunks, per-line diff bodies, or unified diff text
And no `/api/git/commit-diff` request is made until a file row is expanded
```

### `AC-BDR-006`: Expanding a file loads its effective range diff

- Priority: `CRITICAL`
- Linked requirements: `REQ-BDR-004`, `REQ-BDR-005`

```gherkin
Given the review manifest includes `web/src/App.svelte`
When the user expands `web/src/App.svelte`
Then the view displays the diff for `web/src/App.svelte` between the manifest effective base and effective head
And the diff shows added and deleted lines with line-number gutters
```

### `AC-BDR-007`: Invalid refs are rejected safely

- Priority: `CRITICAL`
- Linked requirements: `REQ-BDR-007`, `CFR-BDR-001`

```gherkin
Given the review manifest endpoint receives an invalid request
When the invalid request matches one of:
  | base       | head     | mode       | reason             |
  |            | feature  | merge-base | missing base       |
  | main       |          | merge-base | missing head       |
  | main       | feature  | sideways   | invalid mode       |
  | -c.config  | feature  | merge-base | option-like ref    |
  | main..evil | feature  | merge-base | unsafe ref syntax  |
  | main;rm    | feature  | merge-base | unsafe character   |
Then the server rejects the request with HTTP `400`
And missing or invalid mode requests use error code `bad_request`
And unsafe ref requests use error code `invalid_ref`
And the UI keeps the selected ref values visible for correction
And the repository `HEAD`, index, and worktree status are unchanged after the request
And the feature does not execute checkout, merge, rebase, reset, add, apply, commit, or other git mutation commands
```

### `AC-BDR-008`: No-change ranges show an empty state

- Priority: `MEDIUM`
- Linked requirements: `REQ-BDR-007`, `REQ-BDR-003`

```gherkin
Given base `main` and head `main` resolve to the same commit
When the review manifest is loaded
Then the review view states that the selected range has no file changes
And the view does not show stale file rows from a previous range
```

### `AC-BDR-009`: Binary and deleted files do not break review

- Priority: `HIGH`
- Linked requirements: `REQ-BDR-004`, `REQ-BDR-005`

```gherkin
Given the review manifest includes binary file `image.png`, deleted text file `gone.txt`, and modified text file `keep.txt`
When the user expands `image.png`
Then the row displays a binary textual-diff-unavailable state and no broken diff hunk
When the user expands `gone.txt`
Then the row displays status `deleted` and the removed text lines or an explicit deleted-file diff-unavailable state
When the user expands `keep.txt`
Then the normal text diff loads successfully without reloading the whole review manifest
```

### `AC-BDR-010`: Review controls remain keyboard accessible

- Priority: `MEDIUM`
- Linked requirements: `CFR-BDR-004`, `REQ-BDR-005`

```gherkin
Given the review view is open
When the user navigates through the base selector, head selector, mode control, and file rows with a keyboard
Then each interactive control exposes an accessible name
And the user can expand a file row without pointer input
```

### `AC-BDR-012`: Truncated range responses are explicit

- Priority: `HIGH`
- Linked requirements: `CFR-BDR-002`, `REQ-BDR-007`, `REQ-BDR-003`

```gherkin
Given a selected range exceeds the server-side changed-file manifest limit
When the review manifest is loaded
Then the response marks `truncated` as true
And the response includes the applied manifest `limit`
And the review view shows a truncation state that does not imply the file list is complete
And full diff bodies are still not fetched automatically for the returned files
```

### `AC-BDR-013`: Effective OIDs are visible in the review UI

- Priority: `HIGH`
- Linked requirements: `CFR-BDR-003`, `REQ-BDR-006`

```gherkin
Given the user reviews base `main` and head `feature`
And the manifest response contains effective base OID `0123456789abcdef...` and effective head OID `fedcba9876543210...`
When the review summary renders
Then the UI exposes the active comparison mode
And the UI exposes the effective base and head OIDs, at least as copyable or expandable abbreviated commit IDs
And the displayed OIDs change after refresh if the selected branch refs resolve to different commits
```

## Traceability Matrix

| Requirement | Acceptance Criteria |
|-------------|---------------------|
| `REQ-BDR-001` | `AC-BDR-001`, `AC-BDR-011` |
| `REQ-BDR-002` | `AC-BDR-001`, `AC-BDR-011` |
| `REQ-BDR-003` | `AC-BDR-002`, `AC-BDR-004`, `AC-BDR-008`, `AC-BDR-012` |
| `REQ-BDR-004` | `AC-BDR-005`, `AC-BDR-006`, `AC-BDR-009` |
| `REQ-BDR-005` | `AC-BDR-006`, `AC-BDR-009`, `AC-BDR-010` |
| `REQ-BDR-006` | `AC-BDR-003`, `AC-BDR-004`, `AC-BDR-013` |
| `REQ-BDR-007` | `AC-BDR-007`, `AC-BDR-008`, `AC-BDR-012` |
| `CFR-BDR-001` | `AC-BDR-007` |
| `CFR-BDR-002` | `AC-BDR-005`, `AC-BDR-012` |
| `CFR-BDR-003` | `AC-BDR-002`, `AC-BDR-003`, `AC-BDR-013` |
| `CFR-BDR-004` | `AC-BDR-010` |

## Scope

### In Scope

- Base/head branch or ref comparison in `ctx browse`
- Changed-file list for the selected ref range
- Status, additions, deletions, and binary marker per changed file
- Lazy per-file diff expansion using the selected ref range
- Deep-linkable route with base/head encoded
- Clear empty, loading, invalid-ref, and non-git states

### Out Of Scope

- Git mutations: checkout, merge, rebase, staging, reset, patch apply
- Hosted PR provider APIs
- Review comments, approvals, or persistent annotations
- Full unified diff for every file on initial load
- Commit-by-commit review as the primary range workflow

## Considered But Rejected

- Commit-history-first review as the primary workflow: rejected because the final branch delta can differ from a simple mental aggregation of commits, especially after reverts, fixups, and rebases.
- Whole-tree changed-only filtering for the first version: deferred because it introduces browse-wide state and directory-tree behavior before the range manifest contract is validated.
- New top-level `#/review` route for the first version: deferred because the existing Git area already owns refs, commits, worktrees, and related Git views.
- Commit-range picker as the main selector: deferred because the requested workflow is branch/ref comparison, not arbitrary commit-span review.
- Full diff bundle on initial load: rejected because it scales poorly for large ranges and duplicates the existing lazy diff pattern.

## Open Questions / Deferred Decisions

- Should tag names be accepted in the first implementation, given current revision resolution and selector UX are branch/worktree-oriented? Default assumption: support typed valid refs conservatively, but populate selectors from branches/worktrees first.
- How should copy, type-change, and submodule statuses be represented beyond the mandatory added/modified/deleted/renamed set? Default assumption: preserve `raw_status` and fall back to a safe display label.
- Should the UI rewrite the route to resolved OIDs immediately, or expose a separate stable permalink/copy action? Default assumption: keep user-entered refs in the active route and expose effective OIDs in the manifest.
- Should deleted-file rows open a historical source view at `base`, or only show the diff? Default assumption: diff-only in v1.
- Should uncommitted worktree changes be comparable in v1? Default assumption: no; this spec covers committed refs/hashes only.

## Build-path Decision

`apex` — bounded one-shot autonomous build path. The locked spec's L3 acceptance criteria are the implementation and verification contract.
