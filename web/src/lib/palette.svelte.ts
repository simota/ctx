// Cmd/Ctrl+Shift+P command palette — state + open/close/execute.
//
// Mirrors `finder.svelte.ts` for lifecycle parity (lastFocus restore,
// announce, query/selectedIndex reset). Distinct concerns vs FuzzyFinder:
//  - searches a static command list instead of a fetched file tree
//  - executes a `run()` callback rather than navigating
//  - renders disabled rows when `when()` predicate fails
//
// The execute() helper enforces the "close before run" overlay mutex so a
// command like `palette.openFile` (which opens FuzzyFinder) cannot stack two
// modals.

import { announce } from './announce.svelte';
import type { Command } from './commands';

export interface PaletteState {
  open: boolean;
  query: string;
  selectedIndex: number;
}

export const palette = $state<PaletteState>({
  open: false,
  query: '',
  selectedIndex: 0,
});

let lastFocus: HTMLElement | null = null;

export function openPalette(commandCount: number): void {
  if (palette.open) return;
  lastFocus = (document.activeElement as HTMLElement) ?? null;
  palette.open = true;
  palette.query = '';
  palette.selectedIndex = 0;
  announce(`Command palette opened, ${commandCount} commands`);
}

export function closePalette(): void {
  if (!palette.open) return;
  palette.open = false;
  palette.query = '';
  palette.selectedIndex = 0;
  announce('Command palette closed');
  const target = lastFocus;
  lastFocus = null;
  if (target && typeof target.focus === 'function') {
    queueMicrotask(() => target.focus());
  }
}

export function togglePalette(commandCount: number): void {
  if (palette.open) closePalette();
  else openPalette(commandCount);
}

// Execute a command with the overlay mutex + announcement contract.
// `try/finally` guarantees palette closes even if `run()` throws.
export function execute(cmd: Command): void {
  if (cmd.when && !cmd.when()) return;
  // Stash the label first — closePalette() resets state we want for the
  // announce below, and the post-execute message uses a different word stem
  // ("executed") than the open/result messages so REPEAT_WINDOW_MS dedup in
  // announce.svelte.ts can't accidentally swallow it.
  const label = cmd.label;
  try {
    closePalette();
    cmd.run();
  } finally {
    announce(`Command executed: ${label}`);
  }
}

// ---- fuzzy scoring (simplified vs finder.svelte.ts — no basename bonus) ----

export interface CommandHit {
  cmd: Command;
  score: number;
  // Matched character indices into the *label* string (used for <mark>
  // highlight). We only highlight the label, not id/keywords/category, since
  // the visible row text is the label.
  positions: number[];
}

export function scoreCommand(cmd: Command, query: string): CommandHit | null {
  if (query === '') {
    return { cmd, score: 0, positions: [] };
  }
  const q = query.toLowerCase();
  // Build the haystack: label + id + category + keywords. We score against
  // the label first (so its match positions can be used for highlight),
  // falling back to the wider haystack for "matched-anywhere" credit.
  const label = cmd.label;
  const labelLower = label.toLowerCase();
  const positions: number[] = [];
  let qi = 0;
  let ti = 0;
  let score = 0;
  let runLen = 0;
  while (qi < q.length && ti < labelLower.length) {
    if (labelLower[ti] === q[qi]) {
      positions.push(ti);
      score += 2;
      runLen += 1;
      if (runLen > 1) score += runLen * 2;
      if (ti === 0) score += 4;
      if (ti > 0) {
        const prev = labelLower[ti - 1];
        if (prev === ' ' || prev === '.' || prev === '_' || prev === '-' || prev === ':') {
          score += 2;
        }
      }
      qi += 1;
      ti += 1;
    } else {
      runLen = 0;
      ti += 1;
    }
  }
  if (qi >= q.length) {
    // Label fully matched — apply length penalty so shorter labels win.
    score -= Math.floor(label.length / 20);
    return { cmd, score, positions };
  }
  // Label didn't fully match — try the wider haystack (no highlight).
  const wide = `${cmd.id} ${cmd.category} ${cmd.keywords.join(' ')}`.toLowerCase();
  qi = 0;
  ti = 0;
  let altScore = 0;
  while (qi < q.length && ti < wide.length) {
    if (wide[ti] === q[qi]) {
      altScore += 1;
      qi += 1;
    }
    ti += 1;
  }
  if (qi < q.length) return null;
  return { cmd, score: altScore, positions: [] };
}
