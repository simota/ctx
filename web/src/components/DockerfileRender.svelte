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

<div class="docker-render">
  {#if !insights.ok}
    <p class="empty muted">Empty Dockerfile.</p>
  {:else}
    {#if insights.args.length > 0}
      <section class="args-card">
        <h4>Build args</h4>
        <div class="chips">
          {#each insights.args as a (a.line)}
            <button type="button" class="chip" onclick={() => onJump?.(a.line)} title={`${a.key}${a.value ? `=${a.value}` : ''}`}>
              {a.key}{a.value ? `=${a.value}` : ''}
            </button>
          {/each}
        </div>
      </section>
    {/if}

    <ol class="pipeline">
      {#each insights.stages as s (s.index)}
        <li class="stage">
          {#if s.from.length > 0}
            <div class="edge" aria-label={`Inputs: ${s.from.join(', ')}`}>
              ↑ COPY --from {s.from.join(', ')}
            </div>
          {/if}
          <article class="stage-card">
            <header>
              <button type="button" class="stage-head" onclick={() => onJump?.(s.line)}>
                <span class="idx">{s.index}</span>
                {#if s.name}<span class="stage-name mono">{s.name}</span>{/if}
                <span class="base mono" title={s.base}>{s.base}</span>
              </button>
              {#if s.ports.length > 0}
                <span class="ports">{#each s.ports as p (p)}<span class="port mono">:{p}</span>{/each}</span>
              {/if}
            </header>
            {#if s.steps.length > 0}
              <ul class="steps">
                {#each s.steps as step, i (`${step.line}:${i}`)}
                  <li>
                    <button type="button" class="step" onclick={() => onJump?.(step.line)} title={step.text}>
                      <span class="kw mono">{step.keyword}</span>
                      <span class="txt mono">{step.text}</span>
                    </button>
                  </li>
                {/each}
              </ul>
            {/if}
            {#if s.entrypoint || s.cmd}
              <footer>
                {#if s.entrypoint}<div class="entry"><span class="kw mono">ENTRYPOINT</span> <span class="mono">{s.entrypoint}</span></div>{/if}
                {#if s.cmd}<div class="entry"><span class="kw mono">CMD</span> <span class="mono">{s.cmd}</span></div>{/if}
              </footer>
            {/if}
          </article>
        </li>
      {/each}
    </ol>
  {/if}
</div>

<style>
  .docker-render {
    padding: 12px;
    overflow: auto;
    font-size: 12px;
  }
  .empty {
    padding: 12px;
  }
  h4 {
    margin: 0 0 6px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  .args-card {
    margin-block-end: 12px;
  }
  .chips {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
  }
  .chip {
    font-family: var(--ctx-mono, monospace);
    font-size: 11px;
    padding: 2px 8px;
    border-radius: 999px;
    border: 1px solid var(--ctx-border);
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
    cursor: pointer;
    max-width: 240px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .chip:hover {
    border-color: var(--ctx-accent);
  }
  .pipeline {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0;
  }
  .stage {
    display: flex;
    flex-direction: column;
  }
  .edge {
    align-self: center;
    font-size: 10px;
    color: var(--ctx-fg-dim);
    padding: 4px 0;
  }
  .stage-card {
    border: 1px solid var(--ctx-border);
    border-radius: 8px;
    background: var(--ctx-bg-elev);
    overflow: hidden;
  }
  .stage + .stage .stage-card {
    margin-block-start: 0;
  }
  /* A connector line between consecutive stage cards when there's no edge. */
  .stage + .stage:not(:has(.edge))::before {
    content: '↓';
    align-self: center;
    color: var(--ctx-fg-dim);
    font-size: 12px;
    padding: 4px 0;
  }
  header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 8px 10px;
    border-block-end: 1px solid var(--ctx-border);
  }
  .stage-head {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: 1;
    min-width: 0;
    border: 0;
    background: transparent;
    color: var(--ctx-fg);
    cursor: pointer;
    text-align: start;
    padding: 0;
  }
  .idx {
    flex: none;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--ctx-accent);
    color: var(--ctx-bg);
    font-size: 11px;
    font-weight: 700;
    display: grid;
    place-items: center;
  }
  .stage-name {
    color: var(--ctx-link);
    font-weight: 600;
  }
  .base {
    color: var(--ctx-fg-dim);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ports {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .port {
    font-size: 11px;
    color: var(--ctx-accent);
    background: color-mix(in srgb, var(--ctx-accent) 14%, transparent);
    border-radius: 4px;
    padding: 0 5px;
  }
  .steps {
    list-style: none;
    margin: 0;
    padding: 4px 0;
  }
  .step {
    display: flex;
    gap: 8px;
    width: 100%;
    align-items: baseline;
    border: 0;
    background: transparent;
    color: var(--ctx-fg);
    cursor: pointer;
    text-align: start;
    padding: 2px 10px;
    font-size: 11px;
  }
  .step:hover {
    background: var(--ctx-bg);
  }
  .kw {
    flex: none;
    min-width: 78px;
    color: var(--ctx-fg-dim);
    font-size: 10px;
  }
  .txt {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  footer {
    padding: 6px 10px;
    border-block-start: 1px solid var(--ctx-border);
    background: var(--ctx-bg);
  }
  .entry {
    font-size: 11px;
    display: flex;
    gap: 8px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .entry .kw {
    color: var(--ctx-accent);
  }
</style>
