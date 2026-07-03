// Cross-file symbol definition lookup with a session-scoped LRU cache.
//
// Hover-driven anchorisation in FileDetail produces many small fetches; cache
// hits keep the picker latency-free for symbols the user has already touched.
// We key on `${name}|${from ?? ''}` because the backend's ranking is anchor-
// dependent (same-directory wins are different per `from`).
//
// LRU: Map preserves insertion order, so re-inserting on hit promotes; size
// cap is 5000 entries (~10MB upper bound assuming ~100 candidates @ ~200B).

import {
  fetchDefinition,
  type DefinitionCandidate,
  type DefinitionResponse,
} from './api';

const MAX_ENTRIES = 5000;

const cache: Map<string, DefinitionCandidate[]> = new Map();

interface InflightEntry {
  promise: Promise<DefinitionCandidate[]>;
  // Own controller for the outbound fetch, decoupled from any single waiter's
  // signal — see lookup() for why.
  controller: AbortController;
  waiters: number;
}

const inflight: Map<string, InflightEntry> = new Map();

function keyFor(name: string, from?: string): string {
  return `${name}|${from ?? ''}`;
}

function recordHit(key: string, value: DefinitionCandidate[]): void {
  // Re-insert to move to MRU position.
  if (cache.has(key)) cache.delete(key);
  cache.set(key, value);
  if (cache.size > MAX_ENTRIES) {
    // Drop least-recently-used (oldest insertion).
    const oldest = cache.keys().next().value as string | undefined;
    if (oldest !== undefined) cache.delete(oldest);
  }
}

// lookup: cache-first, single-flight per key. AbortSignal aborts only the
// outbound network request; cache reads always resolve synchronously.
//
// Multiple callers can race for the same key (e.g. two hovers over the same
// symbol before the first resolves). They share one outbound fetch, but each
// carries its own AbortSignal — the shared fetch is only aborted once every
// waiter for that key has aborted (refcounted), so one caller giving up
// doesn't starve the others still waiting on a valid result.
export function lookup(
  name: string,
  from?: string,
  signal?: AbortSignal,
): Promise<DefinitionCandidate[]> {
  if (!name) return Promise.resolve([]);
  const key = keyFor(name, from);
  const cached = cache.get(key);
  if (cached !== undefined) {
    // Touch MRU.
    recordHit(key, cached);
    return Promise.resolve(cached);
  }
  let entry = inflight.get(key);
  if (!entry) {
    const controller = new AbortController();
    const promise = fetchDefinition(name, { from, signal: controller.signal })
      .then((r: DefinitionResponse) => {
        const list = r.candidates ?? [];
        recordHit(key, list);
        return list;
      })
      .catch((e: unknown) => {
        // Aborted hovers are an expected outcome — surface as 0 candidates
        // without polluting the cache so the next hover can retry.
        if (e instanceof DOMException && e.name === 'AbortError') return [];
        // Other failures: cache an empty list briefly so a broken backend
        // doesn't generate a hover-storm; user can still click the gutter
        // line link to navigate manually.
        recordHit(key, []);
        return [];
      })
      .finally(() => {
        inflight.delete(key);
      });
    entry = { promise, controller, waiters: 0 };
    inflight.set(key, entry);
  }
  entry.waiters++;
  if (signal) {
    const e = entry;
    const onAbort = () => {
      e.waiters--;
      if (e.waiters <= 0) e.controller.abort();
    };
    if (signal.aborted) onAbort();
    else signal.addEventListener('abort', onAbort, { once: true });
  }
  return entry.promise;
}

// peek: synchronous cache check (used for instant anchor decoration of
// previously-seen symbols on render).
export function peek(name: string, from?: string): DefinitionCandidate[] | undefined {
  return cache.get(keyFor(name, from));
}

// clearDefinitions: test/debug helper. Not wired to any UI surface.
export function clearDefinitions(): void {
  cache.clear();
  inflight.clear();
}
