// Tab state — array of open file paths (insertion order = visual tab order).
// Session-only: no localStorage. URL `?open=A,B,C` seeds tabs at startup.

interface TabsState {
  paths: string[];
}

export const tabs = $state<TabsState>({ paths: [] });

// openTab: append `path` if not already present. No-op on duplicate.
export function openTab(path: string): void {
  if (!path) return;
  if (tabs.paths.includes(path)) return;
  tabs.paths.push(path);
}

// closeTab: remove `path` and return the path that should become active next
// (right neighbour preferred, then left, then `null` when nothing remains).
export function closeTab(path: string): string | null {
  const idx = tabs.paths.indexOf(path);
  if (idx === -1) return tabs.paths[0] ?? null;
  tabs.paths.splice(idx, 1);
  if (tabs.paths.length === 0) return null;
  // right neighbour first (idx now points at what was right), else left.
  if (idx < tabs.paths.length) return tabs.paths[idx];
  return tabs.paths[tabs.paths.length - 1];
}

// moveTab: reorder. Bounds-safe; out-of-range indices are clamped.
export function moveTab(fromIdx: number, toIdx: number): void {
  const n = tabs.paths.length;
  if (n < 2) return;
  if (fromIdx < 0 || fromIdx >= n) return;
  const to = Math.max(0, Math.min(n - 1, toIdx));
  if (fromIdx === to) return;
  const [moved] = tabs.paths.splice(fromIdx, 1);
  tabs.paths.splice(to, 0, moved);
}

// clearTabs: drop all (debug / "close all" affordance).
export function clearTabs(): void {
  tabs.paths.length = 0;
}

// setTabs: replace the whole list (used to seed from URL ?open=).
export function setTabs(paths: string[]): void {
  const seen = new Set<string>();
  const out: string[] = [];
  for (const p of paths) {
    if (!p || seen.has(p)) continue;
    seen.add(p);
    out.push(p);
  }
  tabs.paths = out;
}
