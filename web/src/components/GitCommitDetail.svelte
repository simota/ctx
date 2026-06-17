<script lang="ts">
  import {
    fetchCommitFiles,
    fetchFileCommitDiff,
    ApiCallError,
    type CommitFileEntry,
    type GitDiffResponse,
  } from '../lib/api';
  import { formatRelative, langFromPath } from '../lib/format';
  import { findCommit, loadGitLog } from '../lib/gitlog.svelte';
  import { toFileHash, navigate } from '../lib/router.svelte';
  import hljs from '../lib/highlight';

  let { hash = '' }: { hash?: string } = $props();

  // Soft-wrap long diff lines (off by default = horizontal scroll, matching
  // the file-detail diff view).
  let wrap = $state(false);

  // Syntax-highlight a single diff line for `path`'s language. Falls back to
  // escaped plaintext when the language is unknown or hljs throws.
  function hl(text: string, path: string): string {
    const lang = langFromPath(path);
    const language = lang && hljs.getLanguage(lang) ? lang : 'plaintext';
    try {
      return hljs.highlight(text, { language, ignoreIllegals: true }).value;
    } catch {
      return escapeHtml(text);
    }
  }

  function escapeHtml(s: string): string {
    return s.replace(/[&<>]/g, (c) => (c === '&' ? '&amp;' : c === '<' ? '&lt;' : '&gt;'));
  }

  function isTextInputFocused(): boolean {
    const active = document.activeElement as HTMLElement | null;
    if (!active) return false;
    const tag = active.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || active.isContentEditable;
  }

  let files = $state<CommitFileEntry[]>([]);
  let filesLoading = $state(false);
  let filesError = $state<string | null>(null);
  let rootEl: HTMLElement | null = $state(null);

  // Per-file lazy-loaded diffs, keyed by path (within the current commit).
  let openDiff = $state<Set<string>>(new Set());
  let diffByPath = $state<Record<string, GitDiffResponse>>({});
  let diffError = $state<Record<string, string>>({});

  // Commit metadata is shared from the list store; ensure it is loaded so a
  // deep link to #/gitlog/<hash> can still show the header.
  $effect(() => {
    void loadGitLog();
  });

  let commit = $derived(hash ? findCommit(hash) : undefined);

  // Reload the changed-file list whenever the selected commit changes.
  $effect(() => {
    const h = hash;
    openDiff = new Set();
    diffByPath = {};
    diffError = {};
    files = [];
    filesError = null;
    if (!h) {
      filesLoading = false;
      return;
    }
    filesLoading = true;
    let cancelled = false;
    fetchCommitFiles(h)
      .then((r) => {
        if (cancelled) return;
        files = r.files;
        filesLoading = false;
      })
      .catch((e) => {
        if (cancelled) return;
        filesLoading = false;
        filesError = e instanceof ApiCallError ? e.message : 'Failed to load changed files.';
      });
    return () => {
      cancelled = true;
    };
  });

  // Fetch a file's diff once (no-op if already loaded or errored).
  async function loadDiff(path: string): Promise<void> {
    if (diffByPath[path] || diffError[path]) return;
    try {
      // `${hash}^` resolves to the first parent (root commit -> empty before).
      const r = await fetchFileCommitDiff(path, `${hash}^`, hash);
      diffByPath = { ...diffByPath, [path]: r };
    } catch (e) {
      diffError = {
        ...diffError,
        [path]:
          e instanceof ApiCallError && e.code === 'invalid_revision'
            ? 'Unable to resolve this commit range.'
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

  // True once every changed file's diff is open.
  let allOpen = $derived(files.length > 0 && files.every((f) => openDiff.has(f.path)));

  function toggleAll(): void {
    if (allOpen) {
      openDiff = new Set();
      return;
    }
    openDiff = new Set(files.map((f) => f.path));
    for (const f of files) void loadDiff(f.path);
  }

  function statusGlyph(s: string): string {
    return s === 'added' ? 'A' : s === 'deleted' ? 'D' : 'M';
  }

  function visibleDiffTargets(): HTMLElement[] {
    if (!rootEl) return [];
    return Array.from(rootEl.querySelectorAll<HTMLElement>('[data-commit-diff-target]'));
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

<section class="commit-detail" aria-label="commit detail" bind:this={rootEl}>
  {#if !hash}
    <div class="placeholder">
      <h2>Git Log</h2>
      <p class="muted">Select a commit from the list to view its changes.</p>
    </div>
  {:else}
    <header class="head">
      <h2 class="subject" title={commit?.subject}>{commit?.subject ?? 'Commit'}</h2>
      <div class="sub muted">
        <span class="hash mono">{commit?.hash ?? hash.slice(0, 7)}</span>
        {#if commit}
          <span class="dot" aria-hidden="true">·</span>
          <span class="author">{commit.author}</span>
          <span class="dot" aria-hidden="true">·</span>
          <span class="date" title={new Date(commit.date * 1000).toISOString()}>{formatRelative(commit.date)}</span>
        {/if}
      </div>
    </header>

    {#if filesError}
      <p class="muted note">{filesError}</p>
    {:else if filesLoading}
      <p class="muted note" aria-busy="true">Loading changed files…</p>
    {:else if files.length === 0}
      <p class="muted note">No file changes in this commit.</p>
    {:else}
      <div class="files-head">
        <p class="muted count">{files.length} file{files.length === 1 ? '' : 's'} changed</p>
        <div class="head-actions">
          <button
            type="button"
            class="all-toggle"
            aria-pressed={allOpen}
            onclick={toggleAll}
          >{allOpen ? 'Collapse all' : 'Expand all'}</button>
          <label class="wrap-toggle">
            <input type="checkbox" bind:checked={wrap} />
            <span>Wrap</span>
          </label>
        </div>
      </div>
      <ul class="files" role="list">
        {#each files as f (f.path)}
          <li class="file" data-commit-diff-target>
            <div class="file-row">
              <button
                type="button"
                class="file-toggle"
                class:open={openDiff.has(f.path)}
                aria-expanded={openDiff.has(f.path)}
                onclick={() => toggleFile(f.path)}
              >
                <span class="twisty" aria-hidden="true">{openDiff.has(f.path) ? '▾' : '▸'}</span>
                <span class="status status-{f.status}" title={f.status}>{statusGlyph(f.status)}</span>
                <span class="path mono" title={f.path}>{f.path}</span>
                {#if f.binary}
                  <span class="stat binary">bin</span>
                {:else if (f.additions ?? 0) > 0 || (f.deletions ?? 0) > 0}
                  <span class="stat">
                    {#if (f.additions ?? 0) > 0}<span class="add">+{f.additions}</span>{/if}{#if (f.deletions ?? 0) > 0}<span class="del">−{f.deletions}</span>{/if}
                  </span>
                {/if}
              </button>
              <button
                type="button"
                class="open-file"
                title="Open file"
                aria-label={`Open ${f.path}`}
                onclick={() => navigate(toFileHash(f.path))}>↗</button
              >
            </div>

            {#if openDiff.has(f.path)}
              {#if diffError[f.path]}
                <p class="muted note">{diffError[f.path]}</p>
              {:else if !diffByPath[f.path]}
                <p class="muted note" aria-busy="true">Loading diff…</p>
              {:else if diffByPath[f.path].binary}
                <p class="muted note">Binary file — diff not available.</p>
              {:else if diffByPath[f.path].no_change && !diffByPath[f.path].added && !diffByPath[f.path].deleted}
                <p class="muted note">No textual changes.</p>
              {:else}
                <pre class="diff" class:wrap><code class="hljs">{#if diffByPath[f.path].added}<div class="diff-meta">New file</div>{/if}{#if diffByPath[f.path].deleted}<div class="diff-meta">File deleted</div>{/if}{#each diffByPath[f.path].lines as ln (ln)}<div class="diff-line diff-{ln.type}"><span class="gutter" aria-hidden="true"><span class="g-old">{ln.old_num || ''}</span><span class="g-new">{ln.new_num || ''}</span><span class="g-sign">{ln.type === 'add' ? '+' : ln.type === 'del' ? '-' : ' '}</span></span><span class="ln-text">{@html hl(ln.text, f.path) || ' '}</span></div>{/each}{#if diffByPath[f.path].truncated}<div class="diff-meta">Diff truncated — use the CLI for the full diff.</div>{/if}</code></pre>
              {/if}
            {/if}
          </li>
        {/each}
      </ul>
    {/if}
  {/if}
</section>

<style>
  .commit-detail {
    height: 100%;
    overflow: auto;
    padding: 14px 18px;
  }
  .placeholder {
    padding: 24px 4px;
  }
  .placeholder h2 {
    margin: 0 0 6px;
  }
  .muted {
    color: var(--ctx-fg-dim);
  }
  .mono {
    font-family: var(--ctx-font-mono, var(--ctx-mono, ui-monospace, monospace));
  }
  .head {
    border-bottom: 1px solid var(--ctx-border);
    padding-bottom: 10px;
    margin-bottom: 10px;
  }
  .head .subject {
    font-size: 1.1em;
    margin: 0 0 4px;
    word-break: break-word;
  }
  .sub {
    display: flex;
    align-items: baseline;
    gap: 6px;
    font-size: 0.85em;
    flex-wrap: wrap;
  }
  .hash {
    color: var(--ctx-accent);
  }
  .note {
    padding: 6px 0;
    font-size: 0.88em;
  }
  .files-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    margin: 0 0 6px;
  }
  .count {
    font-size: 0.82em;
    margin: 0;
  }
  .head-actions {
    display: inline-flex;
    align-items: center;
    gap: 10px;
  }
  .all-toggle {
    background: transparent;
    border: 1px solid var(--ctx-border);
    color: var(--ctx-fg-dim);
    font-size: 0.76em;
    padding: 2px 8px;
    border-radius: 4px;
    cursor: pointer;
  }
  .all-toggle:hover {
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
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
  .stat {
    flex: 0 0 auto;
    display: inline-flex;
    gap: 5px;
    font-size: 0.76em;
    font-variant-numeric: tabular-nums;
  }
  .stat .add {
    color: var(--ctx-git-added);
  }
  .stat .del {
    color: var(--ctx-git-deleted);
  }
  .stat.binary {
    color: var(--ctx-fg-dim);
    font-style: italic;
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
  .file-toggle:hover {
    background: var(--ctx-bg-elev);
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
  .open-file {
    flex: 0 0 auto;
    background: transparent;
    border: 0;
    color: var(--ctx-link);
    cursor: pointer;
    padding: 2px 6px;
    border-radius: 4px;
  }
  .open-file:hover {
    background: var(--ctx-bg-elev);
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
  /* Wrap mode: long lines fold instead of overflowing horizontally. The
     gutter stays fixed-width and the text column wraps within the row. */
  .diff.wrap .diff-line {
    white-space: pre-wrap;
  }
  .diff.wrap .ln-text {
    white-space: pre-wrap;
    word-break: break-word;
    overflow-wrap: anywhere;
    min-width: 0;
  }
</style>
