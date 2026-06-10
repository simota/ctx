<script lang="ts">
  import type { Symbol } from '../lib/api';

  // `onJump(line)` is the historic intra-file form; `(line, path)` is the
  // cross-file extension used by the definition picker. SymbolList itself
  // does not currently produce a cross-file jump (its data is always for
  // the active file), so the second argument is forwarded as undefined for
  // existing call sites.
  let {
    symbols,
    onJump,
  }: {
    symbols: Symbol[];
    onJump?: (line: number, path?: string) => void;
  } = $props();

  function kindColor(k: string): string {
    switch (k) {
      case 'func':
      case 'function':
      case 'method':
        return 'var(--ctx-link)';
      case 'type':
      case 'struct':
      case 'interface':
      case 'class':
        return 'var(--ctx-accent-text, var(--ctx-accent))';
      case 'const':
      case 'var':
        return 'var(--ctx-warn)';
      default:
        return 'var(--ctx-fg-dim)';
    }
  }
</script>

<section class="symbol-list" aria-label="symbols">
  <h3>Symbols <span class="count muted">{symbols.length}</span></h3>
  {#if symbols.length === 0}
    <p class="muted">None.</p>
  {:else}
    <ul>
      {#each symbols as s, i (`${s.kind}:${s.name}:${s.line}:${i}`)}
        <li>
          <button
            type="button"
            class="row"
            aria-label={`Jump to ${s.name} on line ${s.line}`}
            onclick={() => onJump?.(s.line)}
          >
            <span class="kind" style="color: {kindColor(s.kind)}">{s.kind}</span>
            <span class="name mono">{s.name}</span>
            <span class="line muted">L{s.line}</span>
          </button>
        </li>
      {/each}
    </ul>
  {/if}
</section>

<style>
  .symbol-list {
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
    grid-template-columns: 56px 1fr auto;
    gap: 6px;
    width: 100%;
    padding: 3px 4px;
    font-size: 12px;
    align-items: baseline;
    text-align: left;
    border: 0;
    background: transparent;
    border-radius: 3px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .name {
    word-break: break-all;
  }
  .line {
    font-size: 10px;
  }
</style>
