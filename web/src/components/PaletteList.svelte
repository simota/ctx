<script lang="ts">
  export type PaletteEntry = {
    line: number;
    /** CSS custom-property name when the color comes from a `--name: …` declaration. */
    name?: string;
    /** The full text of the value or color expression as it appears in the file. */
    value: string;
    /** A browser-parseable colour string we hand to background-color for the swatch. */
    swatch: string;
  };

  let {
    palette,
    onJump,
  }: {
    palette: PaletteEntry[];
    onJump?: (line: number) => void;
  } = $props();
</script>

<section class="palette-list" aria-label="color palette">
  <h3>Colors <span class="count muted">{palette.length}</span></h3>
  {#if palette.length === 0}
    <p class="muted">No colors.</p>
  {:else}
    <ul>
      {#each palette as entry, i (`${entry.line}:${i}:${entry.value}`)}
        <li>
          <button
            type="button"
            class="row"
            aria-label={`Jump to ${entry.name ?? entry.value} on line ${entry.line}`}
            onclick={() => onJump?.(entry.line)}
          >
            <span class="swatch" aria-hidden="true" style="--swatch-color:{entry.swatch}"></span>
            <span class="label mono">{entry.name ?? entry.value}</span>
            <span class="line muted">L{entry.line}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .palette-list {
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
    display: grid;
    grid-template-columns: 14px 1fr auto;
    gap: 8px;
    width: 100%;
    padding: 3px 4px;
    font-size: 12px;
    align-items: center;
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
  .swatch {
    /* Checkerboard backdrop reveals alpha-channel colours so #ffffff00 does
       not look identical to a missing swatch. The colour itself is layered
       on top via --swatch-color from inline style. */
    width: 14px;
    height: 14px;
    border-radius: 3px;
    border: 1px solid var(--ctx-border);
    background:
      linear-gradient(var(--swatch-color, transparent), var(--swatch-color, transparent)),
      linear-gradient(45deg, var(--ctx-fg-dim, #888) 25%, transparent 25%) 0 0/6px 6px,
      linear-gradient(-45deg, var(--ctx-fg-dim, #888) 25%, transparent 25%) 0 3px/6px 6px,
      linear-gradient(45deg, transparent 75%, var(--ctx-fg-dim, #888) 75%) 3px -3px/6px 6px,
      linear-gradient(-45deg, transparent 75%, var(--ctx-fg-dim, #888) 75%) -3px 0/6px 6px,
      var(--ctx-bg-panel);
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
  .line {
    font-size: 10px;
  }
</style>
