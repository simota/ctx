<script lang="ts">
  import { formatRelative } from '../lib/format';
  import { announce } from '../lib/announce.svelte';
  import { route, navigate, toGitLogHash } from '../lib/router.svelte';
  import { gitlog, loadGitLog } from '../lib/gitlog.svelte';

  const LIMIT = 100;

  let commits = $derived(gitlog.commits);
  let truncated = $derived(gitlog.truncated);
  let loading = $derived(gitlog.loading);
  let error = $derived(gitlog.error);

  $effect(() => {
    void loadGitLog(LIMIT);
  });

  // Announce once after load; auto-select the newest commit when none is
  // selected (e.g. opened via the nav, not a deep link).
  $effect(() => {
    if (!gitlog.loaded) return;
    announce(`${commits.length} commit${commits.length === 1 ? '' : 's'} loaded`);
    if (!route.path && commits.length > 0) {
      navigate(toGitLogHash(commits[0].hash_full));
    }
  });

  function select(hash: string): void {
    navigate(toGitLogHash(hash));
  }
</script>

<nav class="gitlog-list" aria-label="git log commits">
  <header>
    <h2>Git Log</h2>
    {#if commits.length > 0}
      <span class="muted count">({commits.length}{truncated ? '+' : ''})</span>
    {/if}
  </header>

  {#if loading}
    <p class="muted note" aria-busy="true">Loading…</p>
  {:else if error}
    <p class="note err"><code class="mono">{error}</code></p>
  {:else if commits.length === 0}
    <p class="muted note">No commit history.</p>
  {:else}
    <ul role="list">
      {#each commits as c (c.hash_full)}
        <li>
          <button
            type="button"
            class="row"
            class:active={route.path === c.hash_full}
            aria-current={route.path === c.hash_full ? 'true' : undefined}
            onclick={() => select(c.hash_full)}
          >
            <span class="subject" title={c.subject}>{c.subject}</span>
            <span class="meta">
              <span class="author" title={c.author}>{c.author}</span>
              <span class="dot" aria-hidden="true">·</span>
              <span class="date" title={new Date(c.date * 1000).toISOString()}>{formatRelative(c.date)}</span>
              <span class="hash mono">{c.hash}</span>
            </span>
          </button>
        </li>
      {/each}
    </ul>
    {#if truncated}
      <p class="muted note">Truncated at {LIMIT} commits.</p>
    {/if}
  {/if}
</nav>

<style>
  .gitlog-list {
    display: flex;
    flex-direction: column;
    height: 100%;
    overflow: auto;
  }
  header {
    display: flex;
    align-items: baseline;
    gap: 8px;
    padding: 10px 12px 8px;
    position: sticky;
    top: 0;
    background: var(--ctx-bg-panel, var(--ctx-bg));
    border-bottom: 1px solid var(--ctx-border);
    z-index: 1;
  }
  h2 {
    font-size: 0.95em;
    margin: 0;
  }
  .muted {
    color: var(--ctx-fg-dim);
  }
  .count {
    font-size: 0.82em;
  }
  .note {
    padding: 8px 12px;
    font-size: 0.85em;
  }
  .err {
    color: var(--ctx-err);
  }
  .mono {
    font-family: var(--ctx-font-mono, var(--ctx-mono, ui-monospace, monospace));
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .row {
    display: flex;
    flex-direction: column;
    gap: 3px;
    width: 100%;
    text-align: left;
    background: transparent;
    border: 0;
    border-left: 2px solid transparent;
    color: var(--ctx-fg);
    padding: 7px 12px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
  }
  .row.active {
    background: var(--ctx-bg-elev);
    border-left-color: var(--ctx-accent);
  }
  .subject {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 0.9em;
  }
  .meta {
    display: flex;
    align-items: baseline;
    gap: 5px;
    font-size: 0.76em;
    color: var(--ctx-fg-dim);
  }
  .author {
    max-width: 10ch;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .date {
    white-space: nowrap;
  }
  .hash {
    margin-left: auto;
    color: var(--ctx-accent);
  }
</style>
