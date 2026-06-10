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
const inflight: Map<string, Promise<DefinitionCandidate[]>> = new Map();

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
  const existing = inflight.get(key);
  if (existing) return existing;
  const p = fetchDefinition(name, { from, signal })
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
  inflight.set(key, p);
  return p;
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
