<script lang="ts">
  import { tabs, closeTab, moveTab } from '../lib/tabs.svelte';
  import { navigate, toFileHash } from '../lib/router.svelte';
  import { announce } from '../lib/announce.svelte';
  import { panes, openRight } from '../lib/panes.svelte';
  import { openContextMenu, type ContextMenuItem } from '../lib/context-menu.svelte';
  import { repo } from '../lib/repo.svelte';
  import { ensurePinsRoot, isPinned, pinFile, unpinFile } from '../lib/pins.svelte';

  let {
    activePath,
    onActivate,
  } = $props<{
    activePath: string;
    // Optional handler for "user wants to activate this tab". Lets the parent
    // route the click into a focused pane (left/right). When omitted we fall
    // back to the legacy behaviour of navigating to the file route directly.
    onActivate?: (path: string) => void;
  }>();

  // DnD local state — null when not dragging.
  let dragFrom = $state<number | null>(null);
  let dropTarget = $state<number | null>(null);

  // Mirror the App-level breakpoint so "Open to the Side" disables itself
  // when the right pane is suppressed.
  let isMobile = $state(typeof window !== 'undefined' && window.innerWidth < 800);
  $effect(() => {
    if (typeof window === 'undefined') return;
    const mql = window.matchMedia('(max-width: 799px)');
    isMobile = mql.matches;
    const onChange = (e: MediaQueryListEvent) => {
      isMobile = e.matches;
    };
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  });

  function basename(p: string): string {
    const i = p.lastIndexOf('/');
    return i === -1 ? p : p.slice(i + 1);
  }

  function onTabClick(path: string) {
    if (path === activePath) return;
    if (onActivate) {
      onActivate(path);
      return;
    }
    navigate(toFileHash(path));
  }

  function onTabKey(e: KeyboardEvent, path: string) {
    // Make the tab list keyboard-operable: arrow keys move focus across tabs,
    // Enter / Space activates, Delete / Backspace closes.
    const list = tabs.paths;
    const idx = list.indexOf(path);
    if (idx === -1) return;
    if (e.key === 'Enter' || e.key === ' ') {
      e.preventDefault();
      onTabClick(path);
      return;
    }
    if (e.key === 'Delete' || e.key === 'Backspace') {
      e.preventDefault();
      onCloseClick(e, path);
      return;
    }
    let next = -1;
    if (e.key === 'ArrowLeft') next = idx - 1;
    else if (e.key === 'ArrowRight') next = idx + 1;
    else if (e.key === 'Home') next = 0;
    else if (e.key === 'End') next = list.length - 1;
    if (next < 0 || next >= list.length) return;
    e.preventDefault();
    const target = document.querySelector<HTMLElement>(
      `[data-tab-path="${cssEscape(list[next])}"]`,
    );
    target?.focus();
  }

  function cssEscape(s: string): string {
    // Minimal CSS.escape shim — paths only contain "/" and ascii filename
    // characters in practice; escape quotes/backslashes for safety.
    return s.replace(/\\/g, '\\\\').replace(/"/g, '\\"');
  }

  function onCloseClick(e: Event, path: string) {
    e.stopPropagation();
    e.preventDefault();
    const wasActive = path === activePath;
    const next = closeTab(path);
    if (wasActive && next) {
      if (onActivate) onActivate(next);
      else navigate(toFileHash(next));
      // If no tab remains, leave the route alone — App.svelte renders the
      // empty placeholder when there's nothing to show.
    }
  }

  // ---- HTML5 DnD (mouse-only; keyboard reorder is out of scope) ----
  function onDragStart(e: DragEvent, idx: number) {
    dragFrom = idx;
    dropTarget = null;
    if (e.dataTransfer) {
      e.dataTransfer.effectAllowed = 'move';
      // Required by some browsers (Firefox) to start the drag.
      try {
        e.dataTransfer.setData('text/plain', String(idx));
      } catch {
        // ignore — Safari occasionally throws inside iframes / tests.
      }
    }
  }

  function onDragOver(e: DragEvent, idx: number) {
    if (dragFrom === null) return;
    // preventDefault is required to allow the drop event to fire.
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
    if (dropTarget !== idx) dropTarget = idx;
  }

  function onDragLeave() {
    // We don't clear dropTarget here — the next dragover on a sibling will
    // overwrite it. Clearing on leave causes flicker as the pointer moves
    // between the tab and its child <button class="close">.
  }

  function onDrop(e: DragEvent, idx: number) {
    e.preventDefault();
    const from = dragFrom;
    dragFrom = null;
    dropTarget = null;
    if (from === null) return;
    if (from === idx) return;
    moveTab(from, idx);
    announce(`Moved tab to position ${idx + 1}`);
  }

  function onDragEnd() {
    dragFrom = null;
    dropTarget = null;
  }

  function onTabContextMenu(e: MouseEvent, path: string) {
    e.preventDefault();
    const alreadyInRight = panes.rightOpen && panes.rightPath === path;
    const pinned = isPinned(path);
    const items: ContextMenuItem[] = [
      {
        id: 'open-to-side',
        label: 'Open to the Side',
        disabled: isMobile || alreadyInRight,
        run: () => {
          openRight(path);
          announce('Right pane opened');
        },
      },
      {
        id: pinned ? 'unpin-file' : 'pin-file',
        label: pinned ? 'Unpin File' : 'Pin File',
        disabled: !repo.root,
        run: () => {
          ensurePinsRoot(repo.root);
          const result = pinned ? unpinFile(path) : pinFile(path);
          announce(result.message);
        },
      },
      {
        id: 'close',
        label: 'Close',
        run: () => {
          // Reuse the same close-then-navigate path as the × button so the
          // active tab transition stays consistent across entry points.
          onCloseClick(e, path);
        },
      },
    ];
    openContextMenu(e.clientX, e.clientY, items);
  }
</script>

{#if tabs.paths.length > 0}
  <div class="tabbar" role="tablist" aria-label="Open files">
    {#each tabs.paths as path, idx (path)}
      {@const active = path === activePath}
      {@const dragging = dragFrom === idx}
      {@const dropHere = dropTarget === idx && dragFrom !== null && dragFrom !== idx}
      <div
        class="tab"
        class:active
        class:dragging
        class:drop-target={dropHere}
        role="tab"
        tabindex={active ? 0 : -1}
        aria-selected={active}
        title={path}
        data-tab-path={path}
        draggable="true"
        onclick={() => onTabClick(path)}
        oncontextmenu={(e) => onTabContextMenu(e, path)}
        onkeydown={(e) => onTabKey(e, path)}
        ondragstart={(e) => onDragStart(e, idx)}
        ondragover={(e) => onDragOver(e, idx)}
        ondragleave={onDragLeave}
        ondrop={(e) => onDrop(e, idx)}
        ondragend={onDragEnd}
      >
        <span class="label mono">{basename(path)}</span>
        <button
          type="button"
          class="close"
          aria-label={`Close ${path}`}
          tabindex="-1"
          draggable="false"
          onclick={(e) => onCloseClick(e, path)}
        >×</button>
      </div>
    {/each}
  </div>
{/if}

<style>
  .tabbar {
    display: flex;
    flex: 0 0 auto;
    overflow-x: auto;
    overflow-y: hidden;
    background: var(--ctx-bg-panel);
    border-bottom: 1px solid var(--ctx-border);
    scrollbar-width: thin;
    -webkit-overflow-scrolling: touch;
  }
  .tabbar::-webkit-scrollbar {
    height: 6px;
  }

  .tab {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 4px 12px;
    border-right: 1px solid var(--ctx-border);
    border-bottom: 2px solid transparent;
    background: transparent;
    color: var(--ctx-fg-dim);
    cursor: pointer;
    white-space: nowrap;
    user-select: none;
    -webkit-user-select: none;
    font-size: 12px;
    max-width: 220px;
    min-width: 0;
    flex: 0 0 auto;
    /* `position: relative` anchors the drop-indicator ::before pseudo. */
    position: relative;
  }
  .tab:hover {
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
  }
  .tab.active {
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    border-bottom-color: var(--ctx-accent);
  }
  .tab:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  /* Source tab during a drag — dimmed so the user can see the pointer ghost. */
  .tab.dragging {
    opacity: 0.4;
  }
  /* Drop-target indicator — a 2px accent line on the left edge previewing
     where the dragged tab will land. */
  .tab.drop-target::before {
    content: '';
    position: absolute;
    left: 0;
    top: 0;
    bottom: 0;
    width: 2px;
    background: var(--ctx-accent);
    pointer-events: none;
  }
  .label {
    overflow: hidden;
    text-overflow: ellipsis;
    min-width: 0;
  }
  .close {
    border: 0;
    background: transparent;
    color: var(--ctx-fg-dim);
    padding: 2px 6px;
    font-size: 16px;
    line-height: 1;
    border-radius: 3px;
    cursor: pointer;
    /* hit target — WCAG 2.2 SC 2.5.8 (24x24px AA) requires an interactive
       target ≥24×24px, but this control is inside an already-clickable parent
       so an inline spacing exception applies. */
    min-width: 18px;
  }
  .close:hover {
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
  }
  .close:focus-visible {
    outline: 1px solid var(--ctx-accent);
    outline-offset: -1px;
  }
</style>
