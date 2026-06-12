<script lang="ts">
  import {
    theme,
    setTheme,
    themeMeta,
    THEMES,
    CATEGORY_LABELS,
    type ThemeName,
    type ThemeCategory,
  } from '../lib/theme.svelte';
  import { announce } from '../lib/announce.svelte';

  // THEMES is sorted by category; collapse into groups for the picker so users
  // see at a glance which themes are safe for long sessions vs. novelty bursts.
  const grouped = (Object.keys(CATEGORY_LABELS) as ThemeCategory[]).map((category) => ({
    category,
    label: CATEGORY_LABELS[category],
    items: THEMES.filter((t) => t.category === category),
  })) satisfies ReadonlyArray<{
    category: ThemeCategory;
    label: string;
    items: readonly (typeof THEMES)[number][];
  }>;

  // Picker is a WAI-ARIA menubutton + menuitemradio popover. The trigger lives
  // in the topbar; the menu pops down-right from it. We deliberately avoid
  // <dialog> + showModal() here: the picker is contextual (anchored to its
  // trigger), not a full-screen overlay.

  let open = $state(false);
  let triggerEl: HTMLButtonElement | null = $state(null);
  let menuEl: HTMLUListElement | null = $state(null);

  let currentMeta = $derived(themeMeta(theme.name));

  function openMenu() {
    open = true;
    queueMicrotask(() => {
      const items = menuItems();
      const activeIdx = items.findIndex(
        (b) => b.dataset.themeName === theme.name,
      );
      (items[activeIdx >= 0 ? activeIdx : 0] as HTMLElement | undefined)?.focus();
    });
  }

  function closeMenu(returnFocus = true) {
    if (!open) return;
    open = false;
    if (returnFocus) triggerEl?.focus();
  }

  function toggle() {
    if (open) closeMenu();
    else openMenu();
  }

  function choose(name: ThemeName) {
    setTheme(name);
    announce(`Theme: ${name}`);
    closeMenu();
  }

  function menuItems(): HTMLButtonElement[] {
    if (!menuEl) return [];
    return Array.from(menuEl.querySelectorAll<HTMLButtonElement>('[role="menuitemradio"]'));
  }

  function onMenuKey(e: KeyboardEvent) {
    const items = menuItems();
    const idx = items.indexOf(document.activeElement as HTMLButtonElement);
    if (e.key === 'Escape') {
      e.preventDefault();
      closeMenu();
      return;
    }
    if (e.key === 'Tab') {
      // Tabbing out of the menu closes it but lets focus continue naturally.
      closeMenu(false);
      return;
    }
    if (e.key === 'ArrowDown' || e.key === 'ArrowRight') {
      e.preventDefault();
      const next = items[(idx + 1) % items.length];
      next?.focus();
      return;
    }
    if (e.key === 'ArrowUp' || e.key === 'ArrowLeft') {
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
  }

  function onTriggerKey(e: KeyboardEvent) {
    if ((e.key === 'ArrowDown' || e.key === 'Enter' || e.key === ' ') && !open) {
      e.preventDefault();
      openMenu();
    }
  }

  function onDocPointer(e: PointerEvent) {
    if (!open) return;
    const t = e.target as Node | null;
    if (!t) return;
    if (triggerEl?.contains(t)) return;
    if (menuEl?.contains(t)) return;
    closeMenu(false);
  }

  $effect(() => {
    if (!open) return;
    document.addEventListener('pointerdown', onDocPointer, true);
    return () => document.removeEventListener('pointerdown', onDocPointer, true);
  });
</script>

<div class="picker">
  <button
    bind:this={triggerEl}
    type="button"
    class="trigger"
    aria-haspopup="menu"
    aria-expanded={open}
    aria-label={`Theme: ${currentMeta.label} — open picker`}
    onclick={toggle}
    onkeydown={onTriggerKey}
  >
    <span class="swatch" style:background={currentMeta.swatch} aria-hidden="true"></span>
    <span class="label-text">Theme: {currentMeta.label}</span>
    <span class="caret" aria-hidden="true">{open ? '▴' : '▾'}</span>
  </button>

  {#if open}
    <ul
      bind:this={menuEl}
      class="menu"
      role="menu"
      aria-label="Theme picker"
      onkeydown={onMenuKey}
    >
      {#each grouped as group, gi (group.category)}
        {#if gi > 0}
          <li role="separator" class="separator"></li>
        {/if}
        <li role="presentation" class="group-label">{group.label}</li>
        {#each group.items as t (t.name)}
          {@const active = t.name === theme.name}
          <li role="none">
            <button
              type="button"
              role="menuitemradio"
              class="item"
              class:active
              aria-checked={active}
              data-theme-name={t.name}
              tabindex={active ? 0 : -1}
              onclick={() => choose(t.name)}
            >
              <span class="swatch" style:background={t.swatch} aria-hidden="true"></span>
              <span class="text">
                <span class="name-text">{t.label}</span>
                <span class="desc">{t.description}</span>
              </span>
              <span class="check" aria-hidden="true">{active ? '✓' : ''}</span>
            </button>
          </li>
        {/each}
      {/each}
    </ul>
  {/if}
</div>

<style>
  .picker {
    position: relative;
    display: inline-block;
  }

  .trigger {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    padding: 2px 6px 2px 8px;
    background: transparent;
    color: var(--ctx-fg);
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    cursor: pointer;
  }
  .trigger:hover {
    background: var(--ctx-bg-panel);
  }
  .trigger:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .caret {
    color: var(--ctx-fg-dim);
    font-size: 9px;
    margin-left: 2px;
  }

  .swatch {
    display: inline-block;
    width: 10px;
    height: 10px;
    border-radius: 50%;
    border: 1px solid var(--ctx-border);
    flex: 0 0 auto;
  }

  .menu {
    position: absolute;
    top: calc(100% + 4px);
    right: 0;
    margin: 0;
    padding: 4px;
    list-style: none;
    min-width: 240px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    z-index: 50;
    opacity: 1;
    transform: translateY(0);
    transition: opacity var(--motion-fast) ease-out, transform var(--motion-fast) ease-out;
    @starting-style {
      opacity: 0;
      transform: translateY(-4px);
    }
    /* WHY: the topbar has 'overflow: visible' but its parent layout uses grid;
       z-index lifts the popover above the main content panes regardless of
       stacking-context ordering. */
  }

  .item {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    padding: 6px 8px;
    background: transparent;
    color: var(--ctx-fg);
    border: 0;
    border-radius: 4px;
    text-align: left;
    cursor: pointer;
    font: inherit;
  }
  .item:hover,
  .item:focus-visible {
    background: var(--ctx-bg-panel);
    outline: none;
  }
  .item:focus-visible {
    box-shadow: inset 0 0 0 2px var(--ctx-accent);
  }
  .item.active {
    color: var(--ctx-fg-strong, var(--ctx-fg));
  }

  .text {
    display: flex;
    flex-direction: column;
    flex: 1 1 auto;
    min-width: 0;
  }
  .name-text {
    font-size: 12px;
    font-weight: 500;
  }
  .desc {
    font-size: 10px;
    color: var(--ctx-fg-dim);
    margin-top: 1px;
  }
  .check {
    color: var(--ctx-accent);
    width: 1ch;
    text-align: right;
  }

  .group-label {
    padding: 6px 8px 2px;
    font-size: 10px;
    font-weight: 500;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--ctx-fg-dim);
    user-select: none;
    pointer-events: none;
  }

  .separator {
    margin: 4px 0 2px;
    height: 1px;
    background: var(--ctx-border);
    list-style: none;
  }
</style>
