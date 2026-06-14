<script lang="ts">
  import {
    fetchBranches,
    fetchWorktrees,
    fetchTags,
    ApiCallError,
    type BranchEntry,
    type WorktreeEntry,
    type TagEntry,
  } from '../lib/api';
  import { basename, formatRelative } from '../lib/format';

  let branches = $state<BranchEntry[]>([]);
  let worktrees = $state<WorktreeEntry[]>([]);
  let tags = $state<TagEntry[]>([]);
  let error = $state<string | null>(null);

  let branchesOpen = $state(false);
  let worktreesOpen = $state(false);
  let tagsOpen = $state(false);

  let current = $derived(branches.find((b) => b.current));

  $effect(() => {
    let cancelled = false;
    Promise.all([fetchBranches(), fetchWorktrees(), fetchTags()])
      .then(([b, w, t]) => {
        if (cancelled) return;
        branches = b.branches;
        worktrees = w.worktrees;
        tags = t.tags;
      })
      .catch((e) => {
        if (cancelled) return;
        error = e instanceof ApiCallError ? e.message : 'Failed to load refs.';
      });
    return () => {
      cancelled = true;
    };
  });

  function aheadBehind(b: BranchEntry): string {
    const parts: string[] = [];
    if (b.ahead) parts.push(`↑${b.ahead}`);
    if (b.behind) parts.push(`↓${b.behind}`);
    return parts.join(' ');
  }

  function worktreeLabel(w: WorktreeEntry): string {
    if (w.bare) return 'bare';
    if (w.detached || !w.branch) return `detached @ ${w.head ?? ''}`;
    return w.branch;
  }
</script>

<section class="refs" aria-label="git branches and worktrees">
  {#if error}
    <p class="muted note err"><code class="mono">{error}</code></p>
  {:else}
    <div class="current" title={current?.name}>
      <span class="branch-icon" aria-hidden="true">⎇</span>
      <span class="current-name mono">{current?.name ?? 'detached HEAD'}</span>
    </div>

    {#if branches.length > 0}
      <details class="group" bind:open={branchesOpen}>
        <summary>
          <span class="twisty" aria-hidden="true">{branchesOpen ? '▾' : '▸'}</span>
          Branches <span class="muted count">({branches.length})</span>
        </summary>
        <ul role="list">
          {#each branches as b (b.name)}
            <li class="ref-row branch" class:active={b.current} title={b.subject}>
              <span class="dot" class:on={b.current} aria-hidden="true"></span>
              <span class="name mono">{b.name}</span>
              {#if aheadBehind(b)}
                <span class="ab" title={`${b.ahead ?? 0} ahead / ${b.behind ?? 0} behind HEAD`}>{aheadBehind(b)}</span>
              {/if}
              {#if b.date}
                <span class="when" title={new Date(b.date * 1000).toISOString()}>{formatRelative(b.date)}</span>
              {/if}
              <span class="hash mono">{b.hash}</span>
            </li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if tags.length > 0}
      <details class="group" bind:open={tagsOpen}>
        <summary>
          <span class="twisty" aria-hidden="true">{tagsOpen ? '▾' : '▸'}</span>
          Tags <span class="muted count">({tags.length})</span>
        </summary>
        <ul role="list">
          {#each tags as t (t.name)}
            <li class="ref-row">
              <span class="tag-icon" aria-hidden="true">🏷</span>
              <span class="name mono" title={t.name}>{t.name}</span>
              {#if t.date}
                <span class="when" title={new Date(t.date * 1000).toISOString()}>{formatRelative(t.date)}</span>
              {/if}
              <span class="hash mono">{t.hash}</span>
            </li>
          {/each}
        </ul>
      </details>
    {/if}

    {#if worktrees.length > 0}
      <details class="group" bind:open={worktreesOpen}>
        <summary>
          <span class="twisty" aria-hidden="true">{worktreesOpen ? '▾' : '▸'}</span>
          Worktrees <span class="muted count">({worktrees.length})</span>
        </summary>
        <ul role="list">
          {#each worktrees as w (w.path)}
            <li class="ref-row wt">
              <span class="name mono" title={w.path}>{basename(w.path) || w.path}</span>
              <span class="wt-branch" class:detached={w.detached || w.bare}>{worktreeLabel(w)}</span>
            </li>
          {/each}
        </ul>
      </details>
    {/if}
  {/if}
</section>

<style>
  .refs {
    border-bottom: 1px solid var(--ctx-border);
    padding: 8px 12px;
  }
  .muted {
    color: var(--ctx-fg-dim);
  }
  .mono {
    font-family: var(--ctx-font-mono, var(--ctx-mono, ui-monospace, monospace));
  }
  .note {
    font-size: 0.82em;
    margin: 0;
  }
  .err {
    color: var(--ctx-err);
  }
  .current {
    display: flex;
    align-items: center;
    gap: 6px;
    font-size: 0.9em;
    margin-bottom: 4px;
  }
  .branch-icon {
    color: var(--ctx-accent);
  }
  .current-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-weight: 600;
  }
  .group {
    font-size: 0.82em;
  }
  summary {
    cursor: pointer;
    list-style: none;
    padding: 3px 0;
    color: var(--ctx-fg-dim);
    user-select: none;
  }
  summary::-webkit-details-marker {
    display: none;
  }
  .twisty {
    display: inline-block;
    width: 1em;
    font-size: 0.85em;
  }
  .count {
    font-size: 0.95em;
  }
  ul {
    list-style: none;
    margin: 0 0 4px;
    padding: 0 0 0 1.2em;
  }
  .ref-row {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 2px 0;
  }
  .ref-row.active .name {
    color: var(--ctx-accent);
    font-weight: 600;
  }
  .dot {
    flex: 0 0 auto;
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: transparent;
    border: 1px solid var(--ctx-border);
  }
  .dot.on {
    background: var(--ctx-git-added, var(--ctx-accent));
    border-color: transparent;
  }
  .name {
    flex: 1 1 auto;
    min-width: 0;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .hash {
    flex: 0 0 auto;
    color: var(--ctx-accent);
    font-size: 0.92em;
  }
  .ab {
    flex: 0 0 auto;
    font-size: 0.88em;
    color: var(--ctx-fg-dim);
    font-variant-numeric: tabular-nums;
    white-space: nowrap;
  }
  .when {
    flex: 0 0 auto;
    font-size: 0.85em;
    color: var(--ctx-fg-dim);
    white-space: nowrap;
  }
  .tag-icon {
    flex: 0 0 auto;
    font-size: 0.85em;
  }
  /* On branch rows the name yields space first so the hash/date/ahead-behind
     stay visible. */
  .ref-row.branch .name {
    flex: 1 1 auto;
  }
  .wt-branch {
    flex: 0 0 auto;
    color: var(--ctx-fg-dim);
    font-size: 0.92em;
  }
  .wt-branch.detached {
    font-style: italic;
  }
</style>
