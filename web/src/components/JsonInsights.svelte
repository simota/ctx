<script lang="ts">
  import PaletteList from './PaletteList.svelte';
  import {
    extractJsonInsights,
    type JsonValueType,
    type JsonDependency,
  } from '../lib/json-insights';

  let {
    path,
    content,
    onJump,
  }: {
    path: string;
    content: string;
    onJump?: (line: number) => void;
  } = $props();

  let insights = $derived(extractJsonInsights(path, content));

  // Type glyph: a short ASCII tag rather than an icon font, so it stays
  // readable in any theme without bundle dependencies.
  function typeGlyph(t: JsonValueType): string {
    switch (t) {
      case 'object': return '{}';
      case 'array': return '[]';
      case 'string': return '""';
      case 'number': return '#';
      case 'boolean': return 'T/F';
      case 'null': return '∅';
    }
  }

  function depsLabel(d: JsonDependency[]): string {
    return d.length === 1 ? '1 dep' : `${d.length} deps`;
  }
</script>

<div class="insights">
  {#if !insights.ok}
    <section>
      <p class="muted">Could not parse JSON.</p>
    </section>
  {:else}
    {#if insights.outline.length > 0}
      <section aria-label="JSON outline">
        <h3>
          Outline <span class="count muted">{insights.outline.length}</span>
          <span class="meta muted">d{insights.maxDepth} · {insights.totalKeys}k</span>
        </h3>
        <ul>
          {#each insights.outline as e, i (`${e.key}:${i}`)}
            <li>
              <button
                type="button"
                class="row outline-row"
                aria-label={`Jump to ${e.key} on line ${e.line}`}
                onclick={() => onJump?.(e.line)}
              >
                <span class="glyph muted" aria-hidden="true">{typeGlyph(e.type)}</span>
                <span class="key mono">{e.key}</span>
                <span class="preview muted">{e.preview}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.isPackageJson}
      {#if insights.scripts.length > 0}
        <section aria-label="package scripts">
          <h3>Scripts <span class="count muted">{insights.scripts.length}</span></h3>
          <ul>
            {#each insights.scripts as s, i (`${s.name}:${i}`)}
              <li>
                <button
                  type="button"
                  class="row dep-row"
                  title={s.command}
                  aria-label={`Jump to script ${s.name} on line ${s.line}`}
                  onclick={() => onJump?.(s.line)}
                >
                  <span class="key mono">{s.name}</span>
                  <span class="value muted mono">{s.command}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if insights.deps.length > 0}
        <section aria-label="runtime dependencies">
          <h3>Dependencies <span class="count muted">{depsLabel(insights.deps)}</span></h3>
          <ul>
            {#each insights.deps as d, i (`${d.name}:${i}`)}
              <li>
                <button
                  type="button"
                  class="row dep-row"
                  aria-label={`Jump to ${d.name} on line ${d.line}`}
                  onclick={() => onJump?.(d.line)}
                >
                  <span class="key mono">{d.name}</span>
                  <span class="value muted mono">{d.version}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if insights.devDeps.length > 0}
        <section aria-label="dev dependencies">
          <h3>Dev Dependencies <span class="count muted">{depsLabel(insights.devDeps)}</span></h3>
          <ul>
            {#each insights.devDeps as d, i (`${d.name}:${i}`)}
              <li>
                <button
                  type="button"
                  class="row dep-row"
                  aria-label={`Jump to ${d.name} on line ${d.line}`}
                  onclick={() => onJump?.(d.line)}
                >
                  <span class="key mono">{d.name}</span>
                  <span class="value muted mono">{d.version}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if insights.peerDeps.length > 0}
        <section aria-label="peer dependencies">
          <h3>Peer Dependencies <span class="count muted">{depsLabel(insights.peerDeps)}</span></h3>
          <ul>
            {#each insights.peerDeps as d, i (`${d.name}:${i}`)}
              <li>
                <button
                  type="button"
                  class="row dep-row"
                  aria-label={`Jump to ${d.name} on line ${d.line}`}
                  onclick={() => onJump?.(d.line)}
                >
                  <span class="key mono">{d.name}</span>
                  <span class="value muted mono">{d.version}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}

    {#if insights.palette.length > 0}
      <PaletteList palette={insights.palette} {onJump} />
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
  /* Outline row: glyph | key | preview-on-the-right.
     The preview is truncated rather than wrapped so each entry stays a
     single line — the source view shows the full value. */
  .outline-row {
    grid-template-columns: 22px 1fr auto;
  }
  .outline-row .glyph {
    font-size: 10px;
    text-align: center;
  }
  .outline-row .key,
  .outline-row .preview {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .outline-row .preview {
    font-size: 10px;
    max-width: 110px;
  }
  /* Dep / script row: key on the left, version-or-command on the right.
     The command is the most likely to be truncated, so the right column
     is allowed to ellipsis and the full text is in the button's title. */
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
</style>
