// Cmd-P / Ctrl-P fuzzy file finder state.
// Flattens the tree once on first open and caches the file path list; the tree
// is re-fetched on demand (refresh()) but typical sessions hit the cache.

import { fetchTree, type TreeNode } from './api';
import { announce } from './announce.svelte';
import { treeState } from './tree-state.svelte';

export interface FinderState {
  open: boolean;
  query: string;
  selectedIndex: number;
  files: string[];
  loading: boolean;
  error: string | null;
}

export const finder = $state<FinderState>({
  open: false,
  query: '',
  selectedIndex: 0,
  files: [],
  loading: false,
  error: null,
});

let lastFocus: HTMLElement | null = null;
let loadPromise: Promise<void> | null = null;
// reloadKey the cached file list was built from. A tree reload (Shift+R / ↻)
// bumps treeState.reloadKey, which marks the cache stale; the next open
// revalidates in the background while the old list stays visible.
let loadedReloadKey = -1;

function flatten(node: TreeNode, out: string[]): void {
  if (!node.is_dir) {
    out.push(node.path);
  }
  if (node.children) {
    for (const c of node.children) flatten(c, out);
  }
}

export function loadFiles(force = false): Promise<void> {
  if (!force && finder.files.length > 0 && loadedReloadKey === treeState.reloadKey) {
    return Promise.resolve();
  }
  if (loadPromise) return loadPromise;
  const reloadKey = treeState.reloadKey;
  finder.loading = true;
  finder.error = null;
  loadPromise = fetchTree({ depth: 32, tokens: false, git: false })
    .then((r) => {
      const out: string[] = [];
      flatten(r.tree, out);
      finder.files = out;
      loadedReloadKey = reloadKey;
    })
    .catch((e: unknown) => {
      finder.error = e instanceof Error ? e.message : String(e);
    })
    .finally(() => {
      finder.loading = false;
      loadPromise = null;
    });
  return loadPromise;
}

export function openFinder(): void {
  if (finder.open) return;
  lastFocus = (document.activeElement as HTMLElement) ?? null;
  finder.open = true;
  finder.query = '';
  finder.selectedIndex = 0;
  void loadFiles();
  announce('File finder opened');
}

export function closeFinder(): void {
  if (!finder.open) return;
  finder.open = false;
  finder.query = '';
  finder.selectedIndex = 0;
  announce('File finder closed');
  // Return focus to the element that owned it pre-open.
  const target = lastFocus;
  lastFocus = null;
  if (target && typeof target.focus === 'function') {
    queueMicrotask(() => target.focus());
  }
}

export function refreshFiles(): Promise<void> {
  return loadFiles(true);
}
