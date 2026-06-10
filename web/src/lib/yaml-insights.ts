// YAML insight extraction for the FileDetail sidebar.
//
// Approach: a single line-based scan that recognises block structure via
// indentation without pulling in a YAML parser. The viewer only needs
// structural orientation — block scalars, flow collections and complex
// anchors are summarised, not faithfully reconstructed.
//
// Two-layer design:
//   - Universal sections — top-level outline, document separators, anchors.
//   - Lensed sections — GitHub Actions workflows, docker-compose services,
//     Kubernetes manifests.
//
// Tabs are not legal block indentation in YAML; if a file uses them we
// treat each tab as one space for indent comparison, which keeps the scan
// from misaligning siblings without claiming to validate the file.

export type YamlValueType = 'mapping' | 'sequence' | 'scalar' | 'null' | 'empty';

export interface YamlOutlineEntry {
  // For mapping entries: the key. For sequence items at root: `[i]`.
  key: string;
  type: YamlValueType;
  preview: string;
  count?: number;
  line: number;
}

export interface YamlDocument {
  index: number; // 1-based
  line: number;  // line of the `---` separator (or 1 for the first implicit doc)
}

export interface YamlAnchor {
  name: string;
  line: number;
}

export interface YamlJob {
  id: string;
  line: number;
  stepCount: number;
}

export interface YamlTrigger {
  name: string;
  line: number;
}

export interface YamlService {
  name: string;
  image: string;
  line: number;
}

export interface YamlK8sResource {
  kind: string;
  name: string;
  namespace: string;
  line: number;
}

export interface YamlInsights {
  ok: boolean;
  outline: YamlOutlineEntry[];
  totalKeys: number;
  maxDepth: number;
  documents: YamlDocument[];
  anchors: YamlAnchor[];
  // Lenses
  isGithubAction: boolean;
  triggers: YamlTrigger[];
  jobs: YamlJob[];
  isDockerCompose: boolean;
  services: YamlService[];
  isKubernetes: boolean;
  resources: YamlK8sResource[];
}

interface ScanLine {
  no: number;        // 1-based
  raw: string;       // original line text (no trailing newline)
  body: string;      // raw with trailing comment stripped (outside quotes)
  indent: number;    // leading whitespace columns (tab = 1)
  blank: boolean;    // empty after trim
  isComment: boolean;
  isDocSeparator: boolean;
  isDocEnd: boolean;
}

export function extractYamlInsights(path: string, content: string): YamlInsights {
  const lines = scanLines(content);

  const empty: YamlInsights = {
    ok: false,
    outline: [],
    totalKeys: 0,
    maxDepth: 0,
    documents: [],
    anchors: [],
    isGithubAction: false,
    triggers: [],
    jobs: [],
    isDockerCompose: false,
    services: [],
    isKubernetes: false,
    resources: [],
  };

  // A file with zero non-blank/non-comment lines is treated as un-parseable
  // so the panel can show a friendly message.
  const hasContent = lines.some((l) => !l.blank && !l.isComment);
  if (!hasContent) return empty;

  const documents = collectDocuments(lines);
  const outline = buildOutline(lines);
  const { totalKeys, maxDepth } = stats(lines);
  const anchors = collectAnchors(lines);

  const isGithubAction = /(?:^|\/)\.github\/workflows\/[^/]+\.ya?ml$/i.test(path);
  const triggers = isGithubAction ? extractTriggers(lines) : [];
  const jobs = isGithubAction ? extractJobs(lines) : [];

  const isDockerCompose = /(?:^|\/)(?:docker-)?compose(?:\.[\w.-]+)?\.ya?ml$/i.test(path);
  const services = isDockerCompose ? extractServices(lines) : [];

  // Kubernetes detection is content-based: any document that declares both
  // `apiVersion:` and `kind:` at depth 0 is treated as a manifest. Works
  // for multi-doc files (helm output, kustomize build) and single-doc.
  const resources = extractKubernetesResources(lines, documents);
  const isKubernetes = resources.length > 0;

  return {
    ok: true,
    outline,
    totalKeys,
    maxDepth,
    documents,
    anchors,
    isGithubAction,
    triggers,
    jobs,
    isDockerCompose,
    services,
    isKubernetes,
    resources,
  };
}

function scanLines(content: string): ScanLine[] {
  const out: ScanLine[] = [];
  const raws = content.split('\n');
  for (let i = 0; i < raws.length; i++) {
    const raw = raws[i];
    const indent = countIndent(raw);
    const trimmed = raw.trim();
    const blank = trimmed.length === 0;
    const isComment = !blank && trimmed.startsWith('#');
    const isDocSeparator = trimmed === '---' || trimmed.startsWith('--- ');
    const isDocEnd = trimmed === '...' || trimmed.startsWith('... ');
    out.push({
      no: i + 1,
      raw,
      body: blank || isComment ? trimmed : stripTrailingComment(raw),
      indent,
      blank,
      isComment,
      isDocSeparator,
      isDocEnd,
    });
  }
  return out;
}

function countIndent(s: string): number {
  let n = 0;
  for (let i = 0; i < s.length; i++) {
    const c = s[i];
    if (c === ' ' || c === '\t') n++;
    else break;
  }
  return n;
}

function stripTrailingComment(line: string): string {
  // Strip ` # …` only when the `#` is outside of quotes — comments require
  // a space (or start-of-line) before `#`. This handles "value # note" and
  // leaves `password: "p#1"` alone.
  let inSingle = false;
  let inDouble = false;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '"' && !inSingle) inDouble = !inDouble;
    else if (c === "'" && !inDouble) inSingle = !inSingle;
    else if (c === '#' && !inSingle && !inDouble) {
      if (i === 0 || /\s/.test(line[i - 1])) {
        return line.slice(0, i).replace(/\s+$/, '');
      }
    }
  }
  return line.replace(/\s+$/, '');
}

function collectDocuments(lines: ScanLine[]): YamlDocument[] {
  const docs: YamlDocument[] = [];
  for (const l of lines) {
    if (l.isDocSeparator) docs.push({ index: docs.length + 1, line: l.no });
  }
  // If the file has content before the first `---` (or has no `---` at all),
  // there is an implicit first document starting at line 1.
  const firstContent = lines.find((l) => !l.blank && !l.isComment);
  if (firstContent && (docs.length === 0 || firstContent.no < docs[0].line)) {
    docs.unshift({ index: 1, line: firstContent.no });
    for (let i = 1; i < docs.length; i++) docs[i].index = i + 1;
  }
  return docs;
}

function buildOutline(lines: ScanLine[]): YamlOutlineEntry[] {
  // Top-level entries live at the first non-blank/comment indent we
  // encounter — usually 0, but YAML allows everything to be indented
  // uniformly. We anchor on whichever indent the first real content uses.
  const firstContent = lines.find((l) => !l.blank && !l.isComment && !l.isDocSeparator && !l.isDocEnd);
  if (!firstContent) return [];
  const rootIndent = firstContent.indent;

  const entries: YamlOutlineEntry[] = [];
  let arrayIdx = 0;
  for (let i = 0; i < lines.length && entries.length < 50; i++) {
    const l = lines[i];
    if (l.blank || l.isComment || l.isDocSeparator || l.isDocEnd) continue;
    if (l.indent !== rootIndent) continue;

    const seq = matchSequenceItem(l.body, l.indent);
    if (seq) {
      const previewAndType = previewSequenceItem(seq.rest, lines, i, l.indent);
      entries.push({
        key: `[${arrayIdx++}]`,
        type: previewAndType.type,
        preview: previewAndType.preview,
        count: previewAndType.count,
        line: l.no,
      });
      continue;
    }

    const kv = matchMappingEntry(l.body, l.indent);
    if (kv) {
      const previewAndType = previewMappingValue(kv.value, lines, i, l.indent);
      entries.push({
        key: kv.key,
        type: previewAndType.type,
        preview: previewAndType.preview,
        count: previewAndType.count,
        line: l.no,
      });
    }
  }
  return entries;
}

function matchMappingEntry(body: string, indent: number): { key: string; value: string } | null {
  const trimmed = body.slice(indent);
  // Quoted keys first so quote characters aren't swallowed by the unquoted
  // branch. YAML requires the colon to be followed by whitespace or the
  // end of the line to count as a mapping indicator, which keeps `key:value`
  // and URL fragments like `https://…` from being misread as mappings.
  // Single-quoted key:
  let m = /^'([^']*)'\s*:(?:\s+(.*)|\s*$)/.exec(trimmed);
  if (m) return { key: m[1], value: (m[2] ?? '').trim() };
  // Double-quoted key:
  m = /^"((?:[^"\\]|\\.)*)"\s*:(?:\s+(.*)|\s*$)/.exec(trimmed);
  if (m) return { key: m[1], value: (m[2] ?? '').trim() };
  // Unquoted key: any run not starting with a flow indicator or quote, up
  // to the first ` :` or `:<eol>`. Permissive enough for unusual but legal
  // keys like `.`, `./path`, or `org.example.foo`.
  m = /^([^:'"\s,[\]{}#&*!|>%@`][^:]*?)\s*:(?:\s+(.*)|\s*$)/.exec(trimmed);
  if (m) return { key: m[1].trim(), value: (m[2] ?? '').trim() };
  return null;
}

function matchSequenceItem(body: string, indent: number): { rest: string } | null {
  const trimmed = body.slice(indent);
  if (trimmed === '-') return { rest: '' };
  if (trimmed.startsWith('- ')) return { rest: trimmed.slice(2).trim() };
  return null;
}

function previewMappingValue(
  inlineValue: string,
  lines: ScanLine[],
  i: number,
  parentIndent: number,
): { type: YamlValueType; preview: string; count?: number } {
  if (inlineValue.length > 0) {
    // Drop a trailing anchor/alias marker from the displayed value so the
    // preview reads as the scalar the user wrote rather than YAML plumbing.
    const value = inlineValue.replace(/^&\S+\s*/, '').replace(/^\*\S+\s*$/, (s) => s);
    if (value === '~' || value.toLowerCase() === 'null') return { type: 'null', preview: 'null' };
    if (value === '[]') return { type: 'sequence', preview: '0 items', count: 0 };
    if (value === '{}') return { type: 'mapping', preview: '0 keys', count: 0 };
    if (value === '|' || value === '>' || /^[|>][+\-]?\d*$/.test(value)) {
      // Block scalar; count the lines in its body.
      const lineCount = countBlockScalarLines(lines, i, parentIndent);
      return { type: 'scalar', preview: `${lineCount} line${lineCount === 1 ? '' : 's'}` };
    }
    if (value.startsWith('[')) return { type: 'sequence', preview: clip(value, 40) };
    if (value.startsWith('{')) return { type: 'mapping', preview: clip(value, 40) };
    return { type: 'scalar', preview: clip(stripQuotes(value), 60) };
  }

  // Empty inline value: peek at the next non-blank line to decide between
  // mapping, sequence, or genuinely empty.
  const child = nextChild(lines, i, parentIndent);
  if (!child) return { type: 'empty', preview: '∅' };
  const childBody = child.body;
  if (matchSequenceItem(childBody, child.indent)) {
    const count = countSiblings(lines, i, parentIndent, 'sequence');
    return { type: 'sequence', preview: `${count} item${count === 1 ? '' : 's'}`, count };
  }
  if (matchMappingEntry(childBody, child.indent)) {
    const count = countSiblings(lines, i, parentIndent, 'mapping');
    return { type: 'mapping', preview: `${count} key${count === 1 ? '' : 's'}`, count };
  }
  return { type: 'empty', preview: '∅' };
}

function previewSequenceItem(
  rest: string,
  lines: ScanLine[],
  i: number,
  itemIndent: number,
): { type: YamlValueType; preview: string; count?: number } {
  if (rest.length === 0) {
    // Bare `-` then indented children
    const child = nextChild(lines, i, itemIndent);
    if (!child) return { type: 'empty', preview: '∅' };
    if (matchMappingEntry(child.body, child.indent)) {
      // Count keys belonging to this list item: lines at indent === child.indent
      // up until indent <= itemIndent or another `- ` at itemIndent.
      const count = countItemMappingKeys(lines, i, itemIndent, child.indent);
      return { type: 'mapping', preview: `${count} key${count === 1 ? '' : 's'}`, count };
    }
    return { type: 'scalar', preview: '…' };
  }
  // `- key: value` shorthand — surface the key.
  const kv = matchMappingEntry(rest, 0);
  if (kv) {
    return { type: 'mapping', preview: `${kv.key}${kv.value ? ': ' + clip(stripQuotes(kv.value), 40) : ''}` };
  }
  return { type: 'scalar', preview: clip(stripQuotes(rest), 60) };
}

function nextChild(lines: ScanLine[], i: number, parentIndent: number): ScanLine | null {
  for (let j = i + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) return null;
    if (l.blank || l.isComment) continue;
    if (l.indent <= parentIndent) return null;
    return l;
  }
  return null;
}

function countSiblings(
  lines: ScanLine[],
  i: number,
  parentIndent: number,
  kind: 'mapping' | 'sequence',
): number {
  let count = 0;
  let childIndent = -1;
  for (let j = i + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank || l.isComment) continue;
    if (l.indent <= parentIndent) break;
    if (childIndent === -1) childIndent = l.indent;
    if (l.indent !== childIndent) continue;
    if (kind === 'mapping' && matchMappingEntry(l.body, l.indent)) count++;
    else if (kind === 'sequence' && matchSequenceItem(l.body, l.indent)) count++;
  }
  return count;
}

function countItemMappingKeys(
  lines: ScanLine[],
  i: number,
  itemIndent: number,
  childIndent: number,
): number {
  let count = 0;
  for (let j = i + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank || l.isComment) continue;
    if (l.indent <= itemIndent) break;
    if (l.indent !== childIndent) continue;
    if (matchMappingEntry(l.body, l.indent)) count++;
  }
  return count;
}

function countBlockScalarLines(lines: ScanLine[], i: number, parentIndent: number): number {
  let n = 0;
  for (let j = i + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank) {
      n++;
      continue;
    }
    if (l.indent <= parentIndent) break;
    n++;
  }
  return n;
}

function stats(lines: ScanLine[]): { totalKeys: number; maxDepth: number } {
  // Lightweight: count mapping entries and the maximum indent observed
  // among them. Sequences contribute to depth via their items' indent.
  let totalKeys = 0;
  let maxIndent = 0;
  let baseIndent = -1;
  for (const l of lines) {
    if (l.blank || l.isComment || l.isDocSeparator || l.isDocEnd) continue;
    if (baseIndent === -1) baseIndent = l.indent;
    if (matchMappingEntry(l.body, l.indent)) totalKeys++;
    if (l.indent > maxIndent) maxIndent = l.indent;
  }
  // Convert indent columns to nesting depth (assume 2-space — the YAML
  // norm — and clamp; this is a hint, not a guarantee).
  const indentUnit = 2;
  const maxDepth = Math.max(0, Math.floor((maxIndent - Math.max(baseIndent, 0)) / indentUnit));
  return { totalKeys, maxDepth };
}

function collectAnchors(lines: ScanLine[]): YamlAnchor[] {
  // Anchors are `&name` markers; record where each is defined. Aliases
  // (`*name`) are intentionally not surfaced — they only echo anchors and
  // would double the row count without adding orientation.
  const out: YamlAnchor[] = [];
  const seen = new Set<string>();
  const re = /(?:^|[\s,{[])&([A-Za-z_][\w-]*)/g;
  for (const l of lines) {
    if (l.blank || l.isComment) continue;
    re.lastIndex = 0;
    let m: RegExpExecArray | null;
    while ((m = re.exec(l.raw)) !== null) {
      const name = m[1];
      if (seen.has(name)) continue;
      seen.add(name);
      out.push({ name, line: l.no });
    }
  }
  return out;
}

function extractTriggers(lines: ScanLine[]): YamlTrigger[] {
  const out: YamlTrigger[] = [];
  const onLine = findTopLevelKey(lines, 'on');
  if (onLine < 0) return out;
  // Inline form: `on: push` or `on: [push, pull_request]`.
  const inline = lines[onLine];
  const kv = matchMappingEntry(inline.body, inline.indent);
  if (kv && kv.value) {
    const inlineVal = kv.value;
    if (inlineVal.startsWith('[')) {
      for (const name of inlineVal.replace(/^\[|\]$/g, '').split(',')) {
        const trimmed = name.trim();
        if (trimmed) out.push({ name: stripQuotes(trimmed), line: inline.no });
      }
    } else {
      out.push({ name: stripQuotes(inlineVal), line: inline.no });
    }
    return out;
  }
  // Block form: child keys are trigger names.
  const parentIndent = inline.indent;
  let childIndent = -1;
  for (let j = onLine + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank || l.isComment) continue;
    if (l.indent <= parentIndent) break;
    if (childIndent === -1) childIndent = l.indent;
    if (l.indent !== childIndent) continue;
    const ck = matchMappingEntry(l.body, l.indent);
    if (ck) out.push({ name: ck.key, line: l.no });
    const seq = matchSequenceItem(l.body, l.indent);
    if (seq && seq.rest) out.push({ name: stripQuotes(seq.rest), line: l.no });
  }
  return out;
}

function extractJobs(lines: ScanLine[]): YamlJob[] {
  const out: YamlJob[] = [];
  const jobsLine = findTopLevelKey(lines, 'jobs');
  if (jobsLine < 0) return out;
  const parent = lines[jobsLine];
  let jobIndent = -1;
  for (let j = jobsLine + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank || l.isComment) continue;
    if (l.indent <= parent.indent) break;
    if (jobIndent === -1) jobIndent = l.indent;
    if (l.indent !== jobIndent) continue;
    const ck = matchMappingEntry(l.body, l.indent);
    if (!ck) continue;
    const stepCount = countNestedSequence(lines, j, jobIndent, 'steps');
    out.push({ id: ck.key, line: l.no, stepCount });
  }
  return out;
}

function extractServices(lines: ScanLine[]): YamlService[] {
  const out: YamlService[] = [];
  const svcLine = findTopLevelKey(lines, 'services');
  if (svcLine < 0) return out;
  const parent = lines[svcLine];
  let svcIndent = -1;
  for (let j = svcLine + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank || l.isComment) continue;
    if (l.indent <= parent.indent) break;
    if (svcIndent === -1) svcIndent = l.indent;
    if (l.indent !== svcIndent) continue;
    const ck = matchMappingEntry(l.body, l.indent);
    if (!ck) continue;
    const image = findChildScalar(lines, j, svcIndent, 'image') ?? findChildScalar(lines, j, svcIndent, 'build') ?? '';
    out.push({ name: ck.key, line: l.no, image });
  }
  return out;
}

function extractKubernetesResources(lines: ScanLine[], documents: YamlDocument[]): YamlK8sResource[] {
  const out: YamlK8sResource[] = [];
  if (documents.length === 0) return out;
  for (let d = 0; d < documents.length; d++) {
    const start = lines.findIndex((l) => l.no === documents[d].line);
    if (start < 0) continue;
    const end = d + 1 < documents.length
      ? lines.findIndex((l) => l.no === documents[d + 1].line)
      : lines.length;
    const slice = lines.slice(start, end);
    const apiVersion = findScalarInSlice(slice, 'apiVersion');
    const kind = findScalarInSlice(slice, 'kind');
    if (!apiVersion || !kind) continue;
    const metaName = findNestedScalarInSlice(slice, 'metadata', 'name') ?? '';
    const namespace = findNestedScalarInSlice(slice, 'metadata', 'namespace') ?? '';
    // Anchor the row to the `kind:` line — it's the most identifying
    // top-level field and jumping there gives the most context.
    const kindLine = slice.find(
      (l) => !l.blank && !l.isComment && matchMappingEntry(l.body, l.indent)?.key === 'kind',
    );
    out.push({
      kind,
      name: metaName,
      namespace,
      line: kindLine ? kindLine.no : documents[d].line,
    });
  }
  return out;
}

function findTopLevelKey(lines: ScanLine[], key: string): number {
  // Index in `lines` array (not the line number) of the first top-level
  // mapping with the given key. Top-level = indent equal to the first
  // non-blank content's indent (usually 0).
  const first = lines.find((l) => !l.blank && !l.isComment && !l.isDocSeparator && !l.isDocEnd);
  if (!first) return -1;
  const rootIndent = first.indent;
  for (let i = 0; i < lines.length; i++) {
    const l = lines[i];
    if (l.blank || l.isComment || l.isDocSeparator || l.isDocEnd) continue;
    if (l.indent !== rootIndent) continue;
    const kv = matchMappingEntry(l.body, l.indent);
    if (kv && kv.key === key) return i;
  }
  return -1;
}

function findChildScalar(lines: ScanLine[], parentIdx: number, parentIndent: number, key: string): string | null {
  let childIndent = -1;
  for (let j = parentIdx + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank || l.isComment) continue;
    if (l.indent <= parentIndent) break;
    if (childIndent === -1) childIndent = l.indent;
    if (l.indent !== childIndent) continue;
    const kv = matchMappingEntry(l.body, l.indent);
    if (kv && kv.key === key && kv.value) return stripQuotes(kv.value);
  }
  return null;
}

function countNestedSequence(lines: ScanLine[], parentIdx: number, parentIndent: number, key: string): number {
  let childIndent = -1;
  // Find the nested key, then count `- ` items under it.
  for (let j = parentIdx + 1; j < lines.length; j++) {
    const l = lines[j];
    if (l.isDocSeparator || l.isDocEnd) break;
    if (l.blank || l.isComment) continue;
    if (l.indent <= parentIndent) break;
    if (childIndent === -1) childIndent = l.indent;
    if (l.indent !== childIndent) continue;
    const kv = matchMappingEntry(l.body, l.indent);
    if (!kv || kv.key !== key) continue;
    return countSiblings(lines, j, l.indent, 'sequence');
  }
  return 0;
}

function findScalarInSlice(slice: ScanLine[], key: string): string | null {
  const first = slice.find((l) => !l.blank && !l.isComment && !l.isDocSeparator && !l.isDocEnd);
  if (!first) return null;
  const rootIndent = first.indent;
  for (const l of slice) {
    if (l.blank || l.isComment || l.isDocSeparator || l.isDocEnd) continue;
    if (l.indent !== rootIndent) continue;
    const kv = matchMappingEntry(l.body, l.indent);
    if (kv && kv.key === key && kv.value) return stripQuotes(kv.value);
  }
  return null;
}

function findNestedScalarInSlice(slice: ScanLine[], parent: string, key: string): string | null {
  const first = slice.find((l) => !l.blank && !l.isComment && !l.isDocSeparator && !l.isDocEnd);
  if (!first) return null;
  const rootIndent = first.indent;
  let inParent = false;
  let parentIndent = -1;
  let childIndent = -1;
  for (let i = 0; i < slice.length; i++) {
    const l = slice[i];
    if (l.blank || l.isComment || l.isDocSeparator || l.isDocEnd) continue;
    if (!inParent) {
      if (l.indent !== rootIndent) continue;
      const kv = matchMappingEntry(l.body, l.indent);
      if (kv && kv.key === parent) {
        inParent = true;
        parentIndent = l.indent;
      }
      continue;
    }
    if (l.indent <= parentIndent) return null;
    if (childIndent === -1) childIndent = l.indent;
    if (l.indent !== childIndent) continue;
    const kv = matchMappingEntry(l.body, l.indent);
    if (kv && kv.key === key && kv.value) return stripQuotes(kv.value);
  }
  return null;
}

function clip(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + '…';
}

function stripQuotes(s: string): string {
  const t = s.trim();
  if ((t.startsWith('"') && t.endsWith('"')) || (t.startsWith("'") && t.endsWith("'"))) {
    return t.slice(1, -1);
  }
  return t;
}
