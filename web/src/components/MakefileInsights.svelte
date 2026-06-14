<script lang="ts">
  import { extractMakefileInsights } from '../lib/makefile-insights';

  let {
    content,
    onJump,
  }: {
    content: string;
    onJump?: (line: number) => void;
  } = $props();

  let insights = $derived(extractMakefileInsights(content));
</script>

<div class="insights">
  {#if !insights.ok}
    <section>
      <p class="muted">Empty Makefile.</p>
    </section>
  {:else}
    {#if insights.targets.length > 0}
      <section aria-label="Makefile targets">
        <h3>
          Targets <span class="count muted">{insights.targets.length}</span>
          {#if insights.phonyCount > 0}
            <span class="meta muted">{insights.phonyCount} phony</span>
          {/if}
        </h3>
        <ul>
          {#each insights.targets as t, i (`${t.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row target-row"
                title={t.doc || (t.prereqs.length ? `deps: ${t.prereqs.join(' ')}` : t.name)}
                aria-label={`Jump to target ${t.name} on line ${t.line}`}
                onclick={() => onJump?.(t.line)}
              >
                <span class="glyph muted" aria-hidden="true">{t.phony ? '◇' : '▸'}</span>
                <span class="key mono">{t.name}</span>
                <span class="preview muted">{t.doc || t.prereqs.join(' ')}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.variables.length > 0}
      <section aria-label="Makefile variables">
        <h3>Variables <span class="count muted">{insights.variables.length}</span></h3>
        <ul>
          {#each insights.variables as v, i (`${v.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row dep-row"
                title={`${v.name} ${v.op} ${v.value}`}
                aria-label={`Jump to variable ${v.name} on line ${v.line}`}
                onclick={() => onJump?.(v.line)}
              >
                <span class="key mono">{v.name}</span>
                <span class="value muted mono">{v.value || '—'}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.includes.length > 0}
      <section aria-label="Makefile includes">
        <h3>Includes <span class="count muted">{insights.includes.length}</span></h3>
        <ul>
          {#each insights.includes as inc, i (`${inc.path}:${i}`)}
            <li>
              <button
                type="button"
                class="row inc-row"
                aria-label={`Jump to include ${inc.path} on line ${inc.line}`}
                onclick={() => onJump?.(inc.line)}
              >
                <span class="key mono">{inc.path}</span>
                <span class="line muted">{inc.optional ? 'opt' : ''} L{inc.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .insights {
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
  /* Target row: glyph | name | doc/prereqs. */
  .target-row {
    grid-template-columns: 18px 1fr auto;
  }
  .target-row .glyph {
    font-size: 10px;
    text-align: center;
  }
  .target-row .key,
  .target-row .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .target-row .preview {
    font-size: 10px;
    max-width: 130px;
  }
  /* Two-column key + value rows (variables). */
  .dep-row {
    grid-template-columns: 1fr 1fr;
  }
  .dep-row .key,
  .dep-row .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dep-row .value {
    font-size: 10px;
    text-align: end;
  }
  /* Single-column rows with a line tag (includes). */
  .inc-row {
    grid-template-columns: 1fr auto;
  }
  .inc-row .key {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .inc-row .line {
    font-size: 10px;
  }
</style>
