// Mix selection state — tracks which repo-relative file paths are included
// in the next Bounce (save as .mix). Lives outside components so TreeNode,
// StatusBar, and BounceDialog all see the same reactive object.
//
// Mutation pattern: always REPLACE the Set with a new reference so Svelte 5
// $state detects the change (Set mutation is transparent to the proxy).
// This mirrors how treeState.expanded works in tree-state.svelte.ts.

interface MixSelection {
  includedPaths: Set<string>;
}

export const mixSelection = $state<MixSelection>({ includedPaths: new Set() });

export function toggleInclude(path: string): void {
  const prev = mixSelection.includedPaths;
  if (prev.has(path)) {
    const next = new Set(prev);
    next.delete(path);
    mixSelection.includedPaths = next;
  } else {
    mixSelection.includedPaths = new Set([...prev, path]);
  }
}

export function clearSelection(): void {
  mixSelection.includedPaths = new Set();
}

export function isIncluded(path: string): boolean {
  return mixSelection.includedPaths.has(path);
}

// Replace the entire selection — used by Recall to restore a saved mix.
export function setSelection(paths: string[]): void {
  mixSelection.includedPaths = new Set(paths);
}
