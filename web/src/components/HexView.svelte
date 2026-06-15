<script lang="ts">
  import { formatHexLine, rowCount } from '../lib/hex';
  import { announce } from '../lib/announce.svelte';
  import { openContextMenu, type ContextMenuItem } from '../lib/context-menu.svelte';

  interface Props {
    bytes: Uint8Array;
    path: string;
  }

  let { bytes, path }: Props = $props();

  const ROW_HEIGHT = 20; // px — monospace line height
  const OVERSCAN = 10;   // rows rendered above/below the viewport

  let containerEl: HTMLDivElement | null = $state(null);
  let scrollTop = $state(0);
  let viewportHeight = $state(400);

  const totalRows = $derived(rowCount(bytes.length));
  const totalHeight = $derived(totalRows * ROW_HEIGHT);

  // Visible row window
  const firstVisible = $derived(Math.max(0, Math.floor(scrollTop / ROW_HEIGHT) - OVERSCAN));
  const lastVisible = $derived(
    Math.min(totalRows - 1, Math.ceil((scrollTop + viewportHeight) / ROW_HEIGHT) + OVERSCAN),
  );

  interface RenderedRow {
    rowIdx: number;
    offset: string;
    hex: string;
    ascii: string;
    top: number;
  }

  const visibleRows = $derived.by<RenderedRow[]>(() => {
    if (bytes.length === 0) return [];
    const rows: RenderedRow[] = [];
    for (let i = firstVisible; i <= lastVisible; i++) {
      const byteOffset = i * 16;
      const line = formatHexLine(bytes, byteOffset);
      rows.push({ rowIdx: i, ...line, top: i * ROW_HEIGHT });
    }
    return rows;
  });

  function onScroll(e: Event) {
    const el = e.currentTarget as HTMLDivElement;
    scrollTop = el.scrollTop;
    viewportHeight = el.clientHeight;
  }

  function isTextInputFocused(): boolean {
    const a = document.activeElement as HTMLElement | null;
    if (!a) return false;
    const tag = a.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
    if (a.isContentEditable) return true;
    return false;
  }

  function scrollByRows(delta: number) {
    if (!containerEl) return;
    containerEl.scrollTop += delta * ROW_HEIGHT;
  }

  function scrollToRow(row: number) {
    if (!containerEl) return;
    containerEl.scrollTop = Math.max(0, Math.min(row * ROW_HEIGHT, (totalRows - 1) * ROW_HEIGHT));
  }

  function onKeydown(e: KeyboardEvent) {
    if (isTextInputFocused()) return;
    switch (e.key) {
      case 'ArrowUp':
        e.preventDefault();
        scrollByRows(-1);
        break;
      case 'ArrowDown':
        e.preventDefault();
        scrollByRows(1);
        break;
      case 'PageUp':
        e.preventDefault();
        scrollByRows(-Math.floor(viewportHeight / ROW_HEIGHT));
        break;
      case 'PageDown':
        e.preventDefault();
        scrollByRows(Math.floor(viewportHeight / ROW_HEIGHT));
        break;
      case 'Home':
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          scrollToRow(0);
        }
        break;
      case 'End':
        if (e.ctrlKey || e.metaKey) {
          e.preventDefault();
          scrollToRow(totalRows - 1);
        }
        break;
      case 'g':
        e.preventDefault();
        scrollToRow(0);
        announce('Top of hex dump');
        break;
      case 'G':
        e.preventDefault();
        scrollToRow(totalRows - 1);
        announce('Bottom of hex dump');
        break;
    }
  }

  $effect(() => {
    window.addEventListener('keydown', onKeydown);
    return () => window.removeEventListener('keydown', onKeydown);
  });

  // Measure viewport on mount and resize
  $effect(() => {
    if (!containerEl) return;
    viewportHeight = containerEl.clientHeight;
    const ro = new ResizeObserver(() => {
      if (containerEl) viewportHeight = containerEl.clientHeight;
    });
    ro.observe(containerEl);
    return () => ro.disconnect();
  });

  // Announce on mount
  $effect(() => {
    if (bytes.length === 0) {
      announce('Hex dump: empty file');
    } else {
      announce(`Hex dump: ${bytes.length} bytes, ${totalRows} rows`);
    }
  });

  // Build full hex text for clipboard (only visible window for large files; all for small)
  const MAX_COPY_BYTES = 64 * 1024; // 64 KiB

  function buildHexText(all: boolean): string {
    const limit = all ? bytes.length : Math.min(bytes.length, MAX_COPY_BYTES);
    const lines: string[] = [];
    for (let offset = 0; offset < limit; offset += 16) {
      const line = formatHexLine(bytes, offset);
      lines.push(`${line.offset}  ${line.hex}  |${line.ascii.trimEnd()}|`);
    }
    if (limit < bytes.length) {
      lines.push(`... (truncated at ${MAX_COPY_BYTES} bytes)`);
    }
    return lines.join('\n');
  }

  async function copyToClipboard(text: string, label: string) {
    try {
      await navigator.clipboard.writeText(text);
      announce(`Copied ${label}`);
    } catch {
      announce('Copy failed');
    }
  }

  function onContextMenu(e: MouseEvent) {
    e.preventDefault();
    const items: ContextMenuItem[] = [
      {
        id: 'copy-hex',
        label: bytes.length > MAX_COPY_BYTES ? `Copy Hex (first 64 KiB)` : 'Copy Hex',
        run: () => copyToClipboard(buildHexText(false), 'hex dump'),
      },
      {
        id: 'copy-ascii',
        label: 'Copy ASCII',
        run: () => {
          const ascii = Array.from(bytes.slice(0, MAX_COPY_BYTES))
            .map((b) => (b >= 0x20 && b <= 0x7e ? String.fromCharCode(b) : '.'))
            .join('');
          copyToClipboard(ascii + (bytes.length > MAX_COPY_BYTES ? '...' : ''), 'ASCII text');
        },
      },
    ];
    openContextMenu(e.clientX, e.clientY, items);
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="hex-view"
  role="grid"
  aria-label={`Hex dump of ${path}`}
  aria-rowcount={totalRows}
  onscroll={onScroll}
  oncontextmenu={onContextMenu}
  bind:this={containerEl}
  tabindex="0"
>
  {#if bytes.length === 0}
    <div class="hex-empty">(empty file)</div>
  {:else}
    <!-- Spacer establishes full scroll height -->
    <div class="hex-spacer" style="height: {totalHeight}px; position: relative;">
      {#each visibleRows as row (row.rowIdx)}
        <div
          class="hex-row"
          role="row"
          aria-rowindex={row.rowIdx + 1}
          style="top: {row.top}px;"
        >
          <span class="hex-offset" role="gridcell">{row.offset}</span>
          <span class="hex-bytes" role="gridcell">{row.hex}</span>
          <span class="hex-ascii" role="gridcell" aria-hidden="true">|{row.ascii}|</span>
        </div>
      {/each}
    </div>
  {/if}
</div>

<style>
  .hex-view {
    overflow-y: auto;
    overflow-x: auto;
    flex: 1 1 auto;
    min-height: 0;
    font-family: ui-monospace, SFMono-Regular, Menlo, 'Cascadia Code', monospace;
    font-size: var(--ctx-code-font-size, 13px);
    line-height: 1;
    background: var(--ctx-bg, #1e1e1e);
    color: var(--ctx-fg, #d4d4d4);
    outline: none;
  }

  .hex-empty {
    padding: 24px;
    color: var(--ctx-fg-dim, #888);
    font-style: italic;
  }

  /* .hex-spacer is purely a sizing container for absolute-positioned rows */

  .hex-row {
    position: absolute;
    left: 0;
    right: 0;
    display: flex;
    align-items: center;
    height: 20px;
    padding: 0 12px;
    white-space: pre;
    gap: 0;
  }

  .hex-row:hover {
    background: var(--ctx-bg-elev, #252526);
  }

  .hex-offset {
    color: var(--ctx-fg-dim, #888);
    margin-right: 2ch;
    user-select: none;
  }

  .hex-bytes {
    color: var(--ctx-fg, #d4d4d4);
    margin-right: 2ch;
    letter-spacing: 0.02em;
  }

  .hex-ascii {
    color: var(--hl-string, #98c379);
    opacity: 0.75;
    user-select: none;
  }
</style>
