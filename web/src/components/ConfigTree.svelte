<script lang="ts">
  import type { TreeNode } from '../lib/config-tree';

  let {
    roots,
    onJump,
  }: {
    roots: TreeNode[];
    onJump?: (line: number) => void;
  } = $props();

  function glyph(n: TreeNode): string {
    if (n.kind === 'map') return '{}';
    if (n.kind === 'list') return '[]';
    return '';
  }

  function summary(n: TreeNode): string {
    const c = n.children?.length ?? 0;
    if (n.kind === 'map') return `${c} key${c === 1 ? '' : 's'}`;
    return `${c} item${c === 1 ? '' : 's'}`;
  }
</script>

<!--
  Each container node is a native <details> — accessible, keyboard-toggleable
  and stateful without per-node Svelte state. Containers default open for the
  first two levels so the shape is visible at a glance, then collapse to keep
  deep documents scannable. Scalars render as leaf rows.
-->
{#snippet node(n: TreeNode, depth: number)}
  {#if n.kind === 'scalar'}
    <div class="leaf" style={`--depth:${depth}`}>
      <span class="key mono">{n.key}</span>
      <span class="val mono" title={n.value}>{n.value}</span>
      {#if n.line && onJump}
        <button type="button" class="ln" aria-label={`Jump to line ${n.line}`} onclick={() => onJump?.(n.line!)}>L{n.line}</button>
      {/if}
    </div>
  {:else}
    <details class="branch" open={depth < 2} style={`--depth:${depth}`}>
      <summary>
        <span class="glyph mono" aria-hidden="true">{glyph(n)}</span>
        <span class="key mono">{n.key || '(root)'}</span>
        <span class="count muted">{summary(n)}</span>
        {#if n.line && onJump}
          <button
            type="button"
            class="ln"
            aria-label={`Jump to line ${n.line}`}
            onclick={(e) => {
              e.preventDefault();
              e.stopPropagation();
              onJump?.(n.line!);
            }}
          >L{n.line}</button>
        {/if}
      </summary>
      {#each n.children ?? [] as child, i (`${child.key}:${i}`)}
        {@render node(child, depth + 1)}
      {/each}
    </details>
  {/if}
{/snippet}

<div class="config-tree">
  {#each roots as r, i (`${r.key}:${i}`)}
    {@render node(r, 0)}
  {/each}
</div>

<style>
  .config-tree {
    font-size: 12px;
    line-height: 1.55;
    padding: 8px 12px 16px;
    overflow: auto;
  }
  .branch {
    margin: 0;
  }
  summary {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 1px 4px;
    border-radius: 3px;
    cursor: pointer;
    list-style: none;
    padding-inline-start: calc(var(--depth) * 14px + 4px);
  }
  summary::-webkit-details-marker {
    display: none;
  }
  /* A disclosure triangle that rotates on open, drawn before the glyph. */
  summary::before {
    content: '▸';
    color: var(--ctx-fg-dim);
    font-size: 9px;
    transform: translateY(-1px);
    transition: transform 0.12s ease;
  }
  details[open] > summary::before {
    transform: translateY(-1px) rotate(90deg);
  }
  summary:hover {
    background: var(--ctx-bg-elev);
  }
  summary:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .leaf {
    display: flex;
    align-items: baseline;
    gap: 6px;
    padding: 1px 4px;
    padding-inline-start: calc(var(--depth) * 14px + 18px);
  }
  .glyph {
    color: var(--ctx-fg-dim);
    font-size: 10px;
  }
  .key {
    color: var(--ctx-link);
    white-space: nowrap;
  }
  .leaf .key {
    color: var(--ctx-fg);
  }
  .count {
    font-size: 10px;
  }
  .val {
    color: var(--ctx-fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ln {
    margin-inline-start: auto;
    flex: none;
    font-size: 10px;
    font-family: inherit;
    color: var(--ctx-fg-dim);
    background: transparent;
    border: 0;
    padding: 0 2px;
    cursor: pointer;
    border-radius: 3px;
  }
  .ln:hover {
    color: var(--ctx-accent);
    background: var(--ctx-bg-elev);
  }
</style>
