<script lang="ts">
  import { untrack } from 'svelte';
  import { route, navigate, toTreeHash, toBudgetHash, toReplayHash, toFileHash, toMixdownsHash } from './lib/router.svelte';
  import ThemePicker from './components/ThemePicker.svelte';
  import { openFinder } from './lib/finder.svelte';
  import TreeView from './components/TreeView.svelte';
  import FileDetail from './components/FileDetail.svelte';
  import SearchBar from './components/SearchBar.svelte';
  import SearchResults from './components/SearchResults.svelte';
  import BudgetPanel from './components/BudgetPanel.svelte';
  import DirOverview from './components/DirOverview.svelte';
  import ReplayPanel from './components/ReplayPanel.svelte';
  import StatusBar from './components/StatusBar.svelte';
  import FuzzyFinder from './components/FuzzyFinder.svelte';
  import Cheatsheet from './components/Cheatsheet.svelte';
  import CommandPalette from './components/CommandPalette.svelte';
  import ContextMenu from './components/ContextMenu.svelte';
  import DefinitionPicker from './components/DefinitionPicker.svelte';
  import RootsPicker from './components/RootsPicker.svelte';
  import TabBar from './components/TabBar.svelte';
  import BounceDialog from './components/BounceDialog.svelte';
  import MixdownsPanel from './components/MixdownsPanel.svelte';
  import PaneSplitter from './components/PaneSplitter.svelte';
  import { announceState, announce } from './lib/announce.svelte';
  import { toggleCheatsheet, cheatsheet } from './lib/cheatsheet.svelte';
  import { finder } from './lib/finder.svelte';
  import { palette, openPalette } from './lib/palette.svelte';
  import { definitionPicker } from './lib/definition-picker.svelte';
  import { rootsPicker, openRootsPicker } from './lib/roots-picker.svelte';
  import { bounceDialog } from './lib/bounce-dialog.svelte';
  import { roots, loadRoots, currentRoot } from './lib/roots.svelte';
  import { repo } from './lib/repo.svelte';
  import { basename } from './lib/format';
  import { COMMANDS } from './lib/commands';
  import { tabs, openTab, closeTab, setTabs } from './lib/tabs.svelte';
  import { panes, openRight, closeRight, setFocused } from './lib/panes.svelte';
  import { view, toggleTree, toggleSymbols } from './lib/view.svelte';
  import { reloadTree } from './lib/tree-state.svelte';

  let selectedPath = $derived(route.name === 'file' ? route.path : '');
  let searchQuery = $derived(route.name === 'search' ? route.query : '');

  let rightTab: 'file' | 'search' | 'budget' | 'replay' | 'dir' | 'mixdowns' = $derived.by(() => {
    if (route.name === 'search') return 'search';
    if (route.name === 'budget') return 'budget';
    if (route.name === 'replay') return 'replay';
    if (route.name === 'dir') return 'dir';
    if (route.name === 'mixdowns') return 'mixdowns';
    return 'file';
  });

  // Mobile breakpoint — below this width the right pane is suppressed.
  // We mirror the @media query in JS so keyboard shortcuts can short-circuit
  // and so `rightOpen` is forcibly cleared on resize-into-mobile.
  let isMobile = $state(typeof window !== 'undefined' && window.innerWidth < 800);
  $effect(() => {
    if (typeof window === 'undefined') return;
    const mql = window.matchMedia('(max-width: 799px)');
    isMobile = mql.matches;
    const onChange = (e: MediaQueryListEvent) => {
      isMobile = e.matches;
      if (e.matches && panes.rightOpen) {
        // Force-collapse so the user isn't stuck with hidden state.
        closeRight();
      }
    };
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  });

  // Effective two-pane mode — true only when route is a file AND user wants
  // the right pane AND we're not on a narrow viewport.
  let twoPane = $derived(
    route.name === 'file' && panes.rightOpen && !isMobile && panes.rightPath !== '',
  );

  function isTextInputFocused(): boolean {
    const a = document.activeElement as HTMLElement | null;
    if (!a) return false;
    const tag = a.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
    if (a.isContentEditable) return true;
    return false;
  }

  // ----- Cmd-K Cmd-→/← chord state ---------------------------------------
  let pendingCmdK = $state(false);
  let pendingCmdKTimer: ReturnType<typeof setTimeout> | null = null;
  function armCmdK() {
    pendingCmdK = true;
    if (pendingCmdKTimer) clearTimeout(pendingCmdKTimer);
    pendingCmdKTimer = setTimeout(() => {
      pendingCmdK = false;
      pendingCmdKTimer = null;
    }, 1500);
  }
  function disarmCmdK() {
    pendingCmdK = false;
    if (pendingCmdKTimer) {
      clearTimeout(pendingCmdKTimer);
      pendingCmdKTimer = null;
    }
  }

  // Cmd-P (macOS) / Ctrl-P (others) opens the fuzzy file finder. We override
  // the browser print shortcut intentionally (VS Code parity).
  // `?` (Shift+/) toggles the keyboard cheatsheet, but is suppressed while
  // typing into any text input so it doesn't intercept the literal character.
  // Cmd-W / Ctrl-W closes the active tab; Cmd-1..9 / Ctrl-1..9 switches to
  // the Nth tab. Both are suppressed while typing in a text input so the
  // browser-default (close window / switch browser tab) only triggers when
  // we're sure the user isn't relying on it in a focused field.
  function onGlobalKey(e: KeyboardEvent) {
    const mod = e.metaKey || e.ctrlKey;

    // Cmd-K chord — when armed, ArrowLeft/ArrowRight switches pane focus.
    if (pendingCmdK) {
      if (e.key === 'ArrowLeft') {
        e.preventDefault();
        disarmCmdK();
        if (panes.rightOpen) {
          setFocused('left');
          announce('Focus left pane');
        }
        return;
      }
      if (e.key === 'ArrowRight') {
        e.preventDefault();
        disarmCmdK();
        if (panes.rightOpen) {
          setFocused('right');
          announce('Focus right pane');
        }
        return;
      }
      // Any other key cancels the chord — but we still let the original
      // handler below process it.
      disarmCmdK();
    }

    // Cmd/Ctrl+Shift+P — command palette. Hook *before* Cmd+P so Firefox /
     // Safari can't open a private window with their default mapping; also
     // before the bare Cmd+P arm because both branches match `key === 'p'`.
    if (mod && e.shiftKey && !e.altKey && (e.key === 'p' || e.key === 'P')) {
      e.preventDefault();
      // Mutex with the other overlays — close them first so we don't stack.
      if (finder.open) return;
      if (cheatsheet.open) return;
      if (!palette.open) openPalette(COMMANDS.length);
      return;
    }
    if (mod && !e.shiftKey && !e.altKey && (e.key === 'p' || e.key === 'P')) {
      e.preventDefault();
      // Mutex with the other overlays — never stack on top of an existing modal.
      if (palette.open) return;
      if (cheatsheet.open) return;
      openFinder();
      return;
    }
    if (e.key === '?' && !e.isComposing && !e.metaKey && !e.ctrlKey && !e.altKey) {
      if (isTextInputFocused()) return;
      if (finder.open) return;
      if (palette.open) return;
      if (definitionPicker.open) return;
      if (rootsPicker.open) return;
      if (bounceDialog.open) return;
      e.preventDefault();
      toggleCheatsheet();
      return;
    }
    if (mod && !e.shiftKey && !e.altKey && (e.key === 'w' || e.key === 'W')) {
      if (isTextInputFocused()) return;
      if (tabs.paths.length === 0) return;
      // Target the focused pane: the right pane's path when it has focus,
      // otherwise the left pane (route.path). Mirrors onTabActivate.
      const rightFocused = twoPane && panes.focused === 'right';
      const active = rightFocused
        ? panes.rightPath
        : route.name === 'file' ? route.path : '';
      const target = active || tabs.paths[tabs.paths.length - 1];
      if (!target) return;
      e.preventDefault();
      const next = closeTab(target);
      if (target === active) {
        if (rightFocused) {
          if (next) panes.rightPath = next;
          else closeRight();
        } else if (next) {
          navigate(toFileHash(next));
        }
      }
      return;
    }
    if (mod && !e.shiftKey && !e.altKey && e.key >= '1' && e.key <= '9') {
      if (isTextInputFocused()) return;
      const idx = Number(e.key) - 1;
      if (idx < 0 || idx >= tabs.paths.length) return;
      e.preventDefault();
      const target = tabs.paths[idx];
      if (!target) return;
      // Same focused-pane routing as Cmd-W above.
      if (twoPane && panes.focused === 'right') {
        if (target !== panes.rightPath) panes.rightPath = target;
      } else if (target !== (route.name === 'file' ? route.path : '')) {
        navigate(toFileHash(target));
      }
      return;
    }
    // Cmd-Shift-B / Ctrl-Shift-B — open the project-root picker. Matched
    // before the bare Cmd-B branch below so the Shift modifier wins (both
    // branches see `key === 'b' || 'B'`).
    if (mod && e.shiftKey && !e.altKey && (e.key === 'b' || e.key === 'B')) {
      if (isTextInputFocused()) return;
      // Overlay mutex — never stack on top of an existing modal.
      if (
        finder.open ||
        palette.open ||
        cheatsheet.open ||
        definitionPicker.open ||
        rootsPicker.open
      ) return;
      e.preventDefault();
      openRootsPicker();
      return;
    }
    // Cmd-B / Ctrl-B — toggle the file-tree sidebar (VS Code parity).
    if (mod && !e.shiftKey && !e.altKey && (e.key === 'b' || e.key === 'B')) {
      if (isTextInputFocused()) return;
      e.preventDefault();
      toggleTree();
      announce(view.showTree ? 'File tree shown' : 'File tree hidden');
      return;
    }
    // Cmd-\ / Ctrl-\ — toggle right pane (file route + non-mobile only).
    if (mod && !e.shiftKey && !e.altKey && e.key === '\\') {
      if (isTextInputFocused()) return;
      if (route.name !== 'file' || !route.path) return;
      if (isMobile) return;
      e.preventDefault();
      if (panes.rightOpen) {
        closeRight();
        announce('Right pane closed');
      } else {
        openRight(route.path);
        announce('Right pane opened');
      }
      return;
    }
    // Cmd-K — arm the focus-switch chord.
    if (mod && !e.shiftKey && !e.altKey && (e.key === 'k' || e.key === 'K')) {
      if (isTextInputFocused()) return;
      if (!panes.rightOpen) return;
      e.preventDefault();
      armCmdK();
      return;
    }
    // Shift+R — refresh the file tree (mirrors the TreeView ↻ button).
    if (!mod && !e.altKey && e.shiftKey && e.key === 'R') {
      if (isTextInputFocused()) return;
      if (finder.open || palette.open || cheatsheet.open) return;
      e.preventDefault();
      reloadTree();
      announce('Reloading file tree');
      return;
    }
  }

  $effect(() => {
    window.addEventListener('keydown', onGlobalKey);
    return () => window.removeEventListener('keydown', onGlobalKey);
  });

  // One-shot: seed tabs from `?open=A,B,C` on the initial URL. Subsequent
  // route changes do not re-seed — once the user is interacting, the URL is
  // read-only with respect to tab state.
  let tabsSeeded = false;
  $effect(() => {
    if (tabsSeeded) return;
    if (route.name === 'file') {
      const seed: string[] = [];
      for (const p of route.openPaths) seed.push(p);
      if (route.path && !seed.includes(route.path)) seed.push(route.path);
      if (route.rightPath && !seed.includes(route.rightPath)) seed.push(route.rightPath);
      if (seed.length > 0) setTabs(seed);
      // Seed right-pane state from URL exactly once.
      if (route.rightPath && !isMobile) {
        panes.rightPath = route.rightPath;
        panes.rightOpen = true;
        // Left pane keeps initial focus on first load — user typically wants
        // to inspect/customize the right pane next.
      }
      tabsSeeded = true;
    } else if (route.name !== 'tree' || route.openPaths.length === 0) {
      // Non-file landing: nothing to seed; mark done so we don't keep re-checking.
      tabsSeeded = true;
    }
  });

  // Auto-add a tab whenever we navigate to a file route. Also add right-pane
  // path if it's distinct (so the tab list reflects everything visible).
  // untrack: openTab reads tabs.paths internally; tracking it would re-run
  // this effect when a tab is closed and resurrect the closed tab while
  // route.path still points at it. Depend on route.path only.
  $effect(() => {
    if (route.name === 'file' && route.path) {
      untrack(() => openTab(route.path));
    }
  });
  $effect(() => {
    if (panes.rightOpen && panes.rightPath) {
      untrack(() => openTab(panes.rightPath));
    }
  });

  // URL write-back: keep `?open=A,B,C&right=<p>` in sync with the live state.
  // We use `history.replaceState` (not pushState) to avoid polluting back-button
  // history; router.svelte.ts's hashchange listener does NOT fire on
  // replaceState, so state and URL stay aligned without a feedback loop.
  $effect(() => {
    if (!tabsSeeded) return;
    if (route.name !== 'file' || !route.path) return;
    // Reactive reads:
    const open = [...tabs.paths];
    const rightPathForUrl = panes.rightOpen && panes.rightPath ? panes.rightPath : '';
    if (open.length === 0 && !rightPathForUrl) return;
    const target = toFileHash(route.path, {
      open: open.length > 0 ? open : undefined,
      line: route.lineHint,
      right: rightPathForUrl || undefined,
      mode: route.mode,
    });
    const current = typeof window !== 'undefined' ? window.location.hash : '';
    if (current === target) return;
    if (typeof window !== 'undefined') {
      window.history.replaceState(null, '', target);
      // Keep route.rightPath in sync with what we just put in the URL so a
      // future hashchange parse round-trips cleanly.
      route.rightPath = rightPathForUrl;
    }
  });

  // ----- TabBar click routing -------------------------------------------
  // The single TabBar updates whichever pane currently has focus. This is
  // the simplest mental model: "what I click goes to the highlighted pane".
  function onTabActivate(path: string) {
    if (panes.rightOpen && panes.focused === 'right' && !isMobile) {
      if (panes.rightPath === path) return;
      panes.rightPath = path;
      return;
    }
    if (path === route.path) return;
    navigate(toFileHash(path));
  }

  // The active path that the TabBar should highlight = focused-pane path.
  let tabBarActivePath = $derived.by(() => {
    if (twoPane && panes.focused === 'right') return panes.rightPath;
    return selectedPath;
  });

  function focusLeft() {
    setFocused('left');
  }
  function focusRight() {
    setFocused('right');
  }

  // Eagerly load the roots registry on mount so the header subtitle can
  // resolve the current root's pretty name without waiting for the picker
  // to open. untrack: loadRoots reads/writes roots.loading internally;
  // tracking it would re-run this effect every time the fetch settles
  // (loading flips back to false), causing an infinite /api/roots loop.
  $effect(() => {
    untrack(() => void loadRoots());
  });

  // Subtitle for the header brand. Prefer the registry entry's name (matches
  // what the user typed into `ctx roots add`) and fall back to the basename
  // of the served repo root. Pre-load shows "browse" as the legacy default.
  let headerRootName = $derived.by(() => {
    if (!repo.root) return roots.loaded ? '' : 'browse';
    const entry = currentRoot();
    if (entry) return entry.name;
    return basename(repo.root);
  });
</script>

<div class="layout">
  <header class="topbar">
    <div class="brand">
      <a href={toTreeHash()} aria-label="ctx home" onclick={(e) => { e.preventDefault(); navigate(toTreeHash()); }}>
        <span class="brand-mark">ctx</span>
      </a>
      {#if headerRootName}
        <span class="brand-sep" aria-hidden="true">·</span>
        <button
          type="button"
          class="brand-sub-btn"
          title="Switch project root (⌘⇧B / Ctrl+Shift+B)"
          aria-label={`Current root: ${headerRootName}. Switch project root.`}
          onclick={openRootsPicker}
        >{headerRootName}</button>
      {/if}
    </div>
    <SearchBar />
    <nav class="topnav" aria-label="primary">
      <a
        href={toTreeHash()}
        class:active={route.name === 'tree' || route.name === 'file' || route.name === 'dir'}
        onclick={(e) => { e.preventDefault(); navigate(toTreeHash()); }}
      >Tree</a>
      <a
        href={toBudgetHash()}
        class:active={route.name === 'budget'}
        onclick={(e) => { e.preventDefault(); navigate(toBudgetHash()); }}
      >Budget</a>
      <a
        href={toReplayHash()}
        class:active={route.name === 'replay'}
        onclick={(e) => { e.preventDefault(); navigate(toReplayHash()); }}
      >Replay</a>
      <a
        href={toMixdownsHash()}
        class:active={route.name === 'mixdowns'}
        onclick={(e) => { e.preventDefault(); navigate(toMixdownsHash()); }}
      >Mixdowns</a>
      <button
        type="button"
        class="topnav-action"
        aria-label="Switch project root"
        title="Switch project root (⌘⇧B / Ctrl+Shift+B)"
        onclick={openRootsPicker}
      >Roots</button>
      <button
        type="button"
        class="view-toggle"
        class:active={view.showTree}
        aria-pressed={view.showTree}
        aria-label="Toggle file tree"
        title="Toggle file tree (⌘B / Ctrl+B)"
        onclick={toggleTree}
      >
        <span aria-hidden="true">▤</span>
      </button>
      <button
        type="button"
        class="view-toggle"
        class:active={view.showSymbols}
        aria-pressed={view.showSymbols}
        aria-label="Toggle symbols panel"
        title="Toggle symbols panel"
        onclick={toggleSymbols}
      >
        <span aria-hidden="true">⌘</span>
      </button>
      <ThemePicker />
    </nav>
  </header>

  <main class="content" class:two-pane={twoPane} class:no-tree={!view.showTree}>
    {#if view.showTree}
      <aside class="pane left" aria-label="file tree">
        <TreeView selectedPath={selectedPath} />
      </aside>
    {/if}

    {#if rightTab !== 'file'}
      <section class="pane right" aria-live="polite">
        {#if rightTab === 'search'}
          <SearchResults query={searchQuery} />
        {:else if rightTab === 'budget'}
          <BudgetPanel />
        {:else if rightTab === 'replay'}
          <ReplayPanel />
        {:else if rightTab === 'dir'}
          <DirOverview path={route.path} />
        {:else if rightTab === 'mixdowns'}
          <MixdownsPanel />
        {/if}
      </section>
    {:else if !selectedPath}
      <section class="pane right" aria-live="polite">
        <div class="placeholder">
          <h2>ctx browse</h2>
          <p class="muted">Select a file from the tree, or search above.</p>
          <ul class="muted hints">
            <li><kbd>/</kbd> focus search</li>
            <li><kbd>⌘</kbd>+<kbd>P</kbd> / <kbd>Ctrl</kbd>+<kbd>P</kbd> find file by name</li>
            <li>Click a file in the tree to view it</li>
            <li><a
                href={toBudgetHash()}
                onclick={(e) => { e.preventDefault(); navigate(toBudgetHash()); }}
              >Open Budget panel</a></li>
          </ul>
        </div>
      </section>
    {:else if !twoPane}
      <!-- 1-pane file view: identical to legacy behaviour. -->
      <section class="pane right" aria-live="polite">
        <div class="file-stack">
          <TabBar activePath={selectedPath} onActivate={onTabActivate} />
          <FileDetail path={selectedPath} />
        </div>
      </section>
    {:else}
      <!-- 2-pane file view: TabBar atop the focused pane (we put it above the
           left pane visually + give it the focused-pane path so closing /
           activating routes through the focused pane). Splitter resizes
           between the two FileDetail wrappers. -->
      <section class="pane right two-pane-shell" aria-label="files">
        <TabBar activePath={tabBarActivePath} onActivate={onTabActivate} />
        <div
          class="split-row"
          style="grid-template-columns: {panes.splitPercent}% 4px {100 - panes.splitPercent}%;"
        >
          <section
            class="file-pane left-file"
            class:focused={panes.focused === 'left'}
            aria-label="Left pane"
            aria-current={panes.focused === 'left' ? 'true' : undefined}
            onclickcapture={focusLeft}
            onfocusin={focusLeft}
          >
            <FileDetail path={selectedPath} pane="left" />
          </section>
          <PaneSplitter />
          <section
            class="file-pane right-file"
            class:focused={panes.focused === 'right'}
            aria-label="Right pane"
            aria-current={panes.focused === 'right' ? 'true' : undefined}
            onclickcapture={focusRight}
            onfocusin={focusRight}
          >
            {#if panes.rightPath}
              <FileDetail path={panes.rightPath} pane="right" />
            {:else}
              <div class="placeholder small">
                <p class="muted">Select a file for the right pane.</p>
              </div>
            {/if}
          </section>
        </div>
      </section>
    {/if}
  </main>

  <StatusBar />
</div>

<FuzzyFinder />

<CommandPalette />

<ContextMenu />

<DefinitionPicker />

<RootsPicker />

<Cheatsheet />

<BounceDialog />

<!-- WHY: visually-hidden polite live region. Bumping `key` forces SR re-read
     even when the same message reoccurs (Theme: dark -> dark via reload). -->
<div class="sr-only" aria-live="polite" aria-atomic="true">
  {#key announceState.key}{announceState.message}{/key}
</div>

<style>
  .layout {
    display: grid;
    grid-template-rows: 40px 1fr 22px;
    height: 100vh;
    width: 100vw;
  }
  .topbar {
    display: grid;
    grid-template-columns: minmax(180px, 280px) 1fr auto;
    align-items: center;
    gap: 16px;
    padding: 0 12px;
    background: var(--ctx-bg-elev);
    border-bottom: 1px solid var(--ctx-border);
  }
  .brand {
    display: flex;
    align-items: baseline;
    gap: 6px;
    min-width: 0;
  }
  .brand a {
    color: inherit;
    font-weight: 600;
    flex: 0 0 auto;
  }
  .brand-mark {
    color: var(--ctx-accent);
  }
  .brand-sep {
    color: var(--ctx-fg-dim);
    flex: 0 0 auto;
  }
  .brand-sub-btn {
    color: var(--ctx-fg-dim);
    font: inherit;
    font-weight: 400;
    background: transparent;
    border: 0;
    padding: 2px 6px;
    border-radius: 3px;
    cursor: pointer;
    min-width: 0;
    max-width: 220px;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .brand-sub-btn:hover,
  .brand-sub-btn:focus-visible {
    color: var(--ctx-fg);
    background: var(--ctx-bg-panel);
  }
  .brand-sub-btn:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .topnav {
    display: flex;
    gap: 12px;
  }
  .topnav a {
    color: var(--ctx-fg-dim);
    padding: 4px 8px;
    border-radius: 3px;
  }
  .topnav a.active,
  .topnav a:hover {
    color: var(--ctx-fg);
    background: var(--ctx-bg-panel);
  }
  .topnav-action {
    color: var(--ctx-fg-dim);
    padding: 4px 8px;
    border-radius: 3px;
    border: 0;
    background: transparent;
    font: inherit;
    cursor: pointer;
  }
  .topnav-action:hover {
    color: var(--ctx-fg);
    background: var(--ctx-bg-panel);
  }
  .view-toggle {
    padding: 4px 8px;
    line-height: 1;
    border: 0;
    color: var(--ctx-fg-dim);
    background: transparent;
  }
  .view-toggle:hover {
    color: var(--ctx-fg);
    background: var(--ctx-bg-panel);
  }
  .view-toggle.active {
    color: var(--ctx-fg);
  }
  .content {
    display: grid;
    grid-template-columns: minmax(260px, 360px) 1fr;
    min-height: 0;
  }
  .content.no-tree {
    grid-template-columns: 1fr;
  }
  .pane {
    overflow: auto;
    min-height: 0;
  }
  .pane.left {
    border-right: 1px solid var(--ctx-border);
    background: var(--ctx-bg-panel);
  }
  .pane.right {
    background: var(--ctx-bg);
  }
  /* 2-pane shell wraps a single TabBar + a horizontal split row. We keep the
     shell's overflow at hidden so each child FileDetail owns its own scroll. */
  .pane.right.two-pane-shell {
    display: flex;
    flex-direction: column;
    overflow: hidden;
  }
  .split-row {
    flex: 1 1 auto;
    display: grid;
    min-height: 0;
    min-width: 0;
  }
  .file-pane {
    min-width: 0;
    min-height: 0;
    overflow: hidden;
    display: flex;
    flex-direction: column;
    border-top: 2px solid transparent;
  }
  .file-pane.focused {
    border-top-color: var(--ctx-accent);
  }
  .file-pane.left-file {
    border-right: 1px solid var(--ctx-border);
  }
  .file-stack {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .placeholder {
    padding: 32px;
  }
  .placeholder.small {
    padding: 16px;
  }
  .placeholder h2 {
    margin: 0 0 8px;
    color: var(--ctx-accent);
  }
  .hints {
    list-style: none;
    padding: 0;
    margin-top: 16px;
  }
  .hints li {
    padding: 4px 0;
  }
  kbd {
    font-family: var(--ctx-font-mono);
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    padding: 1px 6px;
    font-size: 11px;
  }
</style>
