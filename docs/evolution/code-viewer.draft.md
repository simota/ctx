# Code Viewer Evolution Map Draft

Status: draft
Current phase: CHART_CHECKPOINT
Date: 2026-06-30

## Feature As-Is

`ctx browse` のコードビューアーは、source view を核にした `FileDetail` 画面である。`web/src/components/FileDetail.svelte` が file content、metadata、symbols、preview、diff/history、find、行ジャンプ、definition jump、右サイドバーを統合している。

データは `#/file/<path>?L=<n>` から `fetchFile(path,{symbols:true})` に流れ、`/api/file` が safepath 解決、read、truncate、token count、git status、tree-sitter symbols を返す。raw preview や binary/HTML/media は `/raw/*` で別配信される。

ユーザー可視機能は、syntax highlight、行番号リンク/行ハイライト、Rendered/Source/Hex 切替、Markdown/SVG/HTML/Mermaid/config/image/PDF preview、wrap/find/copy/open raw/context menu、右ペイン split、git diff/history、symbols/TOC/format別 insights/relations/evidence/test insights sidebar、クリックまたは `g d` による definition picker。

Reason for existence: ローカルリポジトリをブラウザ上で安全に探索し、LLM context curator として token、symbols、git 状態、関係情報を読みながら、必要なファイルや根拠へ素早く移動できるようにすること。

## Ground Evidence

- `web/src/components/FileDetail.svelte:955` - file load uses `fetchFile(p, { symbols: true })`.
- `web/src/components/FileDetail.svelte:1070` - chunked highlight avoids blocking large files.
- `web/src/components/FileDetail.svelte:1328` - highlighted identifiers become definition jump anchors.
- `web/src/components/FileDetail.svelte:2713` - source `<pre>` renders line links and highlighted content.
- `web/src/components/FileDetail.svelte:2736` - sidebars switch between TOC, insights, symbols, relations, evidence, and tests.
- `crates/ctx-web/src/handlers/file.rs:226` - `/api/file` reads/truncates content and returns metadata.
- `crates/ctx-web/src/handlers/symbols.rs:187` - `/api/definition` builds a symbol corpus and resolves candidates.
- `crates/ctx-web/src/handlers/raw.rs:1` - `/raw/*` serves raw bytes with protective headers.

## Excavation Set

- Source view is a fast inspection projection, not a whole-file truth surface. `/api/file` truncates large files, while `/raw/*` and hex follow separate byte paths.
- Rendered, Source, and Hex are not just display modes; they are separate projections with different trust boundaries, fetch paths, and line-coordinate fidelity.
- Line numbers are the strongest coordinate system in the viewer, but they primarily belong to Source. Preview, media, HTML, and binary views do not uniformly preserve line mapping.
- Definition jump is a helpful heuristic link layer, not an LSP-grade semantic guarantee. It depends on highlight.js spans, identifier regex fallback, same-file first symbol wins, and repo-wide lookup on demand.
- The viewer already has evidence, relations, tests, symbols, git, and tokens around the file, but these are side overlays rather than a single "context package" for review or LLM handoff.
- The strongest current experience is quick reading and line-level returnability: fast initial highlighting, `?L=`, scroll restore, pulse highlight, and safe previews.
- Likely friction: Rendered is the default for previewable files, Find highlights source lines even when a rendered view is visible, definition affordance is subtle, the Symbols toggle also hides relations/evidence/tests, and truncated files send users back to CLI/editor.

## Insights

Validated:

1. Source coordinates are the product's strongest primitive.
   - Non-obvious truth: line numbers are not just viewer decoration; they are the coordinate system that can make a code view reproducible across LLM prompts, reviews, issues, and shared links.
   - Evidence: `?L=`, gutter links, pulse highlight, scroll restore, copy/context actions, and evidence/sidebar adjacency.
   - Epistemic status: confirmed.

2. Source, Rendered, and Hex are different projections, not equivalent modes.
   - Non-obvious truth: each mode has a different data path and trust boundary (`/api/file`, `/raw/*`, client-side transforms, sandboxed iframe, data URL, hex fetch).
   - Evidence: raw preview safety handling, HTML/SVG/Markdown/Mermaid/config rendering paths, binary/hex gates.
   - Epistemic status: confirmed.

3. Truncation is a reading state, not merely a failure case.
   - Non-obvious truth: for large files, the UX question is not only "show everything"; it is "what slice am I seeing, what is missing, and where should I read next?"
   - Evidence: `/api/file` truncation, partial copy warning, hex/raw fallback, symbols and sidebar context.
   - Epistemic status: hypothesis-to-validate.

4. Definition jump is candidate evidence, not semantic certainty.
   - Non-obvious truth: current definition navigation is a useful heuristic layer, not an LSP-grade guarantee; therefore surfacing candidate reason/confidence may matter as much as the jump itself.
   - Evidence: highlight.js/regex DOM decoration, same-file first symbol wins, lazy `/api/definition` lookup, quiet empty-cache behavior.
   - Epistemic status: confirmed.

5. Side overlays become valuable when bound to a line or range.
   - Non-obvious truth: relations, evidence, tests, diff, and history are not just panels beside a file; they become AI/review context when attached to a source coordinate.
   - Evidence: `SymbolList`, `RelationsPanel`, `EvidencePanel`, `TestInsightsPanel`, diff/history views, and shared line links.
   - Epistemic status: hypothesis-to-validate.

6. The differentiator is verifiable handoff, not IDE imitation.
   - Non-obvious truth: `ctx browse` does not need to compete with a full IDE; its sharper role may be turning local code inspection into a shareable, auditable packet of evidence.
   - Evidence: fast initial read, safe previewing, line links, token/git/symbol metadata, evidence and relation sidebars.
   - Epistemic status: hypothesis-to-validate.

## Evolution Directions

Ranked chart:

1. Verifiable Line Handoff (deepen)
   - Combine Evidence Snippet and Trust Boundary Badges.
   - Value hypothesis: selected source ranges become reproducible evidence for LLM prompts, reviews, issues, and handoffs.
   - Smallest next step: add Copy evidence for visible source ranges with `path`, line range, current projection, dirty/truncated state, and snippet body.
   - Survival constraints: only claim coverage for loaded source content; mark dirty/uncommitted/truncated state explicitly; do not imply full-file truth.
   - Next recipe: `spec` for exact evidence format, then `feature` or `kaizen`.

2. Definition Candidate Evidence (deepen)
   - Value hypothesis: heuristic definition jump becomes more trustworthy when candidates explain why they are shown.
   - Smallest next step: show reason labels such as same-file symbol, exact name match, kind, path, or fallback lookup.
   - Survival constraints: avoid semantic certainty language; do not present confidence as LSP truth unless backed by richer analysis.
   - Next recipe: `spec` or narrow `feature`.

3. Source Return From Derived Views (broaden)
   - Value hypothesis: Rendered/config/find flows become safer when visible results route back to source coordinates.
   - Smallest next step: start with formats that already expose stable structure, such as Markdown headings/code blocks and config nodes.
   - Survival constraints: do not promise full preview/source maps for HTML, media, PDF, Mermaid, or arbitrary rendered DOM.
   - Next recipe: `spec`.

4. Large File Read Order (reframe)
   - Value hypothesis: truncated files are easier to inspect when the UI states what slice is visible and where to read next.
   - Smallest next step: for truncated files, show loaded range, missing-state copy, and source/symbol anchors that suggest next inspection targets.
   - Survival constraints: treat this as navigation, not generated truth; avoid AI summaries as authoritative evidence.
   - Next recipe: `spec` or telemetry-backed `kaizen`.

5. AI Context Bundle (broaden)
   - Value hypothesis: selected source ranges plus enclosing symbol, evidence, relations, tests, diff, and history create a compact handoff packet.
   - Smallest next step: export one selected range with optional enclosing symbol and one adjacent context source.
   - Survival constraints: keep bundle provenance visible; avoid turning the viewer into chat, task management, or a generic prompt workspace.
   - Next recipe: `spec`.

6. Evidence Workspace (reframe)
   - Value hypothesis: users may be collecting multiple source facts, not just reading one file.
   - Smallest next step: session-local tray of evidence snippets with Markdown export.
   - Survival constraints: defer persistence, collaboration, cloud sync, and annotation databases.
   - Next recipe: later `apex`/`spec` after smaller handoff features prove useful.

## Refuted

- Full LSP or IDE-grade semantic graph: too broad and mismatched to the current heuristic symbol/definition architecture.
- Full bidirectional source maps for every rendered mode: high complexity and false precision; start only where line mapping is reliable.
- Treating Rendered, Source, and Hex as interchangeable modes: refuted by separate data paths and trust boundaries.
- Claiming `/api/file` source view as whole-file truth: refuted by truncation behavior and separate raw/hex paths.
- Immediate Evidence Workspace as the first build: valuable as a horizon, but too broad before Evidence Snippet proves the handoff model.

## Handoff

Recommended next pick:

1. Start with Verifiable Line Handoff.
2. Write a small spec for the Evidence Snippet format and UI entry points.
3. Keep Definition Candidate Evidence and Source Return as follow-on slices.

Open choice for the next Nexus recipe:

- `spec`: define exact UX, data shape, edge cases, and acceptance criteria before code.
- `feature`: implement a narrow Copy evidence MVP directly.
- `kaizen`: polish current source view affordances first, especially trust/truncation badges and find-mode feedback.
