// Shared co-change state — the file-relationship network for the git-log
// "Relations" view. Keyed by `since` so switching the date filter forces a
// reload; an empty repo or `nodes: []` resolves cleanly (no graph, no error).

import { fetchCoChange, ApiCallError, type CoChangeResponse } from './api';

export const cochange = $state<{
  data: CoChangeResponse | null;
  loading: boolean;
  error: string | null;
  since: string; // the `since` the current `data` was loaded with
}>({ data: null, loading: false, error: null, since: '' });

// Monotonic request token so a superseding call (new `since` while one is
// still in flight) wins: a completion only applies its result if it's still
// the latest request, instead of being silently dropped by the `loading`
// guard.
let requestToken = 0;

// `since` filters the scan window (''=all). A changed `since` forces a reload.
export async function loadCoChange(
  limit = 500,
  since = '',
  minWeight = 2,
): Promise<void> {
  if (cochange.data && cochange.since === since && !cochange.loading) return;
  if (cochange.loading && cochange.since === since) return;
  const token = ++requestToken;
  cochange.loading = true;
  cochange.error = null;
  cochange.since = since;
  try {
    const data = await fetchCoChange(limit, since || undefined, minWeight);
    if (token !== requestToken) return; // superseded by a newer request
    cochange.data = data;
  } catch (e) {
    if (token !== requestToken) return;
    cochange.data = null;
    cochange.error = e instanceof ApiCallError ? e.message : 'Failed to load relations.';
  } finally {
    if (token === requestToken) cochange.loading = false;
  }
}
