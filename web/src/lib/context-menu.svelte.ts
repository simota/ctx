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

export function openContextMenu(x: number, y: number, items: ContextMenuItem[]): void {
  if (items.length === 0) return;
  contextMenu.x = x;
  contextMenu.y = y;
  contextMenu.items = items;
  contextMenu.open = true;
}

export function closeContextMenu(): void {
  contextMenu.open = false;
  contextMenu.items = [];
}
