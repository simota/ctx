// State for the cross-file DefinitionPicker overlay.
//
// Mirrors the lifecycle of finder/palette/cheatsheet stores: open/close,
// announce hooks, and lastFocus restore for keyboard parity. The picker is
// pure-selection (no live filter input) so we don't carry a query field.

import { announce } from './announce.svelte';
import type { DefinitionCandidate } from './api';

export interface DefinitionPickerState {
  open: boolean;
  name: string;
  candidates: DefinitionCandidate[];
  selectedIndex: number;
}

export const definitionPicker = $state<DefinitionPickerState>({
  open: false,
  name: '',
  candidates: [],
  selectedIndex: 0,
});

let lastFocus: HTMLElement | null = null;

export function openDefinitionPicker(
  name: string,
  candidates: DefinitionCandidate[],
): void {
  if (definitionPicker.open) return;
  if (candidates.length === 0) return;
  lastFocus = (document.activeElement as HTMLElement) ?? null;
  definitionPicker.open = true;
  definitionPicker.name = name;
  definitionPicker.candidates = candidates;
  definitionPicker.selectedIndex = 0;
  // Distinct word stem from "Jumped to" so REPEAT_WINDOW_MS dedup in
  // announce.svelte.ts can't swallow back-to-back navigate/picker events.
  announce(`Multiple definitions for ${name}`);
}

export function closeDefinitionPicker(): void {
  if (!definitionPicker.open) return;
  definitionPicker.open = false;
  definitionPicker.name = '';
  definitionPicker.candidates = [];
  definitionPicker.selectedIndex = 0;
  const target = lastFocus;
  lastFocus = null;
  if (target && typeof target.focus === 'function') {
    queueMicrotask(() => target.focus());
  }
}
