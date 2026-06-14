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

<div class="make-render">
  {#if !insights.ok}
    <p class="empty muted">Empty Makefile.</p>
  {:else}
    {#if insights.variables.length > 0}
      <section class="vars">
        <h4>Variables</h4>
        <div class="chips">
          {#each insights.variables as v (v.line)}
            <button type="button" class="chip" onclick={() => onJump?.(v.line)} title={`${v.name} ${v.op} ${v.value}`}>
              <span class="vk mono">{v.name}</span><span class="vop mono">{v.op}</span><span class="vv mono">{v.value || '—'}</span>
            </button>
          {/each}
        </div>
      </section>
    {/if}

    {#if insights.targets.length > 0}
      <section class="targets">
        <h4>Targets <span class="muted">{insights.targets.length}</span></h4>
        <div class="grid">
          {#each insights.targets as t (t.line)}
            <article class="target-card">
              <header>
                <button type="button" class="name" onclick={() => onJump?.(t.line)}>
                  <span class="phony" class:on={t.phony} title={t.phony ? '.PHONY' : 'file target'}>{t.phony ? '◇' : '▸'}</span>
                  <span class="mono">{t.name}</span>
                </button>
                {#if t.prereqs.length > 0}
                  <span class="deps">{#each t.prereqs as d (d)}<span class="dep mono">{d}</span>{/each}</span>
                {/if}
              </header>
              {#if t.doc}<p class="doc">{t.doc}</p>{/if}
              {#if t.recipe.length > 0}
                <pre class="recipe mono">{t.recipe.join('\n')}</pre>
              {/if}
            </article>
          {/each}
        </div>
      </section>
    {/if}

    {#if insights.includes.length > 0}
      <section class="vars">
        <h4>Includes</h4>
        <div class="chips">
          {#each insights.includes as inc (inc.line)}
            <button type="button" class="chip" onclick={() => onJump?.(inc.line)}>
              <span class="mono">{inc.path}</span>{#if inc.optional}<span class="opt muted">opt</span>{/if}
            </button>
          {/each}
        </div>
      </section>
    {/if}
  {/if}
</div>

<style>
  .make-render {
    padding: 12px;
    overflow: auto;
    font-size: 12px;
  }
  .empty {
    padding: 12px;
  }
  h4 {
    margin: 0 0 8px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  section + section {
    margin-block-start: 16px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    display: inline-flex;
    align-items: baseline;
    gap: 4px;
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 6px;
    border: 1px solid var(--ctx-border);
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
    cursor: pointer;
    max-width: 280px;
  }
  .chip:hover {
    border-color: var(--ctx-accent);
  }
  .vk {
    color: var(--ctx-link);
  }
  .vop {
    color: var(--ctx-fg-dim);
    font-size: 10px;
  }
  .vv {
    color: var(--ctx-fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    max-width: 180px;
  }
  .opt {
    font-size: 10px;
  }
  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(240px, 1fr));
    gap: 8px;
  }
  .target-card {
    border: 1px solid var(--ctx-border);
    border-radius: 8px;
    background: var(--ctx-bg-elev);
    padding: 8px 10px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .target-card header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    flex-wrap: wrap;
  }
  .name {
    display: inline-flex;
    align-items: baseline;
    gap: 6px;
    border: 0;
    background: transparent;
    color: var(--ctx-link);
    font-weight: 600;
    cursor: pointer;
    padding: 0;
  }
  .name:hover .mono {
    text-decoration: underline;
  }
  .phony {
    color: var(--ctx-fg-dim);
    font-size: 11px;
  }
  .phony.on {
    color: var(--ctx-accent);
  }
  .deps {
    display: flex;
    flex-wrap: wrap;
    gap: 3px;
  }
  .dep {
    font-size: 10px;
    color: var(--ctx-fg-dim);
    background: var(--ctx-bg);
    border-radius: 4px;
    padding: 0 5px;
  }
  .doc {
    margin: 0;
    font-size: 11px;
    color: var(--ctx-fg-dim);
  }
  .recipe {
    margin: 0;
    font-size: 10.5px;
    line-height: 1.5;
    color: var(--ctx-fg);
    background: var(--ctx-bg);
    border-radius: 6px;
    padding: 6px 8px;
    overflow-x: auto;
    white-space: pre;
    max-height: 160px;
  }
</style>
