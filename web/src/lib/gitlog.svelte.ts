// Shared git-log state — fetched once and consumed by both the left commit
// list (GitLogList) and the right detail pane (GitCommitDetail) so deep links
// to `#/gitlog/<hash>` resolve commit metadata without a second round-trip.

import { fetchGitLog, ApiCallError, type FileLogEntry } from './api';

export const gitlog = $state<{
  commits: FileLogEntry[];
  truncated: boolean;
  loading: boolean;
  error: string | null;
  loaded: boolean;
}>({ commits: [], truncated: false, loading: false, error: null, loaded: false });

export async function loadGitLog(limit = 100, force = false): Promise<void> {
  if ((gitlog.loaded || gitlog.loading) && !force) return;
  gitlog.loading = true;
  gitlog.error = null;
  try {
    const r = await fetchGitLog(limit);
    gitlog.commits = r.commits;
    gitlog.truncated = r.truncated;
    gitlog.loaded = true;
  } catch (e) {
    gitlog.error = e instanceof ApiCallError ? e.message : 'Failed to load git log.';
  } finally {
    gitlog.loading = false;
  }
}

export function findCommit(hashFull: string): FileLogEntry | undefined {
  return gitlog.commits.find((c) => c.hash_full === hashFull);
}
