<script lang="ts">
  import { tick } from 'svelte';
  import { fetchTree, ApiCallError, type TreeNode as TreeNodeT } from '../lib/api';
  import { navigate, toFileHash, toDirHash, toTreeHash, route } from '../lib/router.svelte';
  import { treeState, setExpanded, reloadTree } from '../lib/tree-state.svelte';
  import { setRepoRoot } from '../lib/repo.svelte';
  import TreeNode from './TreeNode.svelte';

  let { selectedPath = '' } = $props<{ selectedPath?: string }>();

  let tree = $state<TreeNodeT | null>(null);
  let total = $state(0);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // When true, only show entries with a non-empty `git` status (and their
  // ancestors). Directory `git` is propagated from the strongest descendant
  // status server-side, so any ancestor of a changed file matches.
  let gitOnly = $state(false);

  // Date-range filter — mirrors `?since=` / `?until=` URL params.
  // Local input state is separate from route to avoid writing URL on every
  // keystroke; a debounced effect commits the value to the URL after 300ms.
  let sinceInput = $state(route.since ?? '');
  let untilInput = $state(route.until ?? '');
  let useMtimeChecked = $state(!!route.useMtime);
  let filterError = $state<{ field: 'since' | 'until'; message: string } | null>(null);

  // Toggle is only useful when at least one date filter is active.
  let mtimeEnabled = $derived(!!(sinceInput || untilInput));

  // Debounce timers — plain let to avoid reactive self-dependency.
  let sinceTimer: ReturnType<typeof setTimeout> | null = null;
  let untilTimer: ReturnType<typeof setTimeout> | null = null;

  // Element refs by path so keyboard nav can programmatically focus rows.
  const rowEls = new Map<string, HTMLElement>();

  function registerRow(path: string, el: HTMLElement | null) {
    if (el) rowEls.set(path, el);
    else rowEls.delete(path);
  }

  // Epoch counter so a slow earlier response can't overwrite a newer one
  // (rapid filter edits / reloads fire overlapping fetches).
  let loadEpoch = 0;

  function load() {
    const epoch = ++loadEpoch;
    loading = true;
    error = null;
    filterError = null;
    fetchTree({
      depth: 6,
      tokens: true,
      git: true,
      since: route.since || undefined,
      until: route.until || undefined,
      useMtime: route.useMtime || undefined,
    })
      .then((r) => {
        if (epoch !== loadEpoch) return;
        tree = r.tree;
        total = r.total;
        setRepoRoot(r.abs_root);
        // Root expanded by default; depth-1 dirs also expanded (mirrors the
        // previous TreeNode default of `depth < 2`). Merge with any pre-existing
        // expansions (e.g. shared state set by a breadcrumb reveal pre-load).
        const e = new Set<string>(treeState.expanded);
        if (r.tree) {
          e.add(r.tree.path);
          if (r.tree.children) {
            for (const c of r.tree.children) {
              if (c.is_dir) e.add(c.path);
            }
          }
        }
        treeState.expanded = e;
      })
      .catch((e: unknown) => {
        if (epoch !== loadEpoch) return;
        if (e instanceof ApiCallError && e.status === 400) {
          const field = e.code === 'invalid_since' ? 'since' : e.code === 'invalid_until' ? 'until' : null;
          if (field) {
            filterError = { field, message: e.message };
            return;
          }
        }
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (epoch === loadEpoch) loading = false;
      });
  }

  // Last-loaded filter triple. Routes that don't carry since/until/use_mtime
  // params (budget/replay/search/mixdowns) parse them as undefined; without
  // this guard, navigating there would silently reset the filter and trigger
  // a hidden full refetch that discards the user's filtered tree.
  let loadedFilterKey: string | null = null;

  $effect(() => {
    // Only react while the tree pane is the active consumer of the date
    // filter params — i.e. on routes whose URLs actually carry them.
    const filterRoute =
      route.name === 'tree' || route.name === 'dir' || route.name === 'file';
    if (!filterRoute && loadedFilterKey !== null) return;
    // Reactive read of route.since / route.until / route.useMtime so that URL
    // changes (e.g. back/forward navigation or initial load with hash params)
    // retrigger load.
    const key = `${route.since ?? ''}|${route.until ?? ''}|${route.useMtime ? '1' : ''}`;
    if (key === loadedFilterKey) return;
    loadedFilterKey = key;
    load();
  });

  // External reload trigger (App-level Shift+R, future palette commands).
  // Skip the seed value (0) so the initial mount $effect above isn't
  // double-firing load(); only later bumps via reloadTree() refetch.
  $effect(() => {
    if (treeState.reloadKey === 0) return;
    load();
  });

  // Sync route → local inputs when URL changes externally (back/forward nav).
  // Skipped on routes that don't carry the filter params so the inputs keep
  // showing the active filter instead of clearing (see the load effect above).
  $effect(() => {
    if (route.name !== 'tree' && route.name !== 'dir' && route.name !== 'file') return;
    sinceInput = route.since ?? '';
    untilInput = route.until ?? '';
    useMtimeChecked = !!route.useMtime;
  });

  function commitSince(value: string) {
    // If since is cleared and there's no until, also clear useMtime.
    const hasDates = !!(value || route.until);
    navigate(toTreeHash({
      since: value || undefined,
      until: route.until || undefined,
      useMtime: hasDates ? (useMtimeChecked || undefined) : undefined,
    }));
  }

  function commitUntil(value: string) {
    navigate(toTreeHash({
      since: route.since || undefined,
      until: value || undefined,
      useMtime: useMtimeChecked || undefined,
    }));
  }

  function onToggleMtime() {
    const next = !useMtimeChecked;
    navigate(toTreeHash({
      since: route.since || undefined,
      until: route.until || undefined,
      useMtime: next || undefined,
    }));
  }

  // Quick presets for the `since` filter. `value` is the string handed to
  // `walk.ParseTimeFilter` on the backend (`Nd` / `Nw` / `Nmo` / `Ny`).
  const sincePresets: ReadonlyArray<{ label: string; value: string }> = [
    { label: '1d', value: '1d' },
    { label: '3d', value: '3d' },
    { label: '5d', value: '5d' },
    { label: '1week', value: '1w' },
    { label: '2week', value: '2w' },
    { label: '1month', value: '1mo' },
  ];

  function onPresetClick(value: string) {
    sinceInput = value;
    // Cancel any pending debounced commit from a previous keystroke to keep
    // the preset click as the authoritative URL write.
    if (sinceTimer !== null) {
      clearTimeout(sinceTimer);
      sinceTimer = null;
    }
    commitSince(value);
  }

  function onSinceInput(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value;
    sinceInput = value;
    if (sinceTimer !== null) clearTimeout(sinceTimer);
    sinceTimer = setTimeout(() => {
      sinceTimer = null;
      commitSince(value);
    }, 300);
  }

  function onUntilInput(e: Event) {
    const value = (e.currentTarget as HTMLInputElement).value;
    untilInput = value;
    if (untilTimer !== null) clearTimeout(untilTimer);
    untilTimer = setTimeout(() => {
      untilTimer = null;
      commitUntil(value);
    }, 300);
  }

  interface FlatRow {
    node: TreeNodeT;
    level: number; // 1-based ARIA level
    posinset: number; // 1-based among siblings
    setsize: number; // sibling count
    isExpanded: boolean;
  }

  // Flatten visible nodes iteratively. Children of collapsed dirs are skipped.
  // Root itself is rendered as a treeitem at level 1.
  // Filter mode: when gitOnly is on, only entries with a non-empty git status
  // are walked. Directories whose git is propagated from descendants are
  // force-expanded so the full match subtree is visible without manual clicks.
  let visible = $derived.by<FlatRow[]>(() => {
    if (!tree) return [];
    const out: FlatRow[] = [];

    function walk(node: TreeNodeT, level: number, posinset: number, setsize: number) {
      if (gitOnly && !node.git) return;
      const isExp = node.is_dir && (gitOnly || treeState.expanded.has(node.path));
      out.push({ node, level, posinset, setsize, isExpanded: isExp });
      if (node.is_dir && isExp && node.children && node.children.length > 0) {
        const kids = gitOnly ? node.children.filter((c) => !!c.git) : node.children;
        for (let i = 0; i < kids.length; i++) {
          walk(kids[i], level + 1, i + 1, kids.length);
        }
      }
    }
    walk(tree, 1, 1, 1);
    return out;
  });

  // Match count for the filter pill — number of changed files currently
  // visible. Directories are excluded since their git status is aggregated.
  let matchCount = $derived(
    gitOnly ? visible.reduce((n, r) => (r.node.is_dir ? n : n + 1), 0) : 0,
  );

  // Path → index map over the current visible list. Built once per visible
  // change so downstream consumers (focused index, tabstop, keyboard nav)
  // do O(1) lookups instead of O(N) linear scans every keystroke. For trees
  // with thousands of files this matters: ArrowDown previously re-scanned
  // the entire visible array twice (focusedIndex derived + onKey fallback).
  let visibleIndexByPath = $derived.by(() => {
    const m = new Map<string, number>();
    for (let i = 0; i < visible.length; i++) m.set(visible[i].node.path, i);
    return m;
  });

  // Index of focused row in the flat visible list. -1 if focusedPath is not
  // visible (e.g. its parent collapsed).
  let focusedIndex = $derived.by(() => {
    if (treeState.focusedPath === null) return -1;
    const i = visibleIndexByPath.get(treeState.focusedPath);
    return i === undefined ? -1 : i;
  });

  // Effective tabstop: the focused row, or — if focused row is hidden / no
  // selection yet — the first selected row, else the first row. Exactly one
  // treeitem should be in the tab order at any time.
  let tabstopPath = $derived.by(() => {
    if (focusedIndex >= 0) return visible[focusedIndex].node.path;
    if (selectedPath && visibleIndexByPath.has(selectedPath)) return selectedPath;
    return visible.length > 0 ? visible[0].node.path : null;
  });

  async function focusPath(path: string) {
    treeState.focusedPath = path;
    await tick();
    const el = rowEls.get(path);
    if (el) {
      el.focus();
      el.scrollIntoView({ block: 'nearest' });
    }
  }

  // React to external reveal requests (e.g. breadcrumb clicks in FileDetail).
  // The monotonic `key` ensures repeated reveals of the same path still fire.
  $effect(() => {
    const req = treeState.revealRequest;
    if (!req) return;
    // Track key to retrigger when same path is requested twice.
    void req.key;
    void focusPath(req.path);
  });

  function activate(row: FlatRow) {
    const { node } = row;
    const filterOpts = {
      since: route.since,
      until: route.until,
      useMtime: route.useMtime,
    };
    if (node.is_dir) {
      setExpanded(node.path, !row.isExpanded);
      void focusPath(node.path);
      // Root tree node uses path "." (or "") depending on server; treat both
      // as the empty-string root for the dir route so `#/dir` is canonical.
      const dirPath = node.path === '.' ? '' : node.path;
      navigate(toDirHash(dirPath, filterOpts));
    } else {
      void focusPath(node.path);
      navigate(toFileHash(node.path, filterOpts));
    }
  }

  // vim-style alias state: track last `g` press for the `gg` two-stroke chord.
  let lastGTime = 0;
  const GG_WINDOW_MS = 500;

  // Map vim aliases to ARIA-equivalent keys. Returns the canonical key, or
  // null if the event should fall through unchanged. Suppressed during IME
  // composition and when a modifier key is held (avoid shortcut conflicts).
  function vimAlias(e: KeyboardEvent): string | null {
    if (e.isComposing) return null;
    if (e.metaKey || e.ctrlKey || e.altKey) return null;
    switch (e.key) {
      case 'j':
        return 'ArrowDown';
      case 'k':
        return 'ArrowUp';
      case 'h':
        return 'ArrowLeft';
      case 'l':
        return 'ArrowRight';
      case 'G':
        return 'End';
      case 'g': {
        const now = Date.now();
        if (now - lastGTime < GG_WINDOW_MS) {
          lastGTime = 0;
          return 'Home';
        }
        lastGTime = now;
        return null;
      }
      default:
        return null;
    }
  }

  // WAI-ARIA Authoring Practices — Tree Pattern keyboard model.
  function onKey(e: KeyboardEvent) {
    if (visible.length === 0) return;
    // Determine current index — fall back to tabstop if user hasn't focused yet.
    let idx = focusedIndex;
    if (idx < 0) {
      const tp = tabstopPath;
      if (tp === null) return;
      idx = visibleIndexByPath.get(tp) ?? 0;
    }
    const row = visible[idx];

    const key = vimAlias(e) ?? e.key;

    switch (key) {
      case 'ArrowDown': {
        e.preventDefault();
        if (idx < visible.length - 1) void focusPath(visible[idx + 1].node.path);
        break;
      }
      case 'ArrowUp': {
        e.preventDefault();
        if (idx > 0) void focusPath(visible[idx - 1].node.path);
        break;
      }
      case 'ArrowRight': {
        e.preventDefault();
        if (row.node.is_dir) {
          if (!row.isExpanded) {
            setExpanded(row.node.path, true);
          } else if (
            row.node.children &&
            row.node.children.length > 0 &&
            idx + 1 < visible.length
          ) {
            void focusPath(visible[idx + 1].node.path);
          }
        }
        break;
      }
      case 'ArrowLeft': {
        e.preventDefault();
        if (row.node.is_dir && row.isExpanded) {
          setExpanded(row.node.path, false);
        } else {
          // Move to parent: scan back for a row with level == row.level - 1.
          for (let i = idx - 1; i >= 0; i--) {
            if (visible[i].level === row.level - 1) {
              void focusPath(visible[i].node.path);
              break;
            }
          }
        }
        break;
      }
      case 'Home': {
        e.preventDefault();
        void focusPath(visible[0].node.path);
        break;
      }
      case 'End': {
        e.preventDefault();
        void focusPath(visible[visible.length - 1].node.path);
        break;
      }
      case 'Enter':
      case ' ': {
        e.preventDefault();
        activate(row);
        break;
      }
    }
  }

  function onFocusIn(e: FocusEvent) {
    const t = e.target as HTMLElement | null;
    if (!t) return;
    const path = t.dataset.path;
    if (path && treeState.focusedPath !== path) {
      treeState.focusedPath = path;
    }
  }
</script>

<div class="tree-view">
  <header class="tree-header">
    <span class="title">Files</span>
    {#if total > 0}
      <span class="muted total" aria-label="total entries">
        {#if gitOnly}{matchCount} / {total}{:else}{total}{/if}
      </span>
    {/if}
    <button
      type="button"
      class="git-filter"
      class:active={gitOnly}
      aria-pressed={gitOnly}
      aria-label="Show only files with git changes"
      title="Show only files with git changes"
      onclick={() => (gitOnly = !gitOnly)}
    >
      <svg
        class="git-filter-glyph"
        viewBox="0 0 16 16"
        width="14"
        height="14"
        aria-hidden="true"
        focusable="false"
      >
        <path
          d="M4.5 5v6M4.5 7c0-2 2-3 4-3h1.5"
          fill="none"
          stroke="currentColor"
          stroke-width="1.4"
          stroke-linecap="round"
          stroke-linejoin="round"
        />
        <circle cx="4.5" cy="3.5" r="1.6" fill="currentColor" />
        <circle cx="4.5" cy="12.5" r="1.6" fill="currentColor" />
        <circle class="git-filter-dot" cx="11.5" cy="4" r="1.7" />
      </svg>
    </button>
    <!-- reloadTree (not bare load): bumping reloadKey also marks peer caches
         stale (e.g. the Cmd-P finder's file list), same as App's Shift+R. -->
    <button
      class="refresh"
      onclick={() => reloadTree()}
      disabled={loading}
      aria-label="reload tree"
      aria-busy={loading}
      title="Reload"
    >
      <span class="refresh-glyph" aria-hidden="true">↻</span>
    </button>
  </header>

  <div class="tree-filter-row">
    <div class="filter-inputs">
      <input
        class="filter-input"
        type="text"
        placeholder="since: 7d, 2026-01-01"
        aria-label="Filter files modified after (e.g. 7d, 2w, 2026-01-01)"
        value={sinceInput}
        oninput={onSinceInput}
      />
      <input
        class="filter-input"
        type="text"
        placeholder="until: 1d, 2026-12-31"
        aria-label="Filter files modified before (e.g. 1d, 1mo, 2026-12-31)"
        value={untilInput}
        oninput={onUntilInput}
      />
      <span
        class="mtime-toggle"
        title={mtimeEnabled
          ? 'Fast mode — judge files by mtime instead of git commit time. ~3× faster on large repos; can over-include files touched locally without commit.'
          : 'Enter since or until to enable fast mode'}
      >
        <input
          type="checkbox"
          id="mtime-toggle"
          class="mtime-checkbox"
          checked={useMtimeChecked}
          disabled={!mtimeEnabled}
          onchange={onToggleMtime}
          aria-label="Fast mode: judge files by mtime instead of git commit time"
        />
        <label
          for="mtime-toggle"
          class="mtime-label"
          class:disabled={!mtimeEnabled}
          class:active={useMtimeChecked && mtimeEnabled}
        >⚡ fast</label>
      </span>
    </div>
    <div class="tree-preset-row" role="group" aria-label="Quick since presets">
      {#each sincePresets as p (p.value)}
        <button
          type="button"
          class="since-preset"
          class:active={sinceInput === p.value}
          aria-pressed={sinceInput === p.value}
          aria-label={`Set since filter to ${p.label}`}
          onclick={() => onPresetClick(p.value)}
        >
          {p.label}
        </button>
      {/each}
    </div>
    {#if filterError}
      <div class="banner banner-error" role="alert">
        {filterError.field === 'since' ? 'since' : 'until'}: {filterError.message}
      </div>
    {/if}
  </div>

  <div class="tree-body">
    {#if loading && !tree}
      <ul class="skel-list" aria-busy="true">
        {#each Array(8) as _, i (i)}
          <li><span class="skel" style="width: {40 + ((i * 17) % 50)}%; height: 12px;"></span></li>
        {/each}
      </ul>
    {:else if error}
      <div class="error">
        <p>Tree load failed.</p>
        <code class="mono">{error}</code>
        <button onclick={load}>Retry</button>
      </div>
    {:else if tree && gitOnly && visible.length === 0}
      <p class="muted empty">No files with git changes.</p>
    {:else if tree && visible.length > 0}
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <ul
        class="tree"
        role="tree"
        aria-label="File tree"
        onkeydown={onKey}
        onfocusin={onFocusIn}
      >
        {#each visible as row (row.node.path)}
          <TreeNode
            node={row.node}
            level={row.level}
            posinset={row.posinset}
            setsize={row.setsize}
            isExpanded={row.isExpanded}
            isSelected={!row.node.is_dir && row.node.path === selectedPath}
            isTabstop={row.node.path === tabstopPath}
            onActivate={() => activate(row)}
            register={(el) => registerRow(row.node.path, el)}
          />
        {/each}
      </ul>
    {:else}
      <p class="muted">No tree data.</p>
    {/if}
  </div>
</div>

<style>
  .tree-view {
    display: flex;
    flex-direction: column;
    height: 100%;
  }
  .tree-header {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--ctx-border);
    background: var(--ctx-bg-elev);
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
    flex: 0 0 auto;
  }
  .title {
    font-weight: 600;
  }
  .total {
    margin-left: auto;
  }
  .refresh,
  .git-filter {
    padding: 4px 8px;
    line-height: 1;
    border: 0;
    transition: background-color var(--motion-quick) ease-out;
  }
  .git-filter.active {
    color: var(--ctx-git-modified);
    background: rgba(215, 186, 125, 0.16);
  }
  .git-filter-glyph {
    display: inline-block;
    vertical-align: middle;
  }
  /* Dot always carries the git-modified hue so the icon reads as a
     git-aware filter even when inactive. Slight dim when inactive keeps
     the resting state from competing with the funnel outline. */
  .git-filter-dot {
    fill: var(--ctx-git-modified);
    transition: opacity var(--motion-quick) ease-out;
  }
  .git-filter:not(.active) .git-filter-dot {
    opacity: 0.75;
  }
  .refresh:disabled {
    cursor: progress;
    opacity: 0.7;
  }
  .refresh-glyph {
    display: inline-block;
    transform-origin: center;
  }
  .refresh[aria-busy='true'] .refresh-glyph {
    animation: ctx-tree-refresh-spin 0.8s linear infinite;
  }
  @keyframes ctx-tree-refresh-spin {
    from {
      transform: rotate(0deg);
    }
    to {
      transform: rotate(360deg);
    }
  }
  .empty {
    padding: 12px;
  }
  .tree-filter-row {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 6px 10px;
    border-bottom: 1px solid var(--ctx-border);
    flex: 0 0 auto;
  }
  .filter-inputs {
    display: flex;
    align-items: center;
    gap: 4px;
  }
  .filter-input {
    flex: 1 1 0;
    min-width: 0;
    box-sizing: border-box;
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 4px 8px;
    outline: none;
  }
  .filter-input:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .mtime-toggle {
    display: flex;
    align-items: center;
    gap: 3px;
    flex: 0 0 auto;
  }
  .mtime-checkbox {
    cursor: pointer;
    accent-color: var(--ctx-accent);
    width: 12px;
    height: 12px;
  }
  .mtime-checkbox:disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  .mtime-label {
    font-size: 11px;
    color: var(--ctx-fg-dim);
    cursor: pointer;
    user-select: none;
    white-space: nowrap;
  }
  .mtime-label.active {
    color: var(--ctx-accent, var(--ctx-fg-strong));
    font-weight: 600;
  }
  .mtime-label.disabled {
    cursor: not-allowed;
    opacity: 0.45;
  }
  .tree-preset-row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 2px;
  }
  .since-preset {
    font-size: 11px;
    line-height: 1;
    padding: 3px 8px;
    border-radius: 12px;
    border: 1px solid var(--ctx-border);
    background: var(--ctx-bg);
    color: var(--ctx-fg-dim);
    cursor: pointer;
    transition:
      background-color var(--motion-quick) ease-out,
      color var(--motion-quick) ease-out,
      border-color var(--motion-quick) ease-out;
  }
  .since-preset:hover {
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
  }
  .since-preset:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  .since-preset.active {
    background: var(--ctx-accent);
    color: var(--ctx-bg);
    border-color: var(--ctx-accent);
  }
  .banner {
    padding: 4px 6px;
    font-size: 11px;
    border-radius: 3px;
  }
  .banner-error {
    color: var(--ctx-git-deleted);
    background: var(--ctx-bg-panel);
  }
  .tree-body {
    flex: 1 1 auto;
    overflow: auto;
    padding: 4px 0;
  }
  .tree {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .skel-list {
    list-style: none;
    margin: 0;
    padding: 4px 12px;
  }
  .skel-list li {
    margin: 6px 0;
  }
  .error {
    padding: 12px;
  }
  .error code {
    display: block;
    margin: 6px 0;
    color: var(--ctx-err);
    word-break: break-all;
    font-size: 11px;
  }
</style>
