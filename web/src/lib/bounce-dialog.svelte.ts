// BounceDialog open/close state — single global flag so App.svelte can mount
// the dialog at root level (same pattern as RootsPicker, FuzzyFinder, etc.).

interface BounceDialogState {
  open: boolean;
}

export const bounceDialog = $state<BounceDialogState>({ open: false });

export function openBounceDialog(): void {
  bounceDialog.open = true;
}

export function closeBounceDialog(): void {
  bounceDialog.open = false;
}
