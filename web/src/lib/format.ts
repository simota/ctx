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

// crude language detection from extension; the API may also return `lang`.
export function langFromPath(path: string): string {
  const ext = path.slice(path.lastIndexOf('.') + 1).toLowerCase();
  const map: Record<string, string> = {
    go: 'go',
    ts: 'typescript',
    tsx: 'typescript',
    js: 'javascript',
    jsx: 'javascript',
    svelte: 'svelte',
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
    sql: 'sql',
    // mermaid isn't in highlight.js's common bundle; falling back to
    // plaintext keeps the source-view path safe (the rendered preview is
    // handled by FileDetail's mermaid pipeline).
    mmd: 'plaintext',
    mermaid: 'plaintext',
  };
  return map[ext] ?? 'plaintext';
}
