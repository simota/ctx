<script lang="ts">
  import { cheatsheet, closeCheatsheet } from '../lib/cheatsheet.svelte';

  let dialogEl: HTMLDialogElement | null = $state(null);

  interface Binding {
    keys: string[];
    desc: string;
  }
  interface Section {
    title: string;
    rows: Binding[];
  }

  const sections: Section[] = [
    {
      title: 'Global',
      rows: [
        { keys: ['⌘⇧P', 'Ctrl+Shift+P'], desc: 'Open command palette' },
        { keys: ['⌘P', 'Ctrl+P'], desc: 'Find file by name' },
        { keys: ['/'], desc: 'Focus search bar' },
        { keys: ['?'], desc: 'Toggle this cheatsheet' },
        { keys: ['Shift+R'], desc: 'Reload file tree' },
        { keys: ['⌘W', 'Ctrl+W'], desc: 'Close tab' },
        { keys: ['⌘1..9', 'Ctrl+1..9'], desc: 'Switch to tab N' },
        { keys: ['⌘\\', 'Ctrl+\\'], desc: 'Toggle right pane' },
        { keys: ['⌘B', 'Ctrl+B'], desc: 'Toggle file tree' },
        { keys: ['⌘⇧B', 'Ctrl+Shift+B'], desc: 'Switch project root' },
        { keys: ['1..9'], desc: 'Quick switch to top bookmark (in root picker)' },
        { keys: ['⌘K ⌘→ / ⌘K ⌘←'], desc: 'Switch focus between panes' },
        { keys: ['g d'], desc: 'Open definition picker' },
      ],
    },
    {
      title: 'File tree',
      rows: [
        { keys: ['↑', 'k'], desc: 'Move up' },
        { keys: ['↓', 'j'], desc: 'Move down' },
        { keys: ['←', 'h'], desc: 'Collapse / parent' },
        { keys: ['→', 'l'], desc: 'Expand / first child' },
        { keys: ['gg'], desc: 'Jump to first row' },
        { keys: ['G', 'End'], desc: 'Jump to last row' },
        { keys: ['Home'], desc: 'Jump to first row' },
        { keys: ['Enter', 'Space'], desc: 'Open / toggle' },
        { keys: ['Click dir'], desc: 'Expand + show directory overview' },
      ],
    },
    {
      title: 'Code viewer',
      rows: [
        { keys: ['⌘⇧F', 'Ctrl+Shift+F'], desc: 'Find in file' },
        { keys: ['Esc'], desc: 'Close find bar' },
        { keys: ['j'], desc: 'Scroll down one line' },
        { keys: ['k'], desc: 'Scroll up one line' },
        { keys: ['{count}j', '{count}k'], desc: 'Scroll N lines' },
        { keys: ['Ctrl+d'], desc: 'Scroll half page down' },
        { keys: ['Ctrl+u'], desc: 'Scroll half page up' },
        { keys: ['Ctrl+f'], desc: 'Scroll one page down' },
        { keys: ['Ctrl+b'], desc: 'Scroll one page up' },
        { keys: ['gg'], desc: 'Jump to top of file' },
        { keys: ['G'], desc: 'Jump to bottom of file' },
        { keys: ['{count}G'], desc: 'Jump to line N (e.g. 42G)' },
        { keys: ['zz'], desc: 'Center current line in viewport' },
        { keys: ['0'], desc: 'Scroll to line start (horizontal)' },
        { keys: ['$'], desc: 'Scroll to line end (horizontal)' },
        { keys: ['Shift+D'], desc: 'Toggle git diff view' },
      ],
    },
    {
      title: 'Theme & display',
      rows: [
        { keys: ['Theme button'], desc: 'Toggle dark / light' },
        { keys: ['Wrap button'], desc: 'Toggle line wrap (code viewer)' },
      ],
    },
  ];

  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      // `<dialog>` natively fires a `cancel` event on Esc which calls onclose,
      // but we intercept here to run closeCheatsheet (state sync + announce).
      // preventDefault stops the browser's built-in close so we own the flow.
      e.preventDefault();
      closeCheatsheet();
    }
  }

  function onDialogClick(e: MouseEvent) {
    // Close when the user clicks the ::backdrop (target === dialog itself).
    if (e.target === dialogEl) closeCheatsheet();
  }

  $effect(() => {
    if (!dialogEl) return;
    if (cheatsheet.open) {
      if (!dialogEl.open) dialogEl.showModal();
      queueMicrotask(() => dialogEl?.focus());
    } else {
      if (dialogEl.open) dialogEl.close();
    }
  });
</script>

<!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
<dialog
  bind:this={dialogEl}
  class="modal"
  aria-label="Keyboard shortcuts"
  onclick={onDialogClick}
  onkeydown={onKey}
  onclose={closeCheatsheet}
>
  <header class="head">
    <h2>Keyboard shortcuts</h2>
    <button
      type="button"
      class="close"
      aria-label="Close cheatsheet"
      onclick={closeCheatsheet}
    >×</button>
  </header>
  <div class="body">
    {#each sections as section, si (`${section.title}|${si}`)}
      <section class="group" aria-labelledby={`ctx-cs-${section.title}`}>
        <h3 id={`ctx-cs-${section.title}`}>{section.title}</h3>
        <table>
          <tbody>
            {#each section.rows as row, ri (`${row.desc}|${row.keys.join(',')}|${ri}`)}
              <tr>
                <th scope="row">
                  {#each row.keys as k, i (i)}
                    {#if i > 0}<span class="sep" aria-hidden="true">or</span>{/if}
                    <kbd>{k}</kbd>
                  {/each}
                </th>
                <td>{row.desc}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      </section>
    {/each}
  </div>
  <footer class="status muted">
    <kbd>?</kbd> or <kbd>Esc</kbd> to close
  </footer>
</dialog>

<style>
  /* Scoped to [open] so the closed <dialog> stays at the browser default
     `display: none` — otherwise `display: flex` overrides that default and
     the closed dialog occupies layout space below the viewport, pushing
     the page taller and scrolling the topbar out of view on smaller screens. */
  dialog[open] {
    width: 100%;
    max-width: 640px;
    margin: 10vh auto 0;
    padding: 0;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    min-height: 0;
    max-height: 80vh;
    outline: none;
    color: var(--ctx-fg);
  }
  dialog:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  dialog::backdrop {
    background: rgba(0, 0, 0, 0.35);
  }
  :global(:root[data-theme='light']) dialog::backdrop {
    background: rgba(0, 0, 0, 0.18);
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 10px 14px;
    border-bottom: 1px solid var(--ctx-border);
  }
  .head h2 {
    margin: 0;
    font-size: 13px;
    font-weight: 600;
    color: var(--ctx-fg);
  }
  .close {
    border: 0;
    padding: 2px 6px;
    font-size: 16px;
    line-height: 1;
    color: var(--ctx-fg-dim);
  }
  .close:hover {
    color: var(--ctx-fg);
  }
  .body {
    padding: 8px 14px 12px;
    overflow: auto;
    flex: 1 1 auto;
    min-height: 0;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 8px 24px;
  }
  .group {
    min-width: 0;
  }
  .group h3 {
    margin: 8px 0 4px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
    font-weight: 600;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  th {
    text-align: left;
    font-weight: 400;
    padding: 3px 8px 3px 0;
    white-space: nowrap;
    vertical-align: top;
    width: 1%;
  }
  td {
    padding: 3px 0;
    color: var(--ctx-fg-dim);
    vertical-align: top;
  }
  kbd {
    display: inline-block;
    font-family: var(--ctx-font-mono);
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    padding: 1px 6px;
    font-size: 11px;
    color: var(--ctx-fg);
  }
  .sep {
    color: var(--ctx-fg-dim);
    margin: 0 4px;
    font-size: 10px;
  }
  .status {
    padding: 6px 14px;
    border-top: 1px solid var(--ctx-border);
    font-size: 11px;
    flex: 0 0 auto;
  }
  .status kbd {
    background: var(--ctx-bg);
  }
  @media (max-width: 560px) {
    .body {
      grid-template-columns: 1fr;
    }
  }
</style>
