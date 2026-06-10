<script lang="ts">
  import {
    mixStore,
    refreshMixList,
    deleteMix,
    loadMix,
  } from '../lib/mix-store.svelte';
  import { setSelection } from '../lib/mix-selection.svelte';
  import { revealPath } from '../lib/tree-state.svelte';
  import { announce } from '../lib/announce.svelte';

  let loadedOnce = $state(false);

  // Load on mount (each mount is a fresh route entry).
  $effect(() => {
    if (!loadedOnce && !mixStore.loading) {
      loadedOnce = true;
      void refreshMixList();
    }
  });

  function formatRelTime(rfc3339: string): string {
    const ms = Date.now() - new Date(rfc3339).getTime();
    const s = Math.floor(ms / 1000);
    if (s < 60) return `${s}s ago`;
    const m = Math.floor(s / 60);
    if (m < 60) return `${m}m ago`;
    const h = Math.floor(m / 60);
    if (h < 24) return `${h}h ago`;
    const d = Math.floor(h / 24);
    return `${d}d ago`;
  }

  async function onRecall(id: string, name: string) {
    try {
      const mix = await loadMix(id);
      setSelection(mix.files);
      if (mix.files.length > 0) {
        revealPath(mix.files[0]);
      }
      announce(`Recalled ${name} (${mix.files.length} files)`);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      announce(`Recall failed: ${msg}`);
    }
  }

  async function onDelete(id: string, name: string) {
    if (!confirm(`Delete mix "${name}"? This cannot be undone.`)) return;
    try {
      await deleteMix(id);
      announce(`Deleted ${name}`);
    } catch (e: unknown) {
      const msg = e instanceof Error ? e.message : String(e);
      announce(`Delete failed: ${msg}`);
    }
  }
</script>

<div class="panel">
  <header class="head">
    <h2 class="title">Mixdowns</h2>
    <button
      type="button"
      class="refresh-btn"
      aria-label="Refresh mix list"
      title="Refresh"
      onclick={() => void refreshMixList()}
      disabled={mixStore.loading}
    >↻</button>
  </header>

  {#if mixStore.loading && mixStore.list.length === 0}
    <div class="state muted">Loading…</div>
  {:else if mixStore.error}
    <div class="state state-err" role="alert">
      <p>{mixStore.error}</p>
      <button type="button" class="retry-btn" onclick={() => void refreshMixList()}>Retry</button>
    </div>
  {:else if mixStore.list.length === 0}
    <div class="empty">
      <h3>No mixes yet</h3>
      <p class="muted">
        A <strong>Mixdown</strong> is a saved snapshot of file selections.
        Select files in the tree, then click <strong>Bounce</strong> to save
        them as a named mix you can recall later.
      </p>
    </div>
  {:else}
    <ul class="list" aria-label="Saved mixes">
      {#each mixStore.list as mix (mix.id)}
        <li class="mix-row">
          <div class="mix-meta">
            <span class="mix-name">{mix.name}</span>
            {#if mix.goal}
              <span class="mix-goal muted" title={mix.goal}>{mix.goal}</span>
            {/if}
            <span class="mix-info muted">
              {mix.file_count} file{mix.file_count !== 1 ? 's' : ''}
              · {formatRelTime(mix.created)}
            </span>
          </div>
          <div class="mix-actions">
            <button
              type="button"
              class="action-btn"
              onclick={() => void onRecall(mix.id, mix.name)}
              aria-label={`Recall mix ${mix.name}`}
              title="Replace current selection with this mix's files"
            >Recall</button>
            <button
              type="button"
              class="action-btn action-del"
              onclick={() => void onDelete(mix.id, mix.name)}
              aria-label={`Delete mix ${mix.name}`}
              title="Delete this mix"
            >Delete</button>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
</div>

<style>
  .panel {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
    font-size: 12px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ctx-border);
    flex: 0 0 auto;
    background: var(--ctx-bg-elev);
  }
  .title {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--ctx-fg);
  }
  .refresh-btn {
    border: 0;
    background: transparent;
    color: var(--ctx-fg-dim);
    cursor: pointer;
    font-size: 14px;
    line-height: 1;
    padding: 2px 6px;
    border-radius: 3px;
  }
  .refresh-btn:hover:not(:disabled) { color: var(--ctx-fg); background: var(--ctx-bg-panel); }
  .refresh-btn:disabled { opacity: 0.4; }
  .state {
    padding: 20px 16px;
    font-size: 12px;
  }
  .state-err {
    color: var(--ctx-err, #f87171);
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .state-err p { margin: 0; }
  .retry-btn {
    align-self: flex-start;
    font: inherit;
    font-size: 11px;
    padding: 2px 10px;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: transparent;
    color: var(--ctx-fg-dim);
    cursor: pointer;
  }
  .retry-btn:hover { color: var(--ctx-fg); }
  .empty {
    padding: 24px 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .empty h3 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--ctx-fg);
  }
  .empty p {
    margin: 0;
    font-size: 11px;
    line-height: 1.6;
  }
  .list {
    list-style: none;
    margin: 0;
    padding: 0;
    overflow-y: auto;
    flex: 1 1 auto;
    min-height: 0;
  }
  .mix-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 8px;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ctx-border);
  }
  .mix-row:hover { background: rgba(255, 255, 255, 0.03); }
  :global(:root[data-theme='light']) .mix-row:hover { background: rgba(0, 0, 0, 0.03); }
  .mix-meta {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
    flex: 1 1 auto;
  }
  .mix-name {
    font-weight: 600;
    color: var(--ctx-fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mix-goal {
    font-size: 11px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .mix-info {
    font-size: 11px;
  }
  .mix-actions {
    display: flex;
    flex-direction: column;
    gap: 4px;
    flex: 0 0 auto;
  }
  .action-btn {
    font: inherit;
    font-size: 11px;
    padding: 2px 8px;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: transparent;
    color: var(--ctx-fg-dim);
    cursor: pointer;
    white-space: nowrap;
  }
  .action-btn:hover { color: var(--ctx-fg); background: var(--ctx-bg-elev); }
  .action-btn:focus-visible { outline: 2px solid var(--ctx-accent); outline-offset: -2px; }
  .action-del:hover { color: var(--ctx-err, #f87171); border-color: var(--ctx-err, #f87171); }
  .muted { color: var(--ctx-fg-dim); }
</style>
