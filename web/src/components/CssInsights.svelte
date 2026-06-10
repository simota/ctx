<script lang="ts">
  import PaletteList from './PaletteList.svelte';
  import { extractCssInsights } from '../lib/css-insights';

  let {
    content,
    onJump,
  }: {
    content: string;
    onJump?: (line: number) => void;
  } = $props();

  let insights = $derived(extractCssInsights(content));
</script>

<div class="insights">
  {#if insights.palette.length > 0}
    <PaletteList palette={insights.palette} {onJump} />
  {/if}

  {#if insights.vars.length > 0}
    <section aria-label="variables">
      <h3>
        Variables <span class="count muted">{insights.vars.length}</span>
        {#if insights.importantCount > 0}
          <span class="warn" title="!important usages — possible specificity smell">!{insights.importantCount}</span>
        {/if}
      </h3>
      <ul>
        {#each insights.vars as v (v.name)}
          <li>
            <button
              type="button"
              class="row var-row"
              aria-label={`Jump to ${v.name} on line ${v.line}, used ${v.usage} times`}
              onclick={() => onJump?.(v.line)}
            >
              <span class="name mono">{v.name}</span>
              <span class="usage muted">×{v.usage}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if insights.fontSizes.length > 0}
    <section aria-label="typography scale">
      <h3>Typography <span class="count muted">{insights.fontSizes.length}</span></h3>
      <ul>
        {#each insights.fontSizes as f, i (`${f.value}:${i}`)}
          <li>
            <button
              type="button"
              class="row font-row"
              aria-label={`Jump to first use of font-size ${f.value} on line ${f.firstLine}`}
              onclick={() => onJump?.(f.firstLine)}
            >
              <span class="font-sample" style="font-size:{f.value}">Aa</span>
              <span class="value mono">{f.value}</span>
              <span class="count-inline muted">×{f.count}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if insights.spacings.length > 0}
    <section aria-label="spacing scale">
      <h3>Space <span class="count muted">{insights.spacings.length}</span></h3>
      <ul>
        {#each insights.spacings as s, i (`${s.value}:${i}`)}
          <li>
            <button
              type="button"
              class="row space-row"
              aria-label={`Jump to first use of spacing ${s.value} on line ${s.firstLine}`}
              onclick={() => onJump?.(s.firstLine)}
            >
              <span class="space-bar" style="--space-len:{s.value}"></span>
              <span class="value mono">{s.value}</span>
              <span class="count-inline muted">×{s.count}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if insights.radii.length > 0}
    <section aria-label="border radius scale">
      <h3>Radius <span class="count muted">{insights.radii.length}</span></h3>
      <ul>
        {#each insights.radii as r, i (`${r.value}:${i}`)}
          <li>
            <button
              type="button"
              class="row radius-row"
              aria-label={`Jump to first use of border-radius ${r.value} on line ${r.firstLine}`}
              onclick={() => onJump?.(r.firstLine)}
            >
              <span class="radius-sample" style="border-radius:{r.value}"></span>
              <span class="value mono">{r.value}</span>
              <span class="count-inline muted">×{r.count}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if insights.shadows.length > 0}
    <section aria-label="elevation shadows">
      <h3>Shadow <span class="count muted">{insights.shadows.length}</span></h3>
      <ul>
        {#each insights.shadows as sh, i (`${sh.value}:${i}`)}
          <li>
            <button
              type="button"
              class="row shadow-row"
              aria-label={`Jump to first use of box-shadow on line ${sh.firstLine}`}
              onclick={() => onJump?.(sh.firstLine)}
            >
              <span class="shadow-sample" style="box-shadow:{sh.value}"></span>
              <span class="value mono">{sh.value}</span>
              <span class="count-inline muted">×{sh.count}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if insights.keyframes.length > 0}
    <section aria-label="keyframes">
      <h3>Motion <span class="count muted">{insights.keyframes.length}</span></h3>
      <ul>
        {#each insights.keyframes as kf, i (`${kf.name}:${kf.line}:${i}`)}
          <li>
            <button
              type="button"
              class="row"
              aria-label={`Jump to @keyframes ${kf.name} on line ${kf.line}`}
              onclick={() => onJump?.(kf.line)}
            >
              <span class="name mono">{kf.name}</span>
              <span class="line muted">L{kf.line}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if insights.zIndex.length > 0}
    <section aria-label="z-index values">
      <h3>Layout <span class="count muted">{insights.zIndex.length}</span></h3>
      <ul>
        {#each insights.zIndex as z, i (`${z.value}:${z.line}:${i}`)}
          <li>
            <button
              type="button"
              class="row"
              aria-label={`Jump to z-index ${z.value} on line ${z.line}`}
              onclick={() => onJump?.(z.line)}
            >
              <span class="value mono">z-index {z.value}</span>
              <span class="line muted">L{z.line}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
  {/if}

  {#if insights.media.length > 0}
    <section aria-label="media queries">
      <h3>Responsive <span class="count muted">{insights.media.length}</span></h3>
      <ul>
        {#each insights.media as m, i (`${m.line}:${i}`)}
          <li>
            <button
              type="button"
              class="row media-row"
              aria-label={`Jump to media query on line ${m.line}`}
              onclick={() => onJump?.(m.line)}
            >
              <span class="condition mono">{m.condition}</span>
              <span class="line muted">L{m.line}</span>
            </button>
          </li>
        {/each}
      </ul>
    </section>
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
  .warn {
    margin-inline-start: auto;
    color: var(--ctx-warn, #d9b870);
    font-size: 10px;
    font-weight: 600;
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
    align-items: center;
    gap: 8px;
    width: 100%;
    padding: 4px;
    font-size: 11px;
    text-align: start;
    border: 0;
    background: transparent;
    color: var(--ctx-fg);
    border-radius: 3px;
    cursor: pointer;
    min-height: 24px;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .row .name,
  .row .condition,
  .row .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row .condition {
    font-size: 10px;
  }
  .row .usage,
  .row .line,
  .row .count-inline {
    font-size: 10px;
  }

  .var-row {
    grid-template-columns: 1fr auto;
  }

  /* Typography row: rendered "Aa" sample + value + count.
     font-sample uses the actual CSS font-size but is clamped to keep row
     height predictable in the narrow sidebar. */
  .font-row {
    grid-template-columns: 32px 1fr auto;
  }
  .font-sample {
    display: inline-block;
    width: 32px;
    line-height: 1;
    color: var(--ctx-fg-strong, var(--ctx-fg));
    font-family: ui-serif, Georgia, 'Noto Serif JP', serif;
    /* Clamp the visual so 96px headings don't blow up the row, but keep
       relative scale within the clamp range. */
    font-size: clamp(8px, var(--bound-size, 1em), 22px);
    text-align: center;
  }

  /* Space row: horizontal bar whose width is the actual CSS length, capped
     by max-width so 256px doesn't push the value off-screen. Percent / vh
     values resolve against the sample's own ancestor (sidebar), which is
     close enough for a relative-scale impression. */
  .space-row {
    grid-template-columns: 56px 1fr auto;
  }
  .space-bar {
    display: inline-block;
    width: var(--space-len, 0);
    max-width: 56px;
    height: 8px;
    background: var(--ctx-accent);
    border-radius: 2px;
  }

  /* Radius row: filled square with the actual border-radius applied. */
  .radius-row {
    grid-template-columns: 22px 1fr auto;
  }
  .radius-sample {
    width: 18px;
    height: 18px;
    background: var(--ctx-accent);
  }

  /* Shadow row: small box with the actual box-shadow applied. The wrapping
     row needs vertical padding so the shadow doesn't get clipped by sibling
     rows. */
  .shadow-row {
    grid-template-columns: 32px 1fr auto;
    padding-block: 8px;
  }
  .shadow-sample {
    display: inline-block;
    width: 24px;
    height: 16px;
    background: var(--ctx-bg-panel, var(--ctx-bg));
    border-radius: 3px;
    border: 1px solid var(--ctx-border);
  }

  .media-row,
  .row:not(.var-row):not(.font-row):not(.space-row):not(.radius-row):not(.shadow-row) {
    grid-template-columns: 1fr auto;
  }
</style>
