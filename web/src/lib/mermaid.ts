// Mermaid renderer wrapper, themed from live ctx CSS tokens.
//
// Mermaid ships theme: 'default' | 'dark' | 'forest' | 'neutral', but ctx
// runs 15+ themes and the user can author more — picking the closest two
// of mermaid's would leave the diagram looking grafted on. Instead we use
// theme: 'base' (a blank canvas designed for variable override) and fill
// every theme-variable surface mermaid exposes from ctx tokens.
//
// All colour reads go through a probe element so modern colour spaces
// (oklch / lab / color-mix) are resolved by the browser to rgb() before
// being handed to mermaid — which itself only parses hex/rgb/hsl reliably.
//
// Lifecycle: the library is dynamically imported on first render so the
// initial bundle stays small. mermaid.initialize is global, so we cache a
// theme key and re-initialise only when the resolved palette changes.

type MermaidApi = {
  initialize: (config: Record<string, unknown>) => void;
  render: (id: string, code: string) => Promise<{ svg: string }>;
};

let cachedMermaid: Promise<MermaidApi> | null = null;
let lastThemeKey: string | null = null;
let renderSeq = 0;
let probe: HTMLSpanElement | null = null;

async function getMermaid(): Promise<MermaidApi> {
  if (cachedMermaid) return cachedMermaid;
  cachedMermaid = import('mermaid').then((m) => m.default as MermaidApi);
  return cachedMermaid;
}

function getProbe(): HTMLSpanElement {
  if (probe && probe.isConnected) return probe;
  probe = document.createElement('span');
  probe.style.cssText =
    'position:absolute;left:-9999px;top:-9999px;visibility:hidden;pointer-events:none';
  document.body.appendChild(probe);
  return probe;
}

function readColor(varName: string, fallback: string): string {
  // Resolve a CSS custom property to its computed rgb() value via a probe
  // element. Works for hex / rgb / hsl / oklch / lab / color-mix etc.
  if (typeof document === 'undefined') return fallback;
  const el = getProbe();
  el.style.color = `var(${varName}, ${fallback})`;
  const c = getComputedStyle(el).color;
  return c || fallback;
}

function relLumRgb(rgb: string): number {
  const m = /^rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(rgb);
  if (!m) return 0;
  const f = (c: number) => {
    const v = c / 255;
    return v <= 0.03928 ? v / 12.92 : Math.pow((v + 0.055) / 1.055, 2.4);
  };
  return (
    0.2126 * f(Number(m[1])) + 0.7152 * f(Number(m[2])) + 0.0722 * f(Number(m[3]))
  );
}

function mix(a: string, b: string, t: number): string {
  // Blend two rgb() strings at ratio t (0..1). Used to derive secondary /
  // tertiary fills, alt section backgrounds, and tinted task colours so the
  // palette stays anchored to the ctx tokens instead of drifting into
  // hard-coded values.
  const pa = /rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(a);
  const pb = /rgba?\(\s*(\d+)[\s,]+(\d+)[\s,]+(\d+)/i.exec(b);
  if (!pa || !pb) return a;
  const r = Math.round(Number(pa[1]) * (1 - t) + Number(pb[1]) * t);
  const g = Math.round(Number(pa[2]) * (1 - t) + Number(pb[2]) * t);
  const bb = Math.round(Number(pa[3]) * (1 - t) + Number(pb[3]) * t);
  return `rgb(${r}, ${g}, ${bb})`;
}

interface CtxTokens {
  bg: string;
  bgElev: string;
  fg: string;
  fgDim: string;
  border: string;
  link: string;
  accent: string;
  hlString: string;
  hlKeyword: string;
  hlAttr: string;
  hlName: string;
  hlLiteral: string;
  isDark: boolean;
  fontFamily: string;
  monoFamily: string;
}

function readCtxTokens(): CtxTokens {
  const bg = readColor('--ctx-bg', '#1e1e1e');
  const bgElev = readColor('--ctx-bg-elev', '#252526');
  const fg = readColor('--ctx-fg', '#d4d4d4');
  const fgDim = readColor('--ctx-fg-dim', '#888');
  const border = readColor('--ctx-border', '#3a3a3a');
  const link = readColor('--ctx-link', '#7fb6d6');
  const accent = readColor('--ctx-accent', '#4ec9b0');
  const hlString = readColor('--hl-string', '#98c379');
  const hlKeyword = readColor('--hl-keyword', '#c678dd');
  const hlAttr = readColor('--hl-attr', '#d19a66');
  const hlName = readColor('--hl-name', '#e06c75');
  const hlLiteral = readColor('--hl-literal', '#56b6c2');
  const isDark = relLumRgb(bg) < 0.5;
  // Match the markdown preview's body font (see buildMarkdownCss). Diagrams
  // shouldn't look like a transplant from a different stylesheet.
  const fontFamily =
    "-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif";
  const monoFamily = 'ui-monospace, SFMono-Regular, Menlo, monospace';
  return {
    bg,
    bgElev,
    fg,
    fgDim,
    border,
    link,
    accent,
    hlString,
    hlKeyword,
    hlAttr,
    hlName,
    hlLiteral,
    isDark,
    fontFamily,
    monoFamily,
  };
}

function buildThemeVariables(t: CtxTokens): Record<string, string> {
  // Map ctx tokens onto mermaid's theme-variables surface. Each block is
  // grouped by diagram type so it's easy to retune one without disturbing
  // the rest. mermaid silently ignores variables it doesn't recognise, so
  // overrun is harmless when names change between versions.
  const nodeFill = t.bgElev;
  const altFill = mix(t.bgElev, t.bg, 0.4);
  const noteBg = mix(t.hlAttr, t.bg, t.isDark ? 0.75 : 0.85);
  const taskFill = mix(nodeFill, t.link, 0.25);
  return {
    background: t.bg,

    // Core
    primaryColor: nodeFill,
    primaryTextColor: t.fg,
    primaryBorderColor: t.border,
    secondaryColor: altFill,
    secondaryTextColor: t.fg,
    secondaryBorderColor: t.border,
    tertiaryColor: mix(nodeFill, t.accent, 0.18),
    tertiaryTextColor: t.fg,
    tertiaryBorderColor: t.accent,
    lineColor: t.fgDim,
    textColor: t.fg,
    mainBkg: nodeFill,
    nodeBorder: t.border,
    nodeTextColor: t.fg,
    clusterBkg: altFill,
    clusterBorder: t.border,
    titleColor: t.fg,
    edgeLabelBackground: t.bg,
    defaultLinkColor: t.fgDim,

    // Sequence
    actorBkg: nodeFill,
    actorBorder: t.border,
    actorTextColor: t.fg,
    actorLineColor: t.fgDim,
    signalColor: t.fg,
    signalTextColor: t.fg,
    labelBoxBkgColor: nodeFill,
    labelBoxBorderColor: t.border,
    labelTextColor: t.fg,
    loopTextColor: t.fg,
    activationBkgColor: mix(nodeFill, t.accent, 0.2),
    activationBorderColor: t.accent,
    sequenceNumberColor: t.bg,
    noteBkgColor: noteBg,
    noteTextColor: t.fg,
    noteBorderColor: t.border,

    // Gantt
    sectionBkgColor: altFill,
    altSectionBkgColor: t.bg,
    sectionBkgColor2: mix(altFill, t.bg, 0.5),
    gridColor: t.border,
    taskBkgColor: taskFill,
    taskBorderColor: t.link,
    taskTextColor: t.fg,
    taskTextLightColor: t.fg,
    taskTextOutsideColor: t.fg,
    taskTextDarkColor: t.bg,
    taskTextClickableColor: t.link,
    activeTaskBkgColor: mix(t.bg, t.accent, 0.45),
    activeTaskBorderColor: t.accent,
    doneTaskBkgColor: mix(nodeFill, t.fgDim, 0.5),
    doneTaskBorderColor: t.fgDim,
    critBkgColor: mix(t.bg, t.hlName, 0.45),
    critBorderColor: t.hlName,
    todayLineColor: t.accent,

    // State
    labelColor: t.fg,
    transitionColor: t.fgDim,
    transitionLabelColor: t.fg,
    stateLabelColor: t.fg,
    stateBkg: nodeFill,
    altBackground: altFill,
    compositeBackground: altFill,
    compositeBorder: t.border,
    compositeTitleBackground: nodeFill,
    innerEndBackground: t.border,
    specialStateColor: t.accent,

    // Class
    classText: t.fg,

    // ER
    attributeBackgroundColorOdd: nodeFill,
    attributeBackgroundColorEven: altFill,

    // Pie — anchored to ctx hljs palette so chart colours feel related to
    // the surrounding source code colouring rather than randomly chosen.
    pie1: t.link,
    pie2: t.accent,
    pie3: t.hlAttr,
    pie4: t.hlString,
    pie5: t.hlKeyword,
    pie6: t.hlLiteral,
    pie7: mix(t.link, t.bg, 0.3),
    pie8: mix(t.accent, t.bg, 0.3),
    pie9: mix(t.hlAttr, t.bg, 0.3),
    pie10: mix(t.hlString, t.bg, 0.3),
    pie11: t.fgDim,
    pie12: t.border,
    pieStrokeColor: t.border,
    pieOuterStrokeColor: t.border,
    pieOuterStrokeWidth: '1px',
    pieTitleTextColor: t.fg,
    pieSectionTextColor: t.bg,
    pieLegendTextColor: t.fg,

    // Git graph (same palette family as pie)
    git0: t.link,
    git1: t.accent,
    git2: t.hlAttr,
    git3: t.hlString,
    git4: t.hlKeyword,
    git5: t.hlLiteral,
    git6: mix(t.link, t.bg, 0.3),
    git7: mix(t.accent, t.bg, 0.3),
    gitBranchLabel0: t.bg,
    gitBranchLabel1: t.bg,
    gitBranchLabel2: t.bg,
    gitBranchLabel3: t.bg,
    gitBranchLabel4: t.bg,
    gitBranchLabel5: t.bg,
    gitBranchLabel6: t.bg,
    gitBranchLabel7: t.bg,
    commitLabelColor: t.fg,
    commitLabelBackground: nodeFill,

    // Journey
    fillType0: t.link,
    fillType1: t.accent,
    fillType2: t.hlAttr,
    fillType3: t.hlString,
    fillType4: t.hlKeyword,
    fillType5: t.hlLiteral,
    fillType6: t.fgDim,
    fillType7: mix(t.link, t.bg, 0.4),

    // Error
    errorBkgColor: mix(t.bg, t.hlName, 0.3),
    errorTextColor: t.fg,

    // Font
    fontFamily: t.fontFamily,
    fontSize: '14px',
  };
}

function buildThemeCss(t: CtxTokens): string {
  // Final layer themeVariables can't reach: stroke widths tuned for the
  // current bg luminance, rounded cluster corners, mono font for class
  // members, and a soft pill background under edge labels so they read
  // when they land on top of a line.
  const stroke = t.isDark ? 1.5 : 1.25;
  return `
.node rect, .node circle, .node ellipse, .node polygon, .node path { stroke-width: ${stroke}px; }
.node .label { color: ${t.fg}; }
.cluster rect { stroke-width: 1px; rx: 8; ry: 8; }
.cluster .cluster-label .nodeLabel { font-weight: 600; }
.edgePath .path, .flowchart-link { stroke-width: 1.5px; }
.edgeLabel { background-color: ${t.bg}; color: ${t.fg}; padding: 1px 4px; border-radius: 3px; }
.edgeLabel rect { fill: ${t.bg}; }
.label, .label foreignObject div { font-family: ${t.fontFamily}; color: ${t.fg}; }
.actor { stroke-width: 1.25px; }
.messageText, .noteText, .loopText { font-family: ${t.fontFamily}; }
.classGroup line { stroke: ${t.border}; }
.classGroup .title, .classTitle { font-weight: 600; font-family: ${t.fontFamily}; }
.classGroup text:not(.title):not(.classTitle) { font-family: ${t.monoFamily}; font-size: 12px; }
.relation { stroke-width: 1.25px; }
.task { stroke-width: 1px; }
.section0, .section2 { fill: ${mix(t.bgElev, t.bg, 0.3)}; }
.grid .tick line { stroke: ${t.border}; opacity: 0.5; }
.titleText { font-family: ${t.fontFamily}; font-weight: 600; }
.pieTitleText { font-weight: 600; }
.pieCircle { stroke-width: 1px; }
.commit-label, .branch-label { font-family: ${t.fontFamily}; }
.state-note { font-family: ${t.fontFamily}; }
foreignObject > div { color: ${t.fg}; }
`;
}

export interface MermaidRenderResult {
  ok: boolean;
  svg: string;
  error?: string;
}

export async function renderMermaid(code: string): Promise<MermaidRenderResult> {
  const mermaid = await getMermaid();
  const tokens = readCtxTokens();
  // Re-init only when the resolved palette actually changes. Comparing a
  // compact key avoids repeated initialize() calls for back-to-back renders
  // of multiple mermaid blocks on the same page.
  const themeKey = `${tokens.bg}|${tokens.fg}|${tokens.accent}|${tokens.border}|${tokens.link}|${tokens.bgElev}|${tokens.hlString}|${tokens.hlKeyword}|${tokens.hlAttr}|${tokens.hlName}`;
  if (themeKey !== lastThemeKey) {
    const themeVariables = buildThemeVariables(tokens);
    mermaid.initialize({
      startOnLoad: false,
      theme: 'base',
      themeVariables,
      themeCSS: buildThemeCss(tokens),
      securityLevel: 'strict',
      fontFamily: tokens.fontFamily,
      flowchart: {
        curve: 'basis',
        useMaxWidth: true,
        htmlLabels: true,
        diagramPadding: 16,
        nodeSpacing: 56,
        rankSpacing: 64,
        padding: 12,
      },
      sequence: {
        useMaxWidth: true,
        diagramMarginX: 24,
        diagramMarginY: 16,
        actorMargin: 56,
        boxMargin: 12,
        messageFontSize: 14,
        noteFontSize: 13,
        actorFontSize: 14,
        messageFontFamily: tokens.fontFamily,
        noteFontFamily: tokens.fontFamily,
        actorFontFamily: tokens.fontFamily,
      },
      gantt: {
        useMaxWidth: true,
        fontSize: 12,
        gridLineStartPadding: 32,
        leftPadding: 96,
      },
      state: { useMaxWidth: true },
      class: { useMaxWidth: true, htmlLabels: false },
      journey: { useMaxWidth: true },
      er: { useMaxWidth: true, fontSize: 14 },
      pie: { useMaxWidth: true, textPosition: 0.6 },
      gitGraph: { useMaxWidth: true, mainBranchName: 'main' },
    });
    lastThemeKey = themeKey;
  }
  const id = `ctx-mermaid-${++renderSeq}`;
  try {
    const out = await mermaid.render(id, code);
    return { ok: true, svg: out.svg };
  } catch (e) {
    return { ok: false, svg: '', error: e instanceof Error ? e.message : String(e) };
  }
}

export function resetMermaidTheme(): void {
  // Force a re-init on next render — called when the host theme changes so
  // the next diagram picks up the fresh CSS-variable values.
  lastThemeKey = null;
}
