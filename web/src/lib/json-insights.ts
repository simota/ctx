// JSON insight extraction for the FileDetail sidebar.
//
// Two-layer design:
//   - Universal Outline — top-level keys (or array items) with type and
//     preview so any JSON file gets a structural summary.
//   - Lensed sections — file-type-specific views (only package.json for
//     now; tsconfig and design-token files are candidates for later).
//
// Line numbers are recovered by re-scanning the source text for each
// top-level key with a regex anchored to the start of a line. This works
// because JSON puts each top-level key on its own line under any
// reasonable formatter, and avoids the cost of a position-aware parser
// for what is essentially a viewer aid.

import type { PaletteEntry } from './css-insights';

export type JsonValueType = 'object' | 'array' | 'string' | 'number' | 'boolean' | 'null';

export interface JsonOutlineEntry {
  key: string;
  type: JsonValueType;
  preview: string;
  count?: number;
  line: number;
}

export interface JsonDependency {
  name: string;
  version: string;
  line: number;
}

export interface JsonScript {
  name: string;
  command: string;
  line: number;
}

export interface JsonInsights {
  ok: boolean;
  outline: JsonOutlineEntry[];
  totalKeys: number;
  maxDepth: number;
  palette: PaletteEntry[];
  isPackageJson: boolean;
  deps: JsonDependency[];
  devDeps: JsonDependency[];
  peerDeps: JsonDependency[];
  scripts: JsonScript[];
}

const COLOR_RE = /^(?:#[0-9a-fA-F]{3,8}|(?:rgba?|hsla?|oklch|oklab|lab|lch|color)\([^)]*\))$/;

export function extractJsonInsights(path: string, content: string): JsonInsights {
  const empty: JsonInsights = {
    ok: false,
    outline: [],
    totalKeys: 0,
    maxDepth: 0,
    palette: [],
    isPackageJson: false,
    deps: [],
    devDeps: [],
    peerDeps: [],
    scripts: [],
  };

  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch {
    return empty;
  }

  const isPackageJson = /(?:^|\/)package\.json$/i.test(path);
  const outline = buildOutline(parsed, content);
  const { totalKeys, maxDepth } = stats(parsed);
  const palette = collectColors(parsed, content);
  const deps = isPackageJson ? extractDeps(parsed, content, 'dependencies') : [];
  const devDeps = isPackageJson ? extractDeps(parsed, content, 'devDependencies') : [];
  const peerDeps = isPackageJson ? extractDeps(parsed, content, 'peerDependencies') : [];
  const scripts = isPackageJson ? extractScripts(parsed, content) : [];

  return {
    ok: true,
    outline,
    totalKeys,
    maxDepth,
    palette,
    isPackageJson,
    deps,
    devDeps,
    peerDeps,
    scripts,
  };
}

function buildOutline(parsed: unknown, content: string): JsonOutlineEntry[] {
  if (Array.isArray(parsed)) {
    // For array roots, surface the first ~50 items with their index as key.
    // Useful for fixtures and translation lists; bigger arrays are
    // intentionally truncated since the source view already shows them all.
    const cap = Math.min(parsed.length, 50);
    const out: JsonOutlineEntry[] = [];
    for (let i = 0; i < cap; i++) {
      const v = parsed[i];
      out.push({
        key: `[${i}]`,
        type: typeOf(v),
        preview: previewValue(v),
        count: countOf(v),
        line: findArrayItemLine(content, i),
      });
    }
    return out;
  }
  if (parsed === null || typeof parsed !== 'object') return [];

  return Object.entries(parsed as Record<string, unknown>).map(([key, value]) => ({
    key,
    type: typeOf(value),
    preview: previewValue(value),
    count: countOf(value),
    line: findKeyLine(content, key),
  }));
}

function stats(parsed: unknown): { totalKeys: number; maxDepth: number } {
  let totalKeys = 0;
  let maxDepth = 0;
  const visit = (v: unknown, depth: number) => {
    if (depth > maxDepth) maxDepth = depth;
    if (Array.isArray(v)) {
      for (const item of v) visit(item, depth + 1);
    } else if (v !== null && typeof v === 'object') {
      for (const [, val] of Object.entries(v as Record<string, unknown>)) {
        totalKeys++;
        visit(val, depth + 1);
      }
    }
  };
  visit(parsed, 0);
  return { totalKeys, maxDepth };
}

function collectColors(parsed: unknown, content: string): PaletteEntry[] {
  // Walk every string value; if it matches a colour expression, surface it
  // with the key path as the swatch's name and a line number recovered by
  // text search on the (escaped) value.
  const palette: PaletteEntry[] = [];
  const seen = new Set<string>();
  const visit = (v: unknown, path: string) => {
    if (typeof v === 'string') {
      const trimmed = v.trim();
      if (COLOR_RE.test(trimmed)) {
        const dedupKey = `${path}|${trimmed}`;
        if (seen.has(dedupKey)) return;
        seen.add(dedupKey);
        palette.push({
          line: findValueLine(content, trimmed),
          name: path,
          value: trimmed,
          swatch: trimmed,
        });
      }
    } else if (Array.isArray(v)) {
      v.forEach((item, i) => visit(item, `${path}[${i}]`));
    } else if (v !== null && typeof v === 'object') {
      for (const [k, val] of Object.entries(v as Record<string, unknown>)) {
        visit(val, path ? `${path}.${k}` : k);
      }
    }
  };
  visit(parsed, '');
  return palette;
}

function extractDeps(parsed: unknown, content: string, blockKey: string): JsonDependency[] {
  if (parsed === null || typeof parsed !== 'object') return [];
  const block = (parsed as Record<string, unknown>)[blockKey];
  if (block === null || typeof block !== 'object') return [];
  return Object.entries(block as Record<string, unknown>).map(([name, version]) => ({
    name,
    version: typeof version === 'string' ? version : String(version),
    line: findNestedKeyLine(content, blockKey, name),
  }));
}

function extractScripts(parsed: unknown, content: string): JsonScript[] {
  if (parsed === null || typeof parsed !== 'object') return [];
  const block = (parsed as Record<string, unknown>).scripts;
  if (block === null || typeof block !== 'object') return [];
  return Object.entries(block as Record<string, unknown>).map(([name, command]) => ({
    name,
    command: typeof command === 'string' ? command : String(command),
    line: findNestedKeyLine(content, 'scripts', name),
  }));
}

function typeOf(v: unknown): JsonValueType {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  const t = typeof v;
  if (t === 'object' || t === 'string' || t === 'number' || t === 'boolean') return t;
  return 'null';
}

function previewValue(v: unknown): string {
  if (Array.isArray(v)) return `${v.length} items`;
  if (v === null) return 'null';
  if (typeof v === 'object') return `${Object.keys(v as object).length} keys`;
  if (typeof v === 'string') {
    const compact = v.replace(/\s+/g, ' ').trim();
    return compact.length > 40 ? `"${compact.slice(0, 40)}…"` : `"${compact}"`;
  }
  return String(v);
}

function countOf(v: unknown): number | undefined {
  if (Array.isArray(v)) return v.length;
  if (v !== null && typeof v === 'object') return Object.keys(v as object).length;
  return undefined;
}

function escapeRegex(s: string): string {
  return s.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
}

function findKeyLine(content: string, key: string): number {
  // Top-level keys live at indent depth 0 or 2 (2-space formatting) — we
  // accept any leading whitespace to handle either, and match `"key":`
  // at the start of a line.
  const re = new RegExp(`^\\s*"${escapeRegex(key)}"\\s*:`, 'm');
  const match = re.exec(content);
  if (!match) return 1;
  return content.slice(0, match.index).split('\n').length;
}

function findNestedKeyLine(content: string, parentKey: string, key: string): number {
  // For "dependencies": find the parent block's start, then within the rest
  // of the file search for the child key. This is good enough when parent
  // names are unique per file (package.json's case).
  const parentRe = new RegExp(`^\\s*"${escapeRegex(parentKey)}"\\s*:`, 'm');
  const parentMatch = parentRe.exec(content);
  if (!parentMatch) return 1;
  const after = content.slice(parentMatch.index);
  const childRe = new RegExp(`^\\s*"${escapeRegex(key)}"\\s*:`, 'm');
  const childMatch = childRe.exec(after);
  if (!childMatch) return 1;
  return content.slice(0, parentMatch.index + childMatch.index).split('\n').length;
}

function findValueLine(content: string, value: string): number {
  // For colour values inside strings: search for the value as a JSON string
  // literal. Falls back to 1 if not found (e.g. value contains characters
  // that JSON.stringify escapes differently than our trimmed form).
  const quoted = `"${escapeRegex(value)}"`;
  const re = new RegExp(quoted);
  const match = re.exec(content);
  if (!match) return 1;
  return content.slice(0, match.index).split('\n').length;
}

function findArrayItemLine(content: string, index: number): number {
  // Cheap heuristic: count opening braces / brackets after the root `[`.
  // For most pretty-printed arrays this is accurate; falls back to 1.
  const rootStart = content.indexOf('[');
  if (rootStart < 0) return 1;
  let depth = 0;
  let itemIdx = -1;
  for (let i = rootStart; i < content.length; i++) {
    const ch = content[i];
    if (ch === '[' || ch === '{') {
      depth++;
      if (depth === 2) {
        itemIdx++;
        if (itemIdx === index) {
          return content.slice(0, i).split('\n').length;
        }
      }
    } else if (ch === ']' || ch === '}') {
      depth--;
    }
  }
  return 1;
}
