// State for the RootsPicker overlay. Mirrors the lifecycle of
// definition-picker.svelte.ts: open/close, lastFocus restore, announce. The
// picker has a live filter (`query`) plus transient `spawnError` for
// inline error surfacing after a failed POST /api/roots/open, and a
// `manageError` slot for failures from POST /api/roots or DELETE /api/roots.

import { announce } from './announce.svelte';
import { loadRoots, roots } from './roots.svelte';

export interface RootsPickerState {
  open: boolean;
  selectedIndex: number;
  // Live filter — matches against name and path (case-insensitive).
  query: string;
  // Set when a POST /api/roots/open fails so the dialog can render an inline
  // banner. Cleared on next selection move or on close.
  spawnError: string | null;
  // Set while a POST /api/roots/open is in flight so the row shows a busy
  // affordance and Enter is debounced.
  spawning: boolean;
  // Set when register / remove fail. Separate slot from spawnError so a
  // failed delete doesn't get clobbered by a subsequent arrow-key move.
  manageError: string | null;
  // Generic in-flight flag for register / remove so buttons disable.
  managing: boolean;
}

export const rootsPicker = $state<RootsPickerState>({
  open: false,
  selectedIndex: 0,
  query: '',
  spawnError: null,
  spawning: false,
  manageError: null,
  managing: false,
});

let lastFocus: HTMLElement | null = null;

export function openRootsPicker(): void {
  if (rootsPicker.open) return;
  lastFocus = (document.activeElement as HTMLElement) ?? null;
  rootsPicker.open = true;
  rootsPicker.selectedIndex = 0;
  rootsPicker.query = '';
  rootsPicker.spawnError = null;
  rootsPicker.spawning = false;
  rootsPicker.manageError = null;
  rootsPicker.managing = false;
  announce('Switch project root');
  // Fire-and-forget: the picker renders a loading state until the promise
  // resolves; errors land in roots.error and the picker shows them inline.
  void loadRoots();
}

export function closeRootsPicker(): void {
  if (!rootsPicker.open) return;
  rootsPicker.open = false;
  rootsPicker.query = '';
  rootsPicker.spawnError = null;
  rootsPicker.spawning = false;
  rootsPicker.manageError = null;
  rootsPicker.managing = false;
  const target = lastFocus;
  lastFocus = null;
  if (target && typeof target.focus === 'function') {
    queueMicrotask(() => target.focus());
  }
}

export function clearSpawnError(): void {
  if (rootsPicker.spawnError !== null) rootsPicker.spawnError = null;
}

export function clearManageError(): void {
  if (rootsPicker.manageError !== null) rootsPicker.manageError = null;
}
