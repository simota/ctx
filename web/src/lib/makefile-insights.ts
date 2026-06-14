// Makefile insight extraction for the FileDetail sidebar.
//
// Approach: a single line-based scan. Makefiles are line-oriented — recipe
// lines start with a tab, targets and variable assignments live in column 0
// — so a parser is overkill. We surface the three things a reader scans a
// Makefile for: which targets exist (and what they do), which variables are
// defined, and which other makefiles are pulled in.
//
// The `## help` convention (a comment after the target's colon, e.g.
// `build: ## Compile everything`) is widely used by self-documenting
// Makefiles — including this repo's own — so we capture it as the target's
// description when present.

export interface MakeTarget {
  name: string;
  line: number;
  prereqs: string[];
  doc: string;
  phony: boolean;
  // Recipe lines (tab-indented commands) that follow the target, for the
  // rendered card. `@`/`-`/`+` prefixes are kept verbatim.
  recipe: string[];
}

export interface MakeVariable {
  name: string;
  op: string; // '=', ':=', '?=', '+=', '::='
  value: string;
  line: number;
}

export interface MakeInclude {
  path: string;
  line: number;
  optional: boolean; // `-include` / `sinclude`
}

export interface MakefileInsights {
  ok: boolean;
  targets: MakeTarget[];
  variables: MakeVariable[];
  includes: MakeInclude[];
  phonyCount: number;
}

const EMPTY: MakefileInsights = {
  ok: false,
  targets: [],
  variables: [],
  includes: [],
  phonyCount: 0,
};

export function extractMakefileInsights(content: string): MakefileInsights {
  const raws = content.split('\n');

  const hasContent = raws.some((l) => {
    const t = l.trim();
    return t.length > 0 && !t.startsWith('#');
  });
  if (!hasContent) return EMPTY;

  const phony = collectPhony(raws);
  const targets: MakeTarget[] = [];
  const variables: MakeVariable[] = [];
  const includes: MakeInclude[] = [];
  const seenTargets = new Set<string>();

  for (let i = 0; i < raws.length; i++) {
    const raw = raws[i];
    // Recipe lines (tab-indented) and blank/comment lines are not declarations.
    if (raw.startsWith('\t')) continue;
    const trimmed = raw.trim();
    if (trimmed.length === 0 || trimmed.startsWith('#')) continue;

    const inc = matchInclude(trimmed);
    if (inc) {
      for (const p of inc.paths) {
        includes.push({ path: p, line: i + 1, optional: inc.optional });
      }
      continue;
    }

    const v = matchVariable(raw);
    if (v) {
      variables.push({ name: v.name, op: v.op, value: clip(v.value, 80), line: i + 1 });
      continue;
    }

    const t = matchTarget(raw);
    if (t) {
      // Collect the recipe: tab-indented lines immediately following, up to
      // the next unindented line. Blank lines inside a recipe are preserved
      // as-is by Make but we drop them from the preview to keep cards tight.
      const recipe: string[] = [];
      for (let j = i + 1; j < raws.length; j++) {
        if (raws[j].startsWith('\t')) {
          const cmd = raws[j].replace(/^\t/, '').trimEnd();
          if (cmd.length > 0) recipe.push(clip(cmd, 100));
        } else if (raws[j].trim().length === 0) {
          continue;
        } else {
          break;
        }
      }
      // Skip pattern rules and special built-in targets from the main list;
      // they're plumbing, not entry points the reader picks from.
      for (const name of t.names) {
        if (name.startsWith('.')) continue; // .PHONY, .DEFAULT, etc.
        if (name.includes('%')) continue; // pattern rule
        if (seenTargets.has(name)) continue;
        seenTargets.add(name);
        targets.push({
          name,
          line: i + 1,
          prereqs: t.prereqs,
          doc: t.doc,
          phony: phony.has(name),
          recipe,
        });
      }
    }
  }

  return {
    ok: true,
    targets,
    variables,
    includes,
    phonyCount: phony.size,
  };
}

function collectPhony(raws: string[]): Set<string> {
  const out = new Set<string>();
  for (const raw of raws) {
    if (raw.startsWith('\t')) continue;
    const m = /^\.PHONY\s*:(.*)$/.exec(raw.trim());
    if (!m) continue;
    for (const name of m[1].trim().split(/\s+/)) {
      if (name) out.add(name);
    }
  }
  return out;
}

function matchInclude(trimmed: string): { paths: string[]; optional: boolean } | null {
  const m = /^(-include|sinclude|include)\s+(.+)$/.exec(trimmed);
  if (!m) return null;
  const optional = m[1] !== 'include';
  const paths = m[2].trim().split(/\s+/).filter(Boolean);
  return { paths, optional };
}

function matchVariable(raw: string): { name: string; op: string; value: string } | null {
  // `export NAME := value`, `NAME = value`, etc. Operator detection comes
  // before target detection because `NAME := x` also contains a colon.
  const m = /^(?:export\s+|override\s+)?([A-Za-z_][A-Za-z0-9_.]*)\s*(::=|\?=|\+=|:=|=)\s*(.*)$/.exec(raw);
  if (!m) return null;
  return { name: m[1], op: m[2], value: m[3].trim() };
}

function matchTarget(raw: string): { names: string[]; prereqs: string[]; doc: string } | null {
  // A target line has an unindented name list, a single `:` (not `:=`), then
  // optional prerequisites. Capture a trailing `## doc` comment if present.
  const line = raw.replace(/\r$/, '');
  // Reject double-colon-equals (already handled as a variable) and assignment.
  const colon = findTargetColon(line);
  if (colon < 0) return null;

  const lhs = line.slice(0, colon).trim();
  if (!lhs) return null;
  // Reject anything that doesn't look like a target list (e.g. stray `:`).
  if (/[=]/.test(lhs)) return null;

  let rhs = line.slice(colon + 1);
  // Double-colon rule: `target:: deps` — drop the second colon.
  if (rhs.startsWith(':')) rhs = rhs.slice(1);

  let doc = '';
  const docMatch = /##\s?(.*)$/.exec(rhs);
  if (docMatch) {
    doc = docMatch[1].trim();
    rhs = rhs.slice(0, docMatch.index);
  } else {
    // Strip an ordinary trailing comment.
    const hash = rhs.indexOf('#');
    if (hash >= 0) rhs = rhs.slice(0, hash);
  }

  const names = lhs.split(/\s+/).filter(Boolean);
  const prereqs = rhs.trim().split(/\s+/).filter(Boolean);
  if (names.length === 0) return null;
  return { names, prereqs, doc };
}

function findTargetColon(line: string): number {
  // First `:` that is not part of `:=` / `::=` and not inside an automatic
  // variable like `$(...)`. Good enough for the common case.
  let depth = 0;
  for (let i = 0; i < line.length; i++) {
    const c = line[i];
    if (c === '(' || c === '{') depth++;
    else if (c === ')' || c === '}') depth--;
    else if (c === ':' && depth === 0) {
      const next = line[i + 1];
      const next2 = line[i + 2];
      if (next === '=') return -1; // `:=`
      if (next === ':' && next2 === '=') return -1; // `::=`
      return i;
    }
  }
  return -1;
}

function clip(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + '…';
}
