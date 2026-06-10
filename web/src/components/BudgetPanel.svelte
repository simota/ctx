<script lang="ts">
  import { fetchBudget, type BudgetResponse } from '../lib/api';
  import { formatTokens } from '../lib/format';
  import { navigate, toFileHash } from '../lib/router.svelte';

  let data = $state<BudgetResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  function load() {
    loading = true;
    error = null;
    fetchBudget({})
      .then((r) => {
        data = r;
      })
      .catch((e: unknown) => {
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        loading = false;
      });
  }

  $effect(() => {
    load();
  });

  let pct = $derived.by(() => {
    if (!data || data.budget <= 0) return 0;
    return Math.min(100, (data.used / data.budget) * 100);
  });

  let pctColor = $derived.by(() => {
    if (pct < 60) return 'var(--ctx-accent)';
    if (pct < 90) return 'var(--ctx-warn)';
    return 'var(--ctx-err)';
  });
</script>

<section class="budget" aria-label="budget panel">
  <header>
    <h2>Budget</h2>
    <button onclick={load} aria-label="reload budget">Reload</button>
  </header>

  {#if loading && !data}
    <p class="muted" aria-busy="true">Loading…</p>
  {:else if error}
    <div class="error">
      <p>Budget load failed.</p>
      <code class="mono">{error}</code>
      <button onclick={load}>Retry</button>
    </div>
  {:else if data}
    <div class="summary">
      <div class="numbers">
        <span
          class="used mono"
          title="Tokens consumed so far by selected files."
        >{formatTokens(data.used)}</span>
        <span class="muted"> / </span>
        <span
          class="total mono"
          title="Token budget for an LLM call. Bigger = more context, more cost."
        >{formatTokens(data.budget)}</span>
        <span class="muted"> tokens</span>
      </div>
      <div
        class="bar"
        role="progressbar"
        aria-valuenow={data.used}
        aria-valuemin="0"
        aria-valuemax={data.budget}
        aria-label="budget usage"
      >
        <div class="fill" style="--scale: {pct / 100}; background: {pctColor};"></div>
      </div>
      <p class="muted pct">{pct.toFixed(1)}% used</p>
    </div>

    <div class="lists">
      <section class="col">
        <h3>Included <span class="muted">({data.included.length})</span></h3>
        <ul>
          {#each data.included as it, ii (`${ii}:${it.path}:${it.group ?? ''}`)}
            <li>
              <button
                type="button"
                class="row mono"
                onclick={() => navigate(toFileHash(it.path))}
                aria-label="open {it.path}"
              >
                <span class="path">{it.path}</span>
                <span class="tokens" title="Approx LLM tokens (cl100k_base).">{formatTokens(it.tokens)}</span>
                {#if it.reason}<span class="reason muted">{it.reason}</span>{/if}
              </button>
            </li>
          {:else}
            <li class="muted">none</li>
          {/each}
        </ul>
      </section>
      <section class="col">
        <h3>Excluded <span class="muted">({data.excluded.length})</span></h3>
        <ul>
          {#each data.excluded as it, ei (`${ei}:${it.path}:${it.group ?? ''}`)}
            <li>
              <button
                type="button"
                class="row mono excluded"
                onclick={() => navigate(toFileHash(it.path))}
                aria-label="open {it.path}"
              >
                <span class="path">{it.path}</span>
                <span class="tokens" title="Approx LLM tokens (cl100k_base).">{formatTokens(it.tokens)}</span>
                {#if it.reason}<span class="reason muted">{it.reason}</span>{/if}
              </button>
            </li>
          {:else}
            <li class="muted">none</li>
          {/each}
        </ul>
      </section>
    </div>
  {/if}
</section>

<style>
  .budget {
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 16px;
    max-width: 1100px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  .summary .numbers {
    font-size: 18px;
  }
  .used {
    color: var(--ctx-accent);
    font-weight: 600;
  }
  .total {
    color: var(--ctx-fg);
  }
  .bar {
    margin-top: 6px;
    height: 8px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    overflow: hidden;
  }
  .fill {
    /* Compositor-only fill: scaleX from a 100% template so progress changes
       stay off the main thread (avoids width-driven layout/paint and the
       CLS contribution that comes with it). transform-origin pins the
       growth to the left edge so the bar fills like a traditional one. */
    height: 100%;
    width: 100%;
    transform-origin: left center;
    transform: scaleX(var(--scale, 0));
    transition: transform var(--motion-slow) ease-out;
  }
  .pct {
    margin: 4px 0 0;
    font-size: 11px;
  }
  .lists {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  @media (max-width: 800px) {
    .lists {
      grid-template-columns: 1fr;
    }
  }
  h3 {
    margin: 0 0 8px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .row {
    width: 100%;
    text-align: left;
    border: 0;
    border-radius: 3px;
    padding: 4px 8px;
    display: grid;
    grid-template-columns: 1fr auto;
    gap: 2px 12px;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 12px;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .path {
    grid-column: 1;
    grid-row: 1;
    color: var(--ctx-link);
    word-break: break-all;
  }
  .tokens {
    grid-column: 2;
    grid-row: 1;
    color: var(--ctx-fg);
  }
  .reason {
    grid-column: 1 / -1;
    grid-row: 2;
    font-size: 11px;
  }
  .excluded .path {
    color: var(--ctx-fg-dim);
    text-decoration: line-through dotted;
  }
  .error code {
    display: block;
    margin: 6px 0;
    color: var(--ctx-err);
  }
</style>
