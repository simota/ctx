// Theme state — persisted in localStorage, respects prefers-color-scheme.
// FOUC avoidance lives in index.html (inline <script> in <head>).
//
// THEMES is the single source of truth: ThemeName, THEME_CYCLE, and the picker
// UI all derive from it. Adding a new theme = one entry here + one CSS
// :root[data-theme='...'] block + one token in index.html's FOUC lookup.

import { announce } from './announce.svelte';

/**
 * Theme category — drives picker grouping so first-time pickers can avoid
 * "I picked synthwave for the demo and now my eyes hurt 30 min in":
 * - neutral: safe defaults, mirror VSCode / GitHub conventions.
 * - personality: warm/cool × dark/light grid + cultural / material variants,
 *   tuned for long sessions.
 * - novelty: high-stimulation, intentionally fatiguing — short bursts.
 * - restraint: zero / near-zero chroma, designed for the calmest possible
 *   reading surface (visual-fatigue / outdoor / sensory-overload contexts).
 */
export type ThemeCategory = 'neutral' | 'personality' | 'novelty' | 'restraint';

export interface ThemeMeta {
  name: string;
  label: string;
  description: string;
  /** Hex preview color (the theme's signature accent) shown in the picker. */
  swatch: string;
  category: ThemeCategory;
}

// Ordered: canonical dark↔light first (the binary toggle most users care about),
// then personality (warm/cool × dark/light grid + cultural / material variants),
// then novelty (high-stimulation), then restraint (zero-chroma reading surface).
// THEMES drives both the cycle order and the picker list — they're the same list.
export const THEMES = [
  { name: 'dark', label: 'Dark', description: 'Classic neutral dark', swatch: '#4ec9b0', category: 'neutral' },
  { name: 'light', label: 'Light', description: 'Classic neutral light', swatch: '#0d8a72', category: 'neutral' },
  { name: 'lofi', label: 'Lofi', description: 'Cozy late-night cafe', swatch: '#ff9ec7', category: 'personality' },
  { name: 'sunrise', label: 'Sunrise', description: 'Warm morning peach', swatch: '#e85d75', category: 'personality' },
  { name: 'ocean', label: 'Ocean', description: 'Deep-sea bioluminescent', swatch: '#5eead4', category: 'personality' },
  { name: 'frost', label: 'Frost', description: 'Snowy-morning ice blue', swatch: '#4a78c8', category: 'personality' },
  { name: 'sumi', label: 'Sumi', description: 'Ink and washi by lamplight', swatch: '#e07a3a', category: 'personality' },
  { name: 'moss', label: 'Moss', description: 'Forest floor after rain', swatch: '#8ec97c', category: 'personality' },
  { name: 'newsprint', label: 'Newsprint', description: 'Morning broadsheet', swatch: '#a83232', category: 'personality' },
  { name: 'sepia', label: 'Sepia', description: 'Aged archival manuscript', swatch: '#7a4f24', category: 'personality' },
  { name: 'solarpunk', label: 'Solarpunk', description: 'Greenhouse optimism', swatch: '#3aa86a', category: 'personality' },
  { name: 'crt', label: 'CRT', description: 'Green phosphor terminal', swatch: '#5fff5f', category: 'novelty' },
  { name: 'synthwave', label: 'Synthwave', description: '1985 Miami neon', swatch: '#ff6ec7', category: 'novelty' },
  { name: 'blueprint', label: 'Blueprint', description: 'Cyanotype drafting paper', swatch: '#7fd4ff', category: 'novelty' },
  { name: 'forge', label: 'Forge', description: "Blacksmith's hot workshop", swatch: '#ff8c2a', category: 'novelty' },
  { name: 'eink', label: 'E-ink', description: 'Monochrome paper reading', swatch: '#1a1a1a', category: 'restraint' },
] as const satisfies readonly ThemeMeta[];

/** Category labels for the picker (kept here so picker UI stays declarative). */
export const CATEGORY_LABELS: Record<ThemeCategory, string> = {
  neutral: 'Neutral',
  personality: 'Personality — long sessions',
  novelty: 'Novelty — short bursts',
  restraint: 'Restraint — minimum stimulation',
};

export type ThemeName = (typeof THEMES)[number]['name'];

export const THEME_CYCLE: readonly ThemeName[] = THEMES.map((t) => t.name);

const STORAGE_KEY = 'ctx-viewer-theme';

const THEME_SET: ReadonlySet<string> = new Set(THEME_CYCLE);

function isThemeName(v: unknown): v is ThemeName {
  return typeof v === 'string' && THEME_SET.has(v);
}

export function themeMeta(name: ThemeName): ThemeMeta {
  // THEMES is exhaustive over ThemeName, so .find never returns undefined.
  // The fallback satisfies the type checker without runtime cost.
  return THEMES.find((t) => t.name === name) ?? THEMES[0];
}

function readInitial(): ThemeName {
  if (typeof window === 'undefined') return 'dark';
  // index.html sets data-theme synchronously before mount. Trust it if present.
  const fromDom = document.documentElement.dataset.theme;
  if (isThemeName(fromDom)) return fromDom;
  try {
    const v = localStorage.getItem(STORAGE_KEY);
    if (isThemeName(v)) return v;
  } catch {
    // ignore (storage blocked)
  }
  if (window.matchMedia?.('(prefers-color-scheme: light)').matches) return 'light';
  return 'dark';
}

export const theme = $state<{ name: ThemeName }>({ name: readInitial() });

export function setTheme(name: ThemeName): void {
  theme.name = name;
  if (typeof document !== 'undefined') {
    document.documentElement.dataset.theme = name;
  }
  try {
    localStorage.setItem(STORAGE_KEY, name);
  } catch {
    // ignore
  }
}

export function toggleTheme(): void {
  const idx = THEME_CYCLE.indexOf(theme.name);
  const next = THEME_CYCLE[(idx + 1) % THEME_CYCLE.length] ?? 'dark';
  setTheme(next);
  announce(`Theme: ${next}`);
}
