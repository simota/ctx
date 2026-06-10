// Command palette command registry.
//
// Each command is an immutable record { id, label, category, keywords, run, when? }.
// `run` performs the side effect; the palette closes itself before invocation
// (overlay mutex — handled in palette.svelte.ts so callers don't need to remember).
//
// Dependency direction: commands.ts depends on state modules (theme/tabs/panes/
// router/finder/cheatsheet) but those modules MUST NOT import this file —
// otherwise the palette/state graph would cycle.

import { toggleTheme, theme } from './theme.svelte';
import { tabs, closeTab, clearTabs } from './tabs.svelte';
import { panes, openRight, closeRight, setFocused } from './panes.svelte';
import {
  navigate,
  toFileHash,
  toTreeHash,
  toBudgetHash,
  toReplayHash,
  route,
} from './router.svelte';
import { openFinder } from './finder.svelte';
import { openCheatsheet } from './cheatsheet.svelte';
import { openRootsPicker } from './roots-picker.svelte';

export type CommandCategory =
  | 'Navigation'
  | 'Find'
  | 'View'
  | 'Tabs'
  | 'Help';

// Stable category display order — drives the section headers in the palette
// when the query is empty.
export const CATEGORY_ORDER: CommandCategory[] = [
  'Navigation',
  'Find',
  'View',
  'Tabs',
  'Help',
];

export interface Command {
  id: string;
  label: string;
  category: CommandCategory;
  // Extra fuzzy-search tokens (verbs, synonyms) so e.g. "dark" hits
  // `theme.cycle` even though the label says "Cycle to next theme".
  keywords: string[];
  // Optional shortcut hint shown right-aligned in the row (display only;
  // execution still happens by selecting the command).
  shortcut?: string;
  // Optional gate. When this returns false the command renders disabled
  // (`aria-disabled`, greyed) — never hidden, so the user always sees the
  // intended affordance.
  when?: () => boolean;
  run: () => void;
}

// Has a file route + non-empty path? Used by tab/right-pane gates.
function hasActiveFile(): boolean {
  return route.name === 'file' && route.path !== '';
}

function activeFilePath(): string {
  return route.name === 'file' ? route.path : '';
}

// Built once at module load — stable identity, fine to share.
export const COMMANDS: Command[] = [
  {
    id: 'palette.openFile',
    label: 'Go to File…',
    category: 'Navigation',
    keywords: ['file', 'find', 'open', 'fuzzy'],
    shortcut: '⌘P / Ctrl+P',
    run: () => {
      // Overlay mutex: palette is closed by the executor before run() fires,
      // so calling openFinder() here cannot stack two modals.
      openFinder();
    },
  },
  {
    id: 'palette.openSearch',
    label: 'Search Repository…',
    category: 'Find',
    keywords: ['search', 'find', 'grep', 'where'],
    shortcut: '/',
    run: () => {
      // Mirror the `/` handler: focus the SearchBar input by id.
      const el = document.getElementById('ctx-search') as HTMLInputElement | null;
      if (el) {
        queueMicrotask(() => {
          el.focus();
          el.select();
        });
      }
    },
  },
  {
    id: 'nav.tree',
    label: 'Go to: Tree',
    category: 'Navigation',
    keywords: ['tree', 'files', 'home'],
    run: () => navigate(toTreeHash()),
  },
  {
    id: 'nav.budget',
    label: 'Go to: Budget',
    category: 'Navigation',
    keywords: ['budget', 'tokens', 'cost'],
    run: () => navigate(toBudgetHash()),
  },
  {
    id: 'nav.replay',
    label: 'Go to: Replay',
    category: 'Navigation',
    keywords: ['replay', 'snapshot', 'history'],
    run: () => navigate(toReplayHash()),
  },
  {
    id: 'roots.switch',
    label: 'Switch Project Root…',
    category: 'Navigation',
    keywords: ['roots', 'root', 'project', 'switch', 'open', 'workspace', 'repo'],
    shortcut: '⌘⇧B / Ctrl+Shift+B',
    run: () => {
      // Overlay mutex: the palette closes itself before run() fires, so
      // calling openRootsPicker() here cannot stack two modals.
      openRootsPicker();
    },
  },
  {
    id: 'theme.cycle',
    label: 'Cycle to next theme',
    category: 'View',
    keywords: ['theme', 'dark', 'light', 'lofi', 'sunrise', 'ocean', 'mode', 'color'],
    run: () => toggleTheme(),
  },
  {
    id: 'pane.toggleRight',
    label: 'Toggle Right Pane',
    category: 'View',
    keywords: ['pane', 'split', 'right', 'second'],
    shortcut: '⌘\\ / Ctrl+\\',
    when: hasActiveFile,
    run: () => {
      if (panes.rightOpen) {
        closeRight();
      } else {
        openRight(activeFilePath());
      }
    },
  },
  {
    id: 'pane.focusLeft',
    label: 'Focus Left Pane',
    category: 'View',
    keywords: ['focus', 'pane', 'left'],
    shortcut: '⌘K ⌘←',
    when: () => panes.rightOpen,
    run: () => setFocused('left'),
  },
  {
    id: 'pane.focusRight',
    label: 'Focus Right Pane',
    category: 'View',
    keywords: ['focus', 'pane', 'right'],
    shortcut: '⌘K ⌘→',
    when: () => panes.rightOpen,
    run: () => setFocused('right'),
  },
  {
    id: 'tab.close',
    label: 'Close Active Tab',
    category: 'Tabs',
    keywords: ['tab', 'close', 'active'],
    shortcut: '⌘W / Ctrl+W',
    when: () => tabs.paths.length > 0,
    run: () => {
      const active = activeFilePath();
      const target = active || tabs.paths[tabs.paths.length - 1];
      if (!target) return;
      const next = closeTab(target);
      if (target === active && next) {
        navigate(toFileHash(next));
      }
    },
  },
  {
    id: 'tab.closeAll',
    label: 'Close All Tabs',
    category: 'Tabs',
    keywords: ['tabs', 'close', 'all', 'clear'],
    when: () => tabs.paths.length > 0,
    run: () => {
      clearTabs();
      // After closing all tabs, route home so the user isn't stranded on
      // a "ghost" file route whose tab no longer exists.
      navigate(toTreeHash());
    },
  },
  {
    id: 'help.cheatsheet',
    label: 'Show Keyboard Shortcuts',
    category: 'Help',
    keywords: ['help', 'shortcuts', 'cheatsheet', 'keys', 'bindings'],
    shortcut: '?',
    run: () => openCheatsheet(),
  },
];

// `theme` is read here only to keep the import "live" for tree-shaking
// expectations — the live label could later use `theme.name` to render
// "Toggle Theme: dark → light". Kept as a no-op reference for now.
void theme;
