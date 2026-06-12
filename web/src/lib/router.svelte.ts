// Hash router based on Svelte 5 runes.
// Routes:
//   #/                       -> tree
//   #/tree                   -> tree
//   #/file/<path>            -> file detail (path may contain slashes)
//   #/file/<path>?L=<n>      -> file detail with line hint (scroll + highlight)
//   #/dir                    -> directory overview (root)
//   #/dir/<path>             -> directory overview for <path>
//   #/search?q=<query>       -> search results
//   #/budget                 -> budget panel
//   #/replay                 -> replay snapshot list
//   #/replay/<id>            -> replay snapshot detail
// The path segment after #/file/ or #/dir/ is treated as the full remaining
// path (no further slash splitting), so paths like "internal/cli/pack.go"
// round-trip correctly. Snapshot ids are opaque strings (no slashes expected,
// but we treat the suffix as the full id for forward compatibility).

export type RouteName = 'tree' | 'file' | 'dir' | 'search' | 'budget' | 'replay' | 'mixdowns';
export type FileViewMode = 'diff' | 'history';

export interface Route {
  name: RouteName;
  path: string;
  query: string;
  lineHint?: number;
  // Additional file paths to pre-open as tabs (from `?open=A,B,C`). Always
  // present (empty array when no `open` param). Active file (route.path) may
  // or may not be in this list — caller is responsible for merging.
  openPaths: string[];
  // Right-pane active path from `?right=<path>` (file route only). Empty
  // string means "no right pane". Lives on Route so deep links recreate the
  // 2-pane layout, but day-to-day pane state lives in `lib/panes.svelte.ts`.
  rightPath: string;
  // File detail display mode. Only file routes currently consume it; undefined
  // means the normal source/preview view.
  mode?: FileViewMode;
  // Date filter params for the tree view. Parsed from `#/tree?since=&until=`.
  since?: string;
  until?: string;
  // When true, use file mtime instead of git commit time for date filtering.
  useMtime?: boolean;
}

function parseOpenParam(raw: string | null): string[] {
  if (!raw) return [];
  const out: string[] = [];
  const seen = new Set<string>();
  for (const token of raw.split(',')) {
    if (!token) continue;
    let decoded: string;
    try {
      decoded = decodeURIComponent(token);
    } catch {
      decoded = token;
    }
    if (!decoded || seen.has(decoded)) continue;
    seen.add(decoded);
    out.push(decoded);
  }
  return out;
}

function parse(rawHash: string): Route {
  const hash = rawHash.startsWith('#') ? rawHash.slice(1) : rawHash;
  if (!hash || hash === '/' || hash === '') {
    return { name: 'tree', path: '', query: '', openPaths: [], rightPath: '' };
  }

  // split off ?query
  const qIdx = hash.indexOf('?');
  const beforeQ = qIdx === -1 ? hash : hash.slice(0, qIdx);
  const afterQ = qIdx === -1 ? '' : hash.slice(qIdx + 1);
  const queryParams = new URLSearchParams(afterQ);

  // strip leading slash
  const p = beforeQ.startsWith('/') ? beforeQ.slice(1) : beforeQ;

  if (p === '' || p === 'tree') {
    const since = queryParams.get('since') ?? undefined;
    const until = queryParams.get('until') ?? undefined;
    const useMtimeRaw = queryParams.get('use_mtime');
    const useMtime = useMtimeRaw === 'true' ? true : undefined;
    return { name: 'tree', path: '', query: '', openPaths: [], rightPath: '', since, until, useMtime };
  }
  if (p === 'budget') {
    return { name: 'budget', path: '', query: '', openPaths: [], rightPath: '' };
  }
  if (p === 'search') {
    return {
      name: 'search',
      path: '',
      query: queryParams.get('q') ?? '',
      openPaths: [],
      rightPath: '',
    };
  }
  if (p.startsWith('file/')) {
    const lineRaw = queryParams.get('L');
    const lineHint = lineRaw && /^\d+$/.test(lineRaw) ? Number(lineRaw) : undefined;
    const modeRaw = queryParams.get('mode');
    const mode: FileViewMode | undefined =
      modeRaw === 'diff' || modeRaw === 'history' ? modeRaw : undefined;
    const rightRaw = queryParams.get('right');
    let rightPath = '';
    if (rightRaw) {
      try {
        rightPath = decodeURIComponent(rightRaw);
      } catch {
        rightPath = rightRaw;
      }
    }
    const since = queryParams.get('since') ?? undefined;
    const until = queryParams.get('until') ?? undefined;
    const useMtime = queryParams.get('use_mtime') === 'true' ? true : undefined;
    return {
      name: 'file',
      path: decodeURIComponent(p.slice('file/'.length)),
      query: '',
      lineHint,
      openPaths: parseOpenParam(queryParams.get('open')),
      rightPath,
      mode,
      since,
      until,
      useMtime,
    };
  }
  if (p === 'dir') {
    const since = queryParams.get('since') ?? undefined;
    const until = queryParams.get('until') ?? undefined;
    const useMtime = queryParams.get('use_mtime') === 'true' ? true : undefined;
    return { name: 'dir', path: '', query: '', openPaths: [], rightPath: '', since, until, useMtime };
  }
  if (p.startsWith('dir/')) {
    const since = queryParams.get('since') ?? undefined;
    const until = queryParams.get('until') ?? undefined;
    const useMtime = queryParams.get('use_mtime') === 'true' ? true : undefined;
    return {
      name: 'dir',
      path: decodeURIComponent(p.slice('dir/'.length)),
      query: '',
      openPaths: [],
      rightPath: '',
      since,
      until,
      useMtime,
    };
  }
  if (p === 'replay') {
    return { name: 'replay', path: '', query: '', openPaths: [], rightPath: '' };
  }
  if (p.startsWith('replay/')) {
    return {
      name: 'replay',
      path: decodeURIComponent(p.slice('replay/'.length)),
      query: '',
      openPaths: [],
      rightPath: '',
    };
  }
  if (p === 'mixdowns') {
    return { name: 'mixdowns', path: '', query: '', openPaths: [], rightPath: '' };
  }
  // unknown -> fallback to tree
  return { name: 'tree', path: '', query: '', openPaths: [], rightPath: '' };
}

const initial: Route =
  typeof window !== 'undefined'
    ? parse(window.location.hash || '#/')
    : { name: 'tree', path: '', query: '', openPaths: [], rightPath: '' };

export const route = $state<Route>({ ...initial });

if (typeof window !== 'undefined') {
  window.addEventListener('hashchange', () => {
    const next = parse(window.location.hash);
    route.name = next.name;
    route.path = next.path;
    route.query = next.query;
    route.lineHint = next.lineHint;
    route.openPaths = next.openPaths;
    route.rightPath = next.rightPath;
    route.mode = next.mode;
    route.since = next.since;
    route.until = next.until;
    route.useMtime = next.useMtime;
  });
}

export function navigate(hash: string): void {
  const target = hash.startsWith('#') ? hash : `#${hash}`;
  if (typeof window !== 'undefined' && window.location.hash !== target) {
    window.location.hash = target;
  }
}

export interface FileHashOpts {
  line?: number;
  open?: string[];
  right?: string;
  mode?: FileViewMode | '';
  since?: string;
  until?: string;
  useMtime?: boolean;
}

// `toFileHash(path)` and `toFileHash(path, lineNumber)` are the historic forms;
// `toFileHash(path, { line, open, right, since, until, useMtime })` adds knobs.
// Date-filter params (since/until/useMtime) auto-inherit from the current
// `route.*` when the caller does not pass them, so navigation between
// tree/dir/file preserves the active filter without every caller threading
// it explicitly. Explicit `undefined` falls through to the inherited value;
// callers that need to clear the filter must pass empty strings / false.
export function toFileHash(path: string, opts?: number | FileHashOpts): string {
  const base = `#/file/${path.split('/').map(encodeURIComponent).join('/')}`;
  const raw: FileHashOpts = typeof opts === 'number' ? { line: opts } : opts ?? {};
  const o: FileHashOpts = {
    ...raw,
    since: raw.since ?? route.since,
    until: raw.until ?? route.until,
    useMtime: raw.useMtime ?? route.useMtime,
  };
  const params: string[] = [];
  if (o.line && o.line > 0) params.push(`L=${o.line}`);
  if (o.open && o.open.length > 0) {
    const enc = o.open.map((p) => encodeURIComponent(p)).join(',');
    params.push(`open=${enc}`);
  }
  if (o.right) {
    params.push(`right=${encodeURIComponent(o.right)}`);
  }
  if (o.mode) params.push(`mode=${encodeURIComponent(o.mode)}`);
  if (o.since) params.push(`since=${encodeURIComponent(o.since)}`);
  if (o.until) params.push(`until=${encodeURIComponent(o.until)}`);
  if (o.useMtime) params.push('use_mtime=true');
  return params.length > 0 ? `${base}?${params.join('&')}` : base;
}

export function toSearchHash(query: string): string {
  const q = new URLSearchParams({ q: query }).toString();
  return `#/search?${q}`;
}

export interface TreeHashOpts {
  since?: string;
  until?: string;
  useMtime?: boolean;
}

export function toTreeHash(opts?: TreeHashOpts): string {
  const o: TreeHashOpts = {
    since: opts?.since ?? route.since,
    until: opts?.until ?? route.until,
    useMtime: opts?.useMtime ?? route.useMtime,
  };
  if (!o.since && !o.until && !o.useMtime) return '#/tree';
  const params = new URLSearchParams();
  if (o.since) params.set('since', o.since);
  if (o.until) params.set('until', o.until);
  if (o.useMtime) params.set('use_mtime', 'true');
  const s = params.toString();
  return s ? `#/tree?${s}` : '#/tree';
}

export function toBudgetHash(): string {
  return '#/budget';
}

export function toReplayHash(id?: string): string {
  if (!id) return '#/replay';
  return `#/replay/${encodeURIComponent(id)}`;
}

export function toMixdownsHash(): string {
  return '#/mixdowns';
}

export interface DirHashOpts {
  since?: string;
  until?: string;
  useMtime?: boolean;
}

// `toDirHash('')` -> `#/dir` (root); `toDirHash('internal/cli')` ->
// `#/dir/internal/cli` with each segment URI-encoded so unusual filenames
// round-trip cleanly through `decodeURIComponent` in `parse()`.
// `toDirHash(path, { since, until, useMtime })` preserves date filter params.
// Date-filter params auto-inherit from the current `route.*` when the caller
// does not pass them (see `toFileHash` for rationale).
export function toDirHash(path: string = '', opts?: DirHashOpts): string {
  const base = !path ? '#/dir' : `#/dir/${path.split('/').map(encodeURIComponent).join('/')}`;
  const o: DirHashOpts = {
    since: opts?.since ?? route.since,
    until: opts?.until ?? route.until,
    useMtime: opts?.useMtime ?? route.useMtime,
  };
  if (!o.since && !o.until && !o.useMtime) return base;
  const params = new URLSearchParams();
  if (o.since) params.set('since', o.since);
  if (o.until) params.set('until', o.until);
  if (o.useMtime) params.set('use_mtime', 'true');
  const s = params.toString();
  return s ? `${base}?${s}` : base;
}
