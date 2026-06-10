// Layout-visibility preferences for the file tree (left pane) and the
// symbols/insights aside inside FileDetail. Persisted in localStorage so the
// chosen layout survives reloads. Pattern mirrors theme.svelte.ts.

const SHOW_TREE_KEY = 'ctx-show-tree';
const SHOW_SYMBOLS_KEY = 'ctx-show-symbols';

function readBool(key: string, def: boolean): boolean {
  if (typeof localStorage === 'undefined') return def;
  try {
    const v = localStorage.getItem(key);
    if (v === null) return def;
    return v === '1';
  } catch {
    return def;
  }
}

function writeBool(key: string, v: boolean): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(key, v ? '1' : '0');
  } catch {
    // ignore (storage blocked / quota)
  }
}

export const view = $state<{ showTree: boolean; showSymbols: boolean }>({
  showTree: readBool(SHOW_TREE_KEY, true),
  showSymbols: readBool(SHOW_SYMBOLS_KEY, true),
});

export function toggleTree(): void {
  view.showTree = !view.showTree;
  writeBool(SHOW_TREE_KEY, view.showTree);
}

export function toggleSymbols(): void {
  view.showSymbols = !view.showSymbols;
  writeBool(SHOW_SYMBOLS_KEY, view.showSymbols);
}
