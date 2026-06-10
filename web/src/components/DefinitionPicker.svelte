<script lang="ts">
  // Cross-file symbol definition picker — opens when ≥2 candidates exist for a
  // hovered/clicked symbol or when the `g d` chord fires. Selection navigates
  // via the hash router; Cmd/Ctrl+Click opens the candidate in a new tab.
  //
  // Pattern: deliberately copied from CommandPalette rather than extracted
  // (rule of three not yet met — picker has different selection model and no
  // filter input).
  import {
    definitionPicker,
    closeDefinitionPicker,
  } from '../lib/definition-picker.svelte';
  import { navigate, toFileHash } from '../lib/router.svelte';
  import { openTab } from '../lib/tabs.svelte';
  import { announce } from '../lib/announce.svelte';
  import { formatTokens } from '../lib/format';

  let dialogEl: HTMLDivElement | null = $state(null);
  let listEl: HTMLUListElement | null = $state(null);

  // Focus the dialog on open so ↑↓/Enter/Esc work without a child input.
  $effect(() => {
    if (definitionPicker.open && dialogEl) {
      const el = dialogEl;
      queueMicrotask(() => el.focus());
    }
  });

  // Keep the active option visible as ↑↓ moves the selection.
  $effect(() => {
    if (!listEl) return;
    const idx = definitionPicker.selectedIndex;
    const opt = listEl.querySelector<HTMLElement>(`[data-idx="${idx}"]`);
    if (opt) opt.scrollIntoView({ block: 'nearest' });
  });

  function activate(idx: number, mod: boolean): void {
    const c = definitionPicker.candidates[idx];
    if (!c) return;
    const name = definitionPicker.name;
    if (mod) {
      // Open the candidate in a new tab without leaving the current view.
      openTab(c.path);
      announce(`Opened ${c.path} in new tab`);
      return;
    }
    closeDefinitionPicker();
    navigate(toFileHash(c.path, { line: c.line }));
    announce(`Jumped to ${name} in ${c.path} line ${c.line}`);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeDefinitionPicker();
      return;
    }
    const len = definitionPicker.candidates.length;
    if (len === 0) return;
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      definitionPicker.selectedIndex = (definitionPicker.selectedIndex + 1) % len;
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      definitionPicker.selectedIndex =
        (definitionPicker.selectedIndex - 1 + len) % len;
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      activate(definitionPicker.selectedIndex, e.metaKey || e.ctrlKey);
      return;
    }
    if (e.key === 'Tab') {
      // Trap focus inside the dialog — only the dialog itself is focusable.
      e.preventDefault();
      return;
    }
  }

  function onOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) closeDefinitionPicker();
  }

  function onRowClick(e: MouseEvent, idx: number) {
    e.preventDefault();
    activate(idx, e.metaKey || e.ctrlKey);
  }
</script>

{#if definitionPicker.open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="overlay"
    onclick={onOverlayClick}
    onkeydown={onKey}
    role="presentation"
  >
    <div
      bind:this={dialogEl}
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ctx-defpicker-title"
      aria-describedby="ctx-defpicker-help"
      tabindex="-1"
    >
      <header class="head">
        <h2 id="ctx-defpicker-title">
          Definitions of <span class="mono accent">{definitionPicker.name}</span>
          <span class="count muted">({definitionPicker.candidates.length})</span>
        </h2>
        <button
          type="button"
          class="close"
          aria-label="Close definition picker"
          onclick={closeDefinitionPicker}
        >×</button>
      </header>
      <span id="ctx-defpicker-help" class="sr-only">
        Use up and down arrows to select a definition, Enter to jump, Command or
        Control plus Enter to open in a new tab, Escape to close.
      </span>
      <ul
        bind:this={listEl}
        class="results"
        role="listbox"
        aria-label={`Definitions of ${definitionPicker.name}`}
      >
        {#each definitionPicker.candidates as c, i (`${c.path}:${c.line}:${i}`)}
          {@const sel = i === definitionPicker.selectedIndex}
          <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
          <li
            class="opt"
            class:selected={sel}
            role="option"
            aria-selected={sel}
            data-idx={i}
            onclick={(e) => onRowClick(e, i)}
            onmousemove={() => (definitionPicker.selectedIndex = i)}
          >
            <span class="kind">{c.kind}</span>
            <span class="name mono">{c.symbol_name}</span>
            <span class="path mono muted">{c.path}<span class="line">:{c.line}</span></span>
            <span class="meta muted">
              {#if c.file_tokens}{formatTokens(c.file_tokens)} tokens{/if}
              {#if c.file_role}<span class="role">{c.file_role}</span>{/if}
            </span>
          </li>
        {/each}
      </ul>
      <footer class="status muted">
        <kbd>↑</kbd><kbd>↓</kbd> navigate • <kbd>Enter</kbd> jump • <kbd>⌘</kbd>+<kbd>Enter</kbd> new tab • <kbd>Esc</kbd> close
      </footer>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 12vh;
    z-index: 1000;
    opacity: 1;
    transition: opacity var(--motion-fast) ease-out;
    /* @starting-style supplies the pre-mount state so the transition
       runs on insertion rather than reading opacity: 1 as a no-op. */
    @starting-style {
      opacity: 0;
    }
  }
  :global(:root[data-theme='light']) .overlay {
    background: rgba(0, 0, 0, 0.18);
  }
  .modal {
    width: 100%;
    max-width: 720px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 70vh;
    outline: none;
    transform: translateY(0) scale(1);
    transition: transform var(--motion-base) ease-out, opacity var(--motion-base) ease-out;
    @starting-style {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
  }
  .modal:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ctx-border);
  }
  .head h2 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--ctx-fg);
  }
  .head .accent {
    color: var(--ctx-accent);
  }
  .head .count {
    font-weight: 400;
    margin-left: 4px;
  }
  .close {
    border: 0;
    padding: 0 8px;
    font-size: 16px;
    line-height: 1;
    color: var(--ctx-fg-dim);
    background: transparent;
  }
  .close:hover {
    color: var(--ctx-fg);
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    padding: 0;
    margin: -1px;
    overflow: hidden;
    clip: rect(0, 0, 0, 0);
    white-space: nowrap;
    border: 0;
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow: auto;
    flex: 1 1 auto;
    min-height: 0;
  }
  .opt {
    padding: 4px 12px;
    cursor: pointer;
    display: grid;
    grid-template-columns: 60px minmax(0, 1fr) minmax(0, 2fr) auto;
    gap: 10px;
    align-items: baseline;
    font-size: 12px;
    line-height: 1.5;
    border-left: 2px solid transparent;
    color: var(--ctx-fg);
  }
  .opt.selected {
    background: var(--ctx-bg-panel);
    border-left-color: var(--ctx-accent);
  }
  .opt .kind {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--ctx-link);
  }
  .opt .name {
    word-break: break-all;
  }
  .opt .path {
    word-break: break-all;
  }
  .opt .line {
    color: var(--ctx-accent);
    margin-left: 2px;
  }
  .opt .meta {
    font-size: 11px;
    white-space: nowrap;
  }
  .opt .role {
    margin-left: 6px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    font-size: 10px;
  }
  .status {
    padding: 6px 12px;
    border-top: 1px solid var(--ctx-border);
    font-size: 11px;
    flex: 0 0 auto;
  }
  .status kbd {
    font-family: var(--ctx-font-mono);
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    padding: 0 4px;
    margin: 0 2px;
    font-size: 10px;
  }
</style>
