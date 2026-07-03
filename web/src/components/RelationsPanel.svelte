<script lang="ts">
  import { fetchRelations, type RelationsResponse } from '../lib/api';
  import { navigate, toFileHash } from '../lib/router.svelte';
  import { openTab } from '../lib/tabs.svelte';
  import { announce } from '../lib/announce.svelte';
  import { basename } from '../lib/format';

  let { path }: { path: string } = $props();

  let data = $state<RelationsResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // Refetch whenever `path` changes. The previous response is held until the
  // new one lands so the panel does not flash empty on every navigation.
  $effect(() => {
    const target = path;
    if (!target) {
      data = null;
      return;
    }
    loading = true;
    error = null;
    let cancelled = false;
    fetchRelations(target)
      .then((r) => {
        if (cancelled) return;
        data = r;
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  // Render only the basename in the label so two-column layouts stay compact.
  // The full repo-relative path is kept in `title` for hover disclosure.
  function dirname(p: string): string {
    const slash = p.lastIndexOf('/');
    return slash >= 0 ? p.slice(0, slash) : '';
  }

  function go(target: string, e: MouseEvent) {
    if (e.metaKey || e.ctrlKey) {
      openTab(target);
      announce(`Opened ${target} in new tab`);
      return;
    }
    navigate(toFileHash(target));
    announce(`Jumped to ${target}`);
  }
</script>

<div class="relations" aria-label="file relations">
  {#if loading && !data}
    <section>
      <p class="muted">Loading…</p>
    </section>
  {:else if error}
    <section>
      <p class="muted">Could not load relations.</p>
      <code class="mono err">{error}</code>
    </section>
  {:else if data}
    {#if data.imports.length === 0 && data.importers.length === 0}
      <section>
        <p class="muted">No in-repo imports detected.</p>
      </section>
    {/if}

    {#if data.imports.length > 0}
      <section aria-label="files this file imports">
        <h3>
          Imports <span class="count muted">{data.imports.length}</span>
          <span class="meta muted">out</span>
        </h3>
        <ul>
          {#each data.imports as it, i (`${it.path}:${i}`)}
            <li>
              <button
                type="button"
                class="row"
                title={it.path}
                aria-label={`Open ${it.path}`}
                onclick={(e) => go(it.path, e)}
              >
                <span class="arrow muted" aria-hidden="true">→</span>
                <span class="name mono">{basename(it.path)}</span>
                <span class="dir muted mono">{dirname(it.path)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if data.importers.length > 0}
      <section aria-label="files that import this file">
        <h3>
          Importers <span class="count muted">{data.importers.length}</span>
          <span class="meta muted">in</span>
        </h3>
        <ul>
          {#each data.importers as it, i (`${it.path}:${i}`)}
            <li>
              <button
                type="button"
                class="row"
                title={it.path}
                aria-label={`Open ${it.path}`}
                onclick={(e) => go(it.path, e)}
              >
                <span class="arrow muted" aria-hidden="true">←</span>
                <span class="name mono">{basename(it.path)}</span>
                <span class="dir muted mono">{dirname(it.path)}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .relations {
    display: flex;
    flex-direction: column;
  }
  section {
    padding: 10px 12px;
    border-block-start: 1px solid var(--ctx-border);
  }
  section:first-child {
    border-block-start: 0;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .meta {
    margin-inline-start: auto;
    font-size: 10px;
    font-weight: 400;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    padding: 0;
  }
  .row {
    display: grid;
    grid-template-columns: 14px 1fr auto;
    align-items: baseline;
    gap: 6px;
    width: 100%;
    padding: 3px 4px;
    font-size: 11px;
    text-align: start;
    border: 0;
    background: transparent;
    color: var(--ctx-fg);
    border-radius: 3px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .arrow {
    font-size: 10px;
    text-align: center;
  }
  .name,
  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dir {
    font-size: 10px;
    max-width: 140px;
  }
  .err {
    display: block;
    margin-top: 4px;
    color: var(--ctx-err);
    word-break: break-all;
    font-size: 10px;
  }
</style>
