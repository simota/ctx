<script lang="ts">
  import { navigate, toSearchHash, route } from '../lib/router.svelte';

  let value = $state(route.name === 'search' ? route.query : '');
  let inputEl: HTMLInputElement | null = $state(null);

  // sync external route changes into the input
  $effect(() => {
    if (route.name === 'search') {
      value = route.query;
    }
  });

  function onSubmit(e: SubmitEvent) {
    e.preventDefault();
    const q = value.trim();
    if (!q) return;
    navigate(toSearchHash(q));
  }

  function onGlobalKey(e: KeyboardEvent) {
    if (e.key === '/' && document.activeElement?.tagName !== 'INPUT' && document.activeElement?.tagName !== 'TEXTAREA') {
      e.preventDefault();
      inputEl?.focus();
    }
  }

  $effect(() => {
    window.addEventListener('keydown', onGlobalKey);
    return () => window.removeEventListener('keydown', onGlobalKey);
  });
</script>

<form class="search-bar" role="search" onsubmit={onSubmit}>
  <label class="visually-hidden" for="ctx-search">Search</label>
  <span class="icon" aria-hidden="true">⌕</span>
  <input
    id="ctx-search"
    bind:this={inputEl}
    bind:value
    type="search"
    placeholder="Search symbols, paths, content…  (press /)"
    autocomplete="off"
    spellcheck="false"
  />
  <button type="submit" aria-label="run search">Go</button>
</form>

<style>
  .search-bar {
    display: grid;
    grid-template-columns: 24px 1fr auto;
    align-items: center;
    gap: 4px;
    background: var(--ctx-bg-panel);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 2px 6px;
    max-width: 640px;
    width: 100%;
    justify-self: stretch;
  }
  .icon {
    color: var(--ctx-fg-dim);
    text-align: center;
  }
  input {
    border: 0;
    background: transparent;
    width: 100%;
    padding: 4px 4px;
  }
  input:focus-visible {
    outline: none;
  }
  .search-bar:focus-within {
    border-color: var(--ctx-accent);
  }
  button {
    border: 0;
    padding: 2px 10px;
    color: var(--ctx-accent);
  }
</style>
