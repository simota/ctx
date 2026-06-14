// Normalised tree model for the structured "Rendered" view of config files.
//
// YAML / JSON / TOML / XML all reduce to the same shape — named or indexed
// nodes that are either a scalar leaf or a container of more nodes — so the
// viewer renders them through one recursive component (ConfigTree.svelte)
// fed by per-format adapters here.
//
// Adapters aim for orientation, not round-trip fidelity: flow collections,
// inline tables and anchors are summarised as scalar previews rather than
// expanded. Line numbers are populated when the parse is line-based (YAML,
// TOML); JSON/XML go through structural parsers that don't preserve them, so
// those nodes carry no `line` and simply don't offer a jump.

export type TreeKind = 'map' | 'list' | 'scalar';

export interface TreeNode {
  key: string; // mapping key, or `[i]` for list items
  kind: TreeKind;
  value?: string; // scalar preview (leaf only)
  children?: TreeNode[];
  line?: number; // 1-based source line, when known
}

export interface ConfigTree {
  ok: boolean;
  roots: TreeNode[];
  error?: string;
}

const MAX = 120; // value preview clip

export function buildConfigTree(kind: string, content: string): ConfigTree {
  try {
    switch (kind) {
      case 'json':
        return jsonToTree(content);
      case 'yaml':
        return yamlToTree(content);
      case 'toml':
        return tomlToTree(content);
      case 'xml':
        return xmlToTree(content);
      default:
        return { ok: false, roots: [], error: `Unsupported: ${kind}` };
    }
  } catch (e) {
    return { ok: false, roots: [], error: e instanceof Error ? e.message : String(e) };
  }
}

// ── JSON ────────────────────────────────────────────────────────────────
function jsonToTree(content: string): ConfigTree {
  const trimmed = content.trim();
  if (trimmed.length === 0) return { ok: false, roots: [], error: 'Empty JSON.' };
  let parsed: unknown;
  try {
    parsed = JSON.parse(content);
  } catch (e) {
    return { ok: false, roots: [], error: e instanceof Error ? e.message : 'Invalid JSON.' };
  }
  return { ok: true, roots: jsonValueToNodes('', parsed) };
}

function jsonValueToNodes(key: string, value: unknown): TreeNode[] {
  return [jsonValueToNode(key, value)];
}

function jsonValueToNode(key: string, value: unknown): TreeNode {
  if (Array.isArray(value)) {
    return {
      key,
      kind: 'list',
      children: value.map((v, i) => jsonValueToNode(`[${i}]`, v)),
    };
  }
  if (value !== null && typeof value === 'object') {
    return {
      key,
      kind: 'map',
      children: Object.entries(value).map(([k, v]) => jsonValueToNode(k, v)),
    };
  }
  return { key, kind: 'scalar', value: scalarPreview(value) };
}

function scalarPreview(v: unknown): string {
  if (v === null) return 'null';
  if (typeof v === 'string') return clip(v, MAX);
  return clip(String(v), MAX);
}

// ── XML ─────────────────────────────────────────────────────────────────
function xmlToTree(content: string): ConfigTree {
  if (content.trim().length === 0) return { ok: false, roots: [], error: 'Empty document.' };
  // DOMParser is available in the browser; it handles entities, namespaces
  // and CDATA natively — far more robust than a hand-rolled tokenizer.
  const doc = new DOMParser().parseFromString(content, 'application/xml');
  const err = doc.querySelector('parsererror');
  if (err) return { ok: false, roots: [], error: 'Malformed XML.' };
  const root = doc.documentElement;
  if (!root) return { ok: false, roots: [], error: 'No root element.' };
  return { ok: true, roots: [elementToNode(root)] };
}

function elementToNode(el: Element): TreeNode {
  const children: TreeNode[] = [];
  // Attributes surface as `@name` scalar leaves so they read distinctly from
  // child elements.
  for (const attr of Array.from(el.attributes)) {
    children.push({ key: `@${attr.name}`, kind: 'scalar', value: clip(attr.value, MAX) });
  }
  const elementChildren = Array.from(el.children);
  if (elementChildren.length === 0) {
    const text = (el.textContent ?? '').trim();
    if (children.length === 0) {
      // Pure text element → scalar leaf.
      return { key: el.tagName, kind: 'scalar', value: clip(text, MAX) };
    }
    if (text) children.push({ key: '#text', kind: 'scalar', value: clip(text, MAX) });
    return { key: el.tagName, kind: 'map', children };
  }
  for (const child of elementChildren) children.push(elementToNode(child));
  return { key: el.tagName, kind: 'map', children };
}

// ── TOML ────────────────────────────────────────────────────────────────
function tomlToTree(content: string): ConfigTree {
  const lines = content.split('\n');
  const hasContent = lines.some((l) => {
    const t = l.trim();
    return t.length > 0 && !t.startsWith('#');
  });
  if (!hasContent) return { ok: false, roots: [], error: 'Empty TOML.' };

  const root: TreeNode = { key: '', kind: 'map', children: [] };
  // `current` is the table that bare `key = value` lines attach to.
  let current = root;

  for (let i = 0; i < lines.length; i++) {
    const raw = lines[i];
    const line = stripTomlComment(raw).trim();
    if (line.length === 0) continue;

    // Array-of-tables `[[a.b]]`.
    const arr = /^\[\[\s*(.+?)\s*\]\]$/.exec(line);
    if (arr) {
      current = pushArrayTable(root, splitTomlPath(arr[1]), i + 1);
      continue;
    }
    // Table `[a.b]`.
    const tbl = /^\[\s*(.+?)\s*\]$/.exec(line);
    if (tbl) {
      current = ensureTablePath(root, splitTomlPath(tbl[1]), i + 1);
      continue;
    }
    // Key/value.
    const eq = line.indexOf('=');
    if (eq > 0) {
      const key = stripTomlKey(line.slice(0, eq).trim());
      const value = line.slice(eq + 1).trim();
      current.children!.push({ key, kind: 'scalar', value: clip(value, MAX), line: i + 1 });
    }
  }

  return { ok: true, roots: root.children!.length ? root.children! : [], error: undefined };
}

function ensureTablePath(root: TreeNode, path: string[], line: number): TreeNode {
  let node = root;
  for (const seg of path) {
    let next = node.children!.find((c) => c.key === seg && c.kind === 'map');
    if (!next) {
      next = { key: seg, kind: 'map', children: [], line };
      node.children!.push(next);
    }
    node = next;
  }
  node.line ??= line;
  return node;
}

function pushArrayTable(root: TreeNode, path: string[], line: number): TreeNode {
  const parent = ensureTablePath(root, path.slice(0, -1), line);
  const name = path[path.length - 1];
  let list = parent.children!.find((c) => c.key === name && c.kind === 'list');
  if (!list) {
    list = { key: name, kind: 'list', children: [], line };
    parent.children!.push(list);
  }
  const item: TreeNode = { key: `[${list.children!.length}]`, kind: 'map', children: [], line };
  list.children!.push(item);
  return item;
}

function splitTomlPath(s: string): string[] {
  // Dotted keys, tolerating quoted segments `[a."b.c"]`.
  const out: string[] = [];
  let cur = '';
  let q: string | null = null;
  for (const ch of s) {
    if (q) {
      if (ch === q) q = null;
      else cur += ch;
    } else if (ch === '"' || ch === "'") q = ch;
    else if (ch === '.') {
      out.push(cur.trim());
      cur = '';
    } else cur += ch;
  }
  if (cur.trim()) out.push(cur.trim());
  return out.map(stripTomlKey);
}

function stripTomlKey(s: string): string {
  const t = s.trim();
  if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'"))) {
    return t.slice(1, -1);
  }
  return t;
}

function stripTomlComment(line: string): string {
  let inS = false;
  let inD = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"' && !inS) inD = !inD;
    else if (c === "'" && !inD) inS = !inS;
    else if (c === '#' && !inS && !inD) return line.slice(0, i);
  }
  return line;
}

// ── YAML ────────────────────────────────────────────────────────────────
interface YLine {
  no: number;
  indent: number;
  body: string; // trimmed, comment-stripped
  blank: boolean;
  comment: boolean;
  docSep: boolean;
}

function yamlToTree(content: string): ConfigTree {
  const lines = scanYaml(content);
  const real = lines.filter((l) => !l.blank && !l.comment && !l.docSep);
  if (real.length === 0) return { ok: false, roots: [], error: 'Empty or comment-only YAML.' };

  // Split into documents on `---`; render each doc as a root (single doc →
  // its content becomes the roots directly).
  const docs = splitDocs(lines);
  if (docs.length === 1) {
    return { ok: true, roots: parseYamlBlock(docs[0], 0, docs[0].length > 0 ? docs[0][0].indent : 0).nodes };
  }
  const roots: TreeNode[] = docs.map((d, i) => {
    const baseIndent = d.length > 0 ? d[0].indent : 0;
    return {
      key: `doc ${i + 1}`,
      kind: 'map',
      children: parseYamlBlock(d, 0, baseIndent).nodes,
      line: d.length > 0 ? d[0].no : undefined,
    };
  });
  return { ok: true, roots };
}

function scanYaml(content: string): YLine[] {
  const raws = content.split('\n');
  return raws.map((raw, i) => {
    const trimmed = raw.trim();
    const blank = trimmed.length === 0;
    const comment = !blank && trimmed.startsWith('#');
    const docSep = trimmed === '---' || trimmed.startsWith('--- ') || trimmed === '...';
    return {
      no: i + 1,
      indent: countIndent(raw),
      body: blank || comment ? trimmed : stripYamlComment(raw).trim(),
      blank,
      comment,
      docSep,
    };
  });
}

function splitDocs(lines: YLine[]): YLine[][] {
  const docs: YLine[][] = [];
  let cur: YLine[] = [];
  for (const l of lines) {
    if (l.docSep) {
      if (cur.some((x) => !x.blank && !x.comment)) docs.push(cur);
      cur = [];
      continue;
    }
    cur.push(l);
  }
  if (cur.some((x) => !x.blank && !x.comment)) docs.push(cur);
  return docs.length ? docs : [lines];
}

// Parse the lines belonging to one indentation block (indent === baseIndent)
// into sibling nodes; recurse for deeper children.
function parseYamlBlock(lines: YLine[], start: number, baseIndent: number): { nodes: TreeNode[]; next: number } {
  const nodes: TreeNode[] = [];
  let i = start;
  let seqIdx = 0;
  while (i < lines.length) {
    const l = lines[i];
    if (l.blank || l.comment) {
      i++;
      continue;
    }
    if (l.indent < baseIndent) break;
    if (l.indent > baseIndent) {
      // Stray deeper line with no recognised parent — skip to stay robust.
      i++;
      continue;
    }

    if (l.body === '-' || l.body.startsWith('- ')) {
      const rest = l.body === '-' ? '' : l.body.slice(2).trim();
      const item = parseSeqItem(lines, i, l.indent, rest, seqIdx++);
      nodes.push(item.node);
      i = item.next;
      continue;
    }

    const kv = matchKey(l.body);
    if (kv) {
      const built = buildMapEntry(lines, i, l.indent, kv);
      nodes.push(built.node);
      i = built.next;
      continue;
    }
    // Unrecognised line (e.g. a bare scalar document) → scalar leaf.
    nodes.push({ key: `[${seqIdx++}]`, kind: 'scalar', value: clip(stripQuotes(l.body), MAX), line: l.no });
    i++;
  }
  return { nodes, next: i };
}

function parseSeqItem(
  lines: YLine[],
  i: number,
  indent: number,
  rest: string,
  idx: number,
): { node: TreeNode; next: number } {
  const key = `[${idx}]`;
  const line = lines[i].no;
  if (rest.length === 0) {
    // Children live deeper than this item's `-`.
    const childStart = i + 1;
    const childIndent = firstChildIndent(lines, childStart, indent);
    if (childIndent > indent) {
      const block = parseYamlBlock(lines, childStart, childIndent);
      return { node: { key, kind: block.nodes.some((n) => n.key.startsWith('[')) ? 'list' : 'map', children: block.nodes, line }, next: block.next };
    }
    return { node: { key, kind: 'scalar', value: '∅', line }, next: i + 1 };
  }
  const kv = matchKey(rest);
  if (kv) {
    // Compact `- key: value` — the dash introduces a map whose first key is
    // inline. Treat the dash column + 2 as the map's indent so sibling keys
    // on following lines attach here.
    const node: TreeNode = { key, kind: 'map', children: [], line };
    const first = buildMapEntryInline(lines, i, indent + 2, kv);
    node.children!.push(first.node);
    // Continue collecting sibling keys at the inline column.
    const cont = parseYamlBlock(lines, first.next, indent + 2);
    node.children!.push(...cont.nodes);
    return { node, next: cont.next };
  }
  return { node: { key, kind: 'scalar', value: clip(stripQuotes(rest), MAX), line }, next: i + 1 };
}

function buildMapEntry(
  lines: YLine[],
  i: number,
  indent: number,
  kv: { key: string; value: string },
): { node: TreeNode; next: number } {
  const line = lines[i].no;
  if (kv.value.length > 0) {
    return { node: scalarOrInline(kv.key, kv.value, line), next: i + 1 };
  }
  const childStart = i + 1;
  const childIndent = firstChildIndent(lines, childStart, indent);
  if (childIndent > indent) {
    const block = parseYamlBlock(lines, childStart, childIndent);
    const isList = block.nodes.length > 0 && block.nodes.every((n) => /^\[\d+\]$/.test(n.key));
    return { node: { key: kv.key, kind: isList ? 'list' : 'map', children: block.nodes, line }, next: block.next };
  }
  return { node: { key: kv.key, kind: 'scalar', value: '∅', line }, next: i + 1 };
}

// Like buildMapEntry but the inline form already consumed line `i`; children
// (if the value is empty) are parsed from the next line at `indent`.
function buildMapEntryInline(
  lines: YLine[],
  i: number,
  indent: number,
  kv: { key: string; value: string },
): { node: TreeNode; next: number } {
  const line = lines[i].no;
  if (kv.value.length > 0) return { node: scalarOrInline(kv.key, kv.value, line), next: i + 1 };
  const childStart = i + 1;
  const childIndent = firstChildIndent(lines, childStart, indent - 1);
  if (childIndent >= indent) {
    const block = parseYamlBlock(lines, childStart, childIndent);
    const isList = block.nodes.length > 0 && block.nodes.every((n) => /^\[\d+\]$/.test(n.key));
    return { node: { key: kv.key, kind: isList ? 'list' : 'map', children: block.nodes, line }, next: block.next };
  }
  return { node: { key: kv.key, kind: 'scalar', value: '∅', line }, next: i + 1 };
}

function scalarOrInline(key: string, value: string, line: number): TreeNode {
  const v = value.trim();
  if (v === '|' || v === '>' || /^[|>][+\-]?\d*$/.test(v)) {
    return { key, kind: 'scalar', value: '(block scalar)', line };
  }
  if (v === '~' || v.toLowerCase() === 'null') return { key, kind: 'scalar', value: 'null', line };
  return { key, kind: 'scalar', value: clip(stripQuotes(v), MAX), line };
}

function firstChildIndent(lines: YLine[], start: number, parentIndent: number): number {
  for (let j = start; j < lines.length; j++) {
    const l = lines[j];
    if (l.blank || l.comment) continue;
    if (l.indent <= parentIndent) return -1;
    return l.indent;
  }
  return -1;
}

function matchKey(body: string): { key: string; value: string } | null {
  let m = /^'([^']*)'\s*:(?:\s+(.*)|\s*$)/.exec(body);
  if (m) return { key: m[1], value: (m[2] ?? '').trim() };
  m = /^"((?:[^"\\]|\\.)*)"\s*:(?:\s+(.*)|\s*$)/.exec(body);
  if (m) return { key: m[1], value: (m[2] ?? '').trim() };
  m = /^([^:'"\s,[\]{}#&*!|>%@`][^:]*?)\s*:(?:\s+(.*)|\s*$)/.exec(body);
  if (m) return { key: m[1].trim(), value: (m[2] ?? '').trim() };
  return null;
}

function countIndent(s: string): number {
  let n = 0;
  for (const c of s) {
    if (c === ' ' || c === '\t') n++;
    else break;
  }
  return n;
}

function stripYamlComment(line: string): string {
  let inS = false;
  let inD = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"' && !inS) inD = !inD;
    else if (c === "'" && !inD) inS = !inS;
    else if (c === '#' && !inS && !inD && (i === 0 || /\s/.test(line[i - 1]))) {
      return line.slice(0, i);
    }
  }
  return line;
}

function stripQuotes(s: string): string {
  const t = s.trim();
  if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'"))) {
    return t.slice(1, -1);
  }
  return t;
}

function clip(s: string, max: number): string {
  const one = s.replace(/\s+/g, ' ').trim();
  if (one.length <= max) return one;
  return one.slice(0, max) + '…';
}
