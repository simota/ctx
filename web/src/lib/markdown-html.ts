// String transforms for the Markdown preview: heading slugs, script
// stripping before the sandboxed iframe, and mermaid block extraction.
// Pure so they can be unit-tested without a DOM.

/**
 * Slug generator matching GitHub's: keeps Latin and Unicode letters/numbers
 * (so Japanese headings keep their text), replaces whitespace with `-`, and
 * disambiguates duplicates with a numeric suffix. One slugger per document.
 */
export function makeSlugger(): (text: string) => string {
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

/**
 * Remove constructs the sandboxed iframe would otherwise warn about.
 * The iframe runs with sandbox="allow-same-origin" (no allow-scripts) so
 * nothing actually executes — but the browser still logs a warning per
 * attempt. Stripping <script> blocks and inline on* handlers silences the
 * console while the iframe sandbox stays the security boundary
 * (defence-in-depth, not a sanitizer).
 */
export function stripExecutableHtml(html: string): string {
  return html
    .replace(/<script\b[^>]*>[\s\S]*?<\/script\s*>/gi, '')
    .replace(/<script\b[^>]*\/?>/gi, '')
    .replace(/\son[a-z]+\s*=\s*"[^"]*"/gi, '')
    .replace(/\son[a-z]+\s*=\s*'[^']*'/gi, '')
    .replace(/\son[a-z]+\s*=\s*[^\s>]+/gi, '');
}

export function decodeHtmlEntities(s: string): string {
  return s
    .replace(/&lt;/g, '<')
    .replace(/&gt;/g, '>')
    .replace(/&quot;/g, '"')
    .replace(/&#39;/g, "'")
    .replace(/&amp;/g, '&');
}

export interface MermaidBlock {
  id: string;
  code: string;
}

// Placeholder ids stay unique for the page's lifetime, across documents.
let mermaidSeq = 0;

/**
 * After marked has rendered the markdown to HTML, pull each mermaid code
 * block out and replace it with a stable-id placeholder. The pattern is
 * tight because markedHighlight always emits `class="hljs language-X"` —
 * mermaid isn't a hljs language, so the inner text comes back unwrapped
 * (just entity-escaped), which is decoded before handing it to mermaid.
 */
export function extractMermaidBlocks(html: string): { html: string; blocks: MermaidBlock[] } {
  const blocks: MermaidBlock[] = [];
  const out = html.replace(
    /<pre><code class="hljs language-mermaid">([\s\S]*?)<\/code><\/pre>/g,
    (_m, body: string) => {
      const id = `m${mermaidSeq++}`;
      blocks.push({ id, code: decodeHtmlEntities(body) });
      return `<div class="mermaid-block" data-mid="${id}"></div>`;
    },
  );
  return { html: out, blocks };
}

/** Escape a value for a double-quoted HTML attribute. */
export function escapeAttr(s: string): string {
  return s.replace(/&/g, '&amp;').replace(/"/g, '&quot;');
}
