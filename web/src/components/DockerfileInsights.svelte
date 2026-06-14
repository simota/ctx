<script lang="ts">
  import { extractDockerfileInsights } from '../lib/dockerfile-insights';

  let {
    content,
    onJump,
  }: {
    content: string;
    onJump?: (line: number) => void;
  } = $props();

  let insights = $derived(extractDockerfileInsights(content));
</script>

<div class="insights">
  {#if !insights.ok}
    <section>
      <p class="muted">Empty Dockerfile.</p>
    </section>
  {:else}
    {#if insights.stages.length > 0}
      <section aria-label="build stages">
        <h3>
          Stages <span class="count muted">{insights.stages.length}</span>
          {#if insights.exposeRaw.length > 0}
            <span class="meta muted">:{insights.exposeRaw.join(' :')}</span>
          {/if}
        </h3>
        <ul>
          {#each insights.stages as s (s.index)}
            <li>
              <button
                type="button"
                class="row stage-row"
                title={`FROM ${s.base}${s.name ? ` AS ${s.name}` : ''} · ${s.runCount} RUN · ${s.copyCount} COPY/ADD`}
                aria-label={`Jump to stage ${s.name || s.base} on line ${s.line}`}
                onclick={() => onJump?.(s.line)}
              >
                <span class="glyph muted" aria-hidden="true">{s.index}</span>
                <span class="key mono">{s.name || s.base}</span>
                <span class="preview muted">{s.name ? s.base : ''}</span>
                <span class="weight muted mono">{s.runCount}R/{s.copyCount}C</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.ports.length > 0}
      <section aria-label="exposed ports">
        <h3>Exposed <span class="count muted">{insights.ports.length}</span></h3>
        <ul>
          {#each insights.ports as p, i (`${p.port}:${i}`)}
            <li>
              <button
                type="button"
                class="row inc-row"
                aria-label={`Jump to EXPOSE ${p.port} on line ${p.line}`}
                onclick={() => onJump?.(p.line)}
              >
                <span class="key mono">{p.port}</span>
                <span class="line muted">L{p.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.args.length > 0}
      <section aria-label="global build args">
        <h3>Build args <span class="count muted">{insights.args.length}</span></h3>
        <ul>
          {#each insights.args as a, i (`${a.key}:${i}`)}
            <li>
              <button
                type="button"
                class="row dep-row"
                title={`${a.key}${a.value ? `=${a.value}` : ''}`}
                aria-label={`Jump to ARG ${a.key} on line ${a.line}`}
                onclick={() => onJump?.(a.line)}
              >
                <span class="key mono">{a.key}</span>
                <span class="value muted mono">{a.value || '—'}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.entrypoint || insights.cmd}
      <section aria-label="container entry">
        <h3>Entry</h3>
        <ul>
          {#if insights.entrypoint}
            <li>
              <div class="row entry-row" title={insights.entrypoint}>
                <span class="kind mono">ENTRYPOINT</span>
                <span class="value mono">{insights.entrypoint}</span>
              </div>
            </li>
          {/if}
          {#if insights.cmd}
            <li>
              <div class="row entry-row" title={insights.cmd}>
                <span class="kind mono">CMD</span>
                <span class="value mono">{insights.cmd}</span>
              </div>
            </li>
          {/if}
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 120px;
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
  button.row:hover {
    background: var(--ctx-bg-elev);
  }
  button.row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  /* Stage row: index | name | base | weight. */
  .stage-row {
    grid-template-columns: 16px auto 1fr auto;
  }
  .stage-row .glyph {
    font-size: 10px;
    text-align: center;
  }
  .stage-row .key,
  .stage-row .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stage-row .key {
    color: var(--ctx-link);
    font-weight: 600;
  }
  .stage-row .preview {
    font-size: 10px;
    max-width: 120px;
  }
  .stage-row .weight {
    font-size: 10px;
  }
  /* Two-column key + value rows (args). */
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
  /* Single-column rows with a line tag (ports). */
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
  /* Entry rows are non-interactive (no jump target distinct from the keyword). */
  .entry-row {
    grid-template-columns: auto 1fr;
    cursor: default;
  }
  .entry-row .kind {
    color: var(--ctx-fg-dim);
    font-size: 10px;
  }
  .entry-row .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
</style>
