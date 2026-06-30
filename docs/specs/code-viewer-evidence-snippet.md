# Code Viewer Evidence Snippet Spec

## Metadata

- Slug: `code-viewer-evidence-snippet`
- Feature title: Code Viewer Verifiable Line Handoff / Evidence Snippet
- Status: `locked`
- Current phase: `FEATURE_IMPLEMENTATION`
- Date: 2026-06-30
- Owner: TBD
- Build-path decision: `feature`

## L0 - Vision

### Problem

`ctx browse` の code viewer は、Source の行番号リンクや selection copy を持っているが、外部へ渡すと `path`、line range、projection、dirty/truncated state が落ちる。

そのため、LLM prompt、レビュー、Issue、調査メモに貼った根拠が、あとから同じ状態で検証しづらい。

### Audience

- コードレビュー担当者
- LLM に調査依頼する開発者
- バグ調査中の保守者
- 新規参加者
- 監査性や local-first/offline 性を重視するチーム

### Job To Be Done

コード上で根拠を見つけたとき、LLM・レビュー・Issue・チーム内相談へ、後から検証できる形でその根拠を渡したい。

### Success Definition

選択した Source 行範囲を、本文だけでなく出典と限界も含む `Evidence Snippet` として一操作でコピーできる。

受け手は snippet だけで、どのファイルのどの行範囲か、どの projection から取られたか、dirty/truncated などの制約があるかを理解できる。

## Reuse And Constraints

Existing reusable assets:

- `web/src/components/FileDetail.svelte` already owns the file detail surface, Source/Rendered/Hex state, source line DOM, copy actions, context menu actions, line links, find state, and truncated warning.
- `web/src/lib/api.ts` exposes `FileResponse` with `path`, `content`, `lines`, `git`, `size`, `tokens`, `truncated`, `lang`, and `symbols`.
- `crates/ctx-web/src/handlers/file.rs` already computes file content, line count, git status, truncation state, and metadata for `/api/file`.
- The source viewer already renders stable line anchors (`#L<n>`) and route links (`#/file/<path>?L=<n>`).

Constraints:

- Source view is a fast inspection projection, not whole-file truth; `/api/file` can truncate large files.
- Rendered, Source, and Hex have different data paths and trust boundaries.
- The current file response has git status but not necessarily commit identity in the same payload.
- MVP should not claim a selected range covers bytes or lines that are not loaded in `data.content`.
- Selection handling should preserve current copy/link/context-menu behavior.

## Scope

### In Scope

- Copy selected visible Source line ranges as Evidence Snippets.
- Include source coordinates and state metadata in the copied text.
- Make truncated/dirty/projection limitations explicit in the snippet.
- Preserve current Source line links and selection copy workflows.

### Out Of Scope

- Full IDE/LSP behavior.
- Full bidirectional source maps for every Rendered view.
- Persistent Evidence Workspace.
- Cloud sharing or collaboration.
- Chat UI.
- Treating truncated source content as whole-file truth.

## Candidate Directions

### Candidate A - Canonical Markdown Evidence

- Shape: Source line range selection produces a human-readable Markdown snippet with `path`, line range, projection, git status, truncation/dirty caveats, and fenced code.
- Rationale: Lowest-friction format for LLM prompts, GitHub issues, PR comments, and notes.
- Tradeoff: Human-readable and easy to paste, but weak as a machine-verifiable artifact.
- Defers: JSON clipboard payload, persistent evidence tray, external issue integration, signed provenance.

### Candidate B - Bounded Evidence Citation

- Shape: Evidence Snippet explicitly states that it is a bounded citation of loaded visible Source content, not immutable whole-file truth.
- Rationale: Avoids overclaiming verifiability when `/api/file` can truncate content and the current payload may not include commit identity.
- Tradeoff: More careful language and caveats make the snippet slightly more verbose.
- Defers: Commit pinning, blob OID, checkout reproduction, audit log, full provenance system.

### Candidate C - Selection-First Source Handoff

- Shape: `Copy Evidence` appears in existing Source selection/right-click workflows. Range selection wins; if no range is selected, the clicked Source line can become a single-line evidence snippet.
- Rationale: Reuses existing Source line DOM, context menu, copy helpers, and route links with minimal UX expansion.
- Tradeoff: Needs clear precedence rules for text selection vs line click vs context menu target.
- Defers: Rendered sourcemaps, Hex byte-range evidence, multi-range selection, semantic selection.

### Candidate D - Machine Metadata Clipboard

- Shape: Clipboard writes `text/plain` Markdown and, where supported, a structured `application/vnd.ctx.evidence+json` payload carrying `path`, range, projection, hash, git state, truncation state, language, and snippet body.
- Rationale: Enables future tools to consume Evidence Snippets without scraping Markdown.
- Tradeoff: Clipboard API compatibility and fallback behavior add implementation complexity.
- Defers: Cloud sharing, remote verification API, signed evidence, external tracker integrations.

### Candidate E - Minimal Review Packet

- Shape: Copy action creates a small template with sections like `Context`, `Evidence`, and `State`, leaving room for a reviewer or LLM prompt to add a claim.
- Rationale: Users often need to transfer a claim plus supporting source, not raw code alone.
- Tradeoff: More useful for review/LLM handoff, but potentially too verbose for ordinary code copy.
- Defers: Chat UI, issue creation UI, persistent notes, editable snippet composer.

### Candidate F - Projection Locator Contract

- Shape: Source is supported as line ranges in MVP; Hex and Rendered are explicitly not supported or marked as weak/derived until their locator semantics are designed.
- Rationale: Prevents Source line semantics from being falsely generalized across projections.
- Tradeoff: Keeps MVP honest but may disappoint users who expected Rendered or Hex snippets.
- Defers: Hex byte-offset snippets, Rendered-to-Source mapping, full preview sourcemaps.

## Challenge Decision

Chosen direction: A+B+C+F integrated MVP.

Decision: conditional GO to SHAPE.

MVP contract:

- Frontend-only if possible; do not change `/api/file` or backend response shape for MVP.
- Source-only; Evidence Snippet is backed by loaded `/api/file` text content and Source line DOM.
- Clipboard output is `text/plain` Markdown only.
- Range semantics are repo-relative path + 1-indexed inclusive line range.
- Route grammar remains `?L=<start>`; ranges are represented in copied text, not in URL semantics.
- Evidence Snippet is a bounded citation of currently loaded content, not immutable whole-file truth and not commit-pinned evidence.
- The copy action is user-initiated only; no background upload, persistence, or sharing.

Required caveats:

- `git` from the current response is worktree status, not commit identity.
- `data.truncated === true` means copied evidence may cover only loaded source content.
- Rendered, Hex, diff, and history views do not have the same Source line locator contract in MVP.
- Clipboard failures and non-Source selections must degrade to explicit feedback, not silent no-ops.

Implementation risk notes:

- Primary touchpoint is `web/src/components/FileDetail.svelte`.
- Existing line DOM uses `.line`, `id="L<n>"`, and `data-line`; selection normalization should derive start/end from closest Source line elements and sort ascending.
- Existing `copyToClipboard` and context menu actions are reusable.
- Avoid changing `toFileHash`, router grammar, generated `crates/ctx-web/dist`, or Rust handlers unless a later phase explicitly expands scope.
- Suggested output cap for MVP specification: 200 lines or 20KB, whichever comes first.

## Considered But Rejected

- Machine-readable clipboard payload (`application/vnd.ctx.evidence+json`) for MVP: deferred until an actual consumer exists.
- Review packet / claim template: deferred; MVP copies evidence only, not commentary.
- Commit identity, blob OID, content hash, signed provenance, or audit log: deferred because current file response does not provide them and the MVP should not claim immutable evidence.
- Persistent Evidence Workspace, session tray, chat UI, issue creation, cloud sharing: out of scope.
- Rendered sourcemaps, Hex byte-range evidence, LSP/IDE semantic handoff, multi-range selection, symbol/context bundle: out of scope for this spec.

## Proposal

### Problem

コードビューアー上の特定行を他者、Issue、review comment、agent に渡すとき、現在は「どのファイルの何行か」と「実際の引用内容」が分離しやすい。結果として、参照先の曖昧さ、手動コピーの欠落、範囲ずれ、過剰な全ファイル貼り付けが起きる。

### Proposed Solution

Source view で選択した行範囲、または右クリックした Source 行を、repo-relative path と 1-indexed inclusive line range を含む Markdown Evidence Snippet として `text/plain` clipboard にコピーできるようにする。

MVP は frontend-only を前提に、既にロード済みの Source content だけを bounded citation として扱う。

### In Scope

- Source view の行範囲選択または Source 行 context menu からのコピー操作。
- Repo-relative path + `L<start>-L<end>` の 1-indexed inclusive range。
- Clipboard は `text/plain` Markdown のみ。
- Route は現状維持で `?L=<start>` のみ。
- Snippet body 上限は 200 lines または 20KB、どちらか先に達した方。
- `data.truncated === true` の場合、コピー対象は loaded source content に限定されることを snippet に明記する。
- コピーは user-initiated action のみ。

### Out Of Scope

- Backend/API response changes.
- JSON/custom MIME clipboard format.
- Persistent workspace or saved evidence set.
- Rendered, Hex, diff, or history view support.
- Rendered sourcemaps.
- LSP/IDE integration.
- Cloud sharing.
- Commit-pinned evidence or whole-file truth guarantees.

### User Flow

1. ユーザーが Source view で通常のテキスト範囲を選択する、または Source 行を右クリックする。
2. Context menu の `Copy evidence snippet` を実行する。
3. 選択範囲が Source line DOM から導出できる場合、開始行と終了行を 1-indexed inclusive range として正規化する。
4. 選択範囲がない場合、右クリックされた Source 行を single-line snippet として扱う。
5. 範囲が 200 lines または 20KB を超える場合はコピーせず、範囲を狭める feedback を出す。
6. Clipboard に Markdown snippet が `text/plain` として入る。
7. URL/deep link は `?L=<start>` のまま維持する。

### Data And Format Shape

Internal frontend shape:

```ts
type EvidenceSnippet = {
  path: string;        // repo-relative
  startLine: number;   // 1-indexed inclusive
  endLine: number;     // 1-indexed inclusive
  language?: string;
  lines: Array<{ number: number; text: string }>;
  truncated: boolean;  // data.truncated; loaded source only
  git?: string;        // worktree status only, not commit identity
  link: string;        // current route with ?L=<start>
};
```

Clipboard Markdown shape:

````md
Source: `crates/example/src/lib.rs:L12-L18`
Link: `#/file/crates/example/src/lib.rs?L=12`
Projection: `source`
Git status: `M`
Note: bounded citation from loaded Source view. `git` reflects worktree status, not commit identity.

```rust
12 | fn example() {
13 |     // ...
18 | }
```
````

When `data.truncated === true`, the note must additionally state that the citation comes from loaded partial source content.

### Assumptions

- Frontend state already has repo-relative path, loaded source lines, language hint, `git`, and `data.truncated`.
- MVP can derive all copied content from loaded Source data without requesting new backend data.
- `git` metadata, if shown, means worktree status only and must not be presented as commit identity.
- `data.truncated` means the loaded source is partial; the snippet is not proof of whole-file contents.

### Validation Hypothesis

If users can copy a Source-only Evidence Snippet in one explicit action, then agent handoff, Issue text, and review comments will contain fewer manual citation mistakes and less over-broad file content.

Initial validation should check that copied snippets include path, inclusive range, bounded content, projection, state caveats, and a start-line link, and that oversized ranges fail closed.

### Fail Condition

- The MVP requires backend/API response changes.
- Source-only support is insufficient and Rendered/Hex/diff/history support becomes mandatory for the feature to be useful.
- A range above 200 lines or 20KB silently copies partial content.
- Snippet wording makes users believe it is commit-pinned evidence or whole-file truth.

## L1 - Requirements

Scope mode: `Standard`. MVP is Source-only, frontend-only where possible, and uses only loaded `/api/file` content.

| ID | Type | Priority | Requirement |
|---|---|---:|---|
| REQ-ES-001 | Functional | MVP | Source view の context menu に限り `Copy evidence snippet` を提供する。MVP では toolbar、Rendered、Hex、diff、history は対象外。 |
| REQ-ES-002 | Functional | MVP | Source の選択範囲から 1-indexed inclusive line range を導出し、昇順に正規化する。選択がない場合は右クリックされた Source 行を single-line evidence とする。 |
| REQ-ES-003 | Functional | MVP | Clipboard には `text/plain` Markdown のみを書き込む。JSON/custom MIME は使わない。 |
| REQ-ES-004 | Functional | MVP | Snippet は既存ロード済み `/api/file` content の bounded citation とし、全ファイル真実性や commit-pinned evidence を主張しない。 |
| REQ-ES-005 | Functional | MVP | Snippet には path、line range、`?L=<start>` link、projection、code body、必要な state caveat を含める。`git` が空なら `Git status` 行は省略する。 |
| REQ-ES-006 | Functional | MVP | `data.truncated === true` の場合、loaded partial Source content からの引用であることを明記する。 |
| REQ-ES-007 | Functional | MVP | 出力上限は 200 lines または final Markdown 20KB の早い方とし、超過時は partial-copy せず feedback を出す。 |
| REQ-ES-008 | Functional | MVP | Copy は user-initiated action のみとし、background upload、persistence、sharing、automatic clipboard write は行わない。 |
| REQ-ES-009 | Cross-functional | MVP | Backend/API response、router grammar、Rust handler、generated embedded assets の変更を MVP 要件にしない。route は `?L=<start>` のまま維持する。 |
| REQ-ES-010 | Cross-functional | MVP | Existing Source line link、selection copy、context menu behavior を regress させない。 |
| REQ-ES-011 | Cross-functional | MVP | Clipboard failure、unsupported view、unmappable selection、oversized selection は silent no-op にせず、ユーザーが次に取る行動を理解できる feedback を出す。 |

## L2 - Detail

### L2-Biz

- Evidence Snippet は、LLM prompt、Issue、review comment、調査メモに貼るための「検証可能な引用単位」とする。
- MVP の価値は、手動で path/range/content/state caveat を組み立てるミスを減らすことにある。
- Snippet は証拠の永続性や commit identity を保証しない。保証するのは「現在ロード済み Source view に基づく bounded citation」である。
- MVP の成功条件は、Source 行範囲を一操作で Markdown としてコピーでき、受け手が path、range、projection、truncated/git caveat を読めること。

### L2-Dev

- Primary touchpoint は `web/src/components/FileDetail.svelte`。
- Data source は既存 `FileResponse` の `path`, `content`, `lines`, `git`, `truncated`, `lang` を使う。新しい backend/API field は追加しない。
- Range derivation は Source view の loaded line に限定する。選択が Source line に完全に対応しない場合は copy しない。
- Range は 1-indexed inclusive。逆方向選択は `startLine <= endLine` に正規化する。
- Link は current file route の `?L=<start>` のみを使う。range URL grammar は追加しない。
- Markdown shape は `Source`, `Link`, `Projection`, optional `Git status`, `Note`, fenced code を基本形にする。
- `git` が空文字、null、undefined 相当の場合、`Git status` 行は出力しない。出力する場合も worktree status であり commit identity ではないと note で明記する。
- 20KB cap は final `text/plain` Markdown の UTF-8 byte size で判定する。
- 200 lines または 20KB を超える場合は fail closed。clipboard へ partial snippet を書かない。
- Clipboard write は context menu action からの user gesture 内で実行する。

### L2-Design

- MVP entry point は Source context menu の `Copy evidence snippet` のみ。
- Unsupported view では action を出さない。内部的に実行不能状態になった場合は明示的 feedback を出す。
- Success feedback は copy 完了が分かる短い文言にする。
- Oversized feedback は「範囲を狭める」行動が分かる文言にする。
- Failure feedback は clipboard permission/error と selection mapping failure を区別できる文言にする。
- Snippet wording は「bounded citation」「loaded Source view」「not commit identity」を誤解なく伝える。

## L3 - Acceptance Criteria

### AC-ES-001 - Source context menu only

**Linked:** REQ-ES-001, REQ-ES-009, REQ-ES-010

Given a user is viewing a loaded file in Source view
When the user opens the context menu on a Source line or Source selection
Then `Copy evidence snippet` is available
And the same MVP action is not offered from toolbar, Rendered, Hex, diff, or history views.

### AC-ES-002 - Normalize selected Source range

**Linked:** REQ-ES-002, REQ-ES-005

Given the user has selected loaded Source content spanning lines 18 through 12
When the user copies an evidence snippet
Then the snippet cites the range as lines 12 through 18
And the code body contains only loaded Source lines within that inclusive range.

### AC-ES-003 - Single-line fallback from context target

**Linked:** REQ-ES-002, REQ-ES-005

Given the user has no active Source selection
When the user opens the context menu on line 42 and copies an evidence snippet
Then the snippet cites line 42 as the evidence range
And the link uses `?L=42`.

### AC-ES-004 - Plain Markdown clipboard format

**Linked:** REQ-ES-003, REQ-ES-005, REQ-ES-008

Given a valid Source range is selected
When the user copies an evidence snippet
Then the clipboard receives `text/plain` Markdown containing Source, Link, Projection, Note, and fenced code
And no JSON or custom MIME payload is written.

### AC-ES-005 - Non-empty git status is labeled as worktree status

**Linked:** REQ-ES-005

Given the loaded file response has a non-empty `git` value
When the user copies an evidence snippet
Then the snippet includes `Git status`
And the note states that `git` reflects worktree status, not commit identity.

### AC-ES-006 - Empty git status is omitted

**Linked:** REQ-ES-005

Given the loaded file response has an empty `git` value
When the user copies an evidence snippet
Then the snippet omits the `Git status` line.

### AC-ES-007 - Truncated content caveat

**Linked:** REQ-ES-004, REQ-ES-006

Given the loaded file response has `truncated === true`
When the user copies an evidence snippet
Then the note states that the citation comes from loaded partial Source content
And the snippet does not claim to represent the whole file.

### AC-ES-008 - Bounded citation wording

**Linked:** REQ-ES-004, REQ-ES-005

Given any valid evidence snippet is copied
When the recipient reads the pasted Markdown
Then the snippet identifies itself as a bounded citation from the loaded Source view
And it does not describe itself as immutable, commit-pinned, signed, or whole-file proof.

### AC-ES-009 - Oversized selection fails closed

**Linked:** REQ-ES-007, REQ-ES-011

Given a selected Source range would exceed 200 lines or 20KB of final Markdown
When the user chooses `Copy evidence snippet`
Then no partial evidence snippet is copied
And the user receives feedback to reduce the selected range.

### AC-ES-010 - Clipboard failure is explicit

**Linked:** REQ-ES-008, REQ-ES-011

Given the browser denies or fails clipboard writing
When the user chooses `Copy evidence snippet`
Then the failure is communicated to the user
And the action does not silently appear successful.

### AC-ES-011 - Existing loaded content only

**Linked:** REQ-ES-004, REQ-ES-009

Given the current `/api/file` response is already loaded
When the user copies an evidence snippet
Then the evidence body is derived only from that loaded Source content
And no additional file-content response field is required by this MVP contract.

### AC-ES-012 - Route grammar remains stable

**Linked:** REQ-ES-005, REQ-ES-009, REQ-ES-010

Given an evidence snippet starts at line 12 and ends at line 18
When the snippet is copied
Then the link remains `?L=12`
And no range URL grammar is introduced.

### AC-ES-013 - Unsupported or unmappable selections do not copy

**Linked:** REQ-ES-001, REQ-ES-011

Given the current selection cannot be mapped entirely to loaded Source lines
When the user chooses `Copy evidence snippet`
Then no evidence snippet is copied
And the user receives feedback explaining that Source lines must be selected.

### AC-ES-014 - Copy is explicit and local-only

**Linked:** REQ-ES-008

Given no user has chosen `Copy evidence snippet`
When a Source selection changes or a file is viewed
Then the clipboard is not written automatically
And no evidence snippet is uploaded, persisted, or shared by the MVP action.

### AC-ES-015 - Existing copy and link actions remain available

**Linked:** REQ-ES-010

Given existing Source line links, normal selection copy, or existing context menu actions are used
When `Copy evidence snippet` is added
Then those existing actions continue to expose the same user-visible behavior as before.

## Traceability Matrix

| REQ | Type | Acceptance Criteria | Status |
|---|---|---|---|
| REQ-ES-001 | Functional | AC-ES-001, AC-ES-013 | Draft |
| REQ-ES-002 | Functional | AC-ES-002, AC-ES-003 | Draft |
| REQ-ES-003 | Functional | AC-ES-004 | Draft |
| REQ-ES-004 | Functional | AC-ES-007, AC-ES-008, AC-ES-011 | Draft |
| REQ-ES-005 | Functional | AC-ES-002, AC-ES-003, AC-ES-004, AC-ES-005, AC-ES-006, AC-ES-008, AC-ES-012 | Draft |
| REQ-ES-006 | Functional | AC-ES-007 | Draft |
| REQ-ES-007 | Functional | AC-ES-009 | Draft |
| REQ-ES-008 | Functional | AC-ES-004, AC-ES-010, AC-ES-014 | Draft |
| REQ-ES-009 | Cross-functional | AC-ES-001, AC-ES-011, AC-ES-012 | Draft |
| REQ-ES-010 | Cross-functional | AC-ES-001, AC-ES-012, AC-ES-015 | Draft |
| REQ-ES-011 | Cross-functional | AC-ES-009, AC-ES-010, AC-ES-013 | Draft |

Traceability completeness: 100% REQ -> AC, 100% AC -> REQ.

## Spec Quality Gate

Status: PASS.

- L1 requirements: 11.
- L3 acceptance criteria: 15.
- Traceability: 100% REQ -> AC and 100% AC -> REQ.
- Attest result: singular, testable, unambiguous, and consistent enough for LOCK.
- Lock-blocking defects: none.

## Open Questions / Deferred Decisions

- MVP open questions: none. Confirmed defaults resolve context-menu-only entry, empty `git` omission, and oversized fail-closed behavior.
- Remaining non-MVP decisions: toolbar copy action, JSON/custom MIME payload, Rendered/Hex/diff/history support, commit-pinned provenance, persistent evidence workspace.
- Metadata still TBD outside this SPECIFY step: owner and build-path decision.

## Build-path Decision

Pending mandatory build-path selection after LOCK.
