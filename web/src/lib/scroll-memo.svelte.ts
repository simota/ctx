// Per-path scrollTop memoization. Session-only (no persistence) — when the user
// switches tabs we want to restore where they were reading, but we don't want
// to surprise them with stale positions after a reload.

const memo = new Map<string, number>();

export function rememberScroll(path: string, top: number): void {
  if (!path) return;
  if (!Number.isFinite(top) || top < 0) return;
  memo.set(path, top);
}

export function recallScroll(path: string): number | undefined {
  if (!path) return undefined;
  return memo.get(path);
}

export function forgetScroll(path: string): void {
  memo.delete(path);
}

export function clearScrollMemo(): void {
  memo.clear();
}
