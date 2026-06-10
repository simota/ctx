// Polite live-region announcer for screen readers.
// One global $state owns the current message + a monotonic `key` so identical
// messages still re-trigger SR readout (aria-atomic="true" alone is not enough
// when only the text changes).

interface AnnounceState {
  message: string;
  key: number;
}

export const announceState = $state<AnnounceState>({ message: '', key: 0 });

// Drop a repeat of the same message within this window — prevents bursts from
// multiple effects landing on the same tick.
const REPEAT_WINDOW_MS = 100;
let lastMessage = '';
let lastAt = 0;

export function announce(message: string): void {
  if (typeof window === 'undefined') return;
  if (!message) return;
  const now =
    typeof performance !== 'undefined' && typeof performance.now === 'function'
      ? performance.now()
      : Date.now();
  if (message === lastMessage && now - lastAt < REPEAT_WINDOW_MS) return;
  lastMessage = message;
  lastAt = now;
  announceState.message = message;
  announceState.key += 1;
}
