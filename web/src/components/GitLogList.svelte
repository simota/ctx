<script lang="ts">
  import { formatRelative, basename } from '../lib/format';
  import { announce } from '../lib/announce.svelte';
  import { route, navigate, toGitLogHash } from '../lib/router.svelte';
  import { gitlog, loadGitLog } from '../lib/gitlog.svelte';
  import { computeGraph, type GraphEdge } from '../lib/git-graph';
  import {
    fetchBranches,
    fetchWorktrees,
    type BranchEntry,
    type WorktreeEntry,
  } from '../lib/api';

  const LIMIT = 100;
  const ROW_H = 46; // fixed row height so graph rails align across rows
  const LANE_W = 14;
  const PALETTE = ['#4e9cf6', '#36b37e', '#f2b53d', '#e35d6a', '#9d7be8', '#46c4d4', '#e8884a', '#8bbf4a'];

  let commits = $derived(gitlog.commits);
  let truncated = $derived(gitlog.truncated);
  let loading = $derived(gitlog.loading);
  let error = $derived(gitlog.error);

  let graph = $derived(computeGraph(commits));
  let graphWidth = $derived(
    (graph.length ? Math.max(...graph.map((r) => r.width)) : 1) * LANE_W,
  );

  function laneX(lane: number): number {
    return lane * LANE_W + LANE_W / 2;
  }
  function laneColor(i: number): string {
    return PALETTE[((i % PALETTE.length) + PALETTE.length) % PALETTE.length];
  }
  // Straight line for a same-lane rail; a smooth elbow when lanes differ.
  function edgePath(e: GraphEdge): string {
    const x1 = laneX(e.fromLane);
    const y1 = e.fromY * ROW_H;
    const x2 = laneX(e.toLane);
    const y2 = e.toY * ROW_H;
    if (x1 === x2) return `M${x1} ${y1} L${x2} ${y2}`;
    const my = (y1 + y2) / 2;
    return `M${x1} ${y1} C${x1} ${my} ${x2} ${my} ${x2} ${y2}`;
  }

  // Ref selector: branches + worktrees a user can re-base the log on. Fetch
  // failures just hide the selector — they never block the default HEAD log.
  let branches = $state<BranchEntry[]>([]);
  let worktrees = $state<WorktreeEntry[]>([]);
  let selectedRef = $state('');

  $effect(() => {
    void loadGitLog(LIMIT);
  });

  $effect(() => {
    void (async () => {
      try {
        const [b, w] = await Promise.all([fetchBranches(), fetchWorktrees()]);
        branches = b.branches;
        worktrees = w.worktrees.filter((wt) => !wt.bare);
      } catch {
        branches = [];
        worktrees = [];
      }
    })();
  });

  // value (=ref) for a worktree: its branch, else the detached short hash.
  function worktreeRef(w: WorktreeEntry): string {
    return w.branch ?? w.head ?? '';
  }
  function worktreeLabel(w: WorktreeEntry): string {
    const name = basename(w.path);
    const at = w.branch ?? (w.head ? `${w.head} (detached)` : 'detached');
    return `${name} — ${at}`;
  }

  async function onRefChange(): Promise<void> {
    await loadGitLog(LIMIT, selectedRef || null, true);
    // The previously selected hash may not exist on the new ref; re-select the
    // newest commit so the detail pane never points at a stale hash.
    if (gitlog.commits.length > 0) {
      navigate(toGitLogHash(gitlog.commits[0].hash_full));
    }
  }

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
    {#if branches.length > 0 || worktrees.length > 0}
      <select
        class="ref-select"
        aria-label="Show log for"
        bind:value={selectedRef}
        onchange={onRefChange}
      >
        <option value="">HEAD (default)</option>
        {#if branches.length > 0}
          <optgroup label="Branches">
            {#each branches as b (b.name)}
              <option value={b.name}>{b.name}{b.current ? ' ●' : ''}</option>
            {/each}
          </optgroup>
        {/if}
        {#if worktrees.length > 1}
          <optgroup label="Worktrees">
            {#each worktrees as w (w.path)}
              <option value={worktreeRef(w)}>{worktreeLabel(w)}</option>
            {/each}
          </optgroup>
        {/if}
      </select>
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
      {#each commits as c, i (c.hash_full)}
        <li
          class="row-wrap"
          class:active={route.path === c.hash_full}
          style="height: {ROW_H}px"
        >
          <svg
            class="graph"
            width={graphWidth}
            height={ROW_H}
            viewBox="0 0 {graphWidth} {ROW_H}"
            aria-hidden="true"
          >
            {#each graph[i]?.edges ?? [] as e (`${e.fromLane}-${e.fromY}-${e.toLane}-${e.toY}`)}
              <path d={edgePath(e)} stroke={laneColor(e.color)} stroke-width="1.5" fill="none" />
            {/each}
            {#if graph[i]}
              <circle
                cx={laneX(graph[i].dotLane)}
                cy={ROW_H / 2}
                r="4"
                fill={laneColor(graph[i].dotColor)}
                stroke="var(--ctx-bg-panel, var(--ctx-bg))"
                stroke-width="1.5"
              />
            {/if}
          </svg>
          <button
            type="button"
            class="row"
            aria-current={route.path === c.hash_full ? 'true' : undefined}
            onclick={() => select(c.hash_full)}
          >
            <span class="subject" title={c.subject}>{c.subject}</span>
            <span class="meta">
              <span class="author" title={c.author}>{c.author}</span>
              <span class="sep" aria-hidden="true">·</span>
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
  .ref-select {
    margin-left: auto;
    max-width: 16ch;
    font-size: 0.78em;
    color: var(--ctx-fg);
    background: var(--ctx-bg-elev, var(--ctx-bg));
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 2px 4px;
    cursor: pointer;
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
  .row-wrap {
    display: flex;
    align-items: stretch;
    border-left: 2px solid transparent;
  }
  .row-wrap:hover {
    background: var(--ctx-bg-elev);
  }
  .row-wrap.active {
    background: var(--ctx-bg-elev);
    border-left-color: var(--ctx-accent);
  }
  .graph {
    flex: 0 0 auto;
    display: block;
  }
  .row {
    display: flex;
    flex-direction: column;
    justify-content: center;
    gap: 2px;
    flex: 1 1 auto;
    min-width: 0;
    text-align: left;
    background: transparent;
    border: 0;
    color: var(--ctx-fg);
    padding: 4px 12px 4px 4px;
    cursor: pointer;
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
