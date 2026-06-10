// Repo-level metadata that is constant for a session. Populated once the
// TreeView's initial /api/tree response arrives. Kept separate from
// tree-state.svelte (which tracks expanded/collapsed UI bits) because this
// value is server-truth, not per-user view state.

interface RepoState {
  root: string; // absolute filesystem root of the served project, e.g. "/Users/foo/repo"
}

export const repo = $state<RepoState>({ root: '' });

export function setRepoRoot(root: string): void {
  repo.root = root;
}

// Join the repo root with a root-relative path to produce an absolute path.
// Returns the empty string if the root is not yet known. Empty/`.` relPath
// yields the root itself (matches how the tree backend treats the root node).
export function absolutePath(relPath: string): string {
  if (!repo.root) return '';
  if (!relPath || relPath === '.') return repo.root;
  return `${repo.root}/${relPath}`;
}
