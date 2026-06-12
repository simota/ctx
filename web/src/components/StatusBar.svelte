<script lang="ts">
  import { route } from '../lib/router.svelte';
  import { mixSelection, clearSelection } from '../lib/mix-selection.svelte';
  import { openBounceDialog } from '../lib/bounce-dialog.svelte';

  let mixCount = $derived(mixSelection.includedPaths.size);

  let online = $state(typeof navigator !== 'undefined' ? navigator.onLine : true);

  $effect(() => {
    const on = () => (online = true);
    const off = () => (online = false);
    window.addEventListener('online', on);
    window.addEventListener('offline', off);
    return () => {
      window.removeEventListener('online', on);
      window.removeEventListener('offline', off);
    };
  });

  let label = $derived.by(() => {
    switch (route.name) {
      case 'file':
        return `file: ${route.path}`;
      case 'search':
        return `search: ${route.query}`;
      case 'budget':
        return 'budget';
      case 'replay':
        return route.path ? `replay: ${route.path}` : 'replay';
      case 'dir':
        return route.path ? `dir: ${route.path}` : 'dir';
      case 'mixdowns':
        return 'mixdowns';
      default:
        return 'tree';
    }
  });
</script>

<footer class="status">
  <span class="route mono" aria-label="current route">{label}</span>
  <span class="spacer"></span>
  {#if mixCount > 0}
    <button
      type="button"
      class="mix-clear muted"
      title="Clear mix selection"
      aria-label={`${mixCount} file${mixCount !== 1 ? 's' : ''} in mix — click to clear`}
      onclick={clearSelection}
    >{mixCount} in mix ×</button>
  {/if}
  <button
    type="button"
    class="bounce-btn"
    disabled={mixCount === 0}
    aria-label={mixCount === 0 ? 'Bounce (no files selected)' : `Bounce ${mixCount} file${mixCount !== 1 ? 's' : ''}`}
    title="Save current file selection as a mix"
    onclick={openBounceDialog}
  >Bounce{mixCount > 0 ? ` (${mixCount})` : ''}</button>
  <span
    class="microcopy muted"
    title="Approx LLM tokens (cl100k_base). Used to estimate AI context cost."
  >tokens = LLM context units (cl100k_base)</span>
  <span class="conn" class:offline={!online} aria-live="polite">
    {online ? 'connected' : 'offline'}
  </span>
  <span class="build muted">ctx v0</span>
</footer>

<style>
  .status {
    display: grid;
    grid-template-columns: 1fr auto auto auto auto auto auto;
    align-items: center;
    gap: 12px;
    padding: 0 12px;
    background: var(--ctx-bg-elev);
    border-top: 1px solid var(--ctx-border);
    font-size: 11px;
    color: var(--ctx-fg-dim);
    height: 22px;
  }
  .spacer {
    width: 0;
  }
  .route {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .microcopy {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  @media (max-width: 600px) {
    .microcopy {
      display: none;
    }
  }
  .mix-clear {
    border: 0;
    background: transparent;
    font: inherit;
    font-size: 11px;
    cursor: pointer;
    padding: 0 4px;
    border-radius: 2px;
    color: var(--ctx-accent);
    opacity: 0.8;
  }
  .mix-clear:hover { opacity: 1; }
  .mix-clear:focus-visible { outline: 1px solid var(--ctx-accent); outline-offset: 1px; }
  .bounce-btn {
    border: 1px solid var(--ctx-border);
    background: transparent;
    font: inherit;
    font-size: 11px;
    cursor: pointer;
    padding: 1px 8px;
    border-radius: 3px;
    color: var(--ctx-fg-dim);
    line-height: 1.4;
  }
  .bounce-btn:hover:not(:disabled) {
    color: var(--ctx-fg);
    border-color: var(--ctx-accent);
    background: rgba(78, 201, 176, 0.08);
  }
  .bounce-btn:disabled { opacity: 0.4; cursor: not-allowed; }
  .bounce-btn:focus-visible { outline: 2px solid var(--ctx-accent); outline-offset: -2px; }
  .conn {
    color: var(--ctx-accent);
  }
  .conn.offline {
    color: var(--ctx-err);
  }
</style>
