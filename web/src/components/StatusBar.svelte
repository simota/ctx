<script lang="ts">
  import { route } from '../lib/router.svelte';

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
      case 'pins':
        return 'pins';
      case 'dir':
        return route.path ? `dir: ${route.path}` : 'dir';
      default:
        return 'tree';
    }
  });
</script>

<footer class="status">
  <span class="route mono" aria-label="current route">{label}</span>
  <span class="spacer"></span>
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
    grid-template-columns: 1fr auto auto auto auto;
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
  .conn {
    color: var(--ctx-accent);
  }
  .conn.offline {
    color: var(--ctx-err);
  }
</style>
