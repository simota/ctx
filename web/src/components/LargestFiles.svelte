<script lang="ts">
  import { fetchTree, type TreeNode } from '../lib/api';
  import { basename, formatSize, isSourceCode } from '../lib/format';
  import { navigate, toFileHash } from '../lib/router.svelte';
  import { announce } from '../lib/announce.svelte';

  interface FileRow {
    path: string;
    name: string;
    lines: number;
    size: number;
    ext: string;
  }

  // Minimum-line-count filter presets. `0` means "no filter / all files".
  const PRESETS: { label: string; lines: number }[] = [
    { label: 'All', lines: 0 },
    { label: '≥ 100', lines: 100 },
    { label: '≥ 300', lines: 300 },
    { label: '≥ 1000', lines: 1000 },
  ];

  // Cap rendered rows so a huge repo can't lock up the DOM. We surface the cap
  // explicitly (never silently truncate) so the count stays honest.
  const RENDER_CAP = 1000;

  let allFiles = $state<FileRow[]>([]);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let headingEl: HTMLHeadingElement | null = $state(null);
  let announcedCount: number | null = $state(null);

  // Filter state. `minLines` is the active minimum line-count threshold.
  let minLines = $state(0);
  // Whether to honor the repo's root `.gitignore` (exclude ignored files).
  // Default on — most users expect ignored/generated files left out.
  let respectGitignore = $state(true);
  // Extension filter: selected extensions (OR-combined). Empty = all.
  let selectedExts = $state<string[]>([]);
  // Path filter: case-insensitive substring match against the full path.
  let pathQuery = $state('');

  function extOf(path: string): string {
    const base = basename(path).toLowerCase();
    const dot = base.lastIndexOf('.');
    return dot > 0 ? base.slice(dot + 1) : '';
  }

  function flatten(node: TreeNode, out: FileRow[]): void {
    if (node.is_dir) {
      for (const c of node.children ?? []) flatten(c, out);
    } else if (isSourceCode(node.path)) {
      out.push({
        path: node.path,
        name: node.name,
        lines: node.lines ?? 0,
        size: node.size ?? 0,
        ext: extOf(node.path),
      });
    }
  }

  function load(gitignore: boolean) {
    loading = true;
    error = null;
    allFiles = [];
    announcedCount = null;
    // No `tokens: true` — line counts come from the walk for free, and
    // skipping the tiktoken pass makes this load substantially faster.
    // `gitignore` prunes `.gitignore`-matched files server-side.
    fetchTree({ gitignore: gitignore || undefined })
      .then((r) => {
        const rows: FileRow[] = [];
        flatten(r.tree, rows);
        rows.sort((a, b) => b.lines - a.lines);
        allFiles = rows;
      })
      .catch((e: unknown) => {
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        loading = false;
      });
  }

  // Reload whenever the gitignore toggle changes (effect tracks the read).
  $effect(() => {
    load(respectGitignore);
  });

  // Extension options with counts, sorted by frequency then name. Drives the
  // toggle chips; recomputed from the loaded set (post-gitignore).
  let extOptions = $derived.by<{ ext: string; count: number }[]>(() => {
    const counts = new Map<string, number>();
    for (const f of allFiles) counts.set(f.ext, (counts.get(f.ext) ?? 0) + 1);
    return [...counts.entries()]
      .map(([ext, count]) => ({ ext, count }))
      .sort((a, b) => b.count - a.count || a.ext.localeCompare(b.ext));
  });

  // Ranked + filtered view. Already sorted desc at load time, so filtering
  // preserves order. Combines: min-lines, extension (OR), and path substring.
  let filtered = $derived.by<FileRow[]>(() => {
    const q = pathQuery.trim().toLowerCase();
    const exts = selectedExts;
    return allFiles.filter(
      (f) =>
        (minLines === 0 || f.lines >= minLines) &&
        (exts.length === 0 || exts.includes(f.ext)) &&
        (q === '' || f.path.toLowerCase().includes(q)),
    );
  });

  function toggleExt(ext: string): void {
    selectedExts = selectedExts.includes(ext)
      ? selectedExts.filter((e) => e !== ext)
      : [...selectedExts, ext];
  }
  let visible = $derived.by<FileRow[]>(() => filtered.slice(0, RENDER_CAP));
  // Largest line count in the *unfiltered* set drives the relative bar width so
  // bars stay comparable as the filter changes.
  let maxLines = $derived.by<number>(() => (allFiles.length > 0 ? allFiles[0].lines : 0));

  $effect(() => {
    if (loading || error) return;
    if (announcedCount === filtered.length) return;
    const firstAnnouncement = announcedCount === null;
    announcedCount = filtered.length;
    announce(`${filtered.length} source files ranked by line count`);
    if (firstAnnouncement && !hasEditableFocus()) {
      queueMicrotask(() => {
        if (!hasEditableFocus()) headingEl?.focus();
      });
    }
  });

  function barWidth(lines: number): number {
    if (maxLines <= 0) return 0;
    return Math.max(2, Math.round((lines / maxLines) * 100));
  }

  function hasEditableFocus(): boolean {
    if (typeof document === 'undefined') return false;
    const active = document.activeElement as HTMLElement | null;
    if (!active) return false;
    const tag = active.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || active.isContentEditable;
  }
</script>

<section class="largest" aria-label="Largest source files">
  <header class="head">
    <h2 bind:this={headingEl} tabindex="-1" class="title">Largest source files</h2>
    <p class="muted sub">Source files ranked by line count, largest first.</p>
  </header>

  <div class="filter" role="group" aria-label="Filter by line count">
    <span class="filter-label">Min lines</span>
    {#each PRESETS as preset (preset.lines)}
      <button
        type="button"
        class="preset"
        class:active={minLines === preset.lines}
        aria-pressed={minLines === preset.lines}
        onclick={() => (minLines = preset.lines)}
      >{preset.label}</button>
    {/each}
    <label class="custom">
      <span class="muted">≥</span>
      <input
        type="number"
        min="0"
        step="1"
        inputmode="numeric"
        aria-label="Minimum line count"
        value={minLines > 0 ? minLines : ''}
        placeholder="0"
        oninput={(e) => {
          const n = Number((e.currentTarget as HTMLInputElement).value);
          minLines = Number.isFinite(n) && n > 0 ? Math.round(n) : 0;
        }}
      />
      <span class="muted">lines</span>
    </label>
    <label class="gitignore-toggle" title="Exclude files matched by the repo's root .gitignore">
      <input
        type="checkbox"
        checked={respectGitignore}
        onchange={(e) => (respectGitignore = (e.currentTarget as HTMLInputElement).checked)}
      />
      <span>Respect .gitignore</span>
    </label>
  </div>

  <div class="filter" role="group" aria-label="Filter by path and extension">
    <label class="path-field">
      <span class="filter-label">Path</span>
      <input
        type="search"
        class="path-input"
        placeholder="substring, e.g. crates/ctx-web"
        aria-label="Filter by path substring"
        value={pathQuery}
        oninput={(e) => (pathQuery = (e.currentTarget as HTMLInputElement).value)}
      />
    </label>
    {#if extOptions.length > 0}
      <span class="filter-label">Ext</span>
      <div class="ext-chips" role="group" aria-label="Filter by extension">
        {#each extOptions as opt (opt.ext)}
          <button
            type="button"
            class="preset ext-chip"
            class:active={selectedExts.includes(opt.ext)}
            aria-pressed={selectedExts.includes(opt.ext)}
            onclick={() => toggleExt(opt.ext)}
          >.{opt.ext} <span class="ext-count">{opt.count}</span></button>
        {/each}
      </div>
      {#if selectedExts.length > 0}
        <button type="button" class="clear-ext" onclick={() => (selectedExts = [])}>Clear</button>
      {/if}
    {/if}
  </div>

  {#if loading && allFiles.length === 0}
    <div class="loading" aria-busy="true">
      <span class="skel" style="width: 50%; height: 14px;"></span>
      <span class="skel" style="width: 80%; height: 12px;"></span>
      <span class="skel" style="width: 65%; height: 12px;"></span>
    </div>
  {:else if error}
    <div class="error">
      <p>Failed to load file list.</p>
      <code class="mono">{error}</code>
      <button onclick={() => load(respectGitignore)}>Retry</button>
    </div>
  {:else}
    <div class="result-meta muted">
      {#if filtered.length === allFiles.length}
        {allFiles.length} source files
      {:else}
        {filtered.length} of {allFiles.length} source files
      {/if}
      {#if minLines > 0}<span> · ≥ {minLines} lines</span>{/if}
      {#if selectedExts.length > 0}<span> · {selectedExts.map((e) => `.${e}`).join(', ')}</span>{/if}
      {#if pathQuery.trim()}<span> · path “{pathQuery.trim()}”</span>{/if}
      {#if filtered.length > RENDER_CAP}<span> · showing top {RENDER_CAP}</span>{/if}
    </div>

    {#if filtered.length === 0}
      <p class="muted empty">No source files match this filter.</p>
    {:else}
      <ol class="rows" aria-label="ranked source files">
        {#each visible as f, i (f.path)}
          <li>
            <button
              type="button"
              class="row"
              onclick={() => navigate(toFileHash(f.path))}
              aria-label={`Rank ${i + 1}: ${f.path}, ${f.lines} lines`}
            >
              <span class="rank mono">{i + 1}</span>
              <span class="name-cell">
                <span class="name mono">{f.name}</span>
                <span class="dir mono muted">{f.path}</span>
              </span>
              <span class="bar-cell" aria-hidden="true">
                <span class="bar" style="width: {barWidth(f.lines)}%"></span>
              </span>
              <span class="lines mono">{f.lines.toLocaleString()}<span class="unit muted"> ln</span></span>
              <span class="size mono muted" title="File size">{formatSize(f.size)}</span>
            </button>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</section>

<style>
  .largest {
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 1100px;
    min-height: 0;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .title {
    margin: 0;
    font-size: 18px;
    color: var(--ctx-accent);
  }
  .title:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 2px;
    border-radius: 3px;
  }
  .sub {
    margin: 0;
    font-size: 12px;
  }

  .filter {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px 8px;
    padding: 8px 12px;
    background: var(--ctx-bg-panel);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
  }
  .filter-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
    margin-right: 2px;
  }
  .preset {
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 2px 8px;
    cursor: pointer;
  }
  .preset:hover {
    border-color: var(--ctx-accent);
  }
  .preset.active {
    background: var(--ctx-accent);
    color: var(--ctx-bg);
    border-color: var(--ctx-accent);
  }
  .preset:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  .custom {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 12px;
    margin-left: auto;
  }
  .custom input {
    width: 72px;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 2px 6px;
    font-variant-numeric: tabular-nums;
  }
  .custom input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .gitignore-toggle {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    color: var(--ctx-fg-dim);
    cursor: pointer;
    white-space: nowrap;
  }
  .gitignore-toggle input {
    margin: 0;
    cursor: pointer;
  }
  .path-field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
  }
  .path-input {
    width: 220px;
    max-width: 40vw;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 2px 6px;
  }
  .path-input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .ext-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    min-width: 0;
  }
  .ext-chip {
    font-variant-numeric: tabular-nums;
  }
  .ext-count {
    opacity: 0.6;
    font-size: 10px;
    margin-left: 1px;
  }
  .ext-chip.active .ext-count {
    opacity: 0.85;
  }
  .clear-ext {
    border: 0;
    background: transparent;
    color: var(--ctx-accent);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
    padding: 2px 4px;
  }
  .clear-ext:hover {
    text-decoration: underline;
  }

  .result-meta {
    font-size: 11px;
  }

  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    background: var(--ctx-bg-panel);
    overflow: auto;
  }
  .row {
    width: 100%;
    text-align: left;
    border: 0;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 12px;
    padding: 5px 12px;
    display: grid;
    grid-template-columns: 36px minmax(0, 1fr) 140px 84px 72px;
    gap: 10px;
    align-items: center;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .rank {
    color: var(--ctx-fg-dim);
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
  .name-cell {
    display: flex;
    flex-direction: column;
    min-width: 0;
  }
  .name {
    color: var(--ctx-link);
    font-weight: 500;
    word-break: break-all;
  }
  .dir {
    font-size: 10px;
    word-break: break-all;
  }
  .bar-cell {
    height: 8px;
    background: var(--ctx-bg-elev);
    border-radius: 2px;
    overflow: hidden;
  }
  .bar {
    display: block;
    height: 100%;
    background: var(--ctx-accent);
    border-radius: 2px;
  }
  .lines {
    text-align: right;
    font-variant-numeric: tabular-nums;
    color: var(--ctx-fg);
  }
  .unit {
    font-size: 10px;
  }
  .size {
    text-align: right;
    font-variant-numeric: tabular-nums;
  }

  .loading {
    padding: 8px 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .error {
    padding: 12px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
  }
  .error code {
    display: block;
    margin: 6px 0;
    color: var(--ctx-err);
    font-size: 11px;
    word-break: break-all;
  }
  .empty {
    margin: 0;
    padding: 8px 0;
    font-size: 12px;
  }

  @media (max-width: 600px) {
    .row {
      grid-template-columns: 28px minmax(0, 1fr) 64px;
      grid-template-rows: auto auto;
      gap: 4px 8px;
    }
    .bar-cell {
      grid-column: 2 / 4;
      grid-row: 2;
    }
    .size {
      display: none;
    }
  }
</style>
