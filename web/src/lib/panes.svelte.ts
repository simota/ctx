// Two-pane (vertical split) state. The left pane's active path lives on
// `route.path`; the right pane keeps its own path here. We share the tab list
// (`tabs.paths`) between panes so the user has a single mental model of
// "open files", and only which pane an action targets depends on `focused`.
//
// Persistence: only `splitPercent` survives a reload (cosmetic preference).
// `rightOpen` / `rightPath` / `focused` are URL-driven (`?right=<path>`) so
// shareable links recreate the layout but a plain reload starts in 1-pane.

export type PaneSide = 'left' | 'right';

interface PanesState {
  rightOpen: boolean;
  rightPath: string;
  focused: PaneSide;
  splitPercent: number;
}

const SPLIT_KEY = 'ctx-pane-split';
const MIN_SPLIT = 15;
const MAX_SPLIT = 85;
const DEFAULT_SPLIT = 50;

function readSplitPref(): number {
  if (typeof localStorage === 'undefined') return DEFAULT_SPLIT;
  try {
    const raw = localStorage.getItem(SPLIT_KEY);
    if (!raw) return DEFAULT_SPLIT;
    const n = Number(raw);
    if (!Number.isFinite(n)) return DEFAULT_SPLIT;
    return clampSplit(n);
  } catch {
    return DEFAULT_SPLIT;
  }
}

function writeSplitPref(n: number): void {
  if (typeof localStorage === 'undefined') return;
  try {
    localStorage.setItem(SPLIT_KEY, String(n));
  } catch {
    // ignore — private mode / quota
  }
}

function clampSplit(n: number): number {
  if (!Number.isFinite(n)) return DEFAULT_SPLIT;
  return Math.max(MIN_SPLIT, Math.min(MAX_SPLIT, Math.round(n)));
}

export const panes = $state<PanesState>({
  rightOpen: false,
  rightPath: '',
  focused: 'left',
  splitPercent: readSplitPref(),
});

export function openRight(path: string): void {
  panes.rightOpen = true;
  if (path) panes.rightPath = path;
  panes.focused = 'right';
}

export function closeRight(): void {
  panes.rightOpen = false;
  panes.focused = 'left';
}

export function setFocused(side: PaneSide): void {
  if (!panes.rightOpen && side === 'right') return; // can't focus what isn't there
  panes.focused = side;
}

export function setSplitPercent(n: number): void {
  const clamped = clampSplit(n);
  if (panes.splitPercent === clamped) return;
  panes.splitPercent = clamped;
  writeSplitPref(clamped);
}

export const SPLIT_MIN = MIN_SPLIT;
export const SPLIT_MAX = MAX_SPLIT;
export const SPLIT_DEFAULT = DEFAULT_SPLIT;
