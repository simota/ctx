<script lang="ts">
  import { navigate, toSearchHash, route } from '../lib/router.svelte';
  import type { SearchMatchMode } from '../lib/router.svelte';
  import { finder } from '../lib/finder.svelte';
  import { palette } from '../lib/palette.svelte';
  import { cheatsheet } from '../lib/cheatsheet.svelte';
  import { definitionPicker } from '../lib/definition-picker.svelte';
  import { rootsPicker } from '../lib/roots-picker.svelte';

  let value = $state(route.name === 'search' ? route.query : '');
  let matchMode = $state<SearchMatchMode>(
    route.name === 'search' ? (route.searchMatch ?? 'all') : 'all',
  );
  let inputEl: HTMLInputElement | null = $state(null);

  // sync external route changes into the input
  $effect(() => {
    if (route.name === 'search') {
      value = route.query;
      matchMode = route.searchMatch ?? 'all';
    }
  });

  function onSubmit(e: SubmitEvent) {
    e.preventDefault();
    const q = value.trim();
    if (!q) return;
    navigate(toSearchHash(q, { match: matchMode }));
  }

  function setMatchMode(mode: SearchMatchMode) {
    matchMode = mode;
    const q = value.trim();
    if (q && route.name === 'search') {
      navigate(toSearchHash(q, { match: mode }));
    }
  }

  function onGlobalKey(e: KeyboardEvent) {
    if (e.key !== '/') return;
    // Bare `/` only — with a modifier held it's some other shortcut, and
    // during IME composition the key is part of the composed text.
    if (e.metaKey || e.ctrlKey || e.altKey || e.isComposing) return;
    // Don't steal the literal character from editable elements.
    const a = document.activeElement as HTMLElement | null;
    if (a && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.tagName === 'SELECT' || a.isContentEditable)) {
      return;
    }
    // Overlay mutex — a modal owns the keyboard while open.
    if (
      finder.open ||
      palette.open ||
      cheatsheet.open ||
      definitionPicker.open ||
      rootsPicker.open
    ) return;
    e.preventDefault();
    inputEl?.focus();
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
  <div class="match-mode" role="group" aria-label="Search match mode">
    <button
      class="mode-button"
      class:active={matchMode === 'all'}
      type="button"
      aria-pressed={matchMode === 'all'}
      onclick={() => setMatchMode('all')}
    >
      All
    </button>
    <button
      class="mode-button"
      class:active={matchMode === 'any'}
      type="button"
      aria-pressed={matchMode === 'any'}
      onclick={() => setMatchMode('any')}
    >
      Any
    </button>
  </div>
  <button class="submit" type="submit" aria-label="run search">Go</button>
</form>

<style>
  .search-bar {
    display: grid;
    grid-template-columns: 24px minmax(0, 1fr) auto auto;
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
  }
  .match-mode {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    overflow: hidden;
  }
  .mode-button {
    min-width: 34px;
    min-height: 24px;
    padding: 2px 8px;
    color: var(--ctx-fg-dim);
    border-left: 1px solid var(--ctx-border);
  }
  .mode-button:first-child {
    border-left: 0;
  }
  .mode-button.active {
    color: var(--ctx-bg);
    background: var(--ctx-accent);
  }
  .submit {
    min-height: 24px;
    padding: 2px 10px;
    color: var(--ctx-accent);
  }
  @media (max-width: 720px) {
    .search-bar {
      grid-template-columns: 24px minmax(0, 1fr) auto;
    }
    .match-mode {
      grid-column: 2 / -1;
      justify-self: start;
    }
  }
</style>
