// XML insight extraction for the FileDetail sidebar.
//
// Approach: a single linear scan that tracks line number and depth via a
// hand-rolled tokenizer. Comments / CDATA / processing instructions are
// skipped (their newlines still count toward line numbers); opening tags
// push depth and capture name/attributes/line; closing tags pop depth;
// self-closing tags do not affect depth.
//
// We deliberately avoid DOMParser:
//   - it discards source positions, so line numbers would need a second
//     pass anyway,
//   - and it throws on benign malformed XML that this viewer should still
//     show insights for.
//
// The result feeds three universal sections (Outline / Tags / Namespaces)
// plus a sitemap lens that walks the source for <loc>…</loc> when the root
// is urlset / sitemapindex.

export interface XmlElement {
  name: string;
  line: number;
  depth: number;
  selfClosing: boolean;
  attrs: Record<string, string>;
  /** Indices into the same `tags` array for direct children — populated post-scan. */
  childIndices: number[];
}

export interface XmlOutlineEntry {
  name: string;
  line: number;
  childCount: number;
  attrSummary: string;
}

export interface XmlTagCount {
  name: string;
  count: number;
  firstLine: number;
}

export interface XmlNamespace {
  prefix: string; // '' for default xmlns
  uri: string;
  line: number;
}

export interface XmlSitemapUrl {
  url: string;
  line: number;
}

export interface XmlInsights {
  ok: boolean;
  rootName: string;
  outline: XmlOutlineEntry[];
  tags: XmlTagCount[];
  namespaces: XmlNamespace[];
  sitemap: XmlSitemapUrl[];
  isSitemap: boolean;
  totalElements: number;
  maxDepth: number;
}

export function extractXmlInsights(content: string): XmlInsights {
  const tags = scanElements(content);

  const empty: XmlInsights = {
    ok: false,
    rootName: '',
    outline: [],
    tags: [],
    namespaces: [],
    sitemap: [],
    isSitemap: false,
    totalElements: 0,
    maxDepth: 0,
  };
  if (tags.length === 0) return empty;

  // Root = first tag at depth 0.
  const root = tags.find((t) => t.depth === 0);
  if (!root) return empty;

  // Stitch up parent → children links via depth tracking.
  const stack: number[] = [];
  for (let i = 0; i < tags.length; i++) {
    const t = tags[i];
    while (stack.length > t.depth) stack.pop();
    const parentIdx = stack.length > 0 ? stack[stack.length - 1] : -1;
    if (parentIdx >= 0) tags[parentIdx].childIndices.push(i);
    if (!t.selfClosing) stack.push(i);
  }

  // Outline: every direct child of the root. Capped at 50 so a 5000-URL
  // sitemap doesn't drown the sidebar — the source view is still there.
  const outline: XmlOutlineEntry[] = root.childIndices.slice(0, 50).map((idx) => {
    const t = tags[idx];
    return {
      name: t.name,
      line: t.line,
      childCount: t.childIndices.length,
      attrSummary: summarizeAttrs(t.attrs),
    };
  });

  // Tag frequency, sorted by descending count then alphabetical.
  const tagAgg = new Map<string, XmlTagCount>();
  let maxDepth = 0;
  for (const t of tags) {
    if (t.depth > maxDepth) maxDepth = t.depth;
    const existing = tagAgg.get(t.name);
    if (existing) {
      existing.count++;
    } else {
      tagAgg.set(t.name, { name: t.name, count: 1, firstLine: t.line });
    }
  }
  const tagCounts = Array.from(tagAgg.values()).sort(
    (a, b) => b.count - a.count || a.name.localeCompare(b.name),
  );

  // Namespaces come from the root element's attributes only — the root
  // declares the document's namespaces; inner re-declarations are rare in
  // hand-written XML and noisy when they happen, so v1 ignores them.
  const namespaces: XmlNamespace[] = [];
  for (const [attr, value] of Object.entries(root.attrs)) {
    if (attr === 'xmlns') {
      namespaces.push({ prefix: '', uri: value, line: root.line });
    } else if (attr.startsWith('xmlns:')) {
      namespaces.push({ prefix: attr.slice(6), uri: value, line: root.line });
    }
  }

  const isSitemap = root.name === 'urlset' || root.name === 'sitemapindex';
  const sitemap: XmlSitemapUrl[] = isSitemap ? extractSitemapUrls(content) : [];

  return {
    ok: true,
    rootName: root.name,
    outline,
    tags: tagCounts,
    namespaces,
    sitemap,
    isSitemap,
    totalElements: tags.length,
    maxDepth,
  };
}

function summarizeAttrs(attrs: Record<string, string>): string {
  // Compact attribute preview. Surfaces id / name / href / src first
  // because those most often disambiguate elements at a glance; falls back
  // to the first attribute otherwise.
  const priority = ['id', 'name', 'href', 'src', 'rel', 'type', 'class'];
  for (const k of priority) {
    if (attrs[k] !== undefined) {
      const v = attrs[k].length > 40 ? attrs[k].slice(0, 40) + '…' : attrs[k];
      return `${k}="${v}"`;
    }
  }
  const keys = Object.keys(attrs);
  if (keys.length === 0) return '';
  const k = keys[0];
  const v = attrs[k].length > 40 ? attrs[k].slice(0, 40) + '…' : attrs[k];
  return `${k}="${v}"`;
}

function scanElements(content: string): XmlElement[] {
  const out: XmlElement[] = [];
  const len = content.length;
  let i = 0;
  let line = 1;
  let depth = 0;

  const advance = (until: number) => {
    for (let j = i; j < until; j++) if (content[j] === '\n') line++;
    i = until;
  };

  while (i < len) {
    const ch = content[i];
    if (ch === '\n') {
      line++;
      i++;
      continue;
    }
    if (ch !== '<') {
      i++;
      continue;
    }

    // Comment <!-- … -->
    if (content.startsWith('<!--', i)) {
      const end = content.indexOf('-->', i);
      if (end < 0) break;
      advance(end + 3);
      continue;
    }
    // CDATA <![CDATA[ … ]]>
    if (content.startsWith('<![CDATA[', i)) {
      const end = content.indexOf(']]>', i);
      if (end < 0) break;
      advance(end + 3);
      continue;
    }
    // Processing instruction <? … ?> or doctype <!… >
    if (content.startsWith('<?', i)) {
      const end = content.indexOf('?>', i);
      if (end < 0) break;
      advance(end + 2);
      continue;
    }
    if (content.startsWith('<!', i)) {
      const end = content.indexOf('>', i);
      if (end < 0) break;
      advance(end + 1);
      continue;
    }
    // Closing tag </name>
    if (content[i + 1] === '/') {
      const end = content.indexOf('>', i);
      if (end < 0) break;
      if (depth > 0) depth--;
      advance(end + 1);
      continue;
    }
    // Opening or self-closing tag
    const end = content.indexOf('>', i);
    if (end < 0) break;
    const inside = content.slice(i + 1, end);
    const trimmed = inside.replace(/\s+$/, '');
    const selfClosing = trimmed.endsWith('/');
    const body = selfClosing ? trimmed.slice(0, -1) : trimmed;
    const nameMatch = /^([a-zA-Z_][\w.-]*(?::[a-zA-Z_][\w.-]*)?)/.exec(body);
    if (nameMatch) {
      out.push({
        name: nameMatch[1],
        line,
        depth,
        selfClosing,
        attrs: parseAttrs(body.slice(nameMatch[0].length)),
        childIndices: [],
      });
      if (!selfClosing) depth++;
    }
    advance(end + 1);
  }

  return out;
}

function parseAttrs(s: string): Record<string, string> {
  const out: Record<string, string> = {};
  // Double- or single-quoted values. Bare attributes (HTML-style) are not
  // valid XML so we don't accept them.
  const re = /([a-zA-Z_][\w.:-]*)\s*=\s*(?:"([^"]*)"|'([^']*)')/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(s)) !== null) {
    out[m[1]] = m[2] !== undefined ? m[2] : (m[3] ?? '');
  }
  return out;
}

function extractSitemapUrls(content: string): XmlSitemapUrl[] {
  const out: XmlSitemapUrl[] = [];
  const re = /<loc[^>]*>\s*([^<]+?)\s*<\/loc>/g;
  let m: RegExpExecArray | null;
  while ((m = re.exec(content)) !== null) {
    const line = content.slice(0, m.index).split('\n').length;
    out.push({ url: m[1].trim(), line });
  }
  return out;
}
