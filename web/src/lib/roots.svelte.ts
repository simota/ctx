// State for the project-root registry view. Distinct from `repo.svelte.ts`
// (which holds the absolute root of the *currently-served* project) — this
// store is the list of *all* known roots, fetched lazily when the RootsPicker
// opens. We deliberately do not cache to localStorage: the registry lives on
// disk on the server side and may be edited by `ctx roots add` from a
// terminal between picker invocations.

import { listRoots, type RootEntry, ApiCallError } from './api';
import { repo } from './repo.svelte';

interface RootsState {
  entries: RootEntry[];
  loaded: boolean;
  loading: boolean;
  // Inline error surfaced inside the picker. Cleared on each successful
  // load so transient failures don't stick around forever.
  error: string | null;
}

export const roots = $state<RootsState>({
  entries: [],
  loaded: false,
  loading: false,
  error: null,
});

// Sort entries MRU (most-recently-opened first). Missing / unparseable /
// Go-zero-value (`0001-...`) timestamps sink to the bottom; name is a stable
// tie-breaker so the order doesn't jitter between equal-time entries.
function sortMru(entries: RootEntry[]): RootEntry[] {
  return [...entries].sort((a, b) => {
    const ta = parseOpenedAt(a.last_opened_at);
    const tb = parseOpenedAt(b.last_opened_at);
    if (tb !== ta) return tb - ta;
    return a.name.localeCompare(b.name);
  });
}

function parseOpenedAt(iso: string | undefined): number {
  if (!iso) return 0;
  if (iso.startsWith('0001-')) return 0;
  const t = Date.parse(iso);
  return Number.isNaN(t) ? 0 : t;
}

export async function loadRoots(): Promise<void> {
  // Re-entrancy guard: avoid hammering the endpoint when the picker is
  // re-opened in quick succession.
  if (roots.loading) return;
  roots.loading = true;
  roots.error = null;
  try {
    const res = await listRoots();
    roots.entries = sortMru(res.roots ?? []);
    roots.loaded = true;
  } catch (e) {
    const msg =
      e instanceof ApiCallError
        ? `Failed to load roots: ${e.message}`
        : e instanceof Error
          ? `Failed to load roots: ${e.message}`
          : 'Failed to load roots.';
    roots.error = msg;
    // Leave `loaded` as-is: if we had a successful prior load, the stale
    // entries stay visible alongside the inline error banner.
  } finally {
    roots.loading = false;
  }
}

// Return the registry entry whose `path` matches the absolute root of the
// project this UI is currently serving. We do a byte-for-byte comparison —
// the server normalizes both sides to absolute paths so this is sufficient
// without OS-level path-normalization heuristics in the browser.
export function currentRoot(): RootEntry | null {
  const here = repo.root;
  if (!here) return null;
  for (const r of roots.entries) {
    if (r.path === here) return r;
  }
  return null;
}
