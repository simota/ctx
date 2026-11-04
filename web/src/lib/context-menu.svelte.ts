// Generic single-instance context menu. Only one menu is open at a time and
// the position is viewport-space (clientX/Y) so callers can hand off the
// raw `MouseEvent` coordinates without translating to document space.

export interface ContextMenuItem {
  id: string;
  label: string;
  shortcut?: string;
  disabled?: boolean;
  run: () => void;
}

interface ContextMenuState {
  open: boolean;
  x: number;
  y: number;
  items: ContextMenuItem[];
}

export const contextMenu = $state<ContextMenuState>({
  open: false,
  x: 0,
  y: 0,
  items: [],
});

// Element focused before the menu opened; focus returns there when the menu
// closes while it still holds focus (Escape, Tab, or an item that did not
// move focus itself).
let lastFocus: HTMLElement | null = null;

export function openContextMenu(x: number, y: number, items: ContextMenuItem[]): void {
  if (items.length === 0) return;
  if (!contextMenu.open && typeof document !== 'undefined') {
    lastFocus = document.activeElement instanceof HTMLElement ? document.activeElement : null;
  }
  contextMenu.x = x;
  contextMenu.y = y;
  contextMenu.items = items;
  contextMenu.open = true;
}

export function closeContextMenu(): void {
  const active = typeof document !== 'undefined' ? document.activeElement : null;
  const focusInMenu = active instanceof HTMLElement && active.closest('.ctx-menu') !== null;
  contextMenu.open = false;
  contextMenu.items = [];
  const target = lastFocus;
  lastFocus = null;
  if (focusInMenu && target?.isConnected) {
    queueMicrotask(() => target.focus());
  }
}
