<script module lang="ts">
  import hljs from '../lib/highlight';
  import { marked } from 'marked';
  import { markedHighlight } from 'marked-highlight';

  // Configure marked once per module load — code blocks get hljs colouring
  // matching the rest of ctx, so a code fence in README looks identical to
  // the source viewer's hljs output. Headings get a stable id="…" so the
  // TOC sidebar can scroll to them by id. Module-level (not per-instance):
  // marked is a global singleton, so registering in the instance script would
  // stack a duplicate highlighter + renderer on every FileDetail mount.
  marked.use(
    markedHighlight({
      langPrefix: 'hljs language-',
      highlight(code: string, lang: string) {
        const language = lang && hljs.getLanguage(lang) ? lang : 'plaintext';
        try {
          return hljs.highlight(code, { language, ignoreIllegals: true }).value;
        } catch {
          return code;
        }
      },
    }),
  );
  marked.use({
    renderer: {
      // marked v18 passes the heading token; if we attached `slug` during
      // the pre-walk, render it as an id. Falling back to no id preserves
      // marked's default behaviour for any token we missed.
      heading(token) {
        const inner = this.parser.parseInline(token.tokens);
        const slug = (token as unknown as { slug?: string }).slug ?? '';
        const idAttr = slug ? ` id="${slug}"` : '';
        return `<h${token.depth}${idAttr}>${inner}</h${token.depth}>\n`;
      },
    },
  });
</script>

<script lang="ts">
  import { tick } from 'svelte';
  import {
    ApiCallError,
    fetchFile,
    fetchGitDiff,
    fetchFileLog,
    fetchFileCommitDiff,
    type FileResponse,
    type DefinitionCandidate,
    type GitDiffResponse,
    type FileLogResponse,
    type Symbol,
  } from '../lib/api';
  import { formatTokens, formatSize, langFromPath, gitColor, gitLabel, formatRelative } from '../lib/format';
  import { renderMermaid, resetMermaidTheme } from '../lib/mermaid';
  import { attachPanZoom, type PanZoomController } from '../lib/pan-zoom';
  import { route, navigate, toFileHash, type FileViewMode } from '../lib/router.svelte';
  import { announce } from '../lib/announce.svelte';
  import { openContextMenu, type ContextMenuItem } from '../lib/context-menu.svelte';
  import { repo, absolutePath } from '../lib/repo.svelte';
  import { revealPath } from '../lib/tree-state.svelte';
  import { rememberScroll, recallScroll } from '../lib/scroll-memo.svelte';
  import { lookup as lookupDefinition, peek as peekDefinition } from '../lib/definitions.svelte';
  import { openDefinitionPicker } from '../lib/definition-picker.svelte';
  import { openTab } from '../lib/tabs.svelte';
  import { theme } from '../lib/theme.svelte';
  import { view } from '../lib/view.svelte';
  import { panes, openRight, closeRight } from '../lib/panes.svelte';
  import SymbolList from './SymbolList.svelte';
  import TocList, { type TocEntry } from './TocList.svelte';
  import CssInsights from './CssInsights.svelte';
  import JsonInsights from './JsonInsights.svelte';
  import XmlInsights from './XmlInsights.svelte';
  import YamlInsights from './YamlInsights.svelte';
  import SqlInsights from './SqlInsights.svelte';
  import RelationsPanel from './RelationsPanel.svelte';
  import TestInsightsPanel from './TestInsightsPanel.svelte';
  import EvidencePanel from './EvidencePanel.svelte';

  // Slug generator matching the one used to build TOC entries. Keeps Latin
  // and Unicode letters/numbers (so Japanese headings keep their text),
  // replaces whitespace with `-`, and tracks duplicates with a numeric
  // suffix the way GitHub does.
  function makeSlugger() {
    const counts = new Map<string, number>();
    return (text: string): string => {
      const base = text
        .toLowerCase()
        .trim()
        .replace(/[^\p{L}\p{N}\s-]/gu, '')
        .replace(/\s+/g, '-')
        .replace(/-+/g, '-');
      const n = counts.get(base) ?? 0;
      counts.set(base, n + 1);
      return n === 0 ? base : `${base}-${n}`;
    };
  }

  // Remove constructs the sandboxed iframe would otherwise warn about.
  // The iframe runs with sandbox="allow-same-origin" (no allow-scripts) so
  // nothing actually executes — but the browser still logs a warning per
  // attempt. Stripping <script> blocks and inline on* handlers silences the
  // console while preserving the iframe sandbox as defence-in-depth.
  function stripExecutableHtml(html: string): string {
    return html
      .replace(/<script\b[^>]*>[\s\S]*?<\/script\s*>/gi, '')
      .replace(/<script\b[^>]*\/?>/gi, '')
      .replace(/\son[a-z]+\s*=\s*"[^"]*"/gi, '')
      .replace(/\son[a-z]+\s*=\s*'[^']*'/gi, '')
      .replace(/\son[a-z]+\s*=\s*[^\s>]+/gi, '');
  }

  // After marked has rendered the markdown to HTML, pull each mermaid code
  // block out and replace it with a stable-id placeholder. The pattern is
  // tight because markedHighlight always emits `class="hljs language-X"` —
  // mermaid isn't a hljs language, so the inner text comes back unwrapped
  // (just entity-escaped), which we decode before passing to mermaid.render.
  let _mermaidSeq = 0;
  function extractMermaidBlocks(
    html: string,
  ): { html: string; blocks: { id: string; code: string }[] } {
    const blocks: { id: string; code: string }[] = [];
    const out = html.replace(
      /<pre><code class="hljs language-mermaid">([\s\S]*?)<\/code><\/pre>/g,
      (_m, body: string) => {
        const id = `m${_mermaidSeq++}`;
        blocks.push({ id, code: decodeHtmlEntities(body) });
        return `<div class="mermaid-block" data-mid="${id}"></div>`;
      },
    );
    return { html: out, blocks };
  }

  function decodeHtmlEntities(s: string): string {
    return s
      .replace(/&lt;/g, '<')
      .replace(/&gt;/g, '>')
      .replace(/&quot;/g, '"')
      .replace(/&#39;/g, "'")
      .replace(/&amp;/g, '&');
  }

  // `pane` — which split-view pane this instance renders in. The split view
  // mounts one FileDetail per pane and each registers window-level keydown
  // handlers, so without a guard j/k/gg/Ctrl-d etc. would act on both panes.
  let { path, pane = 'left' } = $props<{ path: string; pane?: 'left' | 'right' }>();

  // True when this instance should handle window-level keyboard events:
  // either the split view is closed (single pane handles everything) or this
  // instance belongs to the focused pane.
  function isFocusedPane(): boolean {
    if (!panes.rightOpen || isMobile || !panes.rightPath) return true;
    return panes.focused === pane;
  }

  // URL-driven file view mode belongs to the routed left/single pane. The
  // right split pane has its own local mode so `?mode=diff` does not flip both
  // panes at once.
  let routeModeApplies = $derived(
    route.name === 'file' && route.path === path && pane === 'left',
  );

  // Breadcrumb segments for the path. Each crumb is clickable to reveal that
  // ancestor (or self) in the tree. Root `.` is excluded for visual reasons;
  // a single-segment file produces exactly one crumb.
  let crumbs = $derived.by<{ name: string; path: string; last: boolean }[]>(() => {
    if (!path) return [];
    const segs = path.split('/').filter((s: string) => s.length > 0);
    const out: { name: string; path: string; last: boolean }[] = [];
    let acc = '';
    for (let i = 0; i < segs.length; i++) {
      acc = acc === '' ? segs[i] : `${acc}/${segs[i]}`;
      out.push({ name: segs[i], path: acc, last: i === segs.length - 1 });
    }
    return out;
  });

  function onCrumbClick(crumbPath: string) {
    revealPath(crumbPath);
    announce(`Revealing ${crumbPath} in tree`);
  }

  let data = $state<FileResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);

  // viewer controls
  let wrap = $state(readWrapPref());
  let previewView = $state<'rendered' | 'source'>(readPreviewViewPref());
  // diff view — toggled by the Diff button when the file has uncommitted
  // changes. Diff mode takes precedence over the rendered preview so the user
  // sees the actual change set, not the SVG/Markdown render.
  let diffMode = $state(false);
  let diffData = $state<GitDiffResponse | null>(null);
  let diffLoading = $state(false);
  let diffError = $state<string | null>(null);

  // history view — commit log + commit-to-commit diff for the current file.
  let historyMode = $state(false);
  let historyData = $state<FileLogResponse | null>(null);
  let selectedHash = $state<string | null>(null); // full hash
  let commitDiffData = $state<GitDiffResponse | null>(null);
  let commitDiffError = $state<string | null>(null);
  let historyLoading = $state(false);
  let historyError = $state<string | null>(null);
  let historyListEl: HTMLElement | null = $state(null);

  let copyState = $state<'idle' | 'ok' | 'err'>('idle');
  let copyTimer: ReturnType<typeof setTimeout> | null = null;

  // find bar state
  let findOpen = $state(false);
  let findQuery = $state('');
  let findMatches = $state<number[]>([]); // 1-based line numbers
  let findIndex = $state(0);
  let findInputEl: HTMLInputElement | null = $state(null);
  let findDebounce: ReturnType<typeof setTimeout> | null = null;
  let findAnnounceTimer: ReturnType<typeof setTimeout> | null = null;

  // refs
  let codeEl: HTMLPreElement | null = $state(null);
  let mdIframeEl: HTMLIFrameElement | null = $state(null);
  let diffEl: HTMLPreElement | null = $state(null);

  // Markdown TOC click → scroll iframe. Safe because the markdown iframe
  // uses sandbox="allow-same-origin" (no allow-scripts), so the parent can
  // reach into contentDocument while scripts inside the iframe still cannot
  // execute. If the iframe is mid-load or its origin is opaque (older
  // browsers ignoring allow-same-origin on srcdoc), contentDocument is null
  // and we bail silently rather than throwing.
  function jumpToHeading(slug: string) {
    if (!mdIframeEl) return;
    const doc = mdIframeEl.contentDocument;
    if (!doc) return;
    const target = doc.getElementById(slug);
    if (target) {
      target.scrollIntoView({ behavior: 'smooth', block: 'start' });
      announce(`Jumped to heading ${slug}`);
    }
  }
  // ephemeral target highlight (symbol click). Cleared after fade.
  let pulseLine = $state<number | null>(null);
  let pulseTimer: ReturnType<typeof setTimeout> | null = null;

  function readWrapPref(): boolean {
    try {
      return localStorage.getItem('ctx-viewer-wrap') === '1';
    } catch {
      return false;
    }
  }
  function writeWrapPref(v: boolean) {
    try {
      localStorage.setItem('ctx-viewer-wrap', v ? '1' : '0');
    } catch {
      // ignore
    }
  }
  function readPreviewViewPref(): 'rendered' | 'source' {
    try {
      return localStorage.getItem('ctx-viewer-preview') === 'source' ? 'source' : 'rendered';
    } catch {
      return 'rendered';
    }
  }
  function writePreviewViewPref(v: 'rendered' | 'source') {
    try {
      localStorage.setItem('ctx-viewer-preview', v);
    } catch {
      // ignore
    }
  }

  // Rendered preview for SVG and HTML.
  //
  // WHY <img> + data: URL for SVG (not {@html} or <object>): SVG can carry
  // inline <script>, on* handlers, and external <use href>; same-origin
  // {@html} would execute them. <img> with a data URL runs in image
  // rendering mode where script execution and external fetches are disabled.
  //
  // WHY <iframe src="/raw/{path}"> + sandbox="allow-scripts" for HTML:
  //   - Loading via /raw/{path} gives the iframe a real URL whose location
  //     matches the file's directory in the repo. The browser can then
  //     resolve <link href="./style.css">, <script src="app.js">, and
  //     <img src="images/logo.png"> against that URL — srcdoc cannot do
  //     that (its base is about:srcdoc).
  //   - sandbox="allow-scripts" enables JS but withholds allow-same-origin,
  //     so the iframe gets an opaque origin and cannot read ctx app
  //     cookies/localStorage even though the URL is technically same-origin.
  //   - The server adds Content-Security-Policy: sandbox allow-scripts so a
  //     direct navigation (no iframe) still falls into the same sandbox.
  let isSvg = $derived(/\.svg$/i.test(path));
  let isHtml = $derived(/\.x?html?$/i.test(path));
  let isMarkdown = $derived(/\.(md|markdown|mdx)$/i.test(path));
  let isCss = $derived(/\.(css|scss|sass|less|pcss|postcss)$/i.test(path));
  let isJson = $derived(/\.json$/i.test(path));
  // SVG is XML but has its own preview path; .xhtml/.html are handled by
  // isHtml. The remaining .xml/.xsd/.xsl/.xslt/.rss/.atom/.plist files
  // belong to the XML insights sidebar.
  let isXml = $derived(!isSvg && !isHtml && /\.(xml|xsd|xsl|xslt|rss|atom|plist)$/i.test(path));
  let isYaml = $derived(/\.ya?ml$/i.test(path));
  let isSql = $derived(/\.sql$/i.test(path));
  let isMmd = $derived(/\.(mmd|mermaid)$/i.test(path));
  // Files whose right-sidebar should also surface the import graph. Matches
  // the language list in internal/relations.Supported on the backend.
  let isRelationsTarget = $derived(
    /\.(go|tsx?|jsx?|mjs|cjs|svelte|vue|py|java|kts?|php|swift)$/i.test(path),
  );
  let isTestsTarget = $derived(/\.go$/i.test(path));
  let isPreviewable = $derived(isSvg || isHtml || isMarkdown || isMmd);
  let svgDataUrl = $derived.by(() => {
    if (!isSvg || !data) return '';
    return `data:image/svg+xml;utf8,${encodeURIComponent(data.content)}`;
  });
  let rawUrl = $derived.by(() => {
    if (!path) return '';
    return '/raw/' + path.split('/').map(encodeURIComponent).join('/');
  });

  // Markdown preview is built client-side and shipped to a sandbox="" iframe
  // via srcdoc. WHY sandbox="" (no allow-* flags): marked passes raw HTML
  // through by default, so a `<script>` in the markdown would otherwise run;
  // a fully sandboxed iframe blocks scripts/forms/same-origin regardless of
  // what the rendered HTML contains.
  //
  // WHY <base href>: relative <img src> and <a href> in the markdown should
  // resolve against /raw/{dir}/ — without a base, the iframe's about:srcdoc
  // origin makes them resolve nowhere. Anchor links (#section) still work
  // because they target the same iframe document.
  //
  // CSS tokens are read from the live document so the preview tracks the
  // current theme. Referencing theme.name pulls it into the derived's
  // dependency set so theme switches trigger a re-render.
  function readThemeVar(name: string, fallback: string): string {
    if (typeof document === 'undefined') return fallback;
    return getComputedStyle(document.documentElement).getPropertyValue(name).trim() || fallback;
  }
  function escapeAttr(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
  }
  function buildMarkdownCss(): string {
    const bg = readThemeVar('--ctx-bg', '#1e1e1e');
    const fg = readThemeVar('--ctx-fg', '#d4d4d4');
    const fgDim = readThemeVar('--ctx-fg-dim', '#888');
    const border = readThemeVar('--ctx-border', '#3a3a3a');
    const link = readThemeVar('--ctx-link', '#7fb6d6');
    const bgElev = readThemeVar('--ctx-bg-elev', '#252526');
    const accent = readThemeVar('--ctx-accent', '#4ec9b0');
    const hlFg = readThemeVar('--hl-fg', '#abb2bf');
    const hlComment = readThemeVar('--hl-comment', '#5c6370');
    const hlKeyword = readThemeVar('--hl-keyword', '#c678dd');
    const hlName = readThemeVar('--hl-name', '#e06c75');
    const hlLiteral = readThemeVar('--hl-literal', '#56b6c2');
    const hlString = readThemeVar('--hl-string', '#98c379');
    const hlAttr = readThemeVar('--hl-attr', '#d19a66');
    const hlSymbol = readThemeVar('--hl-symbol', '#61aeee');
    const hlBuiltIn = readThemeVar('--hl-built-in', '#e6c07b');
    return `
*, *::before, *::after { box-sizing: border-box; }
html, body { margin: 0; }
body {
  font: 15px/1.7 -apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif;
  color: ${fg};
  background: ${bg};
  padding: 32px clamp(24px, 5vw, 56px);
  max-width: 880px;
  margin-inline: auto;
}
h1, h2, h3, h4, h5, h6 { line-height: 1.25; margin: 1.6em 0 0.55em; font-weight: 600; }
h1 { font-size: 2em; border-bottom: 1px solid ${border}; padding-bottom: 0.3em; }
h2 { font-size: 1.5em; border-bottom: 1px solid ${border}; padding-bottom: 0.2em; }
h3 { font-size: 1.25em; }
h4 { font-size: 1em; }
h5, h6 { font-size: 0.92em; color: ${fgDim}; }
p, ul, ol, blockquote, pre, table, figure { margin: 0.85em 0; }
a { color: ${link}; text-decoration: underline; text-underline-offset: 2px; }
a:hover { text-decoration-thickness: 2px; }
code { background: ${bgElev}; padding: 0.15em 0.35em; border-radius: 3px; font-size: 0.92em; font-family: ui-monospace, SFMono-Regular, Menlo, monospace; }
pre { background: ${bgElev}; padding: 14px 16px; border-radius: 6px; overflow: auto; }
pre code { background: transparent; padding: 0; font-size: 0.88em; line-height: 1.55; }
img { max-width: 100%; height: auto; }
blockquote { border-left: 4px solid ${accent}; padding: 0.2em 1em; color: ${fgDim}; margin-inline: 0; }
table { border-collapse: collapse; display: block; overflow-x: auto; }
th, td { border: 1px solid ${border}; padding: 6px 12px; text-align: left; }
th { background: ${bgElev}; font-weight: 600; }
hr { border: none; border-top: 1px solid ${border}; margin: 2.2em 0; }
ul, ol { padding-left: 1.6em; }
li + li { margin-top: 0.25em; }
li > p { margin: 0.25em 0; }
input[type=checkbox] { margin-right: 0.4em; }
kbd { background: ${bgElev}; border: 1px solid ${border}; border-radius: 3px; padding: 0.1em 0.4em; font-size: 0.85em; font-family: ui-monospace, monospace; }
.hljs { color: ${hlFg}; background: transparent; }
.hljs-comment, .hljs-quote { color: ${hlComment}; font-style: italic; }
.hljs-doctag, .hljs-keyword, .hljs-formula { color: ${hlKeyword}; }
.hljs-section, .hljs-name, .hljs-selector-tag, .hljs-deletion, .hljs-subst { color: ${hlName}; }
.hljs-literal { color: ${hlLiteral}; }
.hljs-string, .hljs-regexp, .hljs-addition, .hljs-attribute, .hljs-meta .hljs-string { color: ${hlString}; }
.hljs-attr, .hljs-variable, .hljs-template-variable, .hljs-type, .hljs-selector-class, .hljs-selector-attr, .hljs-selector-pseudo, .hljs-number { color: ${hlAttr}; }
.hljs-symbol, .hljs-bullet, .hljs-link, .hljs-meta, .hljs-selector-id, .hljs-title { color: ${hlSymbol}; }
.hljs-built_in, .hljs-title.class_, .hljs-class .hljs-title { color: ${hlBuiltIn}; }
.hljs-emphasis { font-style: italic; }
.hljs-strong { font-weight: 700; }
.mermaid-block, .mermaid-svg { position: relative; display: flex; justify-content: center; margin: 1.2em 0; padding: 12px; background: ${bgElev}; border-radius: 6px; overflow: hidden; }
.mermaid-block { min-height: 60px; }
.mermaid-svg svg { max-width: 100%; height: auto; user-select: none; }
.mermaid-controls { position: absolute; top: 8px; right: 8px; display: flex; gap: 2px; padding: 2px; background: ${bg}; border: 1px solid ${border}; border-radius: 4px; opacity: 0; transition: opacity 120ms ease; }
.mermaid-svg:hover .mermaid-controls, .mermaid-controls:focus-within { opacity: 1; }
.mermaid-controls button { appearance: none; min-width: 24px; height: 24px; padding: 0 6px; background: transparent; color: ${fg}; border: 0; border-radius: 3px; font: 13px/1 ui-monospace, SFMono-Regular, Menlo, monospace; cursor: pointer; }
.mermaid-controls button:hover { background: ${bgElev}; }
.mermaid-controls button:focus-visible { outline: 2px solid ${accent}; outline-offset: -1px; }
.mermaid-error { color: ${hlName}; background: ${bgElev}; padding: 12px; border-radius: 6px; border-left: 4px solid ${hlName}; white-space: pre-wrap; }
`;
  }
  // Single source of truth for markdown rendering: lex once, walk heading
  // tokens to assign slugs (which the custom renderer reads as `id="…"`),
  // collect the TOC, then parse the mutated tokens. Sharing the slugger
  // between the renderer and the TOC means the sidebar id always matches
  // the heading id in the iframe.
  //
  // Mermaid blocks (` ```mermaid `) are extracted after marked has rendered
  // them as plain `<pre><code class="hljs language-mermaid">…</code></pre>`
  // (mermaid isn't a hljs language so the code stays untouched-text). Each
  // block is replaced by a placeholder div carrying a stable id and the
  // decoded source is handed to the async render effect, which fills the
  // placeholder once the SVG is ready.
  let mdRender = $derived.by<{
    srcdoc: string;
    toc: TocEntry[];
    mermaidBlocks: { id: string; code: string }[];
  }>(() => {
    if (!isMarkdown || !data) return { srcdoc: '', toc: [], mermaidBlocks: [] };
    void theme.name; // dep: re-render on theme switch
    const tokens = marked.lexer(data.content);
    const slugger = makeSlugger();
    const toc: TocEntry[] = [];
    for (const t of tokens) {
      if ((t as { type?: string }).type === 'heading') {
        const h = t as { depth: number; text: string };
        const slug = slugger(h.text);
        (t as Record<string, unknown>).slug = slug;
        toc.push({ level: h.depth, text: h.text, slug });
      }
    }
    // Strip script tags and inline event handlers before embedding in the
    // sandboxed iframe. The sandbox already blocks execution; this removes the
    // noisy `Blocked script execution in 'about:srcdoc'` console warnings that
    // fire for every literal <script> the markdown source contains (common in
    // README snippets demonstrating JS).
    let html = stripExecutableHtml(marked.parser(tokens));
    const extracted = extractMermaidBlocks(html);
    html = extracted.html;
    const dir = path.split('/').slice(0, -1).join('/');
    const baseHref = dir
      ? `/raw/${dir.split('/').map(encodeURIComponent).join('/')}/`
      : '/raw/';
    const srcdoc = `<!doctype html><html><head><meta charset="utf-8"><base href="${escapeAttr(baseHref)}"><style>${buildMarkdownCss()}</style></head><body>${html}</body></html>`;
    return { srcdoc, toc, mermaidBlocks: extracted.blocks };
  });
  let mdToc = $derived(mdRender.toc);

  // Async-rendered mermaid SVGs keyed by the placeholder id. The effect below
  // fans out one render per block; the srcdoc derivation below substitutes
  // each placeholder with its SVG once it arrives. While renders are in
  // flight the placeholders remain visible as an empty box, matching how
  // images load progressively.
  let mermaidSvgs = $state<Record<string, string>>({});

  $effect(() => {
    const blocks = mdRender.mermaidBlocks;
    void theme.name; // re-render when the theme changes so the diagram tracks
    if (blocks.length === 0) {
      mermaidSvgs = {};
      return;
    }
    resetMermaidTheme();
    let cancelled = false;
    (async () => {
      const next: Record<string, string> = {};
      for (const b of blocks) {
        const res = await renderMermaid(b.code);
        if (cancelled) return;
        next[b.id] = res.ok
          ? res.svg
          : `<pre class="mermaid-error">${escapeHtml(res.error ?? 'Mermaid render failed')}</pre>`;
      }
      if (!cancelled) mermaidSvgs = next;
    })();
    return () => {
      cancelled = true;
    };
  });

  let mdSrcDoc = $derived.by(() => {
    let src = mdRender.srcdoc;
    for (const [id, svg] of Object.entries(mermaidSvgs)) {
      src = src.replace(
        `<div class="mermaid-block" data-mid="${id}"></div>`,
        `<div class="mermaid-svg" data-mid="${id}">${svg}` +
          `<div class="mermaid-controls" role="toolbar" aria-label="Diagram controls">` +
          `<button type="button" data-pz="out" title="Zoom out">&minus;</button>` +
          `<button type="button" data-pz="reset" title="Reset (double-click diagram)">&#x2B6F;</button>` +
          `<button type="button" data-pz="in" title="Zoom in">+</button>` +
          `</div></div>`,
      );
    }
    return src;
  });

  // Standalone .mmd file rendering — same pipeline as markdown's mermaid
  // blocks, but rendered directly in the parent document since the output is
  // safe SVG with no surrounding HTML to sandbox.
  let mmdSvg = $state<string>('');

  $effect(() => {
    if (!isMmd || !data) {
      mmdSvg = '';
      return;
    }
    void theme.name;
    resetMermaidTheme();
    const code = data.content;
    let cancelled = false;
    (async () => {
      const res = await renderMermaid(code);
      if (cancelled) return;
      mmdSvg = res.ok
        ? res.svg
        : `<pre class="mermaid-error">${escapeHtml(res.error ?? 'Mermaid render failed')}</pre>`;
    })();
    return () => {
      cancelled = true;
    };
  });

  // Pan / zoom for the standalone .mmd preview. Re-attached whenever the
  // SVG markup changes (theme switch, file change, re-render).
  let mmdWrapEl: HTMLDivElement | null = $state(null);
  let mmdPz = $state<PanZoomController | null>(null);

  $effect(() => {
    void mmdSvg;
    if (!isMmd || !mmdWrapEl) {
      mmdPz?.destroy();
      mmdPz = null;
      return;
    }
    let attached: PanZoomController | null = null;
    // The SVG appears asynchronously after {@html mmdSvg} settles; defer
    // one microtask so the DOM is up to date before we query it.
    queueMicrotask(() => {
      if (!mmdWrapEl) return;
      const svg = mmdWrapEl.querySelector('svg') as SVGSVGElement | null;
      if (!svg) return;
      attached = attachPanZoom(svg, { wheelRequiresModifier: false });
      mmdPz = attached;
    });
    return () => {
      attached?.destroy();
      mmdPz = null;
    };
  });

  // Pan / zoom for each mermaid block inside the markdown iframe. The
  // iframe is sandbox="allow-same-origin", so the parent reaches into its
  // document directly and binds the controllers on the SVGs it finds.
  // We re-bind after every srcdoc update (mermaid SVGs arrive async, and
  // mdSrcDoc is rebuilt each time the SVG map fills in).
  let mdMermaidControllers: PanZoomController[] = [];

  $effect(() => {
    void mdSrcDoc;
    if (!isMarkdown || !mdIframeEl) {
      mdMermaidControllers.forEach((c) => c.destroy());
      mdMermaidControllers = [];
      return;
    }
    const iframe = mdIframeEl;
    const setup = () => {
      mdMermaidControllers.forEach((c) => c.destroy());
      mdMermaidControllers = [];
      const doc = iframe.contentDocument;
      if (!doc) return;
      doc.querySelectorAll('.mermaid-svg svg').forEach((node) => {
        const ctrl = attachPanZoom(node as SVGSVGElement, {
          wheelRequiresModifier: true,
        });
        mdMermaidControllers.push(ctrl);
      });
      // Wire the iframe-side CSS-only control buttons (injected via
      // mermaid block CSS) to the matching controller by data-mid.
      doc.querySelectorAll('.mermaid-svg').forEach((wrap, i) => {
        const ctrl = mdMermaidControllers[i];
        if (!ctrl) return;
        wrap.querySelectorAll('[data-pz]').forEach((btn) => {
          const action = (btn as HTMLElement).dataset.pz;
          btn.addEventListener('click', (e) => {
            e.preventDefault();
            if (action === 'in') ctrl.zoomIn();
            else if (action === 'out') ctrl.zoomOut();
            else if (action === 'reset') ctrl.reset();
          });
        });
      });
    };
    iframe.addEventListener('load', setup);
    // Iframe may already be loaded (re-render with same readyState).
    if (iframe.contentDocument?.readyState === 'complete') setup();
    return () => {
      iframe.removeEventListener('load', setup);
      mdMermaidControllers.forEach((c) => c.destroy());
      mdMermaidControllers = [];
    };
  });

  // True when the right-side symbols/insights aside is rendered. Mirrors the
  // if/else-if chain in the template so .body can collapse to a single column
  // when the aside is hidden (either by user toggle or no applicable content).
  let asideVisible = $derived.by(() => {
    if (!view.showSymbols || !data) return false;
    // history mode occupies the full width (list + diff); hide the aside.
    if (historyMode) return false;
    if (isMarkdown && previewView === 'rendered' && mdToc.length > 0) return true;
    if (isCss || isJson || isXml || isYaml || isSql) return true;
    if (isTestsTarget) return true;
    if (isRelationsTarget) return true;
    if (data.symbols && data.symbols.length > 0) return true;
    return true;
  });
  function togglePreviewView() {
    previewView = previewView === 'rendered' ? 'source' : 'rendered';
    writePreviewViewPref(previewView);
    announce(`Preview: ${previewView}`);
  }

  // Has the current file got working-tree changes git can diff against?
  // The /api/file response carries git status as a single-letter string
  // (M/A/D/R/?); an empty/space status means unmodified.
  let hasGitChanges = $derived.by<boolean>(() => {
    if (!data) return false;
    const g = data.git;
    if (!g || g === ' ') return false;
    return true;
  });

  let diffActionAvailable = $derived(
    hasGitChanges || diffMode || (routeModeApplies && route.mode === 'diff'),
  );

  function setRouteFileMode(mode: FileViewMode | '') {
    if (!routeModeApplies) return;
    navigate(toFileHash(path, {
      line: route.lineHint,
      open: route.openPaths.length > 0 ? route.openPaths : undefined,
      right: route.rightPath || undefined,
      mode,
    }));
  }

  function loadDiff(p: string) {
    diffLoading = true;
    diffError = null;
    diffData = null;
    fetchGitDiff(p)
      .then((r) => {
        diffData = r;
      })
      .catch((e: unknown) => {
        diffError = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        diffLoading = false;
      });
  }

  function toggleDiff() {
    if (!data) return;
    if (diffMode) {
      diffMode = false;
      setRouteFileMode('');
      announce('Diff view off');
      return;
    }
    // exclusive with historyMode
    historyMode = false;
    diffMode = true;
    setRouteFileMode('diff');
    announce('Diff view on');
    if (!diffData && !diffLoading) {
      loadDiff(path);
    }
  }

  // ---------------------------------------------------------------------------
  // History mode — file commit log + commit-to-commit diff
  // ---------------------------------------------------------------------------

  function toggleHistory() {
    if (!data) return;
    if (historyMode) {
      historyMode = false;
      setRouteFileMode('');
      announce('History view off');
      return;
    }
    // exclusive with diffMode
    diffMode = false;
    historyMode = true;
    setRouteFileMode('history');
    announce('History view on');
  }

  // Fetch commit log whenever historyMode activates or path changes.
  $effect(() => {
    if (!historyMode) {
      // clear when leaving history mode
      historyData = null;
      selectedHash = null;
      commitDiffData = null;
      commitDiffError = null;
      historyError = null;
      return;
    }
    const p = path;
    historyLoading = true;
    historyError = null;
    let cancelled = false;
    fetchFileLog(p, 50)
      .then((r) => {
        if (cancelled) return;
        historyData = r;
        // auto-select the most recent commit
        if (r.commits.length > 0) {
          selectedHash = r.commits[0].hash_full;
        }
        announce(`${r.commits.length} commit${r.commits.length === 1 ? '' : 's'} loaded`);
      })
      .catch((e: unknown) => {
        if (cancelled) return;
        historyError = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (!cancelled) historyLoading = false;
      });
    return () => { cancelled = true; };
  });

  // Fetch commit diff whenever selectedHash changes while in history mode.
  $effect(() => {
    if (!historyMode || !historyData || !selectedHash) {
      commitDiffData = null;
      commitDiffError = null;
      return;
    }
    const commits = historyData.commits;
    const idx = commits.findIndex((c) => c.hash_full === selectedHash);
    if (idx < 0) {
      commitDiffData = null;
      commitDiffError = null;
      return;
    }
    const toHash = commits[idx].hash_full;
    // The server resolves the parent expression; a root commit's parent is an
    // empty before-side so the first file version still renders as "New file".
    const fromHash = idx + 1 < commits.length ? commits[idx + 1].hash_full : `${toHash}^`;
    const p = path;
    let cancelled = false;
    commitDiffData = null;
    commitDiffError = null;
    fetchFileCommitDiff(p, fromHash, toHash)
      .then((r) => { if (!cancelled) commitDiffData = r; })
      .catch((e: unknown) => {
        if (cancelled) return;
        commitDiffData = null;
        commitDiffError = e instanceof ApiCallError && e.code === 'invalid_revision'
          ? 'Unable to resolve this commit range.'
          : e instanceof Error ? e.message : String(e);
      });
    return () => { cancelled = true; };
  });

  // Derive diffLinesView source: in historyMode use commitDiffData, otherwise diffData.
  let effectiveDiffData = $derived(historyMode ? commitDiffData : diffData);

  // Keyboard navigation within history list (j/k, only in historyMode).
  function historyKeyNav(e: KeyboardEvent): void {
    if (!isFocusedPane()) return;
    if (!historyMode || !historyData) return;
    if (isTextInputFocused()) return;
    if (e.shiftKey || e.metaKey || e.ctrlKey || e.altKey) return;
    const commits = historyData.commits;
    if (commits.length === 0) return;
    const cur = commits.findIndex((c) => c.hash_full === selectedHash);
    if (e.key === 'j') {
      e.preventDefault();
      const next = Math.min(cur + 1, commits.length - 1);
      selectedHash = commits[next].hash_full;
    } else if (e.key === 'k') {
      e.preventDefault();
      const prev = Math.max(cur - 1, 0);
      selectedHash = commits[prev].hash_full;
    }
  }

  $effect(() => {
    if (!historyMode) return;
    window.addEventListener('keydown', historyKeyNav);
    return () => window.removeEventListener('keydown', historyKeyNav);
  });

  // Scroll selected history item into view when selectedHash changes.
  $effect(() => {
    if (!historyMode || !selectedHash || !historyListEl) return;
    const el = historyListEl.querySelector<HTMLElement>('[aria-selected="true"]');
    el?.scrollIntoView({ block: 'nearest' });
  });

  // Epoch counter so a slow earlier response can't overwrite a newer file's
  // content (same out-of-order guard as the history/commit-diff effects).
  let loadEpoch = 0;

  function load(p: string) {
    if (!p) return;
    const epoch = ++loadEpoch;
    loading = true;
    error = null;
    data = null;
    // Reset diff state when navigating to a different file — the cached diff
    // belongs to the previous path.
    diffMode = false;
    diffData = null;
    diffError = null;
    diffLoading = false;
    // Reset history state.
    historyMode = false;
    historyData = null;
    selectedHash = null;
    commitDiffData = null;
    commitDiffError = null;
    historyLoading = false;
    historyError = null;
    fetchFile(p, { symbols: true })
      .then((r) => {
        if (epoch !== loadEpoch) return;
        data = r;
      })
      .catch((e: unknown) => {
        if (epoch !== loadEpoch) return;
        error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (epoch === loadEpoch) loading = false;
      });
  }

  $effect(() => {
    load(path);
  });

  let appliedRouteModeKey = '';
  $effect(() => {
    const mode = routeModeApplies ? route.mode : undefined;
    const key = `${path}:${mode ?? ''}`;
    if (key === appliedRouteModeKey) return;
    appliedRouteModeKey = key;
    if (!routeModeApplies) return;

    if (mode === 'diff') {
      historyMode = false;
      diffMode = true;
      if (!diffData && !diffLoading) loadDiff(path);
    } else if (mode === 'history') {
      diffMode = false;
      historyMode = true;
    } else {
      diffMode = false;
      historyMode = false;
    }
  });

  // Scroll memo: remember the current path's scrollTop right before this
  // effect re-runs (i.e. just before `path` becomes a new value), and restore
  // the next path's scrollTop after content layout is committed.
  //
  // WHY cleanup-on-rerun: at cleanup time `path` still refers to the OLD path
  // (Svelte 5 captures the dependency value used during the effect body), so
  // we can save it without storing a separate `prevPath` ref.
  $effect(() => {
    const current = path;
    // restore happens once `data` lands; tracked via a separate effect below.
    return () => {
      if (codeEl) rememberScroll(current, codeEl.scrollTop);
    };
  });

  // Restore on (path, data) pair: wait one tick for the line DOM, then one
  // frame so the browser has committed layout — `el.scrollTop = …` before
  // that point is silently clamped to 0.
  let pendingRestore = $state<string | null>(null);
  $effect(() => {
    if (!data || !codeEl) return;
    // route.lineHint takes precedence — its own effect scrolls to the line.
    if (route.lineHint) return;
    const target = path;
    const top = recallScroll(target);
    if (top === undefined || top <= 0) return;
    pendingRestore = target;
    tick().then(() => {
      requestAnimationFrame(() => {
        // Bail out if the user navigated again while we were waiting.
        if (pendingRestore !== target || !codeEl || path !== target) return;
        codeEl.scrollTop = top;
        pendingRestore = null;
      });
    });
  });

  // Re-apply scroll-restore once all chunks are rendered, in case the saved
  // position was beyond the first-chunk viewport and got clamped.
  $effect(() => {
    if (!readyLines || !codeEl || route.lineHint) return;
    const target = path;
    const top = recallScroll(target);
    if (top === undefined || top <= 0) return;
    requestAnimationFrame(() => {
      if (!codeEl || path !== target) return;
      if (Math.abs(codeEl.scrollTop - top) > 4) codeEl.scrollTop = top;
    });
  });

  function escapeHtml(s: string): string {
    return s.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  }

  // WHY: chunked highlight prevents main-thread blocking on large files.
  // Sync-highlight the first ~100 lines for immediate first paint (LCP), then
  // yield between 50-line chunks so interactions aren't blocked during fill.
  const FIRST_CHUNK = 100;
  const REST_CHUNK = 50;

  // scheduler.yield() (Chromium 129+) resumes at higher priority than new tasks;
  // fall back to setTimeout(0) for other engines.
  const yieldToMain: () => Promise<void> =
    typeof (globalThis as Record<string, unknown>).scheduler === 'object' &&
    typeof (
      (globalThis as Record<string, unknown>).scheduler as Record<string, unknown>
    ).yield === 'function'
      ? () =>
          (
            (globalThis as Record<string, unknown>).scheduler as {
              yield: () => Promise<void>;
            }
          ).yield()
      : () => new Promise<void>((resolve) => setTimeout(resolve, 0));

  // Epoch counter: incremented on each new data load to cancel in-flight loops.
  // Plain `let` (not $state) — it's a cancellation token read only inside the
  // owning effect's body and its async closures. Making it reactive would let
  // the `++linesEpoch` inside the effect register the counter as a dependency
  // and then write to it in the same pass, scheduling the effect to re-run
  // forever (Svelte's effect_update_depth_exceeded).
  let linesEpoch = 0;
  let lines = $state<{ n: number; html: string }[]>([]);
  // true once all chunks are rendered — guards anchor scroll and scroll-restore.
  let readyLines = $state(false);

  $effect(() => {
    if (!data) { lines = []; readyLines = false; return; }
    const epoch = ++linesEpoch;
    const lang = data.lang || langFromPath(data.path);
    const useLang = lang && hljs.getLanguage(lang) ? lang : '';
    const arr = data.content.split('\n');
    if (arr.length > 1 && arr[arr.length - 1] === '') arr.pop();
    const hl = (line: string, i: number): { n: number; html: string } => {
      let html = line === '' ? '' : useLang
        ? (() => { try { return hljs.highlight(line, { language: useLang, ignoreIllegals: true }).value; } catch { return escapeHtml(line); } })()
        : escapeHtml(line);
      return { n: i + 1, html };
    };
    readyLines = false;
    lines = arr.slice(0, FIRST_CHUNK).map(hl);
    if (arr.length <= FIRST_CHUNK) { readyLines = true; return; }
    void (async () => {
      let off = FIRST_CHUNK;
      while (off < arr.length) {
        await yieldToMain();
        if (linesEpoch !== epoch) return;
        // Appending to the $state proxy is safe here: the async closure runs
        // after the effect's tracking pass, so the read doesn't register a
        // dependency — and push keeps accumulation O(N) total instead of the
        // O(N²) of rebuilding the array per chunk.
        lines.push(...arr.slice(off, off + REST_CHUNK).map((l, j) => hl(l, off + j)));
        off += REST_CHUNK;
      }
      if (linesEpoch === epoch) readyLines = true;
    })();
  });

  let lineCount = $derived(lines.length);

  // Epoch counter for diffLinesView cancellation. Plain `let` for the same
  // reason as linesEpoch above — read-then-write inside the owning effect
  // would self-schedule under $state.
  let diffEpoch = 0;
  let diffLinesView = $state<
    { type: 'add' | 'del' | 'eq'; html: string; oldNum?: number; newNum?: number }[]
  >([]);

  // Diff lines with hljs-highlighted content. Per-line highlighting matches
  // the source viewer (`lines` above) so colours stay consistent between Diff
  // and Source views; the language is inferred from the file path because the
  // diff API does not echo `lang`.
  // In historyMode, effectiveDiffData points to commitDiffData; otherwise diffData.
  $effect(() => {
    if (!effectiveDiffData) { diffLinesView = []; return; }
    const epoch = ++diffEpoch;
    const lang = langFromPath(effectiveDiffData.path);
    const useLang = lang && hljs.getLanguage(lang) ? lang : '';
    const all = effectiveDiffData.lines;
    const hl = (ln: (typeof all)[number]): (typeof diffLinesView)[number] => {
      let html = ln.text === '' ? '' : useLang
        ? (() => { try { return hljs.highlight(ln.text, { language: useLang, ignoreIllegals: true }).value; } catch { return escapeHtml(ln.text); } })()
        : escapeHtml(ln.text);
      return { type: ln.type, html, oldNum: ln.old_num, newNum: ln.new_num };
    };
    diffLinesView = all.slice(0, FIRST_CHUNK).map(hl);
    if (all.length <= FIRST_CHUNK) return;
    void (async () => {
      let off = FIRST_CHUNK;
      while (off < all.length) {
        await yieldToMain();
        if (diffEpoch !== epoch) return;
        // push: see the `lines` effect above — O(N) accumulation, no
        // dependency registration outside the tracking pass.
        diffLinesView.push(...all.slice(off, off + REST_CHUNK).map(hl));
        off += REST_CHUNK;
      }
    })();
  });

  // Hunks = indices (into diffLinesView) where a run of non-`eq` lines starts.
  // Each consecutive add/del block is one hunk; `eq` lines between blocks
  // split runs. Drives the Prev/Next jump buttons.
  let diffHunks = $derived.by<number[]>(() => {
    const hunks: number[] = [];
    let inHunk = false;
    for (let i = 0; i < diffLinesView.length; i++) {
      const t = diffLinesView[i].type;
      if (t === 'add' || t === 'del') {
        if (!inHunk) {
          hunks.push(i);
          inHunk = true;
        }
      } else {
        inHunk = false;
      }
    }
    return hunks;
  });
  let currentHunk = $state(0);

  // Reset and auto-scroll to the first hunk whenever a new diff lands.
  $effect(() => {
    const activeMode = diffMode || historyMode;
    if (!activeMode || !effectiveDiffData || diffHunks.length === 0) {
      currentHunk = 0;
      return;
    }
    currentHunk = 0;
    // Wait one tick + one frame so the diff DOM is mounted before scrolling.
    const target = diffHunks[0];
    tick().then(() => {
      requestAnimationFrame(() => scrollToHunkLine(target, 'auto'));
    });
  });

  function scrollToHunkLine(lineIdx: number, behavior: ScrollBehavior = 'smooth') {
    if (!diffEl) return;
    const el = diffEl.querySelector<HTMLElement>(`[data-diff-line="${lineIdx}"]`);
    if (!el) return;
    el.scrollIntoView({ behavior, block: 'center' });
  }

  function jumpDiff(delta: 1 | -1) {
    if (diffHunks.length === 0) return;
    const next = (currentHunk + delta + diffHunks.length) % diffHunks.length;
    currentHunk = next;
    scrollToHunkLine(diffHunks[next]);
    announce(`Diff hunk ${next + 1} of ${diffHunks.length}`);
  }

  // ---------------------------------------------------------------------------
  // scroll-to-line: from URL hint (?L=N) or symbol click.
  // ---------------------------------------------------------------------------
  function scrollToLine(line: number, behavior: ScrollBehavior = 'smooth') {
    if (!codeEl) return;
    const el = codeEl.querySelector<HTMLElement>(`#L${line}`);
    if (!el) return;
    el.scrollIntoView({ behavior, block: 'center' });
  }

  function pulse(line: number) {
    pulseLine = line;
    if (pulseTimer) clearTimeout(pulseTimer);
    pulseTimer = setTimeout(() => {
      pulseLine = null;
      pulseTimer = null;
    }, 1500);
  }

  // react to route.lineHint after content renders.
  // Wait for readyLines when hint > FIRST_CHUNK — the target DOM node is not
  // yet present during progressive chunk fill.
  $effect(() => {
    if (!data) return;
    const hint = route.lineHint;
    if (!hint || hint < 1) return;
    if (!readyLines && hint > FIRST_CHUNK) return;
    if (hint > lineCount) return;
    queueMicrotask(() => {
      scrollToLine(hint, 'auto');
      pulse(hint);
    });
  });

  function jumpToSymbol(line: number, jumpPath?: string) {
    // SymbolList is wired with the active file's symbols, so the optional
    // `jumpPath` is only honoured for cross-file symbol producers (future
    // callers). Same-path or omitted -> intra-file scroll.
    if (jumpPath && jumpPath !== path) {
      navigate(toFileHash(jumpPath, { line }));
      return;
    }
    if (line < 1) return;
    scrollToLine(line, 'smooth');
    pulse(line);
  }

  // ---------------------------------------------------------------------------
  // Symbol jump (Issue #40) — clickable identifiers in the highlighted code.
  //
  // Approach A (DOM walk after highlight): for every line we re-scan the
  // hljs-produced spans for *.hljs-title, *.hljs-variable, *.hljs-type and
  // wrap their text nodes in <a class="ctx-jump"> elements. Same-file symbols
  // resolve immediately from data.symbols (instant click). Cross-file symbols
  // use a 150ms hover debounce to call /api/definition, then re-decorate.
  //
  // Why text-node replacement (not span replacement): hljs spans carry colour
  // classes; rewriting them as <a> would lose the colour. We keep the span
  // and inject an <a> wrapper around its text content only.
  //
  // Trade-off: cost is O(visible-line spans) per render. For 1000+ line files
  // this is acceptable (< 5ms in dev profiles); IntersectionObserver gating
  // is left as #TODO(agent).
  // ---------------------------------------------------------------------------

  // Same-file index: name -> first matching symbol (line + kind). Line of
  // first definition wins on duplicates, which matches the symbols panel.
  let symbolIndex = $derived.by<Map<string, Symbol>>(() => {
    const m = new Map<string, Symbol>();
    if (!data?.symbols) return m;
    for (const s of data.symbols) {
      if (!m.has(s.name)) m.set(s.name, s);
    }
    return m;
  });

  // Hover lazy fetch — single AbortController shared across hovers, so a fast
  // mousemove cancels the previous request without piling up sockets.
  let hoverTimer: ReturnType<typeof setTimeout> | null = null;
  let hoverAbort: AbortController | null = null;
  let lastHoverName = '';

  function cancelPendingHover(): void {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    if (hoverAbort) {
      hoverAbort.abort();
      hoverAbort = null;
    }
  }

  function reduceMotion(): boolean {
    return (
      typeof window !== 'undefined' &&
      window.matchMedia('(prefers-reduced-motion: reduce)').matches
    );
  }

  // Selectors for highlight.js identifier-bearing spans across common langs.
  // Using `[class*="hljs-title"]` covers `hljs-title.function_`, `.class_`,
  // `.invoked` and other language-specific variants without listing each.
  const HLJS_IDENT_SELECTOR =
    '[class*="hljs-title"], .hljs-variable, .hljs-type, .hljs-class, .hljs-function';

  // Decorate one rendered line element with <a class="ctx-jump"> wrappers
  // around the text inside hljs identifier spans. Same-file symbols become
  // hash links; unknown names get a `data-pending` attribute that the hover
  // path uses to upgrade them after a network resolution.
  function decorateLine(lineEl: HTMLElement): void {
    const content = lineEl.querySelector<HTMLElement>('.ln-content');
    if (!content) return;
    const ver = String(decorateVersion);
    if (content.dataset.ctxDecorated === ver) return;
    content.dataset.ctxDecorated = ver;
    const idents = content.querySelectorAll<HTMLElement>(HLJS_IDENT_SELECTOR);
    for (const span of idents) {
      // Only operate on a single text-node child; nested spans (e.g.
      // hljs-title containing hljs-keyword for default-export classes) are
      // skipped to avoid breaking the colouring.
      if (
        span.childNodes.length !== 1 ||
        span.firstChild?.nodeType !== Node.TEXT_NODE
      ) {
        continue;
      }
      const text = span.textContent ?? '';
      const trimmed = text.trim();
      if (!trimmed || !/^[A-Za-z_$][A-Za-z0-9_$]*$/.test(trimmed)) continue;
      const same = symbolIndex.get(trimmed);
      const a = document.createElement('a');
      a.className = 'ctx-jump';
      a.dataset.symbolName = trimmed;
      a.tabIndex = -1;
      if (same) {
        const href = `#/file/${path.split('/').map(encodeURIComponent).join('/')}?L=${same.line}`;
        a.href = href;
        a.dataset.target = 'same-file';
        a.dataset.line = String(same.line);
      } else {
        // Pending — see hover path. We still expose href="#" so the link is
        // keyboard-discoverable; click handler intercepts before navigation.
        a.href = '#';
        a.dataset.target = 'cross-file';
        // Pre-warm from cache if the user has already touched this name.
        const cached = peekDefinition(trimmed, path);
        if (cached !== undefined && cached.length > 0) {
          a.dataset.candidates = String(cached.length);
        }
      }
      a.textContent = text;
      span.replaceChild(a, span.firstChild);
    }
    // Approach B: text-node walk で hljs クラスが付かない識別子を補完ラップ
    // (例: Go のレシーバ付きメソッド名、型パラメータ等)
    decorateTextNodes(content);
  }

  // decorateTextNodes —補完デコレーション (Approach B).
  //
  // hljs が `hljs-title` 等のクラスを付与しないトークン (例: Go のレシーバ付き
  // メソッド名、型パラメータ等) を補完的にラップする。
  //
  // - 既に <a class="ctx-jump"> でラップ済みのノードはスキップ (二重ラップ防止)
  // - キーワード/文字列/コメント/数値の直下テキストノードはスキップ
  // - symbolIndex (同一ファイル) または peekDefinition (クロスファイルキャッシュ)
  //   でヒットする名前のみラップ → キーワード・無関係な識別子はリンク化しない
  //
  // #TODO(agent): IntersectionObserver で不可視行をスキップして
  //              大規模ファイル (>5k 行) のパフォーマンスを改善する
  function decorateTextNodes(content: HTMLElement): void {
    const SKIP_PARENT_CLASSES = [
      'hljs-keyword',
      'hljs-string',
      'hljs-comment',
      'hljs-number',
      'hljs-meta',
      'hljs-literal',
      'ctx-jump',
    ] as const;

    const walker = document.createTreeWalker(content, NodeFilter.SHOW_TEXT, {
      acceptNode(node: Node): number {
        const parent = node.parentElement;
        if (!parent) return NodeFilter.FILTER_REJECT;
        // テキストノードが <a class="ctx-jump"> の子孫なら除外
        if (parent.closest('a.ctx-jump')) return NodeFilter.FILTER_REJECT;
        const cls = parent.className ?? '';
        if (SKIP_PARENT_CLASSES.some((c) => cls.includes(c))) return NodeFilter.FILTER_REJECT;
        return NodeFilter.FILTER_ACCEPT;
      },
    });

    // Walker はライブ DOM を走査するため、先に対象ノードをスナップショットしてから変換する
    const targets: Text[] = [];
    let n: Node | null;
    while ((n = walker.nextNode())) targets.push(n as Text);

    for (const textNode of targets) {
      const text = textNode.nodeValue ?? '';
      if (!text) continue;

      // 識別子パターンにマッチする全箇所を収集し、symbolIndex / peekDefinition でフィルタ
      const IDENT_RE = /[A-Za-z_$][A-Za-z0-9_$]*/g;
      type MatchEntry = { name: string; start: number; end: number };
      const matched: MatchEntry[] = [];
      let m: RegExpExecArray | null;
      while ((m = IDENT_RE.exec(text)) !== null) {
        const name = m[0];
        const inSameFile = symbolIndex.has(name);
        const inCrossFile = !inSameFile && peekDefinition(name, path) !== undefined;
        if (inSameFile || inCrossFile) {
          matched.push({ name, start: m.index, end: m.index + name.length });
        }
      }
      if (matched.length === 0) continue;

      // テキストノードをマッチ区間で分割し、マッチ箇所を <a class="ctx-jump"> でラップ
      const parent = textNode.parentNode;
      if (!parent) continue;

      const frag = document.createDocumentFragment();
      let cursor = 0;
      for (const entry of matched) {
        if (entry.start > cursor) {
          frag.appendChild(document.createTextNode(text.slice(cursor, entry.start)));
        }
        const same = symbolIndex.get(entry.name);
        const a = document.createElement('a');
        a.className = 'ctx-jump';
        a.dataset.symbolName = entry.name;
        a.tabIndex = -1;
        if (same) {
          const href = `#/file/${path.split('/').map(encodeURIComponent).join('/')}?L=${same.line}`;
          a.href = href;
          a.dataset.target = 'same-file';
          a.dataset.line = String(same.line);
        } else {
          a.href = '#';
          a.dataset.target = 'cross-file';
          const cached = peekDefinition(entry.name, path);
          if (cached !== undefined && cached.length > 0) {
            a.dataset.candidates = String(cached.length);
          }
        }
        a.textContent = text.slice(entry.start, entry.end);
        frag.appendChild(a);
        cursor = entry.end;
      }
      if (cursor < text.length) {
        frag.appendChild(document.createTextNode(text.slice(cursor)));
      }
      parent.replaceChild(frag, textNode);
    }
  }

  // Decorate lines lazily as they approach the viewport. Decorating the whole
  // file eagerly walks every rendered line's DOM (TreeWalker + querySelectorAll)
  // and janks scroll on 5k+ line files; the observer defers that work to the
  // lines the user can actually see (rootMargin pre-decorates one viewport
  // ahead in both directions). Clipped lines inside a scrolled pane report no
  // viewport intersection, so observing against the viewport root is correct.
  let decorateObserver: IntersectionObserver | null = null;

  function ensureDecorateObserver(): IntersectionObserver {
    if (decorateObserver) return decorateObserver;
    decorateObserver = new IntersectionObserver(
      (entries) => {
        for (const entry of entries) {
          if (!entry.isIntersecting) continue;
          decorateObserver?.unobserve(entry.target);
          decorateLine(entry.target as HTMLElement);
        }
      },
      { rootMargin: '100% 0px' }
    );
    return decorateObserver;
  }

  $effect(() => () => decorateObserver?.disconnect());

  // Decoration version: bumped when the symbol index is replaced (e.g. the
  // late `/api/file?symbols=true` upgrade). The keyed {#each} reuses .line DOM
  // across data refreshes, so dataset marks survive — versioning them forces a
  // re-observe + re-decorate against the new index (decorateLine is idempotent
  // on already-wrapped content).
  let lastSymbolIndex: Map<string, Symbol> | null = null;
  let decorateVersion = 0;

  // Register lines with the observer. Re-runs as chunks land (lines.length) —
  // which also fixes the old eager pass silently skipping chunks rendered
  // after it ran — and when the symbol index changes.
  $effect(() => {
    if (!data || !codeEl) return;
    const sym = symbolIndex;
    void lines.length;
    if (sym !== lastSymbolIndex) {
      lastSymbolIndex = sym;
      decorateVersion++;
    }
    const ver = String(decorateVersion);
    const el = codeEl;
    tick().then(() => {
      const obs = ensureDecorateObserver();
      const lineEls = el.querySelectorAll<HTMLElement>('.line');
      for (const ln of lineEls) {
        if (ln.dataset.ctxObserved === ver) continue;
        ln.dataset.ctxObserved = ver;
        obs.observe(ln);
      }
    });
  });

  // Lookup driver: wraps definitions.lookup() with the shared AbortController
  // so a stale hover doesn't overwrite a newer request's result.
  // NOTE: hover-only. Do NOT use for click/chord — see resolveForClick().
  async function resolveDefinitions(name: string): Promise<DefinitionCandidate[]> {
    cancelPendingHover();
    hoverAbort = new AbortController();
    lastHoverName = name;
    const list = await lookupDefinition(name, path, hoverAbort.signal);
    if (lastHoverName !== name) return list; // user moved on
    return list;
  }

  // Click/chord lookup — does NOT share hoverAbort, so fast mouse movement
  // after a click cannot abort the in-flight network request.
  // Only the hover debounce timer is cleared (to prevent a duplicate request
  // racing the click fetch); the in-flight hover XHR is left alone.
  function resolveForClick(name: string): Promise<DefinitionCandidate[]> {
    if (hoverTimer) {
      clearTimeout(hoverTimer);
      hoverTimer = null;
    }
    // No signal → never aborted by cancelPendingHover().
    return lookupDefinition(name, path);
  }

  // Hover handler — only used for cross-file pending anchors. 150ms debounce
  // to avoid request-storm during fast mouse traversal.
  function onCodeMouseOver(e: MouseEvent) {
    const target = e.target;
    if (!(target instanceof HTMLAnchorElement)) return;
    if (!target.classList.contains('ctx-jump')) return;
    if (target.dataset.target !== 'cross-file') return;
    if (target.dataset.candidates !== undefined) return; // already resolved
    const name = target.dataset.symbolName ?? '';
    if (!name) return;
    cancelPendingHover();
    hoverTimer = setTimeout(() => {
      hoverTimer = null;
      void resolveDefinitions(name).then((list) => {
        // Stamp every matching anchor in the viewer with the new count.
        if (!codeEl) return;
        const stamp = String(list.length);
        const matches = codeEl.querySelectorAll<HTMLAnchorElement>(
          `a.ctx-jump[data-symbol-name="${CSS.escape(name)}"]`,
        );
        for (const a of matches) a.dataset.candidates = stamp;
      });
    }, 150);
  }

  function onCodeMouseOut(e: MouseEvent) {
    // Cancel pending hover when leaving the anchor before debounce fires.
    const related = e.relatedTarget;
    if (related instanceof HTMLAnchorElement && related.classList.contains('ctx-jump')) {
      return; // moving between adjacent jumps — let the new mouseover decide
    }
    cancelPendingHover();
  }

  function onCodeClick(e: MouseEvent) {
    const target = e.target;
    if (!(target instanceof HTMLAnchorElement)) return;
    if (!target.classList.contains('ctx-jump')) return;
    e.preventDefault();
    const name = target.dataset.symbolName ?? '';
    if (!name) return;
    if (target.dataset.target === 'same-file') {
      const line = Number(target.dataset.line ?? '0');
      if (line < 1) return;
      scrollToLine(line, 'smooth');
      pulse(line);
      announce(`Jumped to ${name} in ${path} line ${line}`);
      return;
    }
    // cross-file — fetch (cache-first) and route on count.
    // resolveForClick: never aborted by hover mouse movement.
    void resolveForClick(name).then((list) => {
      if (list.length === 0) {
        announce(`No definition for ${name}`);
        return;
      }
      if (list.length === 1) {
        const c = list[0];
        if ((e.metaKey || e.ctrlKey) && c.path !== path) {
          openTab(c.path);
          announce(`Opened ${c.path} in new tab`);
          return;
        }
        navigate(toFileHash(c.path, { line: c.line }));
        announce(`Jumped to ${name} in ${c.path} line ${c.line}`);
        return;
      }
      openDefinitionPicker(name, list);
    });
  }

  // ---- vim-like chords — `g d` (existing), `g g`, `z z`, plus j/k/G/Ctrl-d/u/f/b/0/$ ----
  // State machine: at most one of `pendingG`/`pendingZ` is true; `countBuf`
  // accumulates a numeric prefix that is consumed by the next j/k/G/G-jump.
  // Each pending chord owns its own 500ms expiry timer so they don't clobber
  // each other; the count buffer uses 1500ms (vim's typical timeoutlen) so a
  // user pasting "120G" via slow physical typing still resolves correctly.
  let pendingGTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingG = false;
  let pendingZTimer: ReturnType<typeof setTimeout> | null = null;
  let pendingZ = false;
  let countBuf = '';
  let countBufTimer: ReturnType<typeof setTimeout> | null = null;

  function viewportCenterSymbolName(): string | null {
    if (!codeEl) return null;
    const rect = codeEl.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const el = document.elementFromPoint(cx, cy);
    if (!el) return null;
    const a = (el as HTMLElement).closest<HTMLAnchorElement>('a.ctx-jump');
    if (a) return a.dataset.symbolName ?? null;
    // Fallback — find any ctx-jump on the closest .line.
    const line = (el as HTMLElement).closest<HTMLElement>('.line');
    const first = line?.querySelector<HTMLAnchorElement>('a.ctx-jump');
    return first?.dataset.symbolName ?? null;
  }

  function isTextInputFocused(): boolean {
    const a = document.activeElement as HTMLElement | null;
    if (!a) return false;
    const tag = a.tagName;
    if (tag === 'INPUT' || tag === 'TEXTAREA' || tag === 'SELECT') return true;
    if (a.isContentEditable) return true;
    return false;
  }

  // Cached per-frame; the line element is identical for every row so a single
  // measurement is reused across the burst of keystrokes.
  function getLineHeight(): number {
    if (!codeEl) return 20;
    const ln = codeEl.querySelector<HTMLElement>('.line');
    if (!ln) return 20;
    const h = ln.getBoundingClientRect().height;
    return h > 0 ? h : 20;
  }

  function clearCountBuf() {
    countBuf = '';
    if (countBufTimer) {
      clearTimeout(countBufTimer);
      countBufTimer = null;
    }
  }

  function consumeCount(fallback = 1): number {
    const n = countBuf === '' ? fallback : parseInt(countBuf, 10);
    clearCountBuf();
    return Number.isFinite(n) && n > 0 ? n : fallback;
  }

  function clearPendingG() {
    pendingG = false;
    if (pendingGTimer) {
      clearTimeout(pendingGTimer);
      pendingGTimer = null;
    }
  }

  function clearPendingZ() {
    pendingZ = false;
    if (pendingZTimer) {
      clearTimeout(pendingZTimer);
      pendingZTimer = null;
    }
  }

  function viewportCenterLineNumber(): number | null {
    if (!codeEl) return null;
    const rect = codeEl.getBoundingClientRect();
    const cx = rect.left + rect.width / 2;
    const cy = rect.top + rect.height / 2;
    const el = document.elementFromPoint(cx, cy);
    if (!el) return null;
    const lineEl = (el as HTMLElement).closest<HTMLElement>('.line[id^="L"]');
    if (!lineEl) return null;
    const n = Number(lineEl.id.slice(1));
    return Number.isFinite(n) && n > 0 ? n : null;
  }

  function gdJump() {
    const name = viewportCenterSymbolName();
    if (!name) {
      announce('No symbol under cursor');
      return;
    }
    // resolveForClick: never aborted by hover mouse movement.
    void resolveForClick(name).then((list) => {
      if (list.length === 0) {
        announce(`No definition for ${name}`);
        return;
      }
      if (list.length === 1) {
        const c = list[0];
        navigate(toFileHash(c.path, { line: c.line }));
        announce(`Jumped to ${name} in ${c.path} line ${c.line}`);
        return;
      }
      openDefinitionPicker(name, list);
    });
  }

  function onChordKey(e: KeyboardEvent) {
    if (!isFocusedPane()) return;
    if (isTextInputFocused()) return;

    // Shift+D toggles diff display. Handled BEFORE the diffMode/findOpen
    // short-circuits so the same key reliably exits diff mode too. Bare
    // (lowercase) `d` is reserved for the `g d` chord, so this only fires
    // on Shift+D with no other modifiers.
    if (e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey && e.key === 'D') {
      e.preventDefault();
      clearPendingG();
      clearPendingZ();
      clearCountBuf();
      toggleDiff();
      return;
    }

    // Shift+H toggles history display (same pattern as Shift+D above).
    if (e.shiftKey && !e.metaKey && !e.ctrlKey && !e.altKey && e.key === 'H') {
      e.preventDefault();
      clearPendingG();
      clearPendingZ();
      clearCountBuf();
      toggleHistory();
      return;
    }

    // Vim navigation is intentionally off while diff hunk nav (j/k=hunk),
    // history mode (j/k=commit nav), or the find bar (j/k=match) are
    // claiming the same keys.
    if (diffMode) return;
    if (historyMode) return;
    if (findOpen) return;

    const mod = e.metaKey || e.ctrlKey;
    const onlyCtrl = e.ctrlKey && !e.metaKey && !e.altKey && !e.shiftKey;

    // --- Ctrl-based half/full page scroll (modifier-only, so doesn't collide
    // with regular j/k typing). ---
    if (onlyCtrl && codeEl) {
      if (e.key === 'd' || e.key === 'D') {
        e.preventDefault();
        clearPendingG();
        clearPendingZ();
        clearCountBuf();
        codeEl.scrollBy({ top: codeEl.clientHeight / 2, behavior: 'auto' });
        return;
      }
      if (e.key === 'u' || e.key === 'U') {
        e.preventDefault();
        clearPendingG();
        clearPendingZ();
        clearCountBuf();
        codeEl.scrollBy({ top: -codeEl.clientHeight / 2, behavior: 'auto' });
        return;
      }
      if (e.key === 'f' || e.key === 'F') {
        // Ctrl-F: do NOT preempt browser find in macOS Cmd-F flows (those use
        // metaKey); Ctrl-F here means vim page-down on all platforms.
        e.preventDefault();
        clearPendingG();
        clearPendingZ();
        clearCountBuf();
        codeEl.scrollBy({ top: codeEl.clientHeight, behavior: 'auto' });
        return;
      }
      if (e.key === 'b' || e.key === 'B') {
        e.preventDefault();
        clearPendingG();
        clearPendingZ();
        clearCountBuf();
        codeEl.scrollBy({ top: -codeEl.clientHeight, behavior: 'auto' });
        return;
      }
    }

    // For everything below, only bare keys (no modifiers) are interpreted.
    if (mod || e.altKey) return;

    // Esc cancels any pending chord/count.
    if (e.key === 'Escape') {
      if (pendingG || pendingZ || countBuf !== '') {
        clearPendingG();
        clearPendingZ();
        clearCountBuf();
      }
      return;
    }

    // --- Numeric prefix: digits accumulate. `0` is a real command (scroll to
    // left edge) UNLESS the buffer is non-empty, in which case it's a digit. ---
    if (/^[0-9]$/.test(e.key)) {
      if (e.key === '0' && countBuf === '') {
        // bare `0` → horizontal scroll to start.
        e.preventDefault();
        if (codeEl) codeEl.scrollLeft = 0;
        clearPendingG();
        clearPendingZ();
        return;
      }
      e.preventDefault();
      countBuf += e.key;
      // refresh expiry on every digit
      if (countBufTimer) clearTimeout(countBufTimer);
      countBufTimer = setTimeout(clearCountBuf, 1500);
      // a numeric prefix invalidates any pending g/z chord.
      clearPendingG();
      clearPendingZ();
      return;
    }

    // --- `g` first stroke (no count). Existing `gd` and new `gg` both start here. ---
    if (e.key === 'g' && !pendingG) {
      // shift+g is uppercase 'G' which is handled below.
      pendingG = true;
      if (pendingGTimer) clearTimeout(pendingGTimer);
      pendingGTimer = setTimeout(() => {
        pendingG = false;
        pendingGTimer = null;
      }, 500);
      return;
    }

    // --- `g d` — definition picker (preserved) ---
    if (pendingG && e.key === 'd') {
      e.preventDefault();
      clearPendingG();
      gdJump();
      return;
    }

    // --- `g g` — jump to top ---
    if (pendingG && e.key === 'g') {
      e.preventDefault();
      clearPendingG();
      if (codeEl) codeEl.scrollTop = 0;
      announce('Top of file');
      return;
    }

    // Any other key while pendingG dissolves it (and falls through so the key
    // can be reinterpreted as a fresh command, e.g. `g` then `j`).
    if (pendingG) clearPendingG();

    // --- `z` first stroke ---
    if (e.key === 'z' && !pendingZ) {
      pendingZ = true;
      if (pendingZTimer) clearTimeout(pendingZTimer);
      pendingZTimer = setTimeout(() => {
        pendingZ = false;
        pendingZTimer = null;
      }, 500);
      return;
    }

    // --- `z z` — center current line ---
    if (pendingZ && e.key === 'z') {
      e.preventDefault();
      clearPendingZ();
      const ln = viewportCenterLineNumber();
      if (ln !== null) scrollToLine(ln, 'auto');
      return;
    }

    if (pendingZ) clearPendingZ();

    // --- `j` / `k` — line scroll (count-aware) ---
    if (e.key === 'j') {
      e.preventDefault();
      if (codeEl) codeEl.scrollBy({ top: getLineHeight() * consumeCount(), behavior: 'auto' });
      return;
    }
    if (e.key === 'k') {
      e.preventDefault();
      if (codeEl) codeEl.scrollBy({ top: -getLineHeight() * consumeCount(), behavior: 'auto' });
      return;
    }

    // --- `G` — jump to {count} line, or end if no count ---
    if (e.key === 'G') {
      e.preventDefault();
      if (!codeEl) {
        clearCountBuf();
        return;
      }
      if (countBuf !== '') {
        const target = Math.min(Math.max(parseInt(countBuf, 10), 1), lineCount);
        clearCountBuf();
        scrollToLine(target, 'auto');
        announce(`Line ${target}`);
      } else {
        codeEl.scrollTop = codeEl.scrollHeight;
        announce('Bottom of file');
      }
      return;
    }

    // --- `$` — horizontal scroll to end ---
    if (e.key === '$') {
      e.preventDefault();
      if (codeEl) codeEl.scrollLeft = codeEl.scrollWidth;
      return;
    }
  }

  $effect(() => {
    window.addEventListener('keydown', onChordKey);
    return () => window.removeEventListener('keydown', onChordKey);
  });

  // ---------------------------------------------------------------------------
  // Copy
  // ---------------------------------------------------------------------------
  async function copyAll() {
    if (!data) return;
    try {
      await navigator.clipboard.writeText(data.content);
      copyState = 'ok';
      announce(data.truncated ? 'Copied partial content' : 'Copied file content');
    } catch {
      copyState = 'err';
      announce('Copy failed');
    }
    if (copyTimer) clearTimeout(copyTimer);
    copyTimer = setTimeout(() => {
      copyState = 'idle';
      copyTimer = null;
    }, 1500);
  }

  let copyLabel = $derived.by(() => {
    if (copyState === 'ok') return 'Copied!';
    if (copyState === 'err') return 'Failed.';
    return data?.truncated ? 'Copy (partial)' : 'Copy';
  });

  // ---------------------------------------------------------------------------
  // Wrap
  // ---------------------------------------------------------------------------
  function toggleWrap() {
    wrap = !wrap;
    writeWrapPref(wrap);
    announce(`Wrap ${wrap ? 'on' : 'off'}`);
  }

  // ---------------------------------------------------------------------------
  // Split — right-pane toggle. The right pane renders FileDetail too, so a
  // press on the button in the right pane closes its own pane (intuitive).
  // ---------------------------------------------------------------------------
  let isMobile = $state(typeof window !== 'undefined' && window.innerWidth < 800);
  $effect(() => {
    if (typeof window === 'undefined') return;
    const mql = window.matchMedia('(max-width: 799px)');
    isMobile = mql.matches;
    const onChange = (e: MediaQueryListEvent) => {
      isMobile = e.matches;
    };
    mql.addEventListener('change', onChange);
    return () => mql.removeEventListener('change', onChange);
  });

  function toggleSplit() {
    if (panes.rightOpen) {
      closeRight();
      announce('Right pane closed');
    } else {
      openRight(path);
      announce('Right pane opened');
    }
  }

  // ---------------------------------------------------------------------------
  // Find — line-level, case-insensitive, debounced.
  // ---------------------------------------------------------------------------
  function runFind(q: string) {
    if (!data || q === '') {
      findMatches = [];
      findIndex = 0;
      return;
    }
    const needle = q.toLowerCase();
    const arr: number[] = [];
    const src = data.content.split('\n');
    for (let i = 0; i < src.length; i++) {
      if (src[i].toLowerCase().includes(needle)) arr.push(i + 1);
    }
    findMatches = arr;
    findIndex = arr.length > 0 ? 0 : -1;
    if (arr.length > 0) {
      queueMicrotask(() => scrollToLine(arr[0], 'auto'));
    }
  }

  $effect(() => {
    const q = findQuery;
    if (findDebounce) clearTimeout(findDebounce);
    findDebounce = setTimeout(() => runFind(q), 100);
    return () => {
      if (findDebounce) clearTimeout(findDebounce);
    };
  });

  // Announce match count to SR users with 500ms debounce so rapid typing
  // does not produce a stream of "1 match / 12 matches / 3 matches".
  $effect(() => {
    if (!findOpen) return;
    const q = findQuery;
    const count = findMatches.length;
    if (findAnnounceTimer) clearTimeout(findAnnounceTimer);
    if (q === '') return;
    findAnnounceTimer = setTimeout(() => {
      announce(count === 1 ? '1 match' : `${count} matches`);
      findAnnounceTimer = null;
    }, 500);
    return () => {
      if (findAnnounceTimer) clearTimeout(findAnnounceTimer);
    };
  });

  function findNext() {
    if (findMatches.length === 0) return;
    findIndex = (findIndex + 1) % findMatches.length;
    scrollToLine(findMatches[findIndex], 'auto');
  }
  function findPrev() {
    if (findMatches.length === 0) return;
    findIndex = (findIndex - 1 + findMatches.length) % findMatches.length;
    scrollToLine(findMatches[findIndex], 'auto');
  }
  function openFind() {
    findOpen = true;
    queueMicrotask(() => findInputEl?.focus());
    announce('Find bar opened');
  }
  function closeFind() {
    findOpen = false;
    findQuery = '';
    findMatches = [];
    findIndex = 0;
    if (findAnnounceTimer) {
      clearTimeout(findAnnounceTimer);
      findAnnounceTimer = null;
    }
    announce('Find bar closed');
  }

  // Cmd-Shift-F / Ctrl-Shift-F to open find. SearchBar owns "/".
  function onGlobalKey(e: KeyboardEvent) {
    if (!isFocusedPane()) return;
    const mod = e.metaKey || e.ctrlKey;
    if (mod && e.shiftKey && (e.key === 'F' || e.key === 'f')) {
      if (!data) return;
      e.preventDefault();
      openFind();
      return;
    }
    if (e.key === 'Escape' && findOpen) {
      e.preventDefault();
      closeFind();
    }
    // Diff hunk navigation: `n`/`p` when diff mode is on and no input has
    // focus. Guarded against modifier keys so it doesn't collide with the
    // browser's own ctrl/cmd+n. Hidden behind an explicit `data-diff-mode`
    // check so the source viewer stays unaffected.
    if (diffMode && !mod && !e.shiftKey && !e.altKey) {
      const tag = (e.target as HTMLElement | null)?.tagName;
      if (tag === 'INPUT' || tag === 'TEXTAREA') return;
      if (e.key === 'n' || e.key === 'j') {
        if (diffHunks.length === 0) return;
        e.preventDefault();
        jumpDiff(1);
      } else if (e.key === 'p' || e.key === 'k') {
        if (diffHunks.length === 0) return;
        e.preventDefault();
        jumpDiff(-1);
      }
    }
  }
  $effect(() => {
    window.addEventListener('keydown', onGlobalKey);
    return () => window.removeEventListener('keydown', onGlobalKey);
  });

  // Build a Set for O(1) line lookups during render (large match lists).
  let findMatchSet = $derived(new Set(findMatches));
  let findCurrentLine = $derived(
    findMatches.length > 0 && findIndex >= 0 ? findMatches[findIndex] : -1,
  );

  // ---------------------------------------------------------------------------
  // Context menu — right-click anywhere on the detail view. Mirrors the
  // conventions of TreeNode's menu: unavailable actions are disabled, not
  // hidden, and all copy actions funnel through one clipboard helper.
  // ---------------------------------------------------------------------------
  let rootEl: HTMLElement | null = $state(null);

  async function copyToClipboard(text: string, announceLabel: string) {
    if (!text) return;
    try {
      await navigator.clipboard.writeText(text);
      announce(`Copied ${announceLabel}`);
    } catch {
      // Clipboard API requires a secure context; silently no-op when denied.
    }
  }

  // Text of the current selection when it falls inside THIS FileDetail
  // instance (both panes render their own FileDetail, so containment matters).
  // Selections inside the sandboxed preview iframes are unreachable from the
  // parent document — but those iframes also swallow contextmenu, so the menu
  // never opens there anyway.
  function selectionText(): string {
    if (!rootEl) return '';
    const sel = document.getSelection();
    if (!sel || sel.isCollapsed || sel.rangeCount === 0) return '';
    if (!rootEl.contains(sel.getRangeAt(0).commonAncestorContainer)) return '';
    return sel.toString();
  }

  function onContextMenu(e: MouseEvent) {
    if (!path) return;
    // Keep the native menu for text inputs (find bar) — spellcheck, paste, etc.
    const t = e.target as HTMLElement | null;
    if (t instanceof HTMLInputElement || t instanceof HTMLTextAreaElement) return;
    e.preventDefault();
    // Snapshot the selection now: opening the menu can clear it before run().
    const selText = selectionText();
    const items: ContextMenuItem[] = [
      {
        id: 'open-to-side',
        label: 'Open to the Side',
        disabled: isMobile || (panes.rightOpen && panes.rightPath === path),
        run: () => openRight(path),
      },
      {
        id: 'copy-path',
        label: 'Copy Path',
        run: () => copyToClipboard(path, path),
      },
      {
        id: 'copy-full-path',
        label: 'Copy Full Path',
        // Repo root is seeded by TreeView's first /api/tree response; disable
        // defensively if we render before it arrives.
        disabled: !repo.root,
        run: () => copyToClipboard(absolutePath(path), absolutePath(path)),
      },
      {
        id: 'copy-contents',
        label: 'Copy Contents',
        disabled: loading || !!error || !data || !data.content,
        run: () => {
          if (!data) return;
          copyToClipboard(data.content, data.truncated ? 'partial content' : 'file content');
        },
      },
      {
        id: 'copy-selection',
        label: 'Copy Selection',
        disabled: selText === '',
        run: () => copyToClipboard(selText, 'selection'),
      },
      {
        id: 'copy-link',
        label: 'Copy Link',
        run: () =>
          copyToClipboard(location.origin + location.pathname + toFileHash(path), 'link'),
      },
      {
        id: 'open-raw',
        label: 'Open Raw',
        run: () => window.open(rawUrl, '_blank', 'noopener'),
      },
    ];
    openContextMenu(e.clientX, e.clientY, items);
  }
</script>

<article class="file-detail" bind:this={rootEl} oncontextmenu={onContextMenu}>
  <header class="meta">
    <div class="meta-top">
      <nav class="breadcrumb mono" aria-label="path" title={path}>
        {#each crumbs as crumb, ci (`${ci}:${crumb.path}`)}
          <button
            type="button"
            class="crumb"
            onclick={() => onCrumbClick(crumb.path)}
            title={crumb.path}
            aria-label={`Reveal ${crumb.path} in tree`}
          >{crumb.name}</button>
          {#if !crumb.last}
            <span class="sep" aria-hidden="true">/</span>
          {/if}
        {/each}
      </nav>
      <div class="actions">
        <button
          type="button"
          class="action"
          aria-label="Copy file content"
          onclick={copyAll}
          disabled={!data}
        >{copyLabel}</button>
        {#if isPreviewable}
          <button
            type="button"
            class="action"
            aria-label="Toggle preview between rendered output and source"
            aria-pressed={previewView === 'rendered'}
            onclick={togglePreviewView}
            disabled={!data || diffMode}
          >View: {previewView === 'rendered' ? 'Rendered' : 'Source'}</button>
        {/if}
        {#if diffActionAvailable}
          <button
            type="button"
            class="action"
            aria-label={diffMode ? 'Hide git diff' : 'Show git diff against HEAD'}
            aria-pressed={diffMode}
            title={diffMode ? 'Hide diff' : 'Show diff against HEAD'}
            onclick={toggleDiff}
            disabled={!data}
          >Diff{diffMode ? ': on' : ''}</button>
        {/if}
        <button
          type="button"
          class="action"
          aria-label={historyMode ? 'Hide file history' : 'Show file commit history (Shift+H)'}
          aria-pressed={historyMode}
          title={historyMode ? 'Hide history (Shift+H)' : 'Show history (Shift+H)'}
          onclick={toggleHistory}
          disabled={!data}
        >History{historyMode ? ': on' : ''}</button>
        {#if diffMode && diffData && !diffData.binary && diffHunks.length > 0}
          <div class="hunk-nav" role="group" aria-label="Diff hunk navigation">
            <button
              type="button"
              class="action"
              aria-label="Previous hunk (p)"
              title="Previous hunk (p / k)"
              onclick={() => jumpDiff(-1)}
            >‹</button>
            <span class="hunk-count muted" aria-live="polite">{currentHunk + 1}/{diffHunks.length}</span>
            <button
              type="button"
              class="action"
              aria-label="Next hunk (n)"
              title="Next hunk (n / j)"
              onclick={() => jumpDiff(1)}
            >›</button>
          </div>
        {/if}
        <button
          type="button"
          class="action"
          aria-label="Toggle line wrap"
          aria-pressed={wrap}
          onclick={toggleWrap}
          disabled={!data}
        >Wrap: {wrap ? 'on' : 'off'}</button>
        <button
          type="button"
          class="action"
          aria-label="Find in file (Cmd+Shift+F)"
          onclick={openFind}
          disabled={!data}
        >Find</button>
        {#if !isMobile}
          <button
            type="button"
            class="action"
            aria-label={panes.rightOpen ? 'Close split view' : 'Open split view to the side'}
            aria-pressed={panes.rightOpen}
            title={panes.rightOpen ? 'Close split (⌘\\)' : 'Split right (⌘\\)'}
            onclick={toggleSplit}
            disabled={!data}
          >Split{panes.rightOpen ? ': on' : ''}</button>
        {/if}
      </div>
    </div>
    {#if data}
      <dl class="stats" aria-label="file stats">
        <div title="Approx LLM tokens (cl100k_base). Claude Pro caps at ~200k per request."><dt>tokens</dt><dd>{formatTokens(data.tokens)}</dd></div>
        <div><dt>lines</dt><dd>{data.lines || lineCount}</dd></div>
        <div><dt>size</dt><dd>{formatSize(data.size)}</dd></div>
        {#if data.role}
          <div title="Inferred file role (e.g. core / test / config / doc)."><dt>role</dt><dd>{data.role}</dd></div>
        {/if}
        {#if data.git}
          <div>
            <dt>git</dt>
            <dd style="color: {gitColor(data.git)}">{gitLabel(data.git)}</dd>
          </div>
        {/if}
      </dl>
      {#if data.truncated}
        <p class="warn" role="status">Content truncated. Use the CLI for the full file.</p>
      {/if}
    {/if}
  </header>

  {#if findOpen}
    <div class="find" role="search">
      <input
        bind:this={findInputEl}
        bind:value={findQuery}
        type="search"
        aria-label="Find in file"
        placeholder="Find in file…"
        spellcheck="false"
        autocomplete="off"
      />
      <button type="button" aria-label="Previous match" onclick={findPrev} disabled={findMatches.length === 0}>‹</button>
      <button type="button" aria-label="Next match" onclick={findNext} disabled={findMatches.length === 0}>›</button>
      <span class="count muted" aria-live="polite">
        {#if findQuery === ''}
          —
        {:else if findMatches.length === 0}
          0 matches
        {:else}
          {findIndex + 1}/{findMatches.length}
        {/if}
      </span>
      <button type="button" class="close" aria-label="Close find" onclick={closeFind}>×</button>
    </div>
  {/if}

  {#if loading}
    <div class="loading" aria-busy="true">
      <span class="skel" style="width: 60%; height: 14px;"></span>
      <span class="skel" style="width: 80%; height: 12px;"></span>
      <span class="skel" style="width: 70%; height: 12px;"></span>
      <span class="skel" style="width: 50%; height: 12px;"></span>
    </div>
  {:else if error}
    <div class="error">
      <p>Failed to load file.</p>
      <code class="mono">{error}</code>
      <button onclick={() => load(path)}>Retry</button>
    </div>
  {:else if data}
    <div class="body" class:no-aside={!asideVisible}>
      {#if historyMode}
        <div class="history-panel">
          {#if historyLoading}
            <div class="loading" aria-busy="true">
              <span class="skel" style="width: 60%; height: 12px;"></span>
              <span class="skel" style="width: 40%; height: 12px;"></span>
            </div>
          {:else if historyError}
            <div class="error">
              <p>Failed to load history.</p>
              <code class="mono">{historyError}</code>
            </div>
          {:else if historyData && historyData.commits.length === 0}
            <p class="muted diff-note">No commit history found for this file.</p>
          {:else if historyData}
            <div class="history-layout">
              <div class="history-list" role="listbox" aria-label="Commit history" bind:this={historyListEl}>
                {#each historyData.commits as commit (commit.hash_full)}
                  <button
                    type="button"
                    class="history-entry"
                    role="option"
                    aria-selected={selectedHash === commit.hash_full}
                    title={`${commit.hash_full}\n${new Date(commit.date * 1000).toISOString()}\n${commit.author} <${commit.author_email}>`}
                    onclick={() => { selectedHash = commit.hash_full; }}
                  >
                    <span class="h-hash mono">{commit.hash}</span>
                    <span class="h-author">{commit.author}</span>
                    <span class="h-subject">{commit.subject}</span>
                    <span class="h-date muted" title={new Date(commit.date * 1000).toISOString()}>{formatRelative(commit.date)}</span>
                  </button>
                {/each}
                {#if historyData.truncated}
                  <p class="muted diff-note h-truncated">History truncated.</p>
                {/if}
              </div>
              <div class="history-diff">
                {#if !selectedHash}
                  <p class="muted diff-note">Select a commit to view diff.</p>
                {:else if commitDiffError}
                  <p class="muted diff-note">{commitDiffError}</p>
                {:else if !commitDiffData}
                  <div class="loading" aria-busy="true">
                    <span class="skel" style="width: 50%; height: 12px;"></span>
                    <span class="skel" style="width: 70%; height: 12px;"></span>
                  </div>
                {:else if commitDiffData.binary}
                  <p class="muted diff-note">Binary file — diff not available.</p>
                {:else if commitDiffData.no_change && !commitDiffData.added && !commitDiffData.deleted}
                  <p class="muted diff-note">No changes in this commit.</p>
                {:else}
                  <pre
                    class="code diff"
                    class:wrap
                    bind:this={diffEl}
                  ><code class="hljs language-{langFromPath(path)}">{#if commitDiffData.added}<div class="diff-meta">New file</div>{/if}{#if commitDiffData.deleted}<div class="diff-meta">File deleted</div>{/if}{#each diffLinesView as ln, li (li)}<div
                        class="line diff-line diff-{ln.type}"
                        data-diff-line={li}
                      ><span class="gutter diff-gutter" aria-hidden="true"><span class="diff-old-num">{ln.oldNum || ''}</span><span class="diff-new-num">{ln.newNum || ''}</span><span class="diff-sign">{ln.type === 'add' ? '+' : ln.type === 'del' ? '-' : ' '}</span></span><span class="ln-content">{@html ln.html || '&nbsp;'}</span></div>{/each}{#if commitDiffData.truncated}<div class="diff-meta">Diff truncated.</div>{/if}</code></pre>
                {/if}
              </div>
            </div>
          {/if}
        </div>
      {:else if diffMode}
        {#if diffLoading}
          <div class="loading" aria-busy="true">
            <span class="skel" style="width: 50%; height: 12px;"></span>
            <span class="skel" style="width: 70%; height: 12px;"></span>
            <span class="skel" style="width: 60%; height: 12px;"></span>
          </div>
        {:else if diffError}
          <div class="error">
            <p>Failed to load diff.</p>
            <code class="mono">{diffError}</code>
            <button onclick={() => loadDiff(path)}>Retry</button>
          </div>
        {:else if diffData}
          {#if diffData.binary}
            <p class="muted diff-note">Binary file — diff not available.</p>
          {:else if diffData.no_change && !diffData.added && !diffData.deleted}
            <p class="muted diff-note">No changes against HEAD.</p>
          {:else}
            <pre
              class="code diff"
              class:wrap
              bind:this={diffEl}
            ><code class="hljs language-{langFromPath(diffData.path)}">{#if diffData.added}<div class="diff-meta">New file (no HEAD revision)</div>{/if}{#if diffData.deleted}<div class="diff-meta">File removed in worktree</div>{/if}{#each diffLinesView as ln, li (li)}<div
                  class="line diff-line diff-{ln.type}"
                  data-diff-line={li}
                ><span class="gutter diff-gutter" aria-hidden="true"><span class="diff-old-num">{ln.oldNum || ''}</span><span class="diff-new-num">{ln.newNum || ''}</span><span class="diff-sign">{ln.type === 'add' ? '+' : ln.type === 'del' ? '-' : ' '}</span></span><span class="ln-content">{@html ln.html || '&nbsp;'}</span></div>{/each}{#if diffData.truncated}<div class="diff-meta">Diff truncated — use the CLI for the full diff.</div>{/if}</code></pre>
          {/if}
        {/if}
      {:else if isPreviewable && previewView === 'rendered'}
        {#if isSvg}
          <div class="svg-preview" role="img" aria-label={`Rendered SVG preview of ${path}`}>
            <img src={svgDataUrl} alt={`Rendered SVG: ${path}`} />
          </div>
        {:else if isMarkdown}
          <iframe
            class="md-preview"
            bind:this={mdIframeEl}
            sandbox="allow-same-origin"
            srcdoc={mdSrcDoc}
            title={`Rendered Markdown preview of ${path}`}
          ></iframe>
        {:else if isMmd}
          <!-- Mermaid SVG is safe to render in the parent doc — no iframe
               needed (mermaid securityLevel: 'strict' sandboxes user input
               and emits inline SVG with no scripts). The wrap carries the
               pan/zoom controls overlay; the inner div hosts the SVG so
               the attach effect only re-queries on SVG changes. -->
          <div class="mmd-wrap">
            <div
              class="mmd-preview"
              bind:this={mmdWrapEl}
              role="img"
              aria-label={`Rendered Mermaid preview of ${path}`}
            >{@html mmdSvg}</div>
            {#if mmdSvg && mmdPz}
              <div class="mmd-controls" role="toolbar" aria-label="Diagram controls">
                <button
                  type="button"
                  title="Zoom out"
                  aria-label="Zoom out"
                  onclick={() => mmdPz?.zoomOut()}
                >−</button>
                <button
                  type="button"
                  title="Reset (double-click diagram)"
                  aria-label="Reset zoom"
                  onclick={() => mmdPz?.reset()}
                >⭯</button>
                <button
                  type="button"
                  title="Zoom in"
                  aria-label="Zoom in"
                  onclick={() => mmdPz?.zoomIn()}
                >+</button>
              </div>
            {/if}
          </div>
        {:else}
          <iframe
            class="html-preview"
            sandbox="allow-scripts"
            src={rawUrl}
            title={`Rendered HTML preview of ${path}`}
          ></iframe>
        {/if}
      {:else}
      <!-- svelte-ignore a11y_click_events_have_key_events -->
      <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
      <!-- svelte-ignore a11y_mouse_events_have_key_events -->
      <pre
        class="code"
        class:wrap
        class:reduce-motion={reduceMotion()}
        bind:this={codeEl}
        onmouseover={onCodeMouseOver}
        onmouseout={onCodeMouseOut}
        onclick={onCodeClick}
      ><code class="hljs language-{data.lang || langFromPath(data.path)}">{#each lines as ln (ln.n)}<div
              class="line"
              class:target={pulseLine === ln.n}
              class:find-match={findMatchSet.has(ln.n)}
              class:find-current={findCurrentLine === ln.n}
              id={`L${ln.n}`}
              data-line={ln.n}
            ><span class="gutter" aria-hidden="true" tabindex="-1"><a
                  class="gutter-link"
                  href={`#/file/${path.split('/').map(encodeURIComponent).join('/')}?L=${ln.n}`}
                  tabindex="-1"
                >{ln.n}</a></span><span class="ln-content">{@html ln.html || '&nbsp;'}</span></div>{/each}</code></pre>
      {/if}
      {#if view.showSymbols && !historyMode}
        {#if isMarkdown && previewView === 'rendered' && mdToc.length > 0}
          <aside class="symbols">
            <TocList toc={mdToc} onJump={jumpToHeading} />
            <EvidencePanel {path} />
          </aside>
        {:else if isCss}
          <aside class="symbols">
            <CssInsights content={data.content} onJump={(line) => jumpToSymbol(line)} />
            <EvidencePanel {path} />
          </aside>
        {:else if isJson}
          <aside class="symbols">
            <JsonInsights {path} content={data.content} onJump={(line) => jumpToSymbol(line)} />
            <EvidencePanel {path} />
          </aside>
        {:else if isXml}
          <aside class="symbols">
            <XmlInsights content={data.content} onJump={(line) => jumpToSymbol(line)} />
            <EvidencePanel {path} />
          </aside>
        {:else if isYaml}
          <aside class="symbols">
            <YamlInsights {path} content={data.content} onJump={(line) => jumpToSymbol(line)} />
            <EvidencePanel {path} />
          </aside>
        {:else if isSql}
          <aside class="symbols">
            <SqlInsights content={data.content} onJump={(line) => jumpToSymbol(line)} />
            <EvidencePanel {path} />
          </aside>
        {:else if isRelationsTarget}
          <aside class="symbols">
            {#if data.symbols && data.symbols.length > 0}
              <SymbolList symbols={data.symbols} onJump={jumpToSymbol} />
            {/if}
            {#if isTestsTarget}
              <TestInsightsPanel {path} />
            {/if}
            <RelationsPanel {path} />
            <EvidencePanel {path} />
          </aside>
        {:else if data.symbols && data.symbols.length > 0}
          <aside class="symbols">
            <SymbolList symbols={data.symbols} onJump={jumpToSymbol} />
            <EvidencePanel {path} />
          </aside>
        {:else}
          <aside class="symbols">
            <EvidencePanel {path} />
          </aside>
        {/if}
      {/if}
    </div>
  {:else}
    <p class="muted">No file selected.</p>
  {/if}
</article>

<style>
  .file-detail {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .meta {
    padding: 10px 16px;
    border-bottom: 1px solid var(--ctx-border);
    background: var(--ctx-bg-elev);
    flex: 0 0 auto;
  }
  .meta-top {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .breadcrumb {
    margin: 0;
    font-size: 13px;
    font-weight: 500;
    word-break: break-all;
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0;
    line-height: 1.4;
  }
  .breadcrumb .crumb {
    padding: 1px 4px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--ctx-fg);
    font: inherit;
    cursor: pointer;
  }
  .breadcrumb .crumb:hover {
    color: var(--ctx-link);
    background: var(--ctx-bg);
  }
  .breadcrumb .crumb:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
    color: var(--ctx-link);
  }
  .breadcrumb .sep {
    color: var(--ctx-fg-dim);
    padding: 0 1px;
    user-select: none;
    -webkit-user-select: none;
  }
  .actions {
    display: flex;
    gap: 6px;
    flex: 0 0 auto;
  }
  .actions .action {
    font-size: 11px;
    padding: 2px 8px;
  }
  .stats {
    margin: 8px 0 0;
    line-height: 1.4;
    display: flex;
    gap: 16px;
    flex-wrap: wrap;
    font-size: 11px;
  }
  .stats div {
    display: flex;
    gap: 4px;
  }
  .stats dt {
    color: var(--ctx-fg-dim);
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .stats dd {
    margin: 0;
    color: var(--ctx-fg);
    font-weight: 500;
  }

  .find {
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 6px 12px;
    background: var(--ctx-bg-panel);
    border-bottom: 1px solid var(--ctx-border);
    flex: 0 0 auto;
  }
  .find input {
    flex: 1 1 auto;
    min-width: 0;
  }
  .find button {
    padding: 2px 8px;
    font-size: 12px;
  }
  .find .count {
    font-size: 11px;
    min-width: 5em;
    text-align: right;
  }
  .find .close {
    margin-left: 4px;
    font-size: 16px;
    padding: 2px 6px;
    line-height: 1;
  }

  .body {
    flex: 1 1 auto;
    display: grid;
    grid-template-columns: 1fr minmax(0, 240px);
    /* WHY explicit 1fr row: an <iframe> has intrinsic height 150px, so an
       auto row would shrink to that and leave the rest of .body blank
       (white from .body's background). A code <pre> works in auto because
       its content drives row size, but the iframe doesn't. 1fr stretches
       the single row to .body's full height; <pre overflow:auto> is
       unaffected because it still gets the same available height. */
    grid-template-rows: 1fr;
    min-height: 0;
    overflow: hidden;
  }
  .body.no-aside {
    grid-template-columns: 1fr;
  }
  .body > .code {
    overflow: auto;
    margin: 0;
    padding: 8px 0;
    font-size: 12px;
    line-height: 1.55;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
  }
  .body > .code > code {
    display: block;
    background: transparent;
    padding: 0;
  }
  .svg-preview {
    overflow: auto;
    padding: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
    background-color: var(--ctx-bg);
    background-image:
      linear-gradient(45deg, var(--ctx-bg-elev) 25%, transparent 25%),
      linear-gradient(-45deg, var(--ctx-bg-elev) 25%, transparent 25%),
      linear-gradient(45deg, transparent 75%, var(--ctx-bg-elev) 75%),
      linear-gradient(-45deg, transparent 75%, var(--ctx-bg-elev) 75%);
    background-size: 16px 16px;
    background-position: 0 0, 0 8px, 8px -8px, -8px 0;
  }
  .svg-preview img {
    max-width: 100%;
    max-height: 100%;
    object-fit: contain;
  }
  .html-preview {
    width: 100%;
    height: 100%;
    border: 0;
    background: #fff;
    min-height: 0;
  }
  .md-preview {
    width: 100%;
    height: 100%;
    border: 0;
    background: var(--ctx-bg);
    min-height: 0;
  }
  .mmd-wrap {
    position: relative;
    width: 100%;
    height: 100%;
    min-height: 0;
    display: flex;
    flex-direction: column;
    background: var(--ctx-bg);
  }
  .mmd-preview {
    width: 100%;
    flex: 1 1 auto;
    overflow: hidden;
    padding: 24px;
    display: flex;
    align-items: center;
    justify-content: center;
    min-height: 0;
    background: var(--ctx-bg);
  }
  .mmd-preview :global(svg) {
    width: 100%;
    height: 100%;
    max-width: 100%;
    max-height: 100%;
    user-select: none;
  }
  .mmd-preview :global(.mermaid-error) {
    color: var(--hl-name);
    background: var(--ctx-bg-elev);
    padding: 16px;
    border-radius: 6px;
    border-left: 4px solid var(--hl-name);
    white-space: pre-wrap;
    font-family: ui-monospace, SFMono-Regular, Menlo, monospace;
    font-size: 12px;
  }
  .mmd-controls {
    position: absolute;
    top: 12px;
    right: 12px;
    display: flex;
    gap: 2px;
    padding: 3px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 5px;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.18);
    z-index: 2;
  }
  .mmd-controls button {
    appearance: none;
    min-width: 28px;
    height: 28px;
    padding: 0 8px;
    background: transparent;
    color: var(--ctx-fg);
    border: 0;
    border-radius: 3px;
    font: 14px/1 ui-monospace, SFMono-Regular, Menlo, monospace;
    cursor: pointer;
  }
  .mmd-controls button:hover {
    background: var(--ctx-bg-panel, var(--ctx-bg));
  }
  .mmd-controls button:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .symbols {
    border-left: 1px solid var(--ctx-border);
    overflow: auto;
    background: var(--ctx-bg-panel);
  }
  .loading {
    padding: 16px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .error {
    padding: 16px;
  }
  .error code {
    display: block;
    margin: 6px 0;
    color: var(--ctx-err);
    word-break: break-all;
    font-size: 11px;
  }

  /* code viewer lines */
  .line {
    display: grid;
    grid-template-columns: 3.5em 1fr;
    column-gap: 12px;
    padding: 0 16px 0 0;
    transition: background-color var(--motion-settle) ease-out;
  }
  .line:target,
  .line.target {
    background: var(--ctx-line-target);
    transition: background-color var(--motion-base) ease-in;
  }
  .line.find-match {
    background: var(--ctx-line-find);
  }
  .line.find-current {
    background: var(--ctx-line-find-current);
  }
  .gutter {
    user-select: none;
    -webkit-user-select: none;
    text-align: right;
    color: var(--ctx-fg-dim);
    font-family: var(--ctx-font-mono);
    padding: 0 4px 0 8px;
    border-right: 1px solid var(--ctx-border);
    font-variant-numeric: tabular-nums;
  }
  .gutter-link {
    color: inherit;
    text-decoration: none;
    display: block;
  }
  .gutter-link:hover {
    color: var(--ctx-fg);
  }
  .gutter-link:focus-visible {
    outline: 1px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .ln-content {
    white-space: pre;
    min-width: 0;
  }
  .code.wrap .ln-content {
    white-space: pre-wrap;
    word-break: break-all;
  }

  /* Diff view — re-uses the line/gutter grid but widens the gutter to fit
     old + new line numbers and a +/- sign, then tints each row by line type.
     Colours reference the existing gitColor scheme so diff and tree icons
     stay consistent (modified=blue, added=green, deleted=red). */
  .body > .code.diff {
    overflow: auto;
  }
  .code.diff .line {
    grid-template-columns: 9em 1fr;
  }
  .code.diff .diff-gutter {
    display: inline-grid;
    grid-template-columns: 3em 3em 1em;
    column-gap: 6px;
    padding: 0 6px 0 8px;
  }
  .code.diff .diff-old-num,
  .code.diff .diff-new-num {
    color: var(--ctx-fg-dim);
    font-variant-numeric: tabular-nums;
    text-align: right;
    white-space: nowrap;
    overflow: hidden;
  }
  .code.diff .diff-sign {
    text-align: center;
    color: var(--ctx-fg-dim);
  }
  .code.diff .diff-line.diff-add {
    background: color-mix(in srgb, var(--ctx-git-added, #2ea043) 18%, transparent);
  }
  .code.diff .diff-line.diff-del {
    background: color-mix(in srgb, var(--ctx-git-deleted, #f85149) 18%, transparent);
  }
  .code.diff .diff-line.diff-add .diff-sign {
    color: var(--ctx-git-added, #2ea043);
  }
  .code.diff .diff-line.diff-del .diff-sign {
    color: var(--ctx-git-deleted, #f85149);
  }
  .code.diff .diff-meta {
    padding: 6px 16px;
    color: var(--ctx-fg-dim);
    font-style: italic;
    border-bottom: 1px solid var(--ctx-border);
  }
  .diff-note {
    padding: 16px;
  }
  .hunk-nav {
    display: inline-flex;
    align-items: center;
    gap: 2px;
  }
  .hunk-count {
    font-size: 11px;
    font-variant-numeric: tabular-nums;
    padding: 0 4px;
    min-width: 2.5em;
    text-align: center;
  }

  /* Issue #40 — clickable symbol identifiers.
     `.ctx-jump` wraps text inside an hljs span; we keep the span's colour
     and add a subtle underline-on-hover so the affordance is discoverable
     without shouting through the whole file. */
  :global(.ctx-jump) {
    color: inherit;
    text-decoration: none;
    cursor: pointer;
    border-bottom: 1px solid transparent;
    transition: border-color var(--motion-fast) ease-out, background-color var(--motion-fast) ease-out;
  }
  :global(.ctx-jump:hover),
  :global(.ctx-jump:focus-visible) {
    border-bottom-color: var(--ctx-accent);
    background: var(--ctx-bg-elev);
  }
  :global(.ctx-jump:focus-visible) {
    outline: 1px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  /* Strip transition + hover background when the user prefers reduced motion. */
  .code.reduce-motion :global(.ctx-jump) {
    transition: none;
  }
  .code.reduce-motion :global(.ctx-jump:hover) {
    background: transparent;
  }
  @media (prefers-reduced-motion: reduce) {
    :global(.ctx-jump) {
      transition: none;
    }
    :global(.ctx-jump:hover) {
      background: transparent;
    }
  }

  /* History panel — two-row layout: sticky commit list on top, diff below. */
  .history-panel {
    overflow: hidden;
    display: flex;
    flex-direction: column;
    min-height: 0;
    height: 100%;
  }
  .history-layout {
    display: flex;
    flex-direction: column;
    height: 100%;
    min-height: 0;
  }
  .history-list {
    flex: 0 0 auto;
    max-height: 220px;
    overflow-y: auto;
    border-bottom: 1px solid var(--ctx-border);
    background: var(--ctx-bg-panel, var(--ctx-bg-elev));
  }
  .history-entry {
    display: grid;
    grid-template-columns: 6.5em 10em 1fr 5em;
    column-gap: 10px;
    align-items: center;
    width: 100%;
    padding: 4px 12px;
    border: 0;
    border-bottom: 1px solid var(--ctx-border);
    background: transparent;
    color: var(--ctx-fg);
    font-size: 11px;
    line-height: 1.5;
    text-align: left;
    cursor: pointer;
  }
  .history-entry:last-child {
    border-bottom: 0;
  }
  .history-entry:hover {
    background: var(--ctx-bg-elev);
  }
  .history-entry[aria-selected="true"] {
    background: color-mix(in srgb, var(--ctx-accent, #4ec9b0) 14%, transparent);
  }
  .history-entry:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .h-hash {
    font-family: var(--ctx-font-mono, ui-monospace, SFMono-Regular, Menlo, monospace);
    font-size: 11px;
    color: var(--ctx-fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .h-author {
    font-size: 11px;
    color: var(--ctx-fg-dim);
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .h-subject {
    font-size: 11px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .h-date {
    font-size: 11px;
    text-align: right;
    white-space: nowrap;
  }
  .h-truncated {
    font-size: 11px;
    padding: 4px 12px;
  }
  .history-diff {
    flex: 1 1 auto;
    overflow: auto;
    min-height: 0;
    display: flex;
    flex-direction: column;
  }
  .history-diff > .code.diff {
    flex: 1 1 auto;
    min-height: 0;
  }

  @media (max-width: 900px) {
    .body {
      grid-template-columns: 1fr;
    }
    .symbols {
      border-left: 0;
      border-top: 1px solid var(--ctx-border);
      max-height: 160px;
    }
    .history-entry {
      grid-template-columns: 6em 1fr 4em;
    }
    .h-author {
      display: none;
    }
  }
</style>
