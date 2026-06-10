<script lang="ts">
  export type TocEntry = { level: number; text: string; slug: string };

  let {
    toc,
    onJump,
  }: {
    toc: TocEntry[];
    onJump?: (slug: string) => void;
  } = $props();
</script>

<nav class="toc" aria-label="Table of contents">
  <h3>Contents <span class="count muted">{toc.length}</span></h3>
  {#if toc.length === 0}
    <p class="muted">No headings.</p>
  {:else}
    <ul>
      {#each toc as item, i (`${item.slug}:${i}`)}
        <li style="padding-inline-start: {(item.level - 1) * 10}px">
          <button
            type="button"
            class="row level-{item.level}"
            aria-label={`Jump to ${item.text}`}
            onclick={() => onJump?.(item.slug)}
          >
            <span class="text">{item.text}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</nav>

<style>
  .toc {
    padding: 8px 12px;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  .count {
    margin-left: 4px;
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
    display: block;
    width: 100%;
    padding: 3px 4px;
    font-size: 12px;
    line-height: 1.4;
    text-align: start;
    border: 0;
    background: transparent;
    color: var(--ctx-fg);
    border-radius: 3px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
    color: var(--ctx-link);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .level-1 { font-weight: 600; }
  .level-2 { font-weight: 500; }
  .level-3, .level-4, .level-5, .level-6 { color: var(--ctx-fg-dim); }
  .text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    display: block;
  }
</style>
