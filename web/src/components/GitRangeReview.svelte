<script lang="ts">
  import {
    ApiCallError,
    fetchBranches,
    fetchChangedFiles,
    fetchFileCommitDiff,
    fetchWorktrees,
    type BranchEntry,
    type ChangedFileEntry,
    type ChangedFilesMode,
    type ChangedFilesResponse,
    type GitDiffResponse,
    type WorktreeEntry,
  } from '../lib/api';
  import { langFromPath, basename } from '../lib/format';
  import { navigate, route, toFileHash, toGitReviewHash } from '../lib/router.svelte';
  import { view, toggleDiffContextOnly } from '../lib/view.svelte';
  import hljs from '../lib/highlight';

  let branches = $state<BranchEntry[]>([]);
  let worktrees = $state<WorktreeEntry[]>([]);
  let refsLoaded = $state(false);
  let baseRef = $state(route.gitBase ?? '');
  let headRef = $state(route.gitHead ?? '');
  let mode = $state<ChangedFilesMode>(route.gitMode ?? 'merge-base');
  let manifest = $state<ChangedFilesResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let wrap = $state(false);
  let rootEl: HTMLElement | null = $state(null);

  let openDiff = $state<Set<string>>(new Set());
  let diffByPath = $state<Record<string, GitDiffResponse>>({});
  let diffError = $state<Record<string, string>>({});

  $effect(() => {
    let cancelled = false;
    void (async () => {
      try {
        const [b, w] = await Promise.all([fetchBranches(), fetchWorktrees()]);
        if (cancelled) return;
        branches = b.branches;
        worktrees = w.worktrees.filter((wt) => !wt.bare);
      } catch {
        if (cancelled) return;
        branches = [];
        worktrees = [];
      } finally {
        if (!cancelled) refsLoaded = true;
      }
    })();
    return () => {
      cancelled = true;
    };
  });

  $effect(() => {
    if (!refsLoaded) return;
    if (route.path !== 'review') return;
    if (route.gitBase || route.gitHead) return;
    const head = defaultHead();
    const base = defaultBase(head);
    if (base && head) {
      navigate(toGitReviewHash({ base, head, mode: 'merge-base' }));
    }
  });

  $effect(() => {
    if (route.path !== 'review') return;
    baseRef = route.gitBase ?? '';
    headRef = route.gitHead ?? '';
    mode = route.gitMode ?? 'merge-base';
  });

  $effect(() => {
    const base = route.gitBase ?? '';
    const head = route.gitHead ?? '';
    const currentMode = route.gitMode ?? 'merge-base';
    openDiff = new Set();
    diffByPath = {};
    diffError = {};
    manifest = null;
    error = null;
    if (route.path !== 'review' || !base || !head) {
      loading = false;
      return;
    }
    loading = true;
    let cancelled = false;
    fetchChangedFiles(base, head, currentMode)
      .then((r) => {
        if (cancelled) return;
        manifest = r;
        loading = false;
      })
      .catch((e) => {
        if (cancelled) return;
        loading = false;
        error = e instanceof ApiCallError ? e.message : 'Failed to load changed files.';
      });
    return () => {
      cancelled = true;
    };
  });

  function defaultHead(): string {
    return branches.find((b) => b.current)?.name ?? 'HEAD';
  }

  function defaultBase(head: string): string {
    const main = branches.find((b) => b.name === 'main' || b.name === 'master');
    if (main && main.name !== head) return main.name;
    return branches.find((b) => b.name !== head)?.name ?? head;
  }

  function worktreeRef(w: WorktreeEntry): string {
    return w.branch ?? w.head ?? '';
  }

  function worktreeLabel(w: WorktreeEntry): string {
    const name = basename(w.path);
    const at = w.branch ?? (w.head ? `${w.head} (detached)` : 'detached');
    return `${name} - ${at}`;
  }

  function applyRange(): void {
    navigate(toGitReviewHash({ base: baseRef.trim(), head: headRef.trim(), mode }));
  }

  function swapRefs(): void {
    const nextBase = headRef;
    headRef = baseRef;
    baseRef = nextBase;
    applyRange();
  }

  function shortOid(oid?: string): string {
    return oid ? oid.slice(0, 12) : '';
  }

  function statusGlyph(s: string): string {
    if (s === 'added') return 'A';
    if (s === 'deleted') return 'D';
    if (s === 'renamed') return 'R';
    return 'M';
  }

  function fileLabel(f: ChangedFileEntry): string {
    return f.old_path ? `${f.old_path} -> ${f.path}` : f.path;
  }

  function canOpenFile(f: ChangedFileEntry): boolean {
    return f.status !== 'deleted';
  }

  function hl(text: string, path: string): string {
    const lang = langFromPath(path);
    const language = lang && hljs.getLanguage(lang) ? lang : 'plaintext';
    try {
      return hljs.highlight(text, { language, ignoreIllegals: true }).value;
    } catch {
      return escapeHtml(text);
    }
  }

  // Context lines kept on each side of a change when context-only folding is
  // on (view.diffContextOnly). Mirrors FileDetail.svelte's diff folding.
  const DIFF_CONTEXT = 3;

  type DiffRow =
    | { fold: true; count: number }
    | { fold: false; idx: number; ln: GitDiffResponse['lines'][number] };

  // Collapse long runs of unchanged (`eq`) lines to a single fold marker,
  // keeping DIFF_CONTEXT lines around each change. Off (full file) when
  // view.diffContextOnly is false.
  function foldRows(lines: GitDiffResponse['lines']): DiffRow[] {
    if (!view.diffContextOnly) {
      return lines.map((ln, idx) => ({ fold: false, idx, ln }));
    }
    const n = lines.length;
    const keep = new Array<boolean>(n).fill(false);
    for (let i = 0; i < n; i++) {
      if (lines[i].type !== 'eq') {
        for (let j = Math.max(0, i - DIFF_CONTEXT); j <= Math.min(n - 1, i + DIFF_CONTEXT); j++) {
          keep[j] = true;
        }
      }
    }
    const rows: DiffRow[] = [];
    let i = 0;
    while (i < n) {
      if (keep[i]) {
        rows.push({ fold: false, idx: i, ln: lines[i] });
        i++;
      } else {
        let j = i;
        while (j < n && !keep[j]) j++;
        rows.push({ fold: true, count: j - i });
        i = j;
      }
    }
    return rows;
  }

  function escapeHtml(s: string): string {
    return s.replace(/[&<>]/g, (c) => (c === '&' ? '&amp;' : c === '<' ? '&lt;' : '&gt;'));
  }

  async function loadDiff(path: string): Promise<void> {
    if (!manifest || diffByPath[path] || diffError[path]) return;
    const activeBase = manifest.effective_base;
    const activeHead = manifest.effective_head;
    try {
      const r = await fetchFileCommitDiff(path, activeBase, activeHead);
      if (!manifest || manifest.effective_base !== activeBase || manifest.effective_head !== activeHead) {
        return;
      }
      diffByPath = { ...diffByPath, [path]: r };
    } catch (e) {
      diffError = {
        ...diffError,
        [path]:
          e instanceof ApiCallError && e.code === 'invalid_revision'
            ? 'Unable to resolve this review range.'
            : 'Failed to load diff.',
      };
    }
  }

  function toggleFile(path: string): void {
    const next = new Set(openDiff);
    if (next.has(path)) {
      next.delete(path);
      openDiff = next;
      return;
    }
    next.add(path);
    openDiff = next;
    void loadDiff(path);
  }

  let files = $derived(manifest?.files ?? []);
  let allOpen = $derived(files.length > 0 && files.every((f) => openDiff.has(f.path)));

  function toggleAll(): void {
    if (allOpen) {
      openDiff = new Set();
      return;
    }
    openDiff = new Set(files.map((f) => f.path));
    for (const f of files) void loadDiff(f.path);
  }

  function isTextInputFocused(): boolean {
    const active = document.activeElement as HTMLElement | null;
    if (!active) return false;
    const tag = active.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || active.isContentEditable;
  }

  function visibleDiffTargets(): HTMLElement[] {
    if (!rootEl) return [];
    return Array.from(rootEl.querySelectorAll<HTMLElement>('[data-range-diff-target]'));
  }

  function jumpDiff(delta: 1 | -1): void {
    if (!rootEl) return;
    const targets = visibleDiffTargets();
    if (targets.length === 0) return;
    const rootRect = rootEl.getBoundingClientRect();
    const anchor = rootRect.top + rootEl.clientHeight / 2;
    const targetCenter = (el: HTMLElement) => {
      const rect = el.getBoundingClientRect();
      return rect.top + rect.height / 2;
    };
    let target = -1;
    if (delta > 0) {
      target = targets.findIndex((el) => targetCenter(el) > anchor + 1);
      if (target === -1) target = 0;
    } else {
      for (let i = targets.length - 1; i >= 0; i--) {
        if (targetCenter(targets[i]) < anchor - 1) {
          target = i;
          break;
        }
      }
      if (target === -1) target = targets.length - 1;
    }
    targets[target]?.scrollIntoView({ behavior: 'smooth', block: 'center' });
  }

  function onGlobalKey(e: KeyboardEvent): void {
    if (!e.shiftKey || e.metaKey || e.ctrlKey || e.altKey) return;
    if (isTextInputFocused()) return;
    if (e.key === 'ArrowDown') {
      if (visibleDiffTargets().length === 0) return;
      e.preventDefault();
      jumpDiff(1);
    } else if (e.key === 'ArrowUp') {
      if (visibleDiffTargets().length === 0) return;
      e.preventDefault();
      jumpDiff(-1);
    }
  }

  $effect(() => {
    window.addEventListener('keydown', onGlobalKey);
    return () => window.removeEventListener('keydown', onGlobalKey);
  });
</script>

<section class="range-review" aria-label="branch diff review" bind:this={rootEl}>
  <header class="head">
    <h2>Review</h2>
    <form class="controls" onsubmit={(e) => { e.preventDefault(); applyRange(); }}>
      <label>
        <span>Base</span>
        <input list="git-review-refs" aria-label="Base ref" bind:value={baseRef} />
      </label>
      <button type="button" class="icon-btn" aria-label="Swap base and head refs" title="Swap refs" onclick={swapRefs}>⇄</button>
      <label>
        <span>Head</span>
        <input list="git-review-refs" aria-label="Head ref" bind:value={headRef} />
      </label>
      <label>
        <span>Mode</span>
        <select aria-label="Comparison mode" bind:value={mode}>
          <option value="merge-base">merge-base</option>
          <option value="direct">direct</option>
        </select>
      </label>
      <button type="submit" class="apply">Refresh</button>
      <datalist id="git-review-refs">
        {#each branches as b (b.name)}
          <option value={b.name}>{b.name}</option>
        {/each}
        {#each worktrees as w (w.path)}
          {#if worktreeRef(w)}
            <option value={worktreeRef(w)}>{worktreeLabel(w)}</option>
          {/if}
        {/each}
      </datalist>
    </form>
  </header>

  {#if !baseRef || !headRef}
    <p class="muted note">Choose a base and head ref to review changed files.</p>
  {:else if error}
    <p class="note err" role="alert"><code class="mono">{error}</code></p>
  {:else if loading}
    <p class="muted note" aria-busy="true">Loading changed files…</p>
  {:else if manifest}
    <div class="summary">
      <div>
        <p class="range">
          <span class="mono">{manifest.requested_base}</span>
          <span aria-hidden="true">→</span>
          <span class="mono">{manifest.requested_head}</span>
          <span class="pill">{manifest.mode}</span>
        </p>
        <p class="oids muted">
          effective
          <code class="mono" title={manifest.effective_base}>{shortOid(manifest.effective_base)}</code>
          →
          <code class="mono" title={manifest.effective_head}>{shortOid(manifest.effective_head)}</code>
        </p>
      </div>
      <p class="muted count">
        {manifest.summary.files} file{manifest.summary.files === 1 ? '' : 's'}
        <span class="add">+{manifest.summary.additions}</span>
        <span class="del">−{manifest.summary.deletions}</span>
        {#if manifest.summary.binary_files > 0}
          <span>{manifest.summary.binary_files} bin</span>
        {/if}
      </p>
    </div>

    {#if manifest.truncated}
      <p class="note warn" role="status">Changed-file list is truncated at {manifest.limit} entries. Results may be incomplete.</p>
    {/if}

    {#if files.length === 0}
      <p class="muted note">No file changes in this range.</p>
    {:else}
      <div class="files-head">
        <p class="muted count">{files.length} changed file{files.length === 1 ? '' : 's'}</p>
        <div class="head-actions">
          <button type="button" class="all-toggle" aria-pressed={allOpen} onclick={toggleAll}>{allOpen ? 'Collapse all' : 'Expand all'}</button>
          <button
            type="button"
            class="all-toggle"
            aria-pressed={view.diffContextOnly}
            title={view.diffContextOnly ? 'Showing changes only — click to expand full file' : 'Showing full file — click to collapse unchanged lines'}
            onclick={toggleDiffContextOnly}
          >Context: {view.diffContextOnly ? 'changes' : 'full'}</button>
          <label class="wrap-toggle">
            <input type="checkbox" bind:checked={wrap} />
            <span>Wrap</span>
          </label>
        </div>
      </div>
      <ul class="files" role="list">
        {#each files as f (f.path)}
          <li class="file" data-range-diff-target>
            <div class="file-row">
              <button
                type="button"
                class="file-toggle"
                class:open={openDiff.has(f.path)}
                aria-expanded={openDiff.has(f.path)}
                aria-label={`Toggle diff for ${fileLabel(f)}`}
                onclick={() => toggleFile(f.path)}
              >
                <span class="twisty" aria-hidden="true">{openDiff.has(f.path) ? '▾' : '▸'}</span>
                <span class="status status-{f.status}" title={f.raw_status || f.status}>{statusGlyph(f.status)}</span>
                <span class="path mono" title={fileLabel(f)}>{fileLabel(f)}</span>
                {#if f.binary}
                  <span class="stat binary">bin</span>
                {:else if f.additions > 0 || f.deletions > 0}
                  <span class="stat">
                    {#if f.additions > 0}<span class="add">+{f.additions}</span>{/if}{#if f.deletions > 0}<span class="del">−{f.deletions}</span>{/if}
                  </span>
                {/if}
              </button>
              {#if canOpenFile(f)}
                <button
                  type="button"
                  class="open-file"
                  title="Open file"
                  aria-label={`Open ${f.path}`}
                  onclick={() => navigate(toFileHash(f.path))}>↗</button
                >
              {/if}
            </div>

            {#if openDiff.has(f.path)}
              {#if diffError[f.path]}
                <p class="muted note">{diffError[f.path]}</p>
              {:else if !diffByPath[f.path]}
                <p class="muted note" aria-busy="true">Loading diff…</p>
              {:else if diffByPath[f.path].binary}
                <p class="muted note">Binary file - textual diff not available.</p>
              {:else if diffByPath[f.path].no_change && !diffByPath[f.path].added && !diffByPath[f.path].deleted}
                <p class="muted note">No textual changes.</p>
              {:else}
                <pre class="diff" class:wrap><code class="hljs">{#if diffByPath[f.path].added}<div class="diff-meta">New file</div>{/if}{#if diffByPath[f.path].deleted}<div class="diff-meta">File deleted</div>{/if}{#each foldRows(diffByPath[f.path].lines) as row, ri (ri)}{#if row.fold}<div class="diff-fold" aria-hidden="true">⋯ {row.count} unchanged line{row.count === 1 ? '' : 's'}</div>{:else}<div class="diff-line diff-{row.ln.type}"><span class="gutter" aria-hidden="true"><span class="g-old">{row.ln.old_num || ''}</span><span class="g-new">{row.ln.new_num || ''}</span><span class="g-sign">{row.ln.type === 'add' ? '+' : row.ln.type === 'del' ? '-' : ' '}</span></span><span class="ln-text">{@html hl(row.ln.text, f.path) || ' '}</span></div>{/if}{/each}{#if diffByPath[f.path].truncated}<div class="diff-meta">Diff truncated - use the CLI for the full diff.</div>{/if}</code></pre>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .range-review {
    height: 100%;
    overflow: auto;
    padding: 14px 18px;
  }
  .head {
    border-bottom: 1px solid var(--ctx-border);
    padding-bottom: 10px;
    margin-bottom: 10px;
  }
  h2 {
    font-size: 1.1em;
    margin: 0 0 8px;
  }
  .controls {
    display: grid;
    grid-template-columns: minmax(10ch, 1fr) auto minmax(10ch, 1fr) auto auto;
    align-items: end;
    gap: 8px;
  }
  label {
    display: grid;
    gap: 3px;
    font-size: 0.76em;
    color: var(--ctx-fg-dim);
  }
  input,
  select {
    min-width: 0;
    color: var(--ctx-fg);
    background: var(--ctx-bg-elev, var(--ctx-bg));
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 4px 6px;
    font: inherit;
  }
  .apply,
  .icon-btn,
  .all-toggle {
    background: transparent;
    border: 1px solid var(--ctx-border);
    color: var(--ctx-fg-dim);
    border-radius: 4px;
    cursor: pointer;
  }
  .apply {
    padding: 4px 10px;
  }
  .icon-btn {
    min-width: 28px;
    min-height: 28px;
  }
  .apply:hover,
  .icon-btn:hover,
  .all-toggle:hover,
  .open-file:hover,
  .file-toggle:hover {
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
  }
  .muted {
    color: var(--ctx-fg-dim);
  }
  .mono {
    font-family: var(--ctx-font-mono, var(--ctx-mono, ui-monospace, monospace));
  }
  .note {
    padding: 6px 0;
    font-size: 0.88em;
  }
  .err {
    color: var(--ctx-err);
  }
  .warn {
    color: var(--ctx-warn, var(--ctx-git-modified));
  }
  .summary {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    margin-bottom: 8px;
  }
  .range,
  .oids,
  .count {
    margin: 0;
  }
  .range {
    display: flex;
    align-items: baseline;
    gap: 6px;
    flex-wrap: wrap;
  }
  .oids {
    margin-top: 3px;
    font-size: 0.78em;
  }
  .pill {
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 1px 5px;
    font-size: 0.76em;
    color: var(--ctx-fg-dim);
  }
  .add {
    color: var(--ctx-git-added);
  }
  .del {
    color: var(--ctx-git-deleted);
  }
  .files-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 0 0 6px;
  }
  .head-actions {
    display: inline-flex;
    align-items: center;
    gap: 10px;
  }
  .all-toggle {
    font-size: 0.76em;
    padding: 2px 8px;
  }
  .wrap-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 0.78em;
    color: var(--ctx-fg-dim);
    cursor: pointer;
    user-select: none;
  }
  .files {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .file-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1 1 auto;
    min-width: 0;
    text-align: left;
    background: transparent;
    border: 0;
    color: var(--ctx-fg);
    padding: 5px 4px;
    cursor: pointer;
    border-radius: 4px;
  }
  .twisty {
    flex: 0 0 auto;
    color: var(--ctx-fg-dim);
    font-size: 0.8em;
    width: 1em;
  }
  .status {
    flex: 0 0 auto;
    width: 1.4em;
    text-align: center;
    font-weight: 600;
    font-size: 0.8em;
  }
  .status-added {
    color: var(--ctx-git-added);
  }
  .status-deleted {
    color: var(--ctx-git-deleted);
  }
  .status-renamed,
  .status-modified {
    color: var(--ctx-git-modified);
  }
  .path {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.88em;
  }
  .stat {
    flex: 0 0 auto;
    display: inline-flex;
    gap: 5px;
    font-size: 0.76em;
    font-variant-numeric: tabular-nums;
  }
  .stat.binary {
    color: var(--ctx-fg-dim);
    font-style: italic;
  }
  .open-file {
    flex: 0 0 auto;
    background: transparent;
    border: 0;
    color: var(--ctx-link);
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .diff {
    margin: 4px 0 8px 1.6em;
    padding: 8px 0;
    background: var(--ctx-bg-panel);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    overflow: auto;
    font-family: var(--ctx-font-mono, var(--ctx-mono, ui-monospace, monospace));
    font-size: 0.82em;
    line-height: 1.5;
  }
  .diff code {
    display: block;
  }
  .diff-meta {
    color: var(--ctx-fg-dim);
    padding: 2px 12px;
    font-style: italic;
  }
  .diff-fold {
    padding: 2px 12px;
    color: var(--ctx-fg-dim);
    background: var(--ctx-bg-elev);
    border-top: 1px solid var(--ctx-border);
    border-bottom: 1px solid var(--ctx-border);
    font-size: 11px;
    user-select: none;
  }
  .diff-line {
    display: flex;
    white-space: pre;
  }
  .diff-line.diff-add {
    background: color-mix(in srgb, var(--ctx-git-added) 12%, transparent);
  }
  .diff-line.diff-del {
    background: color-mix(in srgb, var(--ctx-git-deleted) 12%, transparent);
  }
  .gutter {
    flex: 0 0 auto;
    display: inline-flex;
    gap: 6px;
    padding: 0 8px;
    color: var(--ctx-fg-dim);
    user-select: none;
    border-right: 1px solid var(--ctx-border);
    margin-right: 8px;
  }
  .g-old,
  .g-new {
    display: inline-block;
    width: 4ch;
    text-align: right;
  }
  .g-sign {
    width: 1ch;
  }
  .ln-text {
    flex: 1 1 auto;
  }
  .diff.wrap .diff-line {
    white-space: pre-wrap;
  }
  .diff.wrap .ln-text {
    white-space: pre-wrap;
    word-break: break-word;
    overflow-wrap: anywhere;
    min-width: 0;
  }
  @media (max-width: 760px) {
    .controls {
      grid-template-columns: 1fr auto;
    }
    .controls label {
      grid-column: span 2;
    }
    .summary,
    .files-head {
      align-items: flex-start;
      flex-direction: column;
    }
  }
</style>
