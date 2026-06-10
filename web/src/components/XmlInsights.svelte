<script lang="ts">
  import { extractXmlInsights } from '../lib/xml-insights';

  let {
    content,
    onJump,
  }: {
    content: string;
    onJump?: (line: number) => void;
  } = $props();

  let insights = $derived(extractXmlInsights(content));
</script>

<div class="insights">
  {#if !insights.ok}
    <section>
      <p class="muted">Could not parse XML.</p>
    </section>
  {:else}
    {#if insights.isSitemap && insights.sitemap.length > 0}
      <section aria-label="sitemap URLs">
        <h3>URLs <span class="count muted">{insights.sitemap.length}</span></h3>
        <ul>
          {#each insights.sitemap as u, i (`${u.line}:${i}`)}
            <li>
              <button
                type="button"
                class="row url-row"
                title={u.url}
                aria-label={`Jump to URL ${u.url} on line ${u.line}`}
                onclick={() => onJump?.(u.line)}
              >
                <span class="url mono">{u.url}</span>
                <span class="line muted">L{u.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.outline.length > 0}
      <section aria-label="XML outline">
        <h3>
          Outline <span class="count muted">{insights.outline.length}</span>
          <span class="meta muted">&lt;{insights.rootName}&gt; · d{insights.maxDepth}</span>
        </h3>
        <ul>
          {#each insights.outline as e, i (`${e.name}:${e.line}:${i}`)}
            <li>
              <button
                type="button"
                class="row outline-row"
                title={e.attrSummary}
                aria-label={`Jump to <${e.name}> on line ${e.line}`}
                onclick={() => onJump?.(e.line)}
              >
                <span class="tag mono">&lt;{e.name}&gt;</span>
                {#if e.attrSummary}
                  <span class="attr muted mono">{e.attrSummary}</span>
                {:else if e.childCount > 0}
                  <span class="attr muted">{e.childCount} children</span>
                {:else}
                  <span class="attr muted">leaf</span>
                {/if}
                <span class="line muted">L{e.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.tags.length > 0}
      <section aria-label="tag frequency">
        <h3>
          Tags <span class="count muted">{insights.tags.length}</span>
          <span class="meta muted">{insights.totalElements} total</span>
        </h3>
        <ul>
          {#each insights.tags as t, i (`${t.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row tag-row"
                aria-label={`Jump to first <${t.name}> on line ${t.firstLine}`}
                onclick={() => onJump?.(t.firstLine)}
              >
                <span class="tag mono">&lt;{t.name}&gt;</span>
                <span class="count-inline muted">×{t.count}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.namespaces.length > 0}
      <section aria-label="namespaces">
        <h3>Namespaces <span class="count muted">{insights.namespaces.length}</span></h3>
        <ul>
          {#each insights.namespaces as ns, i (`${ns.prefix}:${i}`)}
            <li>
              <button
                type="button"
                class="row ns-row"
                title={ns.uri}
                aria-label={`Jump to namespace declaration on line ${ns.line}`}
                onclick={() => onJump?.(ns.line)}
              >
                <span class="prefix mono">{ns.prefix === '' ? 'default' : ns.prefix}</span>
                <span class="uri muted mono">{ns.uri}</span>
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
  /* URL row: full URL takes the available width, line number on the right. */
  .url-row {
    grid-template-columns: 1fr auto;
  }
  .url-row .url {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
  }
  .url-row .line {
    font-size: 10px;
  }
  /* Outline row: tag | attribute summary | line.
     Attribute summary is the most likely to overflow and is title= for hover. */
  .outline-row {
    grid-template-columns: auto 1fr auto;
  }
  .outline-row .tag {
    color: var(--ctx-link);
  }
  .outline-row .attr {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
  }
  .outline-row .line {
    font-size: 10px;
  }
  /* Tag row: tag | count. */
  .tag-row {
    grid-template-columns: 1fr auto;
  }
  .tag-row .tag {
    color: var(--ctx-link);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .tag-row .count-inline {
    font-size: 10px;
  }
  /* Namespace row: prefix | uri. */
  .ns-row {
    grid-template-columns: auto 1fr;
  }
  .ns-row .prefix {
    font-weight: 600;
  }
  .ns-row .uri {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
  }
</style>
