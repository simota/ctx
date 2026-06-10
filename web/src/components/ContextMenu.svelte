<script lang="ts">
  import { contextMenu, closeContextMenu, type ContextMenuItem } from '../lib/context-menu.svelte';
  import { tick } from 'svelte';

  let menuEl: HTMLUListElement | null = $state(null);
  // Position after viewport-clamp. Defaults mirror the raw click until we
  // measure the rendered menu and pull it back inside the window.
  let clampedX = $state(0);
  let clampedY = $state(0);

  function itemButtons(): HTMLButtonElement[] {
    if (!menuEl) return [];
    return Array.from(menuEl.querySelectorAll<HTMLButtonElement>('[role="menuitem"]'));
  }

  function firstEnabled(): HTMLButtonElement | undefined {
    return itemButtons().find((b) => !b.disabled);
  }

  // Position-clamp + initial focus run together: we need the menu rendered
  // (so we can measure) before we can decide its final position, and we want
  // the focus to land on the first enabled item once it's visible.
  $effect(() => {
    if (!contextMenu.open) return;
    clampedX = contextMenu.x;
    clampedY = contextMenu.y;
    tick().then(() => {
      if (!menuEl) return;
      const rect = menuEl.getBoundingClientRect();
      const pad = 4;
      const maxX = window.innerWidth - rect.width - pad;
      const maxY = window.innerHeight - rect.height - pad;
      if (contextMenu.x > maxX) clampedX = Math.max(pad, maxX);
      if (contextMenu.y > maxY) clampedY = Math.max(pad, maxY);
      firstEnabled()?.focus();
    });
  });

  function onKey(e: KeyboardEvent) {
    const items = itemButtons().filter((b) => !b.disabled);
    if (items.length === 0) return;
    const active = document.activeElement as HTMLButtonElement | null;
    const idx = active ? items.indexOf(active) : -1;
    if (e.key === 'Escape') {
      e.preventDefault();
      closeContextMenu();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      const next = items[(idx + 1 + items.length) % items.length];
      next?.focus();
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      const prev = items[(idx - 1 + items.length) % items.length];
      prev?.focus();
      return;
    }
    if (e.key === 'Home') {
      e.preventDefault();
      items[0]?.focus();
      return;
    }
    if (e.key === 'End') {
      e.preventDefault();
      items[items.length - 1]?.focus();
      return;
    }
    if (e.key === 'Enter' || e.key === ' ') {
      if (idx >= 0) {
        e.preventDefault();
        runItem(items[idx]);
      }
    }
  }

  function runItem(btn: HTMLButtonElement) {
    const id = btn.dataset.itemId;
    if (!id) return;
    const item = contextMenu.items.find((it) => it.id === id);
    if (!item || item.disabled) return;
    item.run();
    closeContextMenu();
  }

  function onItemClick(e: MouseEvent, item: ContextMenuItem) {
    if (item.disabled) {
      e.preventDefault();
      return;
    }
    item.run();
    closeContextMenu();
  }

  function onDocPointer(e: PointerEvent) {
    if (!contextMenu.open) return;
    const t = e.target as Node | null;
    if (t && menuEl?.contains(t)) return;
    closeContextMenu();
  }

  // Any scroll shifts viewport coords out from under the menu; just close.
  function onScroll() {
    if (contextMenu.open) closeContextMenu();
  }

  // We deliberately do NOT register a global `contextmenu` listener — a
  // right-click outside our own menu should fall through to the browser's
  // native menu (or another component's handler) after we close.
  $effect(() => {
    if (!contextMenu.open) return;
    document.addEventListener('pointerdown', onDocPointer, true);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', closeContextMenu);
    return () => {
      document.removeEventListener('pointerdown', onDocPointer, true);
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', closeContextMenu);
    };
  });
</script>

{#if contextMenu.open}
  <ul
    bind:this={menuEl}
    class="ctx-menu"
    role="menu"
    style="left: {clampedX}px; top: {clampedY}px;"
    onkeydown={onKey}
  >
    {#each contextMenu.items as item (item.id)}
      <li role="none">
        <button
          type="button"
          role="menuitem"
          class="item"
          data-item-id={item.id}
          disabled={item.disabled}
          aria-disabled={item.disabled ? 'true' : undefined}
          tabindex="-1"
          onclick={(e) => onItemClick(e, item)}
        >
          <span class="label">{item.label}</span>
          {#if item.shortcut}
            <span class="shortcut">{item.shortcut}</span>
          {/if}
        </button>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .ctx-menu {
    position: fixed;
    margin: 0;
    padding: 4px;
    list-style: none;
    min-width: 180px;
    background: var(--ctx-bg-panel);
    color: var(--ctx-fg);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    z-index: 1000;
    font-size: 12px;
  }

  .item {
    display: flex;
    align-items: center;
    gap: 16px;
    width: 100%;
    padding: 6px 12px;
    background: transparent;
    color: inherit;
    border: 0;
    border-radius: 4px;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .item:hover:not(:disabled),
  .item:focus-visible {
    background: var(--ctx-bg-elev);
    outline: none;
  }
  .item:focus-visible {
    box-shadow: inset 0 0 0 2px var(--ctx-accent);
  }
  .item:disabled {
    color: var(--ctx-fg-dim);
    cursor: not-allowed;
  }

  .label {
    flex: 1 1 auto;
    min-width: 0;
  }
  .shortcut {
    color: var(--ctx-fg-dim);
    font-size: 11px;
    margin-left: auto;
  }
</style>
