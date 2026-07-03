import { basename } from './format';

export interface PinEntry {
  path: string;
  pinnedAt: number;
  lastOpenedAt?: number;
  label?: string;
  stale?: boolean;
}

interface PinsState {
  root: string;
  entries: PinEntry[];
  loaded: boolean;
  persistenceWarning: string;
  actionMessage: string;
}

export interface PinResult {
  ok: boolean;
  status:
    | 'pinned'
    | 'unpinned'
    | 'already-pinned'
    | 'not-pinned'
    | 'invalid-path'
    | 'limit'
    | 'no-root'
    | 'cleared'
    | 'moved'
    | 'noop';
  message: string;
}

export const PIN_LIMIT = 200;

export const pins = $state<PinsState>({
  root: '',
  entries: [],
  loaded: false,
  persistenceWarning: '',
  actionMessage: '',
});

export function pinsStorageKey(root: string): string {
  return `ctx-pins:v1:${root}`;
}

function normalizePath(path: string): string | null {
  if (!path || path === '.' || path === '/') return null;
  if (path.endsWith('/')) return null;
  return path;
}

function entryLabel(path: string): string {
  return basename(path);
}

function safeNow(): number {
  return Date.now();
}

function readEntries(root: string): { entries: PinEntry[]; warning: string } {
  if (typeof localStorage === 'undefined') {
    return { entries: [], warning: 'Pins are available for this session, but localStorage is unavailable.' };
  }
  try {
    const raw = localStorage.getItem(pinsStorageKey(root));
    if (!raw) return { entries: [], warning: '' };
    const parsed = JSON.parse(raw) as unknown;
    const rawEntries: unknown[] = Array.isArray(parsed)
      ? parsed
      : typeof parsed === 'object' && parsed !== null && Array.isArray((parsed as { entries?: unknown }).entries)
        ? ((parsed as { entries: unknown[] }).entries)
        : [];
    const out: PinEntry[] = [];
    const seen = new Set<string>();
    for (const item of rawEntries) {
      const candidate =
        typeof item === 'string'
          ? { path: item }
          : typeof item === 'object' && item !== null
            ? (item as Partial<PinEntry>)
            : null;
      if (!candidate) continue;
      const entry = candidate as Partial<PinEntry>;
      const path = normalizePath(entry.path ?? '');
      if (!path || seen.has(path)) continue;
      seen.add(path);
      out.push({
        path,
        pinnedAt: Number.isFinite(entry.pinnedAt) ? Number(entry.pinnedAt) : safeNow(),
        lastOpenedAt: Number.isFinite(entry.lastOpenedAt)
          ? Number(entry.lastOpenedAt)
          : undefined,
        label: entry.label || entryLabel(path),
        stale: !!entry.stale,
      });
      if (out.length >= PIN_LIMIT) break;
    }
    return { entries: out, warning: '' };
  } catch {
    return {
      entries: [],
      warning: 'Pins are available for this session, but saved pins could not be read.',
    };
  }
}

function persist(): boolean {
  if (!pins.root) return false;
  if (typeof localStorage === 'undefined') {
    pins.persistenceWarning = 'Pins are available for this session, but localStorage is unavailable.';
    return false;
  }
  try {
    localStorage.setItem(
      pinsStorageKey(pins.root),
      JSON.stringify({ version: 1, entries: pins.entries }),
    );
    pins.persistenceWarning = '';
    return true;
  } catch {
    pins.persistenceWarning = 'Pins may not persist after reload because localStorage could not be written.';
    return false;
  }
}

function setMessage(message: string): void {
  pins.actionMessage = message;
}

function result(status: PinResult['status'], ok: boolean, message: string): PinResult {
  setMessage(message);
  return { ok, status, message };
}

function withPersistenceMessage(base: string, persisted: boolean): string {
  return persisted ? base : `${base} Pins may not persist after reload.`;
}

export function ensurePinsRoot(root: string): void {
  if (!root) return;
  if (pins.loaded && pins.root === root) return;
  const { entries, warning } = readEntries(root);
  pins.root = root;
  pins.entries = entries;
  pins.loaded = true;
  pins.persistenceWarning = warning;
  pins.actionMessage = '';
}

export function isPinned(path: string): boolean {
  const normalized = normalizePath(path);
  if (!normalized) return false;
  return pins.entries.some((entry) => entry.path === normalized);
}

export function pinFile(path: string): PinResult {
  if (!pins.root) {
    return result('no-root', false, 'Cannot pin file until the project root is loaded.');
  }
  const normalized = normalizePath(path);
  if (!normalized) return result('invalid-path', false, 'Cannot pin this path.');
  if (isPinned(normalized)) {
    return result('already-pinned', true, 'File is already pinned.');
  }
  if (pins.entries.length >= PIN_LIMIT) {
    return result('limit', false, `Pin limit reached (${PIN_LIMIT} files).`);
  }
  pins.entries.push({
    path: normalized,
    pinnedAt: safeNow(),
    label: entryLabel(normalized),
  });
  const persisted = persist();
  return result('pinned', true, withPersistenceMessage(`Pinned ${normalized}`, persisted));
}

export function unpinFile(path: string): PinResult {
  const normalized = normalizePath(path);
  if (!normalized) return result('invalid-path', false, 'Cannot unpin this path.');
  const before = pins.entries.length;
  pins.entries = pins.entries.filter((entry) => entry.path !== normalized);
  if (pins.entries.length === before) {
    return result('not-pinned', true, 'File was not pinned.');
  }
  const persisted = persist();
  return result('unpinned', true, withPersistenceMessage(`Unpinned ${normalized}`, persisted));
}

export function togglePin(path: string): PinResult {
  return isPinned(path) ? unpinFile(path) : pinFile(path);
}

export function clearPins(): PinResult {
  if (pins.entries.length === 0) return result('noop', true, 'No pinned files to clear.');
  pins.entries = [];
  const persisted = persist();
  return result('cleared', true, withPersistenceMessage('Cleared pinned files.', persisted));
}

export function movePin(path: string, delta: -1 | 1): PinResult {
  const normalized = normalizePath(path);
  if (!normalized) return result('invalid-path', false, 'Cannot move this path.');
  const idx = pins.entries.findIndex((entry) => entry.path === normalized);
  if (idx === -1) return result('not-pinned', false, 'File is not pinned.');
  const next = idx + delta;
  if (next < 0 || next >= pins.entries.length) {
    return result('noop', true, 'Pinned file is already at the edge.');
  }
  const [entry] = pins.entries.splice(idx, 1);
  pins.entries.splice(next, 0, entry);
  const persisted = persist();
  return result('moved', true, withPersistenceMessage(`Moved ${normalized}`, persisted));
}

export function recordPinOpened(path: string): void {
  const normalized = normalizePath(path);
  if (!normalized) return;
  const entry = pins.entries.find((item) => item.path === normalized);
  if (!entry) return;
  entry.lastOpenedAt = safeNow();
  entry.stale = false;
  persist();
}

export function markPinStale(path: string): void {
  const normalized = normalizePath(path);
  if (!normalized) return;
  const entry = pins.entries.find((item) => item.path === normalized);
  if (!entry) return;
  entry.stale = true;
  persist();
}
