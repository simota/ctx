<script lang="ts">
  import hljs from '../lib/highlight';
  import { fetchDir, type DirResponse } from '../lib/api';
  import {
    formatTokens,
    formatSize,
    gitColor,
    gitStatusName,
    langFromPath,
  } from '../lib/format';
  import { navigate, toDirHash, toFileHash } from '../lib/router.svelte';
  import { announce } from '../lib/announce.svelte';

  let { path } = $props<{ path: string }>();

  let data = $state<DirResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let headingEl: HTMLHeadingElement | null = $state(null);
  // Tracks the path for which we have already announced to suppress repeats
  // on subsequent reactive runs that re-read the same `data`.
  let announcedFor: string | null = $state(null);

  // Breadcrumb segments. Root crumb ("Root") always appears first and links to
  // `#/dir`. For non-root paths, each ancestor segment links to its own
  // `#/dir/<ancestor>`. The last crumb is non-clickable (current dir).
  let crumbs = $derived.by<{ name: string; path: string; last: boolean }[]>(() => {
    const out: { name: string; path: string; last: boolean }[] = [];
    const segs = path ? path.split('/').filter((s: string) => s.length > 0) : [];
    out.push({ name: 'Root', path: '', last: segs.length === 0 });
    let acc = '';
    for (let i = 0; i < segs.length; i++) {
      acc = acc === '' ? segs[i] : `${acc}/${segs[i]}`;
      out.push({ name: segs[i], path: acc, last: i === segs.length - 1 });
    }
    return out;
  });

  // WHY explicit `string` typing on `p`: the consumer reads `path` from
  // `$props()` which infers as `unknown` without `$props<{...}>` shape, but
  // even with the shape the closure capture in the load promise needs a stable
  // string identity to avoid stale-state reads.
  // Epoch counter so a slow earlier response can't overwrite a newer
  // directory's data after rapid navigation.
  let loadEpoch = 0;

  function load(p: string) {
    const epoch = ++loadEpoch;
    loading = true;
    error = null;
    data = null;
    fetchDir(p)
      .then((r) => {
        if (epoch !== loadEpoch) return;
        data = r;
      })
      .catch((e: unknown) => {
        if (epoch !== loadEpoch) return;
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (epoch === loadEpoch) loading = false;
      });
  }

  $effect(() => {
    load(path);
  });

  // Announce once per successful load. We key on `data.path` rather than the
  // route `path` prop so that two rapid navigations to the same directory do
  // not double-announce, and a switch between dirs always re-announces.
  $effect(() => {
    if (!data) return;
    if (announcedFor === data.path) return;
    announcedFor = data.path;
    const label = data.name || 'Root';
    announce(`Directory ${label} with ${data.file_count} files`);
    queueMicrotask(() => headingEl?.focus());
  });

  function onCrumbClick(crumbPath: string) {
    navigate(toDirHash(crumbPath));
  }

  function onChildClick(child: DirResponse['children'][number]) {
    if (child.is_dir) {
      navigate(toDirHash(child.path));
    } else {
      navigate(toFileHash(child.path));
    }
  }

  // README rendering: hljs auto-detect produces decent inline-code coloring
  // for markdown samples without us shipping a markdown parser. For non-text
  // edge cases hljs returns empty html — we fall back to escaped raw text.
  let readmeHtml = $derived.by<string>(() => {
    if (!data?.readme) return '';
    const src = data.readme;
    // Prefer language inferred from `readme_path` (e.g. .md -> markdown). When
    // hljs lacks the language, fall back to auto-detect.
    const lang = data.readme_path ? langFromPath(data.readme_path) : 'markdown';
    try {
      if (lang && hljs.getLanguage(lang)) {
        return hljs.highlight(src, { language: lang, ignoreIllegals: true }).value;
      }
      return hljs.highlightAuto(src).value;
    } catch {
      return escapeHtml(src);
    }
  });

  function escapeHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  // Aggregate git counters into an iterable row for the stats grid. Zero
  // counts are omitted so the badge area stays compact on clean trees.
  interface GitBadge {
    kind: 'M' | 'A' | 'D' | '?';
    count: number;
    label: string;
  }
  let gitBadges = $derived.by<GitBadge[]>(() => {
    const g = data?.git;
    if (!g) return [];
    const out: GitBadge[] = [];
    if (g.modified) out.push({ kind: 'M', count: g.modified, label: 'modified' });
    if (g.added) out.push({ kind: 'A', count: g.added, label: 'added' });
    if (g.deleted) out.push({ kind: 'D', count: g.deleted, label: 'deleted' });
    if (g.untracked) out.push({ kind: '?', count: g.untracked, label: 'untracked' });
    return out;
  });

  // Children sorted: dirs first, then files; alphabetical within each group.
  // Server may already sort, but enforcing here keeps UI stable across API
  // implementations and against potential out-of-order responses.
  let sortedChildren = $derived.by(() => {
    if (!data) return [];
    const arr = [...data.children];
    arr.sort((a, b) => {
      if (a.is_dir !== b.is_dir) return a.is_dir ? -1 : 1;
      return a.name.localeCompare(b.name);
    });
    return arr;
  });
</script>

<section
  class="dir-overview"
  aria-label="Directory overview"
>
  <header class="head">
    <nav class="breadcrumb mono" aria-label="directory path">
      {#each crumbs as crumb, i (i + ':' + crumb.path)}
        {#if crumb.last}
          <span class="crumb current" aria-current="page">{crumb.name}</span>
        {:else}
          <button
            type="button"
            class="crumb"
            onclick={() => onCrumbClick(crumb.path)}
            aria-label={crumb.path === '' ? 'Go to root directory' : `Go to ${crumb.path}`}
          >{crumb.name}</button>
          <span class="sep" aria-hidden="true">/</span>
        {/if}
      {/each}
    </nav>
    {#if data}
      <h2 bind:this={headingEl} tabindex="-1" class="title">
        {data.name || 'Root'}
      </h2>
    {/if}
  </header>

  {#if loading && !data}
    <div class="loading" aria-busy="true">
      <span class="skel" style="width: 40%; height: 14px;"></span>
      <span class="skel" style="width: 70%; height: 12px;"></span>
      <span class="skel" style="width: 60%; height: 12px;"></span>
    </div>
  {:else if error}
    <div class="error">
      <p>Directory load failed.</p>
      <code class="mono">{error}</code>
      <button onclick={() => load(path)}>Retry</button>
    </div>
  {:else if data}
    <dl class="stats" aria-label="directory stats">
      <div title="Approx LLM tokens (cl100k_base), recursive across the tree.">
        <dt>tokens</dt>
        <dd>{formatTokens(data.tokens)}</dd>
      </div>
      <div>
        <dt>files</dt>
        <dd>{data.file_count}</dd>
      </div>
      <div>
        <dt>dirs</dt>
        <dd>{data.dir_count}</dd>
      </div>
      {#if gitBadges.length > 0}
        <div class="git-cell" aria-label="git status summary">
          <dt>git</dt>
          <dd class="git-badges">
            {#each gitBadges as b, bi (`${bi}:${b.kind}`)}
              <span
                class="badge"
                title={`${b.count} ${b.label}`}
                aria-label={`${b.count} ${b.label}`}
              >
                <span class="dot" style="background: {gitColor(b.kind)}" aria-hidden="true"></span>
                <span class="badge-count">{b.count}</span>
                <span class="badge-kind" aria-hidden="true">{b.kind}</span>
              </span>
            {/each}
          </dd>
        </div>
      {/if}
    </dl>

    <section class="readme-wrap" aria-label="readme">
      <h3>
        README
        {#if data.readme_path}
          <span class="muted mono readme-path" title={data.readme_path}>{data.readme_path}</span>
        {/if}
      </h3>
      {#if data.readme}
        <pre class="readme"><code class="hljs">{@html readmeHtml}</code></pre>
      {:else}
        <p class="muted empty">No README found in this directory.</p>
      {/if}
    </section>

    <section class="children-wrap" aria-label="directory contents">
      <h3>
        Contents
        <span class="muted">({sortedChildren.length})</span>
      </h3>
      {#if sortedChildren.length === 0}
        <p class="muted empty">This directory is empty.</p>
      {:else}
        <ul class="children" role="list">
          {#each sortedChildren as child, ci (`${ci}:${child.path}`)}
            <li>
              <button
                type="button"
                class="child-row"
                class:dir={child.is_dir}
                onclick={() => onChildClick(child)}
                aria-label={child.is_dir
                  ? `Open directory ${child.name}${child.git ? `, contains ${gitStatusName(child.git)}` : ''}`
                  : `Open file ${child.name}${child.git ? `, ${gitStatusName(child.git)}` : ''}`}
              >
                <span class="icon" aria-hidden="true">{child.is_dir ? '📁' : '📄'}</span>
                <span class="name mono">{child.name}</span>
                <span class="meta">
                  {#if child.tokens !== undefined && child.tokens > 0}
                    <span class="tokens" title="Approx LLM tokens (cl100k_base).">{formatTokens(child.tokens)}</span>
                  {/if}
                  {#if !child.is_dir}
                    <span class="size muted" title="File size">{formatSize(child.size)}</span>
                  {/if}
                  {#if child.git}
                    <span
                      class="child-git"
                      style="color: {gitColor(child.git)}"
                      title={gitStatusName(child.git)}
                      aria-hidden="true"
                    >{child.git}</span>
                  {/if}
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    </section>
  {/if}
</section>

<style>
  .dir-overview {
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 1100px;
    min-height: 0;
  }
  .head {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .breadcrumb {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0;
    font-size: 13px;
    line-height: 1.4;
  }
  .breadcrumb .crumb {
    padding: 1px 4px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--ctx-fg);
    font: inherit;
    cursor: pointer;
  }
  .breadcrumb .crumb:hover {
    color: var(--ctx-link);
    background: var(--ctx-bg-elev);
  }
  .breadcrumb .crumb:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
    color: var(--ctx-link);
  }
  .breadcrumb .crumb.current {
    color: var(--ctx-fg-dim);
    cursor: default;
    padding: 1px 4px;
  }
  .breadcrumb .sep {
    color: var(--ctx-fg-dim);
    user-select: none;
    -webkit-user-select: none;
  }
  .title {
    margin: 0;
    font-size: 18px;
    color: var(--ctx-accent);
  }
  .title:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 2px;
    border-radius: 3px;
  }

  /* stats grid */
  .stats {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 8px 16px;
    padding: 10px 12px;
    background: var(--ctx-bg-panel);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
  }
  .stats > div {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .stats dt {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  .stats dd {
    margin: 0;
    color: var(--ctx-fg);
    font-weight: 500;
    font-size: 14px;
    font-variant-numeric: tabular-nums;
  }
  .git-cell .git-badges {
    display: flex;
    flex-wrap: wrap;
    gap: 4px 8px;
    font-size: 12px;
    font-weight: 500;
  }
  .badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 1px 6px;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg-elev);
  }
  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    display: inline-block;
  }
  .badge-count {
    font-variant-numeric: tabular-nums;
  }
  .badge-kind {
    color: var(--ctx-fg-dim);
    font-size: 11px;
  }

  /* readme */
  .readme-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-height: 0;
  }
  h3 {
    margin: 0;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
    font-weight: 600;
    display: flex;
    gap: 8px;
    align-items: baseline;
  }
  .readme-path {
    font-size: 10px;
    text-transform: none;
    letter-spacing: 0;
    word-break: break-all;
  }
  .readme {
    margin: 0;
    padding: 10px 12px;
    background: var(--ctx-bg-panel);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    max-height: 360px;
    overflow: auto;
    font-size: 12px;
    line-height: 1.55;
    color: var(--ctx-fg);
    white-space: pre-wrap;
    word-break: break-word;
  }
  .readme code {
    background: transparent;
    padding: 0;
  }

  /* children */
  .children-wrap {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .children {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    background: var(--ctx-bg-panel);
    max-height: 480px;
    overflow: auto;
  }
  .child-row {
    width: 100%;
    text-align: left;
    border: 0;
    border-radius: 0;
    padding: 5px 12px;
    display: grid;
    grid-template-columns: 22px 1fr auto;
    gap: 8px;
    background: transparent;
    color: inherit;
    cursor: pointer;
    font-size: 12px;
    align-items: center;
  }
  .child-row:hover {
    background: var(--ctx-bg-elev);
  }
  .child-row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .child-row.dir .name {
    color: var(--ctx-link);
    font-weight: 500;
  }
  .icon {
    text-align: center;
    line-height: 1;
  }
  .name {
    word-break: break-all;
    min-width: 0;
  }
  .meta {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 11px;
    color: var(--ctx-fg-dim);
    white-space: nowrap;
  }
  .tokens {
    color: var(--ctx-fg);
    font-variant-numeric: tabular-nums;
  }
  .size {
    font-variant-numeric: tabular-nums;
  }
  .child-git {
    font-weight: 600;
    font-family: var(--ctx-font-mono);
    min-width: 1em;
    text-align: center;
  }

  .loading {
    padding: 8px 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .error {
    padding: 12px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
  }
  .error code {
    display: block;
    margin: 6px 0;
    color: var(--ctx-err);
    font-size: 11px;
    word-break: break-all;
  }
  .empty {
    margin: 0;
    padding: 8px 0;
    font-size: 12px;
  }

  @media (max-width: 600px) {
    .stats {
      grid-template-columns: 1fr;
    }
    .child-row {
      grid-template-columns: 22px 1fr;
      grid-template-rows: auto auto;
    }
    .child-row .meta {
      grid-column: 2;
      grid-row: 2;
      flex-wrap: wrap;
      gap: 6px;
    }
    .readme {
      max-height: 240px;
    }
  }
</style>
