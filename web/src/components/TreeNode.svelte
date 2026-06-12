<script lang="ts">
  import type { TreeNode as TreeNodeT } from '../lib/api';
  import { formatTokens, formatRelative, gitColor, gitLabel, gitStatusName } from '../lib/format';
  import { pickFileIcon } from '../lib/file-icons';
  import { panes, openRight } from '../lib/panes.svelte';
  import { openContextMenu, type ContextMenuItem } from '../lib/context-menu.svelte';
  import { announce } from '../lib/announce.svelte';
  import { repo, absolutePath } from '../lib/repo.svelte';
  import { mixSelection, toggleInclude } from '../lib/mix-selection.svelte';

  let {
    node,
    level,
    posinset,
    setsize,
    isExpanded,
    isSelected,
    isTabstop,
    onActivate,
    register,
  } = $props<{
    node: TreeNodeT;
    level: number;
    posinset: number;
    setsize: number;
    isExpanded: boolean;
    isSelected: boolean;
    isTabstop: boolean;
    onActivate: () => void;
    register: (el: HTMLElement | null) => void;
  }>();

  // Mobile gate mirrors the rest of the app: the right pane refuses to open
  // below 800px, so we surface the "Open to the Side" action as disabled
  // rather than hiding it (consistent affordance, just inert).
  let isMobile = $state(typeof window !== 'undefined' && window.innerWidth < 800);
  $effect(() => {
    if (typeof window === 'undefined') return;
    const mql = window.matchMedia('(max-width: 799px)');
    isMobile = mql.matches;
    const onChange = (e: MediaQueryListEvent) => { isMobile = e.matches; };
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  });

  let rowEl: HTMLLIElement | null = $state(null);

  // Forward the DOM node to the parent for programmatic .focus() during
  // keyboard navigation. `register(null)` on unmount keeps the map tidy.
  $effect(() => {
    register(rowEl);
    return () => register(null);
  });

  // Human-readable label for screen readers. Files include git status name
  // and approximate token count; dirs include entry count + expanded state.
  let ariaLabel = $derived.by(() => {
    const name = node.name || node.path || '/';
    if (node.is_dir) {
      const n = node.children?.length ?? 0;
      const entry = n === 1 ? 'entry' : 'entries';
      const base = `${name} (folder, ${n} ${entry})`;
      const gs = gitStatusName(node.git);
      return gs ? `${base}, contains ${gs}` : base;
    }
    const parts: string[] = [name];
    if (node.tokens !== undefined && node.tokens > 0) {
      parts.push(`${node.tokens} tokens`);
    }
    const gs = gitStatusName(node.git);
    if (gs) parts.push(gs);
    return parts.join(', ');
  });

  function onClick(e: MouseEvent) {
    e.preventDefault();
    onActivate();
  }

  async function copyToClipboard(text: string, announceLabel: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      announce(`Copied ${announceLabel}`);
    } catch {
      // Clipboard API requires a secure context; silently no-op when denied.
    }
  }

  // Include-toggle: Space when a file row is focused toggles mix inclusion.
  // We intercept Space in onRowKey so it doesn't bubble to any scroll handler.
  function onRowKey(e: KeyboardEvent) {
    if (!node.is_dir && e.key === ' ' && !e.shiftKey && !e.metaKey && !e.ctrlKey) {
      e.preventDefault();
      e.stopPropagation();
      toggleInclude(node.path);
      announce(
        mixSelection.includedPaths.has(node.path)
          ? `Added ${node.name} to mix`
          : `Removed ${node.name} from mix`,
      );
    }
  }

  function onContextMenu(e: MouseEvent) {
    e.preventDefault();
    const items: ContextMenuItem[] = [];
    if (!node.is_dir) {
      items.push({
        id: 'open',
        label: 'Open',
        disabled: isSelected,
        run: () => onActivate(),
      });
      items.push({
        id: 'open-to-side',
        label: 'Open to the Side',
        disabled: isMobile || (panes.rightOpen && panes.rightPath === node.path),
        run: () => openRight(node.path),
      });
    }
    items.push({
      id: 'copy-path',
      label: 'Copy Path',
      run: () => copyToClipboard(node.path, node.path),
    });
    items.push({
      id: 'copy-full-path',
      label: 'Copy Full Path',
      // Repo root is seeded by TreeView's first /api/tree response; it should
      // be present by the time the user can right-click a row, but disable
      // defensively if a deeper component instantiated us before then.
      disabled: !repo.root,
      run: () => copyToClipboard(absolutePath(node.path), absolutePath(node.path)),
    });
    openContextMenu(e.clientX, e.clientY, items);
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events -->
<li
  bind:this={rowEl}
  class="row"
  class:dir={node.is_dir}
  class:file={!node.is_dir}
  class:selected={isSelected}
  class:included={!node.is_dir && mixSelection.includedPaths.has(node.path)}
  style="--depth: {level - 1}"
  role="treeitem"
  aria-level={level}
  aria-posinset={posinset}
  aria-setsize={setsize}
  aria-expanded={node.is_dir ? isExpanded : undefined}
  aria-selected={isSelected}
  aria-label={ariaLabel}
  tabindex={isTabstop ? 0 : -1}
  data-path={node.path}
  onclick={onClick}
  onkeydown={onRowKey}
  oncontextmenu={onContextMenu}
>
  <span class="indent" aria-hidden="true"></span>
  {#if node.is_dir}
    <span class="caret" aria-hidden="true">{isExpanded ? '▾' : '▸'}</span>
    <span class="icon dir-icon" aria-hidden="true">▣</span>
    <!-- placeholder so grid columns stay aligned for dir rows -->
    <span class="include-placeholder" aria-hidden="true"></span>
  {:else}
    {@const FileIconComp = pickFileIcon(node.path)}
    {@const included = mixSelection.includedPaths.has(node.path)}
    <span class="caret hidden" aria-hidden="true"></span>
    <span class="icon file-icon" aria-hidden="true"><FileIconComp /></span>
    <button
      type="button"
      class="include-btn"
      class:active={included}
      aria-label={included ? `Remove ${node.name} from mix` : `Add ${node.name} to mix`}
      aria-pressed={included}
      tabindex="-1"
      onclick={(e) => { e.stopPropagation(); toggleInclude(node.path); }}
    >◉</button>
  {/if}
  <span class="name" title={node.path}>{node.name || node.path || '/'}</span>
  {#if node.git}
    <span class="git" style="color: {gitColor(node.git)}" aria-hidden="true">{gitLabel(node.git)}</span>
  {/if}
  {#if !node.is_dir && node.updated_at}
    {@const rel = formatRelative(node.updated_at)}
    {#if rel}
      <span
        class="updated muted"
        aria-hidden="true"
        title={new Date(node.updated_at * 1000).toLocaleString()}
      >{rel}</span>
    {/if}
  {/if}
  {#if node.tokens !== undefined && node.tokens > 0}
    <span
      class="tokens muted"
      aria-hidden="true"
      title="Approx LLM tokens (cl100k_base). Claude Pro caps at ~200k per request."
    >{formatTokens(node.tokens)}</span>
  {/if}
</li>

<style>
  .row {
    display: grid;
    /* indent | caret | icon | include-btn | name | git | updated | tokens */
    grid-template-columns: calc(var(--depth, 0) * 12px) 12px 16px 14px 1fr auto auto auto;
    align-items: center;
    gap: 2px;
    padding: 2px 8px 2px 4px;
    cursor: pointer;
    user-select: none;
    font-family: var(--ctx-font-mono);
    font-size: 12px;
    line-height: 1.6;
  }
  .row:hover {
    background: rgba(255, 255, 255, 0.04);
  }
  :global(:root[data-theme='light']) .row:hover {
    background: rgba(0, 0, 0, 0.04);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .row.selected {
    background: rgba(78, 201, 176, 0.15);
    color: var(--ctx-fg-strong);
  }
  .indent {
    grid-column: 1;
  }
  .caret {
    grid-column: 2;
    color: var(--ctx-fg-dim);
    text-align: center;
  }
  .caret.hidden {
    visibility: hidden;
  }
  .icon {
    grid-column: 3;
    color: var(--ctx-fg-dim);
    text-align: center;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    line-height: 1;
  }
  .dir-icon {
    color: var(--ctx-link);
  }
  /* unplugin-icons emits <svg width="1em" height="1em">; size via font-size
     so the SVG visually balances the surrounding 12px monospace text. */
  .file-icon {
    font-size: 14px;
  }
  .file-icon :global(svg) {
    display: block;
  }
  /* Include-toggle button (column 4) — only visible on file rows, hidden for dirs. */
  .include-placeholder {
    grid-column: 4;
  }
  .include-btn {
    grid-column: 4;
    border: 0;
    background: transparent;
    color: var(--ctx-fg-dim);
    padding: 0;
    font-size: 10px;
    line-height: 1;
    cursor: pointer;
    opacity: 0;
    transition: opacity 80ms;
    display: flex;
    align-items: center;
    justify-content: center;
    width: 14px;
    height: 14px;
    border-radius: 2px;
  }
  .row:hover .include-btn,
  .row.included .include-btn {
    opacity: 1;
  }
  .include-btn.active {
    color: var(--ctx-accent);
    opacity: 1;
  }
  .include-btn:focus-visible {
    outline: 1px solid var(--ctx-accent);
    outline-offset: -1px;
    opacity: 1;
  }
  .include-btn:hover {
    color: var(--ctx-accent);
    background: rgba(78, 201, 176, 0.1);
  }
  /* Row-level emphasis when the file is included in the current mix. */
  .row.included {
    background: rgba(78, 201, 176, 0.06);
  }
  .row.included .name {
    color: var(--ctx-accent);
  }
  :global(:root[data-theme='light']) .row.included {
    background: rgba(0, 150, 120, 0.06);
  }
  .name {
    grid-column: 5;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .row.dir .name {
    color: var(--ctx-link);
  }
  .git {
    grid-column: 6;
    font-weight: 600;
    padding-right: 6px;
  }
  .updated {
    grid-column: 7;
    font-size: 11px;
    color: var(--ctx-muted-fg, var(--ctx-fg-dim));
    padding-right: 4px;
  }
  .tokens {
    grid-column: 8;
    font-size: 11px;
    color: var(--ctx-fg-dim);
  }
  .row.selected .tokens {
    color: var(--ctx-fg);
  }
</style>
