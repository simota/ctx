<script lang="ts">
  import { panes, setSplitPercent, SPLIT_MIN, SPLIT_MAX, SPLIT_DEFAULT } from '../lib/panes.svelte';
  import { announce } from '../lib/announce.svelte';

  // Width of the surrounding grid track that hosts the two panes (left
  // FileDetail + splitter + right FileDetail). We use the splitter's parent
  // bounding box so the percent is calibrated to "the 2-pane area" and not the
  // whole window, which would include the TreeView column.
  let rootEl: HTMLDivElement | null = $state(null);
  let dragging = $state(false);

  function containerBox(): DOMRect | null {
    if (!rootEl?.parentElement) return null;
    return rootEl.parentElement.getBoundingClientRect();
  }

  function pctFromClientX(clientX: number): number {
    const box = containerBox();
    if (!box || box.width <= 0) return panes.splitPercent;
    return ((clientX - box.left) / box.width) * 100;
  }

  function onPointerDown(e: PointerEvent) {
    // Only react to primary (left / pen / touch contact) input.
    if (e.button !== 0) return;
    e.preventDefault();
    dragging = true;
    // Capture pointer so move/up events keep coming even when the cursor leaves
    // the 4px-wide handle. Falls back gracefully if capture is unsupported.
    try {
      (e.currentTarget as Element).setPointerCapture(e.pointerId);
    } catch {
      // ignore
    }
    window.addEventListener('pointermove', onPointerMove);
    window.addEventListener('pointerup', onPointerUp);
    window.addEventListener('pointercancel', onPointerUp);
  }

  function onPointerMove(e: PointerEvent) {
    if (!dragging) return;
    setSplitPercent(pctFromClientX(e.clientX));
  }

  function onPointerUp() {
    if (!dragging) return;
    dragging = false;
    window.removeEventListener('pointermove', onPointerMove);
    window.removeEventListener('pointerup', onPointerUp);
    window.removeEventListener('pointercancel', onPointerUp);
  }

  function onKey(e: KeyboardEvent) {
    let next: number | null = null;
    if (e.key === 'ArrowLeft') next = panes.splitPercent - 5;
    else if (e.key === 'ArrowRight') next = panes.splitPercent + 5;
    else if (e.key === 'Home') next = SPLIT_MIN;
    else if (e.key === 'End') next = SPLIT_MAX;
    else if (e.key === 'Enter') next = SPLIT_DEFAULT;
    if (next === null) return;
    e.preventDefault();
    setSplitPercent(next);
    announce(`Split ${panes.splitPercent}%`);
  }

  function onDouble() {
    setSplitPercent(SPLIT_DEFAULT);
    announce('Split reset to 50%');
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_tabindex, a11y_no_noninteractive_element_interactions, a11y_interactive_supports_focus -->
<div
  bind:this={rootEl}
  class="splitter"
  class:dragging
  role="separator"
  aria-orientation="vertical"
  aria-label="Resize panes"
  aria-valuemin={SPLIT_MIN}
  aria-valuemax={SPLIT_MAX}
  aria-valuenow={panes.splitPercent}
  tabindex="0"
  onpointerdown={onPointerDown}
  onkeydown={onKey}
  ondblclick={onDouble}
>
  <span class="grip" aria-hidden="true"></span>
</div>

<style>
  .splitter {
    width: 4px;
    cursor: col-resize;
    background: var(--ctx-border);
    position: relative;
    /* WCAG 2.2 SC 2.5.8 — the splitter itself is 4px wide so we extend the
       effective hit target via a transparent ::before band. */
    flex: 0 0 auto;
    user-select: none;
    -webkit-user-select: none;
    touch-action: none;
  }
  .splitter::before {
    content: '';
    position: absolute;
    top: 0;
    bottom: 0;
    left: -4px;
    right: -4px;
  }
  .splitter:hover,
  .splitter.dragging,
  .splitter:focus-visible {
    background: var(--ctx-accent);
  }
  .splitter:focus-visible {
    outline: none;
  }
  .grip {
    position: absolute;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: 2px;
    height: 24px;
    background: var(--ctx-fg-dim);
    border-radius: 1px;
    opacity: 0;
    pointer-events: none;
    transition: opacity var(--motion-base) ease-out;
  }
  .splitter:hover .grip,
  .splitter.dragging .grip,
  .splitter:focus-visible .grip {
    opacity: 0.6;
  }
</style>
