// `?` keyboard cheatsheet modal state.
// Mirrors the open/close pattern used by `finder.svelte.ts`.

import { announce } from './announce.svelte';

interface CheatsheetState {
  open: boolean;
}

export const cheatsheet = $state<CheatsheetState>({ open: false });

let lastFocus: HTMLElement | null = null;

export function openCheatsheet(): void {
  if (cheatsheet.open) return;
  lastFocus = (document.activeElement as HTMLElement) ?? null;
  cheatsheet.open = true;
  announce('Keyboard cheatsheet opened');
}

export function closeCheatsheet(): void {
  if (!cheatsheet.open) return;
  cheatsheet.open = false;
  announce('Keyboard cheatsheet closed');
  const target = lastFocus;
  lastFocus = null;
  if (target && typeof target.focus === 'function') {
    queueMicrotask(() => target.focus());
  }
}

export function toggleCheatsheet(): void {
  if (cheatsheet.open) closeCheatsheet();
  else openCheatsheet();
}
