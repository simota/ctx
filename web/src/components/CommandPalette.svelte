<script lang="ts">
  import {
    palette,
    closePalette,
    execute,
    scoreCommand,
    type CommandHit,
  } from '../lib/palette.svelte';
  import { COMMANDS, CATEGORY_ORDER, type CommandCategory } from '../lib/commands';
  import { announce } from '../lib/announce.svelte';

  let inputEl: HTMLInputElement | null = $state(null);
  let listEl: HTMLUListElement | null = $state(null);

  // Empty-query view: every command grouped by category in canonical order.
  // Non-empty: ranked single list (no section headers — VS Code style).
  interface Row {
    kind: 'header' | 'command';
    category?: CommandCategory;
    hit?: CommandHit;
    // Position in the *enabled-commands* array, used for keyboard nav.
    selectableIndex?: number;
  }

  let rows = $derived.by<Row[]>(() => {
    if (!palette.open) return [];
    const q = palette.query.trim();
    if (q === '') {
      // Group by category (canonical order). Headers + commands.
      const out: Row[] = [];
      let sel = 0;
      for (const cat of CATEGORY_ORDER) {
        const inCat = COMMANDS.filter((c) => c.category === cat);
        if (inCat.length === 0) continue;
        out.push({ kind: 'header', category: cat });
        for (const cmd of inCat) {
          out.push({
            kind: 'command',
            hit: { cmd, score: 0, positions: [] },
            selectableIndex: sel,
          });
          sel += 1;
        }
      }
      return out;
    }
    // Ranked: score every command, drop nulls, sort desc by score (then label).
    const hits: CommandHit[] = [];
    for (const cmd of COMMANDS) {
      const h = scoreCommand(cmd, q);
      if (h !== null) hits.push(h);
    }
    hits.sort((a, b) => {
      if (b.score !== a.score) return b.score - a.score;
      return a.cmd.label.length - b.cmd.label.length;
    });
    return hits.map((h, i) => ({
      kind: 'command' as const,
      hit: h,
      selectableIndex: i,
    }));
  });

  // Selectable rows (commands only) — drives ↑↓ navigation + execution.
  let selectable = $derived(rows.filter((r) => r.kind === 'command'));

  // Clamp selection whenever rows change.
  $effect(() => {
    const len = selectable.length;
    if (palette.selectedIndex >= len) {
      palette.selectedIndex = len > 0 ? len - 1 : 0;
    }
    if (palette.selectedIndex < 0) palette.selectedIndex = 0;
  });

  // Focus input on open.
  $effect(() => {
    if (palette.open && inputEl) {
      const el = inputEl;
      queueMicrotask(() => el.focus());
    }
  });

  // Scroll the active option into view when the selection moves.
  $effect(() => {
    if (!listEl) return;
    const idx = palette.selectedIndex;
    const opt = listEl.querySelector<HTMLElement>(`[data-sidx="${idx}"]`);
    if (opt) opt.scrollIntoView({ block: 'nearest' });
  });

  // Debounced result-count announcement (avoid spam while typing).
  let announceTimer: ReturnType<typeof setTimeout> | null = null;
  $effect(() => {
    if (!palette.open) return;
    const q = palette.query;
    const n = selectable.length;
    if (announceTimer) clearTimeout(announceTimer);
    // Only announce after the user has typed something — initial open already
    // announced "{N} commands".
    if (q === '') return;
    announceTimer = setTimeout(() => {
      announce(`${n} match${n === 1 ? '' : 'es'}`);
      announceTimer = null;
    }, 200);
    return () => {
      if (announceTimer) {
        clearTimeout(announceTimer);
        announceTimer = null;
      }
    };
  });

  function isDisabled(hit: CommandHit): boolean {
    return hit.cmd.when ? !hit.cmd.when() : false;
  }

  function activate(hit: CommandHit): void {
    if (isDisabled(hit)) return;
    execute(hit.cmd);
  }

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closePalette();
      return;
    }
    if (e.key === 'Tab') {
      // Trap Tab inside the input — there are no other focusable controls.
      e.preventDefault();
      return;
    }
    if (e.key === 'ArrowDown') {
      e.preventDefault();
      if (selectable.length === 0) return;
      palette.selectedIndex = (palette.selectedIndex + 1) % selectable.length;
      return;
    }
    if (e.key === 'ArrowUp') {
      e.preventDefault();
      if (selectable.length === 0) return;
      palette.selectedIndex =
        (palette.selectedIndex - 1 + selectable.length) % selectable.length;
      return;
    }
    if (e.key === 'Enter') {
      e.preventDefault();
      const sel = selectable[palette.selectedIndex];
      if (sel?.hit) activate(sel.hit);
      return;
    }
  }

  function onOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) closePalette();
  }

  // Highlight matched chars by splitting the label into <mark> / plain spans.
  // Returns segments so we can avoid `@html` (XSS-safe interpolation).
  function highlight(
    label: string,
    positions: number[],
  ): { text: string; mark: boolean }[] {
    if (positions.length === 0) return [{ text: label, mark: false }];
    const out: { text: string; mark: boolean }[] = [];
    let cursor = 0;
    let i = 0;
    while (i < positions.length) {
      const start = positions[i];
      let end = start;
      let j = i + 1;
      while (j < positions.length && positions[j] === positions[j - 1] + 1) {
        end = positions[j];
        j += 1;
      }
      if (start > cursor) out.push({ text: label.slice(cursor, start), mark: false });
      out.push({ text: label.slice(start, end + 1), mark: true });
      cursor = end + 1;
      i = j;
    }
    if (cursor < label.length) out.push({ text: label.slice(cursor), mark: false });
    return out;
  }
</script>

{#if palette.open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="overlay"
    onclick={onOverlayClick}
    onkeydown={onKey}
    role="presentation"
  >
    <div
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ctx-palette-title"
      aria-describedby="ctx-palette-help"
    >
      <h2 id="ctx-palette-title" class="sr-only">Command palette</h2>
      <span id="ctx-palette-help" class="sr-only">
        Type to filter commands. Use up and down arrows to navigate, Enter to run, Escape to close.
      </span>
      <input
        bind:this={inputEl}
        bind:value={palette.query}
        type="text"
        class="input"
        role="combobox"
        aria-expanded={selectable.length > 0}
        aria-controls="ctx-palette-list"
        aria-autocomplete="list"
        aria-activedescendant={selectable.length > 0
          ? `ctx-palette-opt-${palette.selectedIndex}`
          : undefined}
        placeholder="Type a command…"
        autocomplete="off"
        spellcheck="false"
        onkeydown={onKey}
      />
      <ul
        bind:this={listEl}
        id="ctx-palette-list"
        class="results"
        role="listbox"
        aria-label="Available commands"
      >
        {#if rows.length === 0}
          <li class="empty muted">No matching commands.</li>
        {:else}
          {#each rows as row, ri (ri)}
            {#if row.kind === 'header'}
              <li class="section" role="presentation">{row.category}</li>
            {:else if row.hit}
              {@const sel = row.selectableIndex === palette.selectedIndex}
              {@const disabled = isDisabled(row.hit)}
              {@const segs = highlight(row.hit.cmd.label, row.hit.positions)}
              <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_noninteractive_element_interactions -->
              <li
                id={`ctx-palette-opt-${row.selectableIndex}`}
                class="opt"
                class:selected={sel}
                class:disabled
                role="option"
                aria-selected={sel}
                aria-disabled={disabled || undefined}
                tabindex="-1"
                data-sidx={row.selectableIndex}
                onclick={() => row.hit && activate(row.hit)}
                onmousemove={() => {
                  if (row.selectableIndex !== undefined) {
                    palette.selectedIndex = row.selectableIndex;
                  }
                }}
              >
                <span class="label">
                  {#if palette.query.trim() === ''}
                    {row.hit.cmd.label}
                  {:else}
                    {#each segs as seg, si (si)}
                      {#if seg.mark}<mark>{seg.text}</mark>{:else}<span>{seg.text}</span>{/if}
                    {/each}
                  {/if}
                </span>
                {#if row.hit.cmd.shortcut}
                  <span class="shortcut" aria-hidden="true">{row.hit.cmd.shortcut}</span>
                {/if}
                <span class="cat" aria-hidden="true">{row.hit.cmd.category}</span>
              </li>
            {/if}
          {/each}
        {/if}
      </ul>
      <footer class="status muted">
        {#if palette.query.trim() === ''}
          {COMMANDS.length} commands • <kbd>↑</kbd><kbd>↓</kbd> navigate • <kbd>Enter</kbd> run • <kbd>Esc</kbd> close
        {:else}
          {selectable.length} match{selectable.length === 1 ? '' : 'es'} • <kbd>↑</kbd><kbd>↓</kbd> navigate • <kbd>Enter</kbd> run • <kbd>Esc</kbd> close
        {/if}
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
    @starting-style {
      opacity: 0;
    }
  }
  :global(:root[data-theme='light']) .overlay {
    background: rgba(0, 0, 0, 0.18);
  }
  .modal {
    width: 100%;
    max-width: 620px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 70vh;
    transform: translateY(0) scale(1);
    transition: transform var(--motion-base) ease-out, opacity var(--motion-base) ease-out;
    @starting-style {
      opacity: 0;
      transform: translateY(-4px) scale(0.98);
    }
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
  .input {
    border: 0;
    border-bottom: 1px solid var(--ctx-border);
    border-radius: 6px 6px 0 0;
    background: transparent;
    padding: 10px 14px;
    font-size: 14px;
    outline: none;
    color: var(--ctx-fg);
  }
  .input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
    border-color: transparent;
  }
  .results {
    list-style: none;
    margin: 0;
    padding: 4px 0;
    overflow: auto;
    flex: 1 1 auto;
    min-height: 0;
  }
  .section {
    padding: 6px 14px 2px;
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
    font-weight: 600;
  }
  .opt {
    padding: 4px 14px;
    cursor: pointer;
    display: grid;
    grid-template-columns: 1fr auto auto;
    gap: 12px;
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
  .opt.disabled {
    color: var(--ctx-fg-dim);
    cursor: not-allowed;
  }
  .opt .label {
    word-break: break-word;
  }
  .opt mark {
    background: transparent;
    color: var(--ctx-accent);
    font-weight: 600;
    padding: 0;
  }
  .opt .shortcut {
    font-family: var(--ctx-font-mono);
    font-size: 10px;
    color: var(--ctx-fg-dim);
    white-space: nowrap;
  }
  .opt .cat {
    font-size: 10px;
    color: var(--ctx-fg-dim);
    text-transform: uppercase;
    letter-spacing: 0.04em;
    white-space: nowrap;
  }
  .empty {
    padding: 16px;
    text-align: center;
    font-size: 12px;
  }
  .status {
    padding: 6px 14px;
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
