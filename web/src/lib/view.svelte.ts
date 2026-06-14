// Layout-visibility preferences for the file tree (left pane) and the
// symbols/insights aside inside FileDetail. Persisted in localStorage so the
// chosen layout survives reloads. Pattern mirrors theme.svelte.ts.

const SHOW_TREE_KEY = 'ctx-show-tree';
const SHOW_SYMBOLS_KEY = 'ctx-show-symbols';
const SHOW_TOKENS_KEY = 'ctx-show-tokens';
const TREE_GITIGNORE_KEY = 'ctx-tree-gitignore';
const DIFF_CONTEXT_ONLY_KEY = 'ctx-diff-context-only';

function readBool(key: string, def: boolean): boolean {
  if (typeof localStorage === 'undefined') return def;
  try {
    const v = localStorage.getItem(key);
    if (v === null) return def;
    return v === '1';
  } catch {
    return def;
  }
}

function writeBool(key: string, v: boolean): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(key, v ? '1' : '0');
  } catch {
    // ignore (storage blocked / quota)
  }
}

export const view = $state<{
  showTree: boolean;
  showSymbols: boolean;
  showTokens: boolean;
  treeGitignore: boolean;
  diffContextOnly: boolean;
}>({
  showTree: readBool(SHOW_TREE_KEY, true),
  showSymbols: readBool(SHOW_SYMBOLS_KEY, true),
  // Token counts in the file tree are off by default — they require a tiktoken
  // pass over every file (slower load) and add visual noise. Opt in via toggle.
  showTokens: readBool(SHOW_TOKENS_KEY, false),
  // Honor the repo's root .gitignore in the file tree. Off by default so the
  // tree keeps showing every file; opt in to hide ignored/generated files.
  treeGitignore: readBool(TREE_GITIGNORE_KEY, false),
  // Collapse long runs of unchanged lines in diff views, keeping only a few
  // context lines around each change. On by default — full-file diffs bury the
  // actual change; toggle off to expand the whole file.
  diffContextOnly: readBool(DIFF_CONTEXT_ONLY_KEY, true),
});

export function toggleTree(): void {
  view.showTree = !view.showTree;
  writeBool(SHOW_TREE_KEY, view.showTree);
}

export function toggleSymbols(): void {
  view.showSymbols = !view.showSymbols;
  writeBool(SHOW_SYMBOLS_KEY, view.showSymbols);
}

export function toggleTokens(): void {
  view.showTokens = !view.showTokens;
  writeBool(SHOW_TOKENS_KEY, view.showTokens);
}

export function toggleTreeGitignore(): void {
  view.treeGitignore = !view.treeGitignore;
  writeBool(TREE_GITIGNORE_KEY, view.treeGitignore);
}

export function toggleDiffContextOnly(): void {
  view.diffContextOnly = !view.diffContextOnly;
  writeBool(DIFF_CONTEXT_ONLY_KEY, view.diffContextOnly);
}
