// CSS insight extraction for the FileDetail sidebar.
//
// Single-pass line scan that produces six concurrent views over the source:
//   - palette:   colour values (hex / rgb / hsl / oklch / oklab / lab / lch /
//                color()), with `--name: <colour>` declarations grouped as a
//                single named entry.
//   - vars:      every `--name: …;` declaration plus a usage count from
//                matching `var(--name)` occurrences elsewhere in the file.
//   - scale:     length values that appear in font-size, padding, margin,
//                gap, inset, or the directional logical/physical variants —
//                deduped with a per-value count so the histogram surfaces
//                "we have nine different paddings" inconsistencies.
//   - zIndex:    every numeric `z-index: <n>` declaration, sorted ascending.
//   - media:     every `@media (…)` query.
//   - keyframes: every `@keyframes <name>` definition.
//   - importantCount: total `!important` usages — flagged in the UI as a
//                quality smell but not enumerated to avoid noise.
//
// Tradeoff: regexes over a CSS lexer. Comments could in theory hide colour
// values, but in practice this is a viewer aid, not a linter — false hits
// inside `/* … */` are tolerated and trivial to ignore visually.

export interface PaletteEntry {
  line: number;
  /** CSS custom-property name when the colour comes from a `--name: …` declaration. */
  name?: string;
  /** Full text of the value or colour expression as it appears in the file. */
  value: string;
  /** Browser-parseable colour string used as the swatch background. */
  swatch: string;
}

export interface VarEntry {
  name: string;
  value: string;
  line: number;
  usage: number;
}

export interface ScaleEntry {
  value: string;
  count: number;
  firstLine: number;
}

export interface RadiusEntry {
  value: string;
  count: number;
  firstLine: number;
}

export interface ShadowEntry {
  value: string;
  count: number;
  firstLine: number;
}

export interface ZIndexEntry {
  value: number;
  line: number;
}

export interface MediaEntry {
  condition: string;
  line: number;
}

export interface KeyframeEntry {
  name: string;
  line: number;
}

export interface CssInsights {
  palette: PaletteEntry[];
  vars: VarEntry[];
  fontSizes: ScaleEntry[];
  spacings: ScaleEntry[];
  radii: RadiusEntry[];
  shadows: ShadowEntry[];
  zIndex: ZIndexEntry[];
  media: MediaEntry[];
  keyframes: KeyframeEntry[];
  importantCount: number;
}

const COLOR_RE = /#[0-9a-fA-F]{3,8}\b|\b(?:rgba?|hsla?|oklch|oklab|lab|lch|color)\([^)]*\)/g;
const CUSTOM_PROP_RE = /(--[a-zA-Z0-9_-]+)\s*:\s*([^;{}]+?)(?:\s*;|\s*$)/;
const VAR_USAGE_RE = /\bvar\(\s*(--[a-zA-Z0-9_-]+)/g;
const Z_INDEX_RE = /\bz-index\s*:\s*(-?\d+)/g;
const MEDIA_RE = /@media\s+([^{]+)\{/g;
const KEYFRAMES_RE = /@keyframes\s+([a-zA-Z0-9_-]+)/g;
const IMPORTANT_RE = /!important\b/g;
// Length values: 16px, 1.5rem, 0.92em, 100%, 50vh, 33vw. Anchored to a
// preceding boundary so we don't catch the `16` inside `rgb(16, 16, 16)`.
const LENGTH_VALUE_RE = /(?:^|[\s:,(])(\d*\.?\d+(?:px|rem|em|%|vh|vw|svh|svw|dvh|dvw))(?=[\s,);!}]|$)/g;
const SPACING_PROP_RE = /\b(?:padding|margin|gap|row-gap|column-gap|inset|top|right|bottom|left|inline-size|block-size)(?:-(?:top|right|bottom|left|inline|block|inline-start|inline-end|block-start|block-end|x|y))?\s*:/;
const FONT_SIZE_RE = /\bfont-size\s*:/;
const BORDER_RADIUS_RE = /\bborder-radius\s*:\s*([^;}]+?)(?:[;}]|$)/g;
const BOX_SHADOW_RE = /\bbox-shadow\s*:\s*([^;}]+?)(?:[;}]|$)/g;

export function extractCssInsights(content: string): CssInsights {
  const lines = content.split('\n');
  const palette: PaletteEntry[] = [];
  const varsDefs = new Map<string, { value: string; line: number }>();
  const varsUsage = new Map<string, number>();
  const fontSizeAgg = new Map<string, ScaleEntry>();
  const spacingAgg = new Map<string, ScaleEntry>();
  const radiusAgg = new Map<string, RadiusEntry>();
  const shadowAgg = new Map<string, ShadowEntry>();
  const zIndex: ZIndexEntry[] = [];
  const media: MediaEntry[] = [];
  const keyframes: KeyframeEntry[] = [];
  let importantCount = 0;

  for (let i = 0; i < lines.length; i++) {
    const line = lines[i];
    const lineNum = i + 1;

    // Custom-property definition takes the whole line, otherwise scan for
    // inline colour usages so a single line with multiple colours is fully
    // captured.
    const propMatch = line.match(CUSTOM_PROP_RE);
    if (propMatch) {
      const name = propMatch[1];
      const value = propMatch[2].trim();
      varsDefs.set(name, { value, line: lineNum });
      COLOR_RE.lastIndex = 0;
      const colour = COLOR_RE.exec(value);
      if (colour) {
        palette.push({ line: lineNum, name, value, swatch: colour[0] });
      }
    } else {
      COLOR_RE.lastIndex = 0;
      let m: RegExpExecArray | null;
      while ((m = COLOR_RE.exec(line)) !== null) {
        palette.push({ line: lineNum, value: m[0], swatch: m[0] });
      }
    }

    VAR_USAGE_RE.lastIndex = 0;
    let v: RegExpExecArray | null;
    while ((v = VAR_USAGE_RE.exec(line)) !== null) {
      varsUsage.set(v[1], (varsUsage.get(v[1]) ?? 0) + 1);
    }

    Z_INDEX_RE.lastIndex = 0;
    let z: RegExpExecArray | null;
    while ((z = Z_INDEX_RE.exec(line)) !== null) {
      zIndex.push({ value: parseInt(z[1], 10), line: lineNum });
    }

    MEDIA_RE.lastIndex = 0;
    let mq: RegExpExecArray | null;
    while ((mq = MEDIA_RE.exec(line)) !== null) {
      media.push({ condition: mq[1].trim().replace(/\s+/g, ' '), line: lineNum });
    }

    KEYFRAMES_RE.lastIndex = 0;
    let kf: RegExpExecArray | null;
    while ((kf = KEYFRAMES_RE.exec(line)) !== null) {
      keyframes.push({ name: kf[1], line: lineNum });
    }

    IMPORTANT_RE.lastIndex = 0;
    while (IMPORTANT_RE.exec(line) !== null) {
      importantCount++;
    }

    // Scale: classify the line by property and aggregate length values into
    // the matching bucket. Font-size and spacing are kept apart so the UI
    // can render different visualisations for each.
    if (FONT_SIZE_RE.test(line)) {
      LENGTH_VALUE_RE.lastIndex = 0;
      let s: RegExpExecArray | null;
      while ((s = LENGTH_VALUE_RE.exec(line)) !== null) {
        aggregate(fontSizeAgg, s[1], lineNum);
      }
    } else if (SPACING_PROP_RE.test(line)) {
      LENGTH_VALUE_RE.lastIndex = 0;
      let s: RegExpExecArray | null;
      while ((s = LENGTH_VALUE_RE.exec(line)) !== null) {
        aggregate(spacingAgg, s[1], lineNum);
      }
    }

    BORDER_RADIUS_RE.lastIndex = 0;
    let br: RegExpExecArray | null;
    while ((br = BORDER_RADIUS_RE.exec(line)) !== null) {
      aggregate(radiusAgg, br[1].trim(), lineNum);
    }

    BOX_SHADOW_RE.lastIndex = 0;
    let bs: RegExpExecArray | null;
    while ((bs = BOX_SHADOW_RE.exec(line)) !== null) {
      const v = bs[1].trim();
      if (v && v.toLowerCase() !== 'none') {
        aggregate(shadowAgg, v, lineNum);
      }
    }
  }

  const vars: VarEntry[] = Array.from(varsDefs.entries())
    .map(([name, def]) => ({ name, value: def.value, line: def.line, usage: varsUsage.get(name) ?? 0 }))
    .sort((a, b) => b.usage - a.usage || a.name.localeCompare(b.name));

  zIndex.sort((a, b) => a.value - b.value || a.line - b.line);

  const byNumericValue = (a: { value: string }, b: { value: string }) =>
    parseFloat(a.value) - parseFloat(b.value);
  const byCountDesc = (a: { count: number }, b: { count: number }) => b.count - a.count;

  return {
    palette,
    vars,
    fontSizes: Array.from(fontSizeAgg.values()).sort(byNumericValue),
    spacings: Array.from(spacingAgg.values()).sort(byNumericValue),
    radii: Array.from(radiusAgg.values()).sort(byNumericValue),
    shadows: Array.from(shadowAgg.values()).sort(byCountDesc),
    zIndex,
    media,
    keyframes,
    importantCount,
  };
}

function aggregate<T extends { value: string; count: number; firstLine: number }>(
  bucket: Map<string, T>,
  value: string,
  lineNum: number,
): void {
  const existing = bucket.get(value);
  if (existing) {
    existing.count++;
  } else {
    bucket.set(value, { value, count: 1, firstLine: lineNum } as T);
  }
}
