<script lang="ts">
  import { finder, closeFinder } from '../lib/finder.svelte';
  import { navigate, toFileHash } from '../lib/router.svelte';

  let inputEl: HTMLInputElement | null = $state(null);
  let listEl: HTMLUListElement | null = $state(null);

  interface Match {
    path: string;
    score: number;
    positions: number[]; // matched char indices into `path`
  }

  // Fuzzy subsequence match with:
  //  - case-insensitive
  //  - consecutive-run bonus
  //  - basename match bonus (each char in basename worth more)
  //  - earlier-position preference
  //  - small penalty for very long paths
  function scoreMatch(path: string, query: string): Match | null {
    if (query === '') {
      return { path, score: 0, positions: [] };
    }
    const target = path.toLowerCase();
    const q = query.toLowerCase();
    const baseStart = path.lastIndexOf('/') + 1;
    const positions: number[] = [];
    let qi = 0;
    let ti = 0;
    let score = 0;
    let runLen = 0;
    while (qi < q.length && ti < target.length) {
      if (target[ti] === q[qi]) {
        positions.push(ti);
        // Base char score: 2; basename chars: +3 (so 5).
        score += 2;
        if (ti >= baseStart) score += 3;
        // Consecutive run bonus, capped via cumulative growth.
        runLen += 1;
        if (runLen > 1) score += runLen * 2;
        // Match at start of basename or path: +4.
        if (ti === baseStart || ti === 0) score += 4;
        // Match right after a path separator/dot/underscore/dash: +2.
        if (ti > 0) {
          const prev = target[ti - 1];
          if (prev === '/' || prev === '.' || prev === '_' || prev === '-') {
            score += 2;
          }
        }
        qi += 1;
        ti += 1;
      } else {
        runLen = 0;
        ti += 1;
      }
    }
    if (qi < q.length) return null;
    // Prefer shorter targets (small penalty proportional to extra chars).
    score -= Math.floor(path.length / 40);
    // Prefer matches that end nearer the end (closer to basename completion).
    const span = positions[positions.length - 1] - positions[0];
    score -= Math.floor(span / 8);
    return { path, score, positions };
  }

  const MAX_RESULTS = 20;

  let results = $derived.by<Match[]>(() => {
    const q = finder.query;
    if (!finder.open) return [];
    if (finder.files.length === 0) return [];
    if (q === '') {
      // Show the first MAX_RESULTS files unscored when query is empty.
      const out: Match[] = [];
      for (let i = 0; i < Math.min(MAX_RESULTS, finder.files.length); i++) {
        out.push({ path: finder.files[i], score: 0, positions: [] });
      }
      return out;
    }
    const matches: Match[] = [];
    for (let i = 0; i < finder.files.length; i++) {
      const m = scoreMatch(finder.files[i], q);
      if (m !== null) matches.push(m);
    }
    matches.sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score;
      return a.path.length - b.path.length;
    });
    return matches.slice(0, MAX_RESULTS);
  });

  // Clamp selection whenever results change.
  $effect(() => {
    const len = results.length;
    if (finder.selectedIndex >= len) {
      finder.selectedIndex = len > 0 ? len - 1 : 0;
    }
    if (finder.selectedIndex < 0) finder.selectedIndex = 0;
  });

  // Focus input on open and scroll selected option into view on change.
  $effect(() => {
    if (finder.open && inputEl) {
      const el = inputEl;
      queueMicrotask(() => el.focus());
    }
  });

  $effect(() => {
    if (!listEl) return;
    const idx = finder.selectedIndex;
    const opt = listEl.querySelector<HTMLElement>(`[data-idx="${idx}"]`);
    if (opt) opt.scrollIntoView({ block: 'nearest' });
  });

  function open(path: string) {
    closeFinder();
    navigate(toFileHash(path));
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeFinder();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (results.length === 0) return;
      finder.selectedIndex = (finder.selectedIndex + 1) % results.length;
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (results.length === 0) return;
      finder.selectedIndex =
        (finder.selectedIndex - 1 + results.length) % results.length;
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const sel = results[finder.selectedIndex];
      if (sel) open(sel.path);
      return;
    }
  }

  function onOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) closeFinder();
  }

  // Highlight matched chars by splitting the path into segments. Returns an
  // array of {text, mark} pairs for safe interpolation (no @html needed).
  function highlight(path: string, positions: number[]): { text: string; mark: boolean }[] {
    if (positions.length === 0) return [{ text: path, mark: false }];
    const out: { text: string; mark: boolean }[] = [];
    let cursor = 0;
    let i = 0;
    while (i < positions.length) {
      // Collect a run of consecutive positions.
      const start = positions[i];
      let end = start;
      let j = i + 1;
      while (j < positions.length && positions[j] === positions[j - 1] + 1) {
        end = positions[j];
        j += 1;
      }
      if (start > cursor) {
        out.push({ text: path.slice(cursor, start), mark: false });
      }
      out.push({ text: path.slice(start, end + 1), mark: true });
      cursor = end + 1;
      i = j;
    }
    if (cursor < path.length) {
      out.push({ text: path.slice(cursor), mark: false });
    }
    return out;
  }

  function splitPath(p: string): { dir: string; name: string } {
    const i = p.lastIndexOf('/');
    if (i === -1) return { dir: '', name: p };
    return { dir: p.slice(0, i + 1), name: p.slice(i + 1) };
  }
</script>

{#if finder.open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="overlay"
    onclick={onOverlayClick}
    onkeydown={onKey}
    role="presentation"
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-label="File finder"
    >
      <input
        bind:this={inputEl}
        bind:value={finder.query}
        type="text"
        class="input"
        role="combobox"
        aria-expanded={results.length > 0}
        aria-controls="ctx-finder-list"
        aria-autocomplete="list"
        aria-activedescendant={results.length > 0
          ? `ctx-finder-opt-${finder.selectedIndex}`
          : undefined}
        placeholder={finder.loading ? 'Loading files…' : 'Search files…'}
        autocomplete="off"
        spellcheck="false"
        onkeydown={onKey}
      />
      <ul
        bind:this={listEl}
        id="ctx-finder-list"
        class="results"
        role="listbox"
        aria-label="Matching files"
      >
        {#if finder.error}
          <li class="empty error">Failed to load files: {finder.error}</li>
        {:else if finder.loading && finder.files.length === 0}
          <li class="empty muted">Loading file list…</li>
        {:else if results.length === 0}
          <li class="empty muted">
            {finder.query === '' ? 'No files indexed.' : 'No matches.'}
          </li>
        {:else}
          {#each results as r, i (`${i}:${r.path}`)}
            {@const sp = splitPath(r.path)}
            {@const segs = highlight(r.path, r.positions)}
            <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
            <li
              id={`ctx-finder-opt-${i}`}
              class="opt"
              class:selected={i === finder.selectedIndex}
              role="option"
              aria-selected={i === finder.selectedIndex}
              data-idx={i}
              onclick={() => open(r.path)}
              onmousemove={() => (finder.selectedIndex = i)}
            >
              <span class="name mono">
                {#if finder.query === ''}
                  <span class="dir">{sp.dir}</span><span class="base">{sp.name}</span>
                {:else}
                  {#each segs as seg, si (si)}
                    {#if seg.mark}<mark>{seg.text}</mark>{:else}<span>{seg.text}</span>{/if}
                  {/each}
                {/if}
              </span>
            </li>
          {/each}
        {/if}
      </ul>
      <footer class="status muted">
        {#if finder.query === ''}
          {finder.files.length} file{finder.files.length === 1 ? '' : 's'} • <kbd>Esc</kbd> to close
        {:else}
          {results.length} match{results.length === 1 ? '' : 'es'} • <kbd>↑</kbd><kbd>↓</kbd> navigate • <kbd>Enter</kbd> open • <kbd>Esc</kbd> close
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
  }
  :global(:root[data-theme='light']) .overlay {
    background: rgba(0, 0, 0, 0.18);
  }
  .modal {
    width: 100%;
    max-width: 600px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 70vh;
  }
  .input {
    border: 0;
    border-bottom: 1px solid var(--ctx-border);
    border-radius: 6px 6px 0 0;
    background: transparent;
    padding: 10px 14px;
    font-size: 14px;
    outline: none;
  }
  .input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
    border-color: transparent;
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
    padding: 4px 14px;
    cursor: pointer;
    display: block;
    font-size: 12px;
    line-height: 1.5;
    border-left: 2px solid transparent;
  }
  .opt.selected {
    background: var(--ctx-bg-panel);
    border-left-color: var(--ctx-accent);
  }
  .opt .name {
    color: var(--ctx-fg);
    word-break: break-all;
  }
  .opt .dir {
    color: var(--ctx-fg-dim);
  }
  .opt .base {
    color: var(--ctx-fg);
    font-weight: 500;
  }
  .opt mark {
    background: transparent;
    color: var(--ctx-accent);
    font-weight: 600;
    padding: 0;
  }
  .empty {
    padding: 16px;
    text-align: center;
    font-size: 12px;
  }
  .empty.error {
    color: var(--ctx-err);
  }
  .status {
    padding: 6px 14px;
    border-top: 1px solid var(--ctx-border);
    font-size: 11px;
    flex: 0 0 auto;
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
</style>
