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

// `since` filters the scan window (''=all). A changed `since` forces a reload.
export async function loadCoChange(
  limit = 500,
  since = '',
  minWeight = 2,
): Promise<void> {
  if (cochange.data && cochange.since === since && !cochange.loading) return;
  if (cochange.loading) return;
  cochange.loading = true;
  cochange.error = null;
  cochange.since = since;
  try {
    cochange.data = await fetchCoChange(limit, since || undefined, minWeight);
  } catch (e) {
    cochange.data = null;
    cochange.error = e instanceof ApiCallError ? e.message : 'Failed to load relations.';
  } finally {
    cochange.loading = false;
  }
}
