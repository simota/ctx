<script lang="ts">
  import { fetchWhere, type WhereResponse } from '../lib/api';
  import { navigate, toFileHash } from '../lib/router.svelte';

  let { query } = $props<{ query: string }>();

  let data = $state<WhereResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    const q = query?.trim();
    if (!q) {
      data = null;
      return;
    }
    loading = true;
    error = null;
    // Cancellation guard: a slow earlier response must not overwrite a newer
    // query's results after rapid re-searches.
    let cancelled = false;
    fetchWhere(q, { limit: 30 })
      .then((r) => {
        if (!cancelled) data = r;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  function openFile(path: string) {
    navigate(toFileHash(path));
  }
</script>

<section class="results" aria-label="search results">
  <header>
    <h2>Search</h2>
    {#if query}
      <p class="muted query mono">q = {query}</p>
    {/if}
  </header>

  {#if !query}
    <p class="muted">Enter a query above.</p>
  {:else if loading}
    <p class="muted" aria-busy="true">Searching…</p>
  {:else if error}
    <div class="error">
      <p>Search failed.</p>
      <code class="mono">{error}</code>
    </div>
  {:else if data && data.results.length === 0}
    <p class="muted">No matches.</p>
  {:else if data}
    <ol class="hit-list">
      {#each data.results as r, idx (r.path + ':' + idx)}
        <li>
          <button
            class="hit"
            type="button"
            onclick={() => openFile(r.path)}
            aria-label="open {r.path}"
          >
            <span class="path mono">{r.path}</span>
            <span class="score muted">score {r.score.toFixed(2)}</span>
            <span class="reason muted">{r.reason}</span>
            {#if r.matches.length > 0}
              <ul class="matches">
                {#each r.matches.slice(0, 3) as m, mi (`${mi}:${m.line}:${m.column}`)}
                  <li class="mono">
                    <span class="loc muted">L{m.line}:{m.column}</span>
                    <span class="text">{m.text}</span>
                  </li>
                {/each}
              </ul>
            {/if}
          </button>
        </li>
      {/each}
    </ol>
  {/if}
</section>

<style>
  .results {
    padding: 16px 20px;
    max-width: 960px;
  }
  header {
    margin-bottom: 12px;
  }
  h2 {
    margin: 0 0 4px;
    font-size: 18px;
  }
  .query {
    margin: 0;
    font-size: 12px;
  }
  .hit-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .hit {
    display: grid;
    grid-template-columns: 1fr auto;
    grid-template-rows: auto auto auto;
    gap: 2px 12px;
    width: 100%;
    text-align: left;
    padding: 8px 12px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    color: inherit;
    cursor: pointer;
  }
  .hit:hover {
    border-color: var(--ctx-accent-dim);
  }
  .hit:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .path {
    grid-column: 1;
    grid-row: 1;
    color: var(--ctx-link);
    font-weight: 500;
    word-break: break-all;
  }
  .score {
    grid-column: 2;
    grid-row: 1;
    font-size: 11px;
  }
  .reason {
    grid-column: 1 / -1;
    grid-row: 2;
    font-size: 11px;
  }
  .matches {
    grid-column: 1 / -1;
    grid-row: 3;
    list-style: none;
    margin: 4px 0 0;
    padding: 0;
    font-size: 11px;
  }
  .matches li {
    display: grid;
    grid-template-columns: 56px 1fr;
    gap: 6px;
    padding: 1px 0;
  }
  .matches .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .error code {
    display: block;
    margin: 6px 0;
    color: var(--ctx-err);
  }
</style>
