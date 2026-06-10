<script lang="ts">
  // Project-root picker — opens via ⌘⇧B / Ctrl+Shift+B or the `roots.switch`
  // command. The list comes from GET /api/roots; selection POSTs to
  // /api/roots/open which spawns a child `ctx browse` for that root and
  // returns its URL. We then `window.open` that URL in a new tab — keeping
  // the current tab on the current root so there's no destructive switch.
  //
  // Inline management:
  //  - ✕ button per row → DELETE /api/roots?name=... (no confirm — re-add is
  //    cheap, so the friction would outweigh the safety win).
  //  - "+ Register Current Root" button in the footer when the served repo
  //    isn't yet in the registry → POST /api/roots {path: repo.root}.
  //
  // Keyboard model:
  //  - Filter input is the default focus; typing 1–9 there feeds the filter
  //    so the user can narrow down "123" without it being a quick-switch.
  //  - When the filter input is *not* focused, 1–9 quick-launches the Nth
  //    visible bookmark.

  import { rootsPicker, closeRootsPicker, clearSpawnError, clearManageError } from '../lib/roots-picker.svelte';
  import { roots, loadRoots, currentRoot } from '../lib/roots.svelte';
  import { repo } from '../lib/repo.svelte';
  import { openRoot, createRoot, deleteRoot, ApiCallError, type RootEntry } from '../lib/api';
  import { announce } from '../lib/announce.svelte';
  import { basename } from '../lib/format';

  let dialogEl: HTMLDivElement | null = $state(null);
  let listEl: HTMLUListElement | null = $state(null);
  let inputEl: HTMLInputElement | null = $state(null);
  let addInputEl: HTMLInputElement | null = $state(null);
  let addPath = $state('');

  // Focus the filter input on open so the user can immediately type.
  $effect(() => {
    if (rootsPicker.open && inputEl) {
      const el = inputEl;
      queueMicrotask(() => el.focus());
    }
  });

  // Keep the active option visible as ↑↓ moves the selection.
  $effect(() => {
    if (!listEl) return;
    const idx = rootsPicker.selectedIndex;
    const opt = listEl.querySelector<HTMLElement>(`[data-idx="${idx}"]`);
    if (opt) opt.scrollIntoView({ block: 'nearest' });
  });

  // Derived: the registry entry that matches our currently-served repo root.
  // Used to render the "current" badge and a leading bullet.
  let here = $derived(currentRoot());

  function isCurrent(entry: RootEntry): boolean {
    return here !== null && here.path === entry.path;
  }

  // Filtered view — case-insensitive substring match against name and path.
  // Empty query passes everything through. Sorting was already applied in
  // `roots.svelte.ts` (MRU first), so we preserve that order.
  let filteredEntries = $derived.by(() => {
    const q = rootsPicker.query.trim().toLowerCase();
    if (!q) return roots.entries;
    return roots.entries.filter((e) => {
      return (
        e.name.toLowerCase().includes(q) ||
        e.path.toLowerCase().includes(q)
      );
    });
  });

  // Clamp the selection whenever the filter list shrinks.
  $effect(() => {
    const len = filteredEntries.length;
    if (rootsPicker.selectedIndex >= len) {
      rootsPicker.selectedIndex = len > 0 ? len - 1 : 0;
    }
    if (rootsPicker.selectedIndex < 0) rootsPicker.selectedIndex = 0;
  });

  // True when the served repo root is not present in the registry — that's
  // the only case where we offer the "+ Register Current Root" affordance.
  let canRegisterCurrent = $derived(
    roots.loaded && repo.root !== '' && here === null,
  );
  let currentName = $derived(repo.root ? basename(repo.root) : '');

  // Compact relative-time formatter for `last_opened_at`. Verbose phrases
  // ("just now", "5 minutes ago", "yesterday", "2 weeks ago", "1 month ago")
  // so the dim metadata still tells a story at a glance. Returns '' for
  // missing or Go-zero-value ("0001-...") timestamps.
  function formatRelative(iso: string | undefined): string {
    if (!iso) return '';
    if (iso.startsWith('0001-')) return '';
    const t = Date.parse(iso);
    if (Number.isNaN(t)) return '';
    const diffMs = Date.now() - t;
    if (diffMs < 30_000) return 'just now';
    if (diffMs < 0) return 'just now';
    const sec = Math.floor(diffMs / 1000);
    if (sec < 60) return `${sec} sec ago`;
    const min = Math.floor(sec / 60);
    if (min < 60) return min === 1 ? '1 min ago' : `${min} min ago`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return hr === 1 ? '1 hour ago' : `${hr} hours ago`;
    const day = Math.floor(hr / 24);
    if (day === 1) return 'yesterday';
    if (day < 7) return `${day} days ago`;
    const wk = Math.floor(day / 7);
    if (wk < 5) return wk === 1 ? '1 week ago' : `${wk} weeks ago`;
    const mo = Math.floor(day / 30);
    if (mo < 12) return mo === 1 ? '1 month ago' : `${mo} months ago`;
    const yr = Math.floor(day / 365);
    return yr === 1 ? '1 year ago' : `${yr} years ago`;
  }

  async function activate(idx: number): Promise<void> {
    if (rootsPicker.spawning) return;
    const entry = filteredEntries[idx];
    if (!entry) return;
    rootsPicker.spawning = true;
    rootsPicker.spawnError = null;
    try {
      // Prefer name as the registry key — it's stable across rename of the
      // path's leaf directory and matches the CLI's `ctx roots open <name>`.
      const resp = await openRoot({ name: entry.name });
      // window.open returns null when popup-blocked; we still close the
      // picker either way and surface the URL via the announce live region
      // so the user has a chance to find it from the screen reader.
      const win = window.open(resp.url, '_blank', 'noopener');
      if (!win) {
        rootsPicker.spawnError =
          `Spawned ${resp.name} at ${resp.url} but the browser blocked the new tab.`;
        announce(`Spawned ${resp.name} at ${resp.url}; tab was blocked`);
        return;
      }
      announce(`Opened ${resp.name} in new tab`);
      closeRootsPicker();
    } catch (e) {
      const msg = errMsg(e, `Could not open ${entry.name}`);
      rootsPicker.spawnError = msg;
      announce(msg);
    } finally {
      rootsPicker.spawning = false;
    }
  }

  async function onRemove(e: MouseEvent, entry: RootEntry): Promise<void> {
    // Stop the row's click handler from racing the delete + activating the
    // entry we're about to remove.
    e.preventDefault();
    e.stopPropagation();
    if (rootsPicker.managing) return;
    rootsPicker.managing = true;
    rootsPicker.manageError = null;
    try {
      await deleteRoot(entry.name);
      await loadRoots();
      announce(`Removed ${entry.name} from roots`);
    } catch (err) {
      const msg = errMsg(err, `Could not remove ${entry.name}`);
      rootsPicker.manageError = msg;
      announce(msg);
    } finally {
      rootsPicker.managing = false;
    }
  }

  async function onRegisterCurrent(): Promise<void> {
    if (rootsPicker.managing) return;
    if (!repo.root) return;
    rootsPicker.managing = true;
    rootsPicker.manageError = null;
    try {
      const resp = await createRoot({ path: repo.root });
      await loadRoots();
      announce(`Registered ${resp.root.name}`);
    } catch (err) {
      const msg = errMsg(err, 'Could not register current root');
      rootsPicker.manageError = msg;
      announce(msg);
    } finally {
      rootsPicker.managing = false;
    }
  }

  // Register an arbitrary directory typed into the footer input. Path is sent
  // as-is to the backend; server-side `canonicalize` handles `~/...` expansion
  // and abs-path normalisation. Non-existent paths are accepted by design
  // (see internal/config/roots.go canonicalize comment).
  async function onAddByPath(): Promise<void> {
    if (rootsPicker.managing) return;
    const path = addPath.trim();
    if (!path) return;
    rootsPicker.managing = true;
    rootsPicker.manageError = null;
    try {
      const resp = await createRoot({ path });
      await loadRoots();
      addPath = '';
      announce(`Registered ${resp.root.name}`);
    } catch (err) {
      const msg = errMsg(err, `Could not register ${path}`);
      rootsPicker.manageError = msg;
      announce(msg);
    } finally {
      rootsPicker.managing = false;
    }
  }

  function isAddInputFocused(): boolean {
    return document.activeElement === addInputEl;
  }

  function onAddInputKey(e: KeyboardEvent) {
    // Submit on Enter without letting the global picker handler steal it
    // (which would activate the selected list entry instead).
    if (e.key === 'Enter') {
      e.preventDefault();
      e.stopPropagation();
      void onAddByPath();
    }
  }

  function errMsg(e: unknown, prefix: string): string {
    if (e instanceof ApiCallError) return `${prefix}: ${e.message}`;
    if (e instanceof Error) return `${prefix}: ${e.message}`;
    return `${prefix}.`;
  }

  function isFilterFocused(): boolean {
    return document.activeElement === inputEl;
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeRootsPicker();
      return;
    }
    if (e.key === 'Tab') {
      // Focus trap — wrap at the edges so Tab cycles between the filter
      // input, the add-by-path input, the action buttons (× / register / add)
      // and the close button without escaping into the page underneath.
      if (!dialogEl) {
        e.preventDefault();
        return;
      }
      const focusables = Array.from(
        dialogEl.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((el) => el.offsetParent !== null);
      if (focusables.length === 0) {
        e.preventDefault();
        return;
      }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
      // Otherwise let the browser handle Tab normally inside the modal.
      return;
    }
    const len = filteredEntries.length;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      clearSpawnError();
      if (len === 0) return;
      rootsPicker.selectedIndex = (rootsPicker.selectedIndex + 1) % len;
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      clearSpawnError();
      if (len === 0) return;
      rootsPicker.selectedIndex = (rootsPicker.selectedIndex - 1 + len) % len;
      return;
    }
    if (e.key === 'Home' && !isFilterFocused() && !isAddInputFocused()) {
      e.preventDefault();
      clearSpawnError();
      if (len > 0) rootsPicker.selectedIndex = 0;
      return;
    }
    if (e.key === 'End' && !isFilterFocused() && !isAddInputFocused()) {
      e.preventDefault();
      clearSpawnError();
      if (len > 0) rootsPicker.selectedIndex = len - 1;
      return;
    }
    if (e.key === 'Enter' && !isAddInputFocused()) {
      e.preventDefault();
      if (len > 0) void activate(rootsPicker.selectedIndex);
      return;
    }
    // Number-key quick-switch — only when focus is *not* in either text
    // input, so the user can still type "123" as a filter term or as part
    // of a path being typed into the add-by-path field.
    if (
      !isFilterFocused() &&
      !isAddInputFocused() &&
      !e.metaKey &&
      !e.ctrlKey &&
      !e.altKey &&
      e.key >= '1' &&
      e.key <= '9'
    ) {
      const n = Number(e.key) - 1;
      if (n < len) {
        e.preventDefault();
        void activate(n);
      }
      return;
    }
  }

  function onOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) closeRootsPicker();
  }

  function onRowClick(e: MouseEvent, idx: number) {
    e.preventDefault();
    rootsPicker.selectedIndex = idx;
    void activate(idx);
  }

  function onQueryInput() {
    // Any keystroke in the filter implicitly invalidates lingering errors —
    // the user is moving on; the banner should not persist.
    clearSpawnError();
    clearManageError();
  }
</script>

{#if rootsPicker.open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="overlay"
    onclick={onOverlayClick}
    onkeydown={onKey}
    role="presentation"
  >
    <div
      bind:this={dialogEl}
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ctx-rootspicker-title"
      aria-describedby="ctx-rootspicker-help"
      tabindex="-1"
    >
      <header class="head">
        <h2 id="ctx-rootspicker-title">
          Switch Project Root
          {#if roots.loaded}
            <span class="count muted">({filteredEntries.length}{filteredEntries.length !== roots.entries.length ? `/${roots.entries.length}` : ''})</span>
          {/if}
        </h2>
        <button
          type="button"
          class="close"
          aria-label="Close root picker"
          onclick={closeRootsPicker}
        >×</button>
      </header>
      <span id="ctx-rootspicker-help" class="sr-only">
        Type to filter, use up and down arrows to select a project root,
        Enter to open it in a new tab, number keys 1 to 9 to jump to the top
        results, Escape to close.
      </span>
      <div class="filter">
        <input
          bind:this={inputEl}
          bind:value={rootsPicker.query}
          oninput={onQueryInput}
          type="text"
          class="filter-input"
          placeholder="Filter by name or path…"
          aria-label="Filter project roots"
          autocomplete="off"
          spellcheck="false"
        />
      </div>
      {#if roots.error}
        <div class="banner banner-error" role="alert">{roots.error}</div>
      {/if}
      {#if rootsPicker.spawnError}
        <div class="banner banner-error" role="alert">{rootsPicker.spawnError}</div>
      {/if}
      {#if rootsPicker.manageError}
        <div class="banner banner-error" role="alert">{rootsPicker.manageError}</div>
      {/if}
      {#if !roots.loaded && roots.loading}
        <div class="banner banner-info">Loading…</div>
      {:else if roots.entries.length === 0 && roots.loaded}
        <div class="empty muted">
          <p>No registered roots.</p>
          <p class="hint">
            Use the button below or run <code>ctx roots add /path/to/project</code>
            in your terminal to register one.
          </p>
        </div>
      {:else if filteredEntries.length === 0}
        <div class="empty muted">
          <p>No matches for “{rootsPicker.query}”.</p>
        </div>
      {:else}
        <ul
          bind:this={listEl}
          class="results"
          role="listbox"
          aria-label="Registered project roots"
        >
          {#each filteredEntries as entry, i (`${entry.name}|${entry.path}`)}
            {@const sel = i === rootsPicker.selectedIndex}
            {@const cur = isCurrent(entry)}
            {@const rel = formatRelative(entry.last_opened_at)}
            {@const quick = i < 9 ? String(i + 1) : ''}
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
            <li
              class="opt"
              class:selected={sel}
              class:current={cur}
              role="option"
              aria-selected={sel}
              aria-current={cur ? 'true' : undefined}
              data-idx={i}
              onclick={(e) => onRowClick(e, i)}
              onmousemove={() => (rootsPicker.selectedIndex = i)}
            >
              <span class="dot" aria-hidden="true">{cur ? '●' : quick}</span>
              <span class="name">{entry.name}</span>
              <span class="path mono muted">{entry.path}</span>
              <span class="meta muted">
                {#if cur}<span class="badge">current</span>{/if}
                {#if rel}<span class="when">{rel}</span>{/if}
              </span>
              <button
                type="button"
                class="remove"
                aria-label={`Remove ${entry.name} from roots`}
                title="Remove from registry"
                disabled={rootsPicker.managing}
                onclick={(e) => onRemove(e, entry)}
              >×</button>
            </li>
          {/each}
        </ul>
      {/if}
      <footer class="status muted">
        {#if canRegisterCurrent}
          <button
            type="button"
            class="register"
            disabled={rootsPicker.managing}
            onclick={onRegisterCurrent}
          >+ Register Current Root: <span class="mono">{currentName}</span></button>
        {/if}
        <span class="add-by-path">
          <input
            bind:this={addInputEl}
            bind:value={addPath}
            type="text"
            class="add-input mono"
            placeholder="Add directory by path (~/repos/foo)"
            aria-label="Add directory by path"
            autocomplete="off"
            spellcheck="false"
            disabled={rootsPicker.managing}
            onkeydown={onAddInputKey}
          />
          <button
            type="button"
            class="register"
            disabled={rootsPicker.managing || addPath.trim() === ''}
            onclick={onAddByPath}
            aria-label="Register typed directory"
          >+ Add</button>
        </span>
        <span class="hints">
          <kbd>↑</kbd><kbd>↓</kbd> nav •
          <kbd>1</kbd>–<kbd>9</kbd> jump •
          <kbd>Enter</kbd> open •
          <kbd>Esc</kbd> close
        </span>
        {#if rootsPicker.spawning}
          <span class="spawning">spawning…</span>
        {:else if rootsPicker.managing}
          <span class="spawning">updating…</span>
        {/if}
      </footer>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    z-index: 1000;
    opacity: 1;
    transition: opacity var(--motion-fast) ease-out;
    @starting-style {
      opacity: 0;
    }
  }
  :global(:root[data-theme='light']) .overlay {
    background: rgba(0, 0, 0, 0.18);
  }
  .modal {
    width: 100%;
    max-width: 720px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 70vh;
    outline: none;
    transform: translateY(0) scale(1);
    transition: transform var(--motion-base) ease-out, opacity var(--motion-base) ease-out;
    @starting-style {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  .modal:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ctx-border);
  }
  .head h2 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--ctx-fg);
  }
  .head .count {
    font-weight: 400;
    margin-left: 4px;
  }
  .close {
    border: 0;
    padding: 0 8px;
    font-size: 16px;
    line-height: 1;
    color: var(--ctx-fg-dim);
    background: transparent;
    cursor: pointer;
  }
  .close:hover {
    color: var(--ctx-fg);
  }
  .filter {
    padding: 6px 12px;
    border-bottom: 1px solid var(--ctx-border);
  }
  .filter-input {
    width: 100%;
    box-sizing: border-box;
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 4px 8px;
    outline: none;
  }
  .filter-input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .banner {
    padding: 6px 12px;
    font-size: 12px;
    border-bottom: 1px solid var(--ctx-border);
  }
  .banner-error {
    color: var(--ctx-git-deleted);
    background: var(--ctx-bg-panel);
  }
  .banner-info {
    color: var(--ctx-fg-dim);
    background: var(--ctx-bg-panel);
  }
  .empty {
    padding: 24px;
    text-align: center;
    font-size: 12px;
  }
  .empty p {
    margin: 0 0 4px;
  }
  .empty .hint {
    font-size: 11px;
  }
  .empty code {
    font-family: var(--ctx-font-mono);
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    padding: 1px 4px;
    color: var(--ctx-fg);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow: auto;
    flex: 1 1 auto;
    min-height: 0;
  }
  .opt {
    padding: 4px 12px;
    cursor: pointer;
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr) minmax(0, 2fr) auto auto;
    gap: 10px;
    align-items: baseline;
    font-size: 12px;
    line-height: 1.5;
    border-left: 2px solid transparent;
    color: var(--ctx-fg);
  }
  .opt.selected {
    background: var(--ctx-bg-panel);
    border-left-color: var(--ctx-accent);
  }
  .opt.current .name {
    font-weight: 600;
  }
  .opt .dot {
    color: var(--ctx-fg-dim);
    font-size: 10px;
    text-align: center;
    font-family: var(--ctx-font-mono);
  }
  .opt.current .dot {
    color: var(--ctx-accent);
  }
  .opt .name {
    word-break: break-all;
  }
  .opt .path {
    word-break: break-all;
  }
  .opt .meta {
    font-size: 11px;
    white-space: nowrap;
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .badge {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--ctx-accent);
    border: 1px solid var(--ctx-accent);
    border-radius: 3px;
    padding: 0 4px;
    line-height: 1.4;
  }
  .when {
    color: var(--ctx-fg-dim);
  }
  .remove {
    border: 0;
    background: transparent;
    color: var(--ctx-fg-dim);
    font-size: 14px;
    line-height: 1;
    padding: 0 6px;
    cursor: pointer;
    border-radius: 3px;
    opacity: 0;
    transition: opacity var(--motion-fast) ease-out, color var(--motion-fast) ease-out, background var(--motion-fast) ease-out;
  }
  .opt:hover .remove,
  .opt.selected .remove,
  .remove:focus-visible {
    opacity: 1;
  }
  .remove:hover {
    color: var(--ctx-git-deleted);
    background: var(--ctx-bg);
  }
  .remove:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .remove:disabled {
    cursor: not-allowed;
    opacity: 0.4;
  }
  .status {
    padding: 6px 12px;
    border-top: 1px solid var(--ctx-border);
    font-size: 11px;
    flex: 0 0 auto;
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .status .hints {
    margin-left: auto;
  }
  .register {
    border: 1px solid var(--ctx-border);
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 11px;
    padding: 3px 8px;
    border-radius: 3px;
    cursor: pointer;
  }
  .register:hover:not(:disabled) {
    border-color: var(--ctx-accent);
    color: var(--ctx-accent);
  }
  .register:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .register:disabled {
    cursor: not-allowed;
    opacity: 0.6;
  }
  .register .mono {
    font-family: var(--ctx-font-mono);
  }
  .add-by-path {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 1 1 240px;
    min-width: 200px;
  }
  .add-input {
    flex: 1 1 auto;
    min-width: 0;
    font-size: 11px;
    padding: 3px 6px;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
  }
  .add-input::placeholder {
    color: var(--ctx-fg-dim);
  }
  .add-input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
    border-color: var(--ctx-accent);
  }
  .add-input:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .status kbd {
    font-family: var(--ctx-font-mono);
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    padding: 0 4px;
    margin: 0 2px;
    font-size: 10px;
  }
  .spawning {
    color: var(--ctx-accent);
  }
</style>
