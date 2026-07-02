<script lang="ts">
  import { fetchTree, ApiCallError } from '../lib/api';
  import { announce } from '../lib/announce.svelte';
  import { navigate, route, toFileHash } from '../lib/router.svelte';
  import { openRight } from '../lib/panes.svelte';
  import { repo, setRepoRoot } from '../lib/repo.svelte';
  import {
    pins,
    ensurePinsRoot,
    movePin,
    unpinFile,
    clearPins,
    recordPinOpened,
    markPinStale,
    type PinEntry,
  } from '../lib/pins.svelte';

  let query = $state('');
  let confirmClear = $state(false);
  let rootLoading = $state(false);
  let rootLoadStarted = false;
  let rootError = $state<string | null>(null);
  let headingEl: HTMLHeadingElement | null = $state(null);
  let announcedCount: number | null = $state(null);

  let isMobile = $state(typeof window !== 'undefined' && window.innerWidth < 800);
  $effect(() => {
    if (typeof window === 'undefined') return;
    const mql = window.matchMedia('(max-width: 799px)');
    isMobile = mql.matches;
    const onChange = (e: MediaQueryListEvent) => {
      isMobile = e.matches;
    };
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  });

  function loadRoot(): void {
    if (rootLoading) return;
    rootLoadStarted = true;
    rootLoading = true;
    rootError = null;
    fetchTree({ depth: 1 })
      .then((r) => {
        setRepoRoot(r.abs_root);
        ensurePinsRoot(r.abs_root);
      })
      .catch((e: unknown) => {
        rootError = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        rootLoading = false;
      });
  }

  $effect(() => {
    if (repo.root) {
      ensurePinsRoot(repo.root);
      return;
    }
    if (!rootLoadStarted) loadRoot();
  });

  let filtered = $derived.by<PinEntry[]>(() => {
    const q = query.trim().toLowerCase();
    if (!q) return pins.entries;
    return pins.entries.filter((entry) => entry.path.toLowerCase().includes(q));
  });

  $effect(() => {
    if (!pins.loaded) return;
    if (announcedCount === pins.entries.length) return;
    const first = announcedCount === null;
    announcedCount = pins.entries.length;
    announce(`${pins.entries.length} pinned file${pins.entries.length === 1 ? '' : 's'}`);
    if (first && !hasEditableFocus()) {
      queueMicrotask(() => {
        if (!hasEditableFocus()) headingEl?.focus();
      });
    }
  });

  function basename(path: string): string {
    const idx = path.lastIndexOf('/');
    return idx === -1 ? path : path.slice(idx + 1);
  }

  async function validateFile(path: string): Promise<boolean> {
    try {
      const r = await fetchTree({ path, depth: 1 });
      if (r.tree.is_dir) {
        markPinStale(path);
        announce(`Pinned path is not a file: ${path}`);
        return false;
      }
      return true;
    } catch (e: unknown) {
      markPinStale(path);
      if (e instanceof ApiCallError && e.status === 404) {
        announce(`Pinned file is stale: ${path}`);
      } else {
        announce(`Could not validate pinned file: ${path}`);
      }
      return false;
    }
  }

  async function openPinned(path: string): Promise<void> {
    if (!(await validateFile(path))) return;
    recordPinOpened(path);
    navigate(toFileHash(path));
  }

  async function openPinnedToSide(path: string): Promise<void> {
    if (isMobile) return;
    if (!(await validateFile(path))) return;
    recordPinOpened(path);
    openRight(path);
    if (route.name !== 'file' || !route.path) {
      navigate(toFileHash(path, { right: path }));
    }
    announce('Right pane opened');
  }

  async function copyPath(path: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(path);
      announce(`Copied ${path}`);
    } catch {
      announce('Copy failed');
    }
  }

  function unpin(path: string): void {
    const result = unpinFile(path);
    announce(result.message);
    confirmClear = false;
  }

  function move(path: string, delta: -1 | 1): void {
    const result = movePin(path, delta);
    announce(result.message);
  }

  function requestClear(): void {
    confirmClear = true;
    announce('Confirm clear all pinned files');
  }

  function confirmClearAll(): void {
    const result = clearPins();
    announce(result.message);
    confirmClear = false;
  }

  function hasEditableFocus(): boolean {
    if (typeof document === 'undefined') return false;
    const active = document.activeElement as HTMLElement | null;
    if (!active) return false;
    const tag = active.tagName;
    return tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT' || active.isContentEditable;
  }
</script>

<section class="pins-board" aria-label="Pinned files board">
  <header class="head">
    <div>
      <h2 bind:this={headingEl} tabindex="-1" class="title">Pinned files</h2>
      <p class="muted sub">Files intentionally kept for this repository.</p>
    </div>
    {#if pins.entries.length > 0}
      <div class="clear-group">
        {#if confirmClear}
          <span class="muted confirm-text">Clear all pinned files?</span>
          <button type="button" class="danger" onclick={confirmClearAll}>Confirm</button>
          <button type="button" class="secondary" onclick={() => (confirmClear = false)}>Cancel</button>
        {:else}
          <button type="button" class="secondary" onclick={requestClear}>Clear All</button>
        {/if}
      </div>
    {/if}
  </header>

  {#if rootLoading && !pins.loaded}
    <div class="notice" aria-busy="true">Loading repository state…</div>
  {:else if rootError}
    <div class="error">
      <p>Failed to load repository state.</p>
      <code class="mono">{rootError}</code>
      <button
        type="button"
        onclick={() => {
          rootLoadStarted = false;
          loadRoot();
        }}
      >Retry</button>
    </div>
  {:else}
    {#if pins.persistenceWarning}
      <div class="warning" role="status">{pins.persistenceWarning}</div>
    {/if}

    <div class="filter" role="group" aria-label="Filter pinned files">
      <label class="path-field">
        <span class="filter-label">Path</span>
        <input
          type="search"
          class="path-input"
          placeholder="substring, e.g. web/src"
          aria-label="Filter pinned files by path substring"
          value={query}
          oninput={(e) => (query = (e.currentTarget as HTMLInputElement).value)}
        />
      </label>
      <span class="result-meta muted" aria-live="polite">
        {#if query.trim()}
          {filtered.length} of {pins.entries.length} pinned files
        {:else}
          {pins.entries.length} pinned file{pins.entries.length === 1 ? '' : 's'}
        {/if}
      </span>
    </div>

    {#if pins.entries.length === 0}
      <p class="muted empty">No pinned files yet.</p>
    {:else if filtered.length === 0}
      <p class="muted empty">No pinned files match this filter.</p>
    {:else}
      <ol class="rows" aria-label="Pinned files">
        {#each filtered as entry, visibleIndex (entry.path)}
          {@const actualIndex = pins.entries.findIndex((item) => item.path === entry.path)}
          <li class:stale={entry.stale}>
            <div class="row">
              <span class="rank mono">{actualIndex + 1}</span>
              <span class="name-cell">
                <span class="name mono">{basename(entry.path)}</span>
                <span class="dir mono muted">{entry.path}</span>
                {#if entry.stale}
                  <span class="stale-label">Stale</span>
                {/if}
              </span>
              <div class="actions" role="group" aria-label={`Actions for ${entry.path}`}>
                <button type="button" onclick={() => openPinned(entry.path)}>Open</button>
                <button
                  type="button"
                  onclick={() => openPinnedToSide(entry.path)}
                  disabled={isMobile}
                >Open to Side</button>
                <button type="button" onclick={() => copyPath(entry.path)}>Copy Path</button>
                <button
                  type="button"
                  aria-label={`Move ${entry.path} up`}
                  onclick={() => move(entry.path, -1)}
                  disabled={actualIndex <= 0}
                >Up</button>
                <button
                  type="button"
                  aria-label={`Move ${entry.path} down`}
                  onclick={() => move(entry.path, 1)}
                  disabled={actualIndex === -1 || actualIndex >= pins.entries.length - 1}
                >Down</button>
                <button type="button" class="danger-text" onclick={() => unpin(entry.path)}>Unpin</button>
              </div>
            </div>
          </li>
        {/each}
      </ol>
    {/if}
  {/if}
</section>

<style>
  .pins-board {
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 1100px;
    min-height: 0;
  }
  .head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
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
  .clear-group {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    flex-wrap: wrap;
    gap: 6px;
    min-height: 28px;
  }
  .confirm-text {
    font-size: 12px;
  }
  .filter {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 12px;
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
  .path-field {
    display: flex;
    align-items: center;
    gap: 6px;
    min-width: min(420px, 100%);
    flex: 1 1 300px;
  }
  .path-input {
    min-width: 0;
    flex: 1 1 auto;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 4px 7px;
  }
  .path-input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  .result-meta {
    font-size: 12px;
  }
  .rows {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    overflow: hidden;
  }
  .rows li + li {
    border-top: 1px solid var(--ctx-border);
  }
  .row {
    display: grid;
    grid-template-columns: 42px minmax(0, 1fr) auto;
    gap: 12px;
    align-items: center;
    padding: 8px 10px;
    background: var(--ctx-bg);
  }
  .rows li.stale .row {
    background: color-mix(in srgb, var(--ctx-bg-panel) 70%, var(--ctx-bg));
  }
  .rank {
    color: var(--ctx-fg-dim);
    text-align: right;
    font-size: 12px;
  }
  .name-cell {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--ctx-fg);
    font-size: 13px;
  }
  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
  .stale-label {
    align-self: flex-start;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    padding: 1px 5px;
    color: var(--ctx-fg-dim);
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 5px;
  }
  button {
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 3px 8px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    border-color: var(--ctx-accent);
  }
  button:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .secondary {
    color: var(--ctx-fg);
  }
  .danger,
  .danger-text {
    color: var(--ctx-danger, #d14);
  }
  .warning,
  .notice,
  .error,
  .empty {
    margin: 0;
    padding: 10px 12px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    background: var(--ctx-bg-panel);
    font-size: 12px;
  }
  .warning {
    border-color: color-mix(in srgb, var(--ctx-accent) 45%, var(--ctx-border));
  }
  .error {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .error code {
    white-space: pre-wrap;
  }

  @media (max-width: 799px) {
    .pins-board {
      padding: 12px;
    }
    .head {
      flex-direction: column;
    }
    .clear-group {
      justify-content: flex-start;
    }
    .row {
      grid-template-columns: 32px minmax(0, 1fr);
      align-items: start;
    }
    .actions {
      grid-column: 2;
      justify-content: flex-start;
    }
  }
</style>
