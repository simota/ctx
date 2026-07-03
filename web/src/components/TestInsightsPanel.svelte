<script lang="ts">
  import { fetchTestInsights, type TestInsightResponse } from '../lib/api';
  import { navigate, toFileHash } from '../lib/router.svelte';
  import { openTab } from '../lib/tabs.svelte';
  import { announce } from '../lib/announce.svelte';
  import { basename } from '../lib/format';

  let { path }: { path: string } = $props();

  let data = $state<TestInsightResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  $effect(() => {
    const target = path;
    if (!target) {
      data = null;
      return;
    }
    loading = true;
    error = null;
    let cancelled = false;
    fetchTestInsights(target)
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

  function pct(v: number): string {
    return `${v.toFixed(1)}%`;
  }

  function rangeLabel(start: number, end: number): string {
    return start === end ? `L${start}` : `L${start}-${end}`;
  }

  function countLabel(visible: number, total?: number): string {
    if (!total || total <= visible) return String(visible);
    return `${visible}/${total}`;
  }
</script>

<div class="tests" aria-label="test insights">
  {#if loading && !data}
    <section>
      <p class="muted">Loading…</p>
    </section>
  {:else if error}
    <section>
      <p class="muted">Could not load tests.</p>
      <code class="mono err">{error}</code>
    </section>
  {:else if data}
    {#if data.coverage}
      <section aria-label="coverage summary">
        <h3>
          Coverage <span class="count muted">{pct(data.coverage.percent)}</span>
          <span class="meta muted">{data.coverage.profile}</span>
        </h3>
        <div class="meter" aria-label={`Coverage ${pct(data.coverage.percent)}`}>
          <span style={`width: ${Math.max(0, Math.min(100, data.coverage.percent))}%`}></span>
        </div>
        <p class="coverage-detail muted">
          {data.coverage.covered_stmts}/{data.coverage.total_stmts} statements
        </p>
        {#if data.coverage.uncovered_lines && data.coverage.uncovered_lines.length > 0}
          <div class="ranges" aria-label="uncovered lines">
            {#each data.coverage.uncovered_lines.slice(0, 8) as r}
              <a
                class="range mono"
                href={`#/file/${path.split('/').map(encodeURIComponent).join('/')}?L=${r.start}`}
              >
                {rangeLabel(r.start, r.end)}
              </a>
            {/each}
          </div>
        {/if}
      </section>
    {/if}

    {#if data.sources && data.sources.length > 0}
      <section aria-label="source files likely covered by this test">
        <h3>
          Sources <span class="count muted">{countLabel(data.sources.length, data.total_sources)}</span>
          {#if !data.coverage}
            <span class="meta muted">test target</span>
          {/if}
        </h3>
        <ul>
          {#each data.sources as it, i (`${it.path}:${i}`)}
            <li>
              <button
                type="button"
                class="row"
                title={`${it.path}${it.reasons?.length ? ` — ${it.reasons.join(', ')}` : ''}`}
                aria-label={`Open ${it.path}`}
                onclick={(e) => go(it.path, e)}
              >
                <span class="score mono" aria-label={`score ${it.score}`}>{it.score}</span>
                <span class="name mono">{basename(it.path)}</span>
                <span class="dir muted mono">{dirname(it.path)}</span>
              </button>
              {#if it.matched_symbols && it.matched_symbols.length > 0}
                <div class="test-meta muted">
                  <span class="symbols mono">{it.matched_symbols.slice(0, 4).join(', ')}</span>
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    <section aria-label="related tests">
      <h3>
        Tests <span class="count muted">{countLabel(data.tests.length, data.total_tests)}</span>
        {#if !data.coverage}
          <span class="meta muted">no coverage.out</span>
        {/if}
      </h3>
      {#if data.tests.length === 0}
        <p class="muted">No related tests detected.</p>
      {:else}
        <ul>
          {#each data.tests as it, i (`${it.path}:${i}`)}
            <li>
              <button
                type="button"
                class="row"
                title={`${it.path}${it.reasons?.length ? ` — ${it.reasons.join(', ')}` : ''}`}
                aria-label={`Open ${it.path}`}
                onclick={(e) => go(it.path, e)}
              >
                <span class="score mono" aria-label={`score ${it.score}`}>{it.score}</span>
                <span class="name mono">{basename(it.path)}</span>
                <span class="dir muted mono">{dirname(it.path)}</span>
              </button>
              {#if it.test_count || (it.matched_symbols && it.matched_symbols.length > 0)}
                <div class="test-meta muted">
                  {#if it.test_count}
                    <span>{it.test_count} test{it.test_count !== 1 ? 's' : ''}</span>
                  {/if}
                  {#if it.matched_symbols && it.matched_symbols.length > 0}
                    <span class="symbols mono">{it.matched_symbols.slice(0, 4).join(', ')}</span>
                  {/if}
                </div>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</div>

<style>
  .tests {
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
    max-width: 110px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .meter {
    height: 6px;
    border-radius: 3px;
    background: var(--ctx-bg-elev);
    overflow: hidden;
    border: 1px solid var(--ctx-border);
  }
  .meter span {
    display: block;
    height: 100%;
    background: var(--ctx-accent);
  }
  .coverage-detail {
    margin: 6px 0 0;
    font-size: 10px;
  }
  .ranges {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 7px;
  }
  .range {
    color: var(--ctx-fg);
    text-decoration: none;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    padding: 1px 4px;
    font-size: 10px;
  }
  .range:hover {
    background: var(--ctx-bg-elev);
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    padding: 0 0 5px;
  }
  .row {
    display: grid;
    grid-template-columns: 28px 1fr auto;
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
  .score {
    font-size: 10px;
    color: var(--ctx-accent);
    text-align: end;
  }
  .name,
  .dir {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dir {
    font-size: 10px;
    max-width: 120px;
  }
  .test-meta {
    display: flex;
    gap: 6px;
    padding-left: 38px;
    font-size: 10px;
    min-width: 0;
  }
  .symbols {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .err {
    display: block;
    margin-top: 4px;
    color: var(--ctx-err);
    word-break: break-all;
    font-size: 10px;
  }
</style>
