// Shared tree state for TreeView (expansion + roving focus + reveal command queue).
// Lives outside TreeView so peers (FileDetail breadcrumb, future package overview)
// can request a reveal without prop-drilling.

interface RevealRequest {
  path: string;
  // Monotonic key so identical paths re-trigger the $effect in TreeView.
  key: number;
}

interface TreeState {
  expanded: Set<string>;
  focusedPath: string | null;
  revealRequest: RevealRequest | null;
  // Monotonic counter — peers (App-level shortcut, future commands) bump it
  // to request a tree refetch. TreeView's $effect re-runs when this changes
  // and calls its local load(). Starts at 0; TreeView ignores the seed value
  // so the initial mount fetch isn't double-fired.
  reloadKey: number;
}

// Exported as an object so $state proxy semantics survive `import` boundaries.
// Importing a bare `$state` value into another module loses reactivity — the
// consumer ends up with a snapshot `let`, not the live signal.
export const treeState = $state<TreeState>({
  expanded: new Set<string>(),
  focusedPath: null,
  revealRequest: null,
  reloadKey: 0,
});

let nextRevealKey = 1;
let nextReloadKey = 1;

// Bump reloadKey so TreeView refetches. Returns the new key for callers
// that want to correlate (e.g. announce + log).
export function reloadTree(): number {
  const k = nextReloadKey++;
  treeState.reloadKey = k;
  return k;
}

export function setExpanded(path: string, value: boolean): void {
  const next = new Set(treeState.expanded);
  if (value) next.add(path);
  else next.delete(path);
  treeState.expanded = next;
}

// Add every ancestor of `path` to `expanded` and fire a fresh revealRequest.
// Root path `.` is always preserved so the tree root row stays open.
export function revealPath(path: string): void {
  if (typeof window === 'undefined') return;
  if (!path) return;
  const next = new Set(treeState.expanded);
  next.add('.');
  const segs = path.split('/').filter((s) => s.length > 0);
  // Accumulate ancestors: ['internal', 'internal/cli'] for 'internal/cli/audit.go'.
  // The final segment (the leaf itself) is not added — leaf may be a file.
  let acc = '';
  for (let i = 0; i < segs.length - 1; i++) {
    acc = acc === '' ? segs[i] : `${acc}/${segs[i]}`;
    next.add(acc);
  }
  treeState.expanded = next;
  treeState.revealRequest = { path, key: nextRevealKey++ };
}
