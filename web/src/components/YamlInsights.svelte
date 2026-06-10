<script lang="ts">
  import {
    extractYamlInsights,
    type YamlValueType,
  } from '../lib/yaml-insights';

  let {
    path,
    content,
    onJump,
  }: {
    path: string;
    content: string;
    onJump?: (line: number) => void;
  } = $props();

  let insights = $derived(extractYamlInsights(path, content));

  // Type glyph: matches the JsonInsights vocabulary so a user moving
  // between formats reads the same shapes for the same shapes.
  function typeGlyph(t: YamlValueType): string {
    switch (t) {
      case 'mapping': return '{}';
      case 'sequence': return '[]';
      case 'scalar': return '""';
      case 'null': return '∅';
      case 'empty': return '·';
    }
  }
</script>

<div class="insights">
  {#if !insights.ok}
    <section>
      <p class="muted">Empty or un-parseable YAML.</p>
    </section>
  {:else}
    {#if insights.documents.length > 1}
      <section aria-label="YAML documents">
        <h3>Documents <span class="count muted">{insights.documents.length}</span></h3>
        <ul>
          {#each insights.documents as d (`${d.index}:${d.line}`)}
            <li>
              <button
                type="button"
                class="row doc-row"
                aria-label={`Jump to document ${d.index} on line ${d.line}`}
                onclick={() => onJump?.(d.line)}
              >
                <span class="key mono">doc {d.index}</span>
                <span class="line muted">L{d.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.outline.length > 0}
      <section aria-label="YAML outline">
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

    {#if insights.isGithubAction}
      {#if insights.triggers.length > 0}
        <section aria-label="workflow triggers">
          <h3>Triggers <span class="count muted">{insights.triggers.length}</span></h3>
          <ul>
            {#each insights.triggers as t, i (`${t.name}:${i}`)}
              <li>
                <button
                  type="button"
                  class="row trigger-row"
                  aria-label={`Jump to trigger ${t.name} on line ${t.line}`}
                  onclick={() => onJump?.(t.line)}
                >
                  <span class="key mono">{t.name}</span>
                  <span class="line muted">L{t.line}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}

      {#if insights.jobs.length > 0}
        <section aria-label="workflow jobs">
          <h3>Jobs <span class="count muted">{insights.jobs.length}</span></h3>
          <ul>
            {#each insights.jobs as j, i (`${j.id}:${i}`)}
              <li>
                <button
                  type="button"
                  class="row dep-row"
                  aria-label={`Jump to job ${j.id} on line ${j.line}`}
                  onclick={() => onJump?.(j.line)}
                >
                  <span class="key mono">{j.id}</span>
                  <span class="value muted mono">{j.stepCount} step{j.stepCount === 1 ? '' : 's'}</span>
                </button>
              </li>
            {/each}
          </ul>
        </section>
      {/if}
    {/if}

    {#if insights.isDockerCompose && insights.services.length > 0}
      <section aria-label="compose services">
        <h3>Services <span class="count muted">{insights.services.length}</span></h3>
        <ul>
          {#each insights.services as s, i (`${s.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row dep-row"
                title={s.image}
                aria-label={`Jump to service ${s.name} on line ${s.line}`}
                onclick={() => onJump?.(s.line)}
              >
                <span class="key mono">{s.name}</span>
                <span class="value muted mono">{s.image || '—'}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.isKubernetes}
      <section aria-label="kubernetes resources">
        <h3>Resources <span class="count muted">{insights.resources.length}</span></h3>
        <ul>
          {#each insights.resources as r, i (`${r.kind}:${r.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row k8s-row"
                title={r.namespace ? `${r.namespace}/${r.name}` : r.name}
                aria-label={`Jump to ${r.kind} ${r.name} on line ${r.line}`}
                onclick={() => onJump?.(r.line)}
              >
                <span class="kind mono">{r.kind}</span>
                <span class="key mono">{r.name || '—'}</span>
                <span class="line muted">L{r.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.anchors.length > 0}
      <section aria-label="YAML anchors">
        <h3>Anchors <span class="count muted">{insights.anchors.length}</span></h3>
        <ul>
          {#each insights.anchors as a, i (`${a.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row anchor-row"
                aria-label={`Jump to anchor ${a.name} on line ${a.line}`}
                onclick={() => onJump?.(a.line)}
              >
                <span class="key mono">&amp;{a.name}</span>
                <span class="line muted">L{a.line}</span>
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
  /* Outline row mirrors JsonInsights: glyph | key | preview. */
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
  /* Two-column key + value rows (jobs, services). */
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
  /* Single-column rows with a line tag (documents, triggers, anchors). */
  .doc-row,
  .trigger-row,
  .anchor-row {
    grid-template-columns: 1fr auto;
  }
  .doc-row .line,
  .trigger-row .line,
  .anchor-row .line {
    font-size: 10px;
  }
  /* Kubernetes row: kind | name | line. */
  .k8s-row {
    grid-template-columns: auto 1fr auto;
  }
  .k8s-row .kind {
    color: var(--ctx-link);
    font-weight: 600;
  }
  .k8s-row .key {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .k8s-row .line {
    font-size: 10px;
  }
</style>
