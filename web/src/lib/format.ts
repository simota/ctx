// Formatting helpers shared across components.

export function formatTokens(n: number | undefined | null): string {
  if (n === undefined || n === null) return '—';
  if (n < 1000) return String(n);
  if (n < 10_000) return (n / 1000).toFixed(1) + 'k';
  if (n < 1_000_000) return Math.round(n / 1000) + 'k';
  return (n / 1_000_000).toFixed(1) + 'M';
}

export function formatSize(bytes: number | undefined | null): string {
  if (bytes === undefined || bytes === null) return '—';
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(2)} MB`;
}

// Format Unix st_mode permission bits as a symbolic string, e.g.
// 0o100644 -> "-rw-r--r--", 0o40755 -> "drwxr-xr-x". Returns "" for 0/undefined.
export function formatMode(mode: number | undefined | null): string {
  if (!mode) return '';
  const typeChar =
    (mode & 0o170000) === 0o040000 ? 'd' :
    (mode & 0o170000) === 0o120000 ? 'l' : '-';
  const rwx = (bits: number, special: '' | 's' | 't' = ''): string => {
    let s = (bits & 4 ? 'r' : '-') + (bits & 2 ? 'w' : '-');
    if (special && bits & 1) s += special;
    else if (special) s += special.toUpperCase();
    else s += bits & 1 ? 'x' : '-';
    return s;
  };
  const setuid = mode & 0o4000 ? 's' : '';
  const setgid = mode & 0o2000 ? 's' : '';
  const sticky = mode & 0o1000 ? 't' : '';
  return (
    typeChar +
    rwx((mode >> 6) & 7, setuid) +
    rwx((mode >> 3) & 7, setgid) +
    rwx(mode & 7, sticky)
  );
}

// Octal permission string (last 4 digits), e.g. 0o100644 -> "0644".
export function formatModeOctal(mode: number | undefined | null): string {
  if (!mode) return '';
  return '0' + (mode & 0o7777).toString(8).padStart(3, '0');
}

// Absolute local datetime for a Unix epoch-seconds timestamp; "" when falsy.
export function formatDateTime(epochSeconds: number | undefined | null): string {
  if (!epochSeconds) return '';
  return new Date(epochSeconds * 1000).toLocaleString();
}

export function gitColor(g: string | undefined | null): string {
  switch (g) {
    case 'A':
    case 'added':
      return 'var(--ctx-git-added)';
    case 'M':
    case 'modified':
      return 'var(--ctx-git-modified)';
    case 'D':
    case 'deleted':
      return 'var(--ctx-git-deleted)';
    case '?':
    case 'untracked':
      return 'var(--ctx-git-untracked)';
    default:
      return 'var(--ctx-fg-dim)';
  }
}

export function gitLabel(g: string | undefined | null): string {
  if (!g) return '';
  if (g.length === 1) return g;
  return g.charAt(0).toUpperCase();
}

// Long-form git status word for screen-reader aria-labels. Visual sigils stay
// in `gitLabel`; this exists to make `M / A / D / ?` intelligible aloud.
export function gitStatusName(g: string | undefined | null): string {
  if (!g) return '';
  switch (g) {
    case 'A':
    case 'added':
      return 'added';
    case 'M':
    case 'modified':
      return 'modified';
    case 'D':
    case 'deleted':
      return 'deleted';
    case '?':
    case 'untracked':
      return 'untracked';
    case 'R':
    case 'renamed':
      return 'renamed';
    case 'C':
    case 'copied':
      return 'copied';
    case 'T':
    case 'type-changed':
    case 'type changed':
      return 'type changed';
    default:
      return g;
  }
}

// Compact relative time string for a Unix epoch seconds timestamp.
// Returns "" for 0 or undefined (caller should gate on that).
// Positive diff (past): "3m", "2h", "2d", "3w", "5mo", "1y".
// Negative diff (future): "in 3d", "in 2h", etc.
export function formatRelative(epochSeconds: number, now?: Date): string {
  if (!epochSeconds) return '';
  const base = now ?? new Date();
  const diffMs = base.getTime() - epochSeconds * 1000;
  const abs = Math.abs(diffMs);
  const prefix = diffMs < 0 ? 'in ' : '';

  const secs = Math.floor(abs / 1000);
  if (secs < 60) return `${prefix}now`;

  const mins = Math.floor(abs / 60_000);
  if (mins < 60) return `${prefix}${mins}m`;

  const hours = Math.floor(abs / 3_600_000);
  if (hours < 24) return `${prefix}${hours}h`;

  const days = Math.floor(abs / 86_400_000);
  if (days < 7) return `${prefix}${days}d`;

  const weeks = Math.floor(days / 7);
  if (days < 30) return `${prefix}${weeks}w`;

  const months = Math.floor(days / 30);
  if (days < 365) return `${prefix}${months}mo`;

  const years = Math.floor(days / 365);
  return `${prefix}${years}y`;
}

export function basename(path: string): string {
  const i = path.lastIndexOf('/');
  return i === -1 ? path : path.slice(i + 1);
}

export function dirname(path: string): string {
  const i = path.lastIndexOf('/');
  return i === -1 ? '' : path.slice(0, i);
}

// Source-code extensions used by the "Largest" view to keep its ranking
// limited to actual code (excludes docs, config, data, lockfiles, assets).
// Broader than the backend `role==="core"` classification, which only covers
// ts/tsx/js/go/py/rs and so drops svelte/css/etc.
const SOURCE_EXTENSIONS = new Set<string>([
  'ts', 'tsx', 'js', 'jsx', 'mjs', 'cjs', 'svelte', 'vue', 'astro',
  'go', 'rs', 'py', 'rb', 'php', 'java', 'kt', 'kts', 'scala', 'swift', 'dart',
  'c', 'h', 'cc', 'cpp', 'cxx', 'hpp', 'hh', 'cs', 'm', 'mm',
  'css', 'scss', 'sass', 'less', 'html',
  'sh', 'bash', 'zsh', 'fish', 'lua', 'pl', 'pm', 'r', 'sql',
  'ex', 'exs', 'erl', 'clj', 'cljs', 'hs', 'ml', 'vim',
]);

// Whether `path` points at a source-code file (by extension). Dotfiles with no
// real extension (e.g. ".gitignore") and extensionless files are not source.
export function isSourceCode(path: string): boolean {
  const base = (path.slice(path.lastIndexOf('/') + 1)).toLowerCase();
  const dot = base.lastIndexOf('.');
  if (dot <= 0) return false;
  return SOURCE_EXTENSIONS.has(base.slice(dot + 1));
}

// crude language detection from extension; the API may also return `lang`.
export function langFromPath(path: string): string {
  // Build files identify by basename, not extension — `Makefile`,
  // `Dockerfile`, `Dockerfile.dev` etc. have no (or a misleading) suffix, so
  // resolve them before the extension map. Both languages are registered on
  // the shared hljs instance (makefile via the common bundle, dockerfile
  // explicitly in lib/highlight).
  const base = path.slice(path.lastIndexOf('/') + 1);
  if (/^(GNUmakefile|[Mm]akefile)$/.test(base) || /\.(mk|make|mak)$/i.test(base)) return 'makefile';
  if (/^(Dockerfile|Containerfile)(\.[\w.-]+)?$/i.test(base) || /\.dockerfile$/i.test(base)) {
    return 'dockerfile';
  }
  const ext = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  const map: Record<string, string> = {
    go: 'go',
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    svelte: 'svelte',
    vue: 'vue',
    html: 'xml',
    css: 'css',
    json: 'json',
    md: 'markdown',
    yml: 'yaml',
    yaml: 'yaml',
    toml: 'ini',
    sh: 'bash',
    bash: 'bash',
    py: 'python',
    rs: 'rust',
    swift: 'swift',
    kt: 'kotlin',
    kts: 'kotlin',
    sql: 'sql',
    // mermaid isn't in highlight.js's common bundle; falling back to
    // plaintext keeps the source-view path safe (the rendered preview is
    // handled by FileDetail's mermaid pipeline).
    mmd: 'plaintext',
    mermaid: 'plaintext',
  };
  return map[ext] ?? 'plaintext';
}
