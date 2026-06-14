// Dockerfile insight extraction for the FileDetail sidebar.
//
// Approach: a single instruction scan that joins backslash-continued lines
// so multi-line RUN/COPY blocks are read as one instruction. A reader scans
// a Dockerfile for its build stages (the FROM graph), what it exposes, and
// the entrypoint — so those are the lenses we surface, plus a per-stage
// count of the heavier instructions (RUN/COPY) for weight orientation.

export interface DockerStep {
  keyword: string;
  text: string;
  line: number;
}

export interface DockerStage {
  index: number; // 1-based
  base: string; // image ref after FROM
  name: string; // AS <name>, or '' for an unnamed stage
  line: number;
  runCount: number;
  copyCount: number;
  // Notable instructions in this stage, for the rendered stage card.
  steps: DockerStep[];
  // Names/indices referenced via `COPY --from=<ref>` — the stage's inputs.
  from: string[];
  // Ports this stage EXPOSEs.
  ports: string[];
  entrypoint: string;
  cmd: string;
}

export interface DockerPort {
  port: string; // e.g. "8080" or "8080/udp"
  line: number;
}

export interface DockerKeyValue {
  key: string;
  value: string;
  line: number;
}

export interface DockerfileInsights {
  ok: boolean;
  stages: DockerStage[];
  ports: DockerPort[];
  args: DockerKeyValue[]; // ARG declarations before the first stage (global)
  entrypoint: string;
  cmd: string;
  exposeRaw: string[]; // unique exposed port strings, for the summary line
}

const EMPTY: DockerfileInsights = {
  ok: false,
  stages: [],
  ports: [],
  args: [],
  entrypoint: '',
  cmd: '',
  exposeRaw: [],
};

interface Instruction {
  keyword: string; // upper-cased (FROM, RUN, …)
  rest: string;
  line: number; // 1-based line of the instruction start
}

export function extractDockerfileInsights(content: string): DockerfileInsights {
  const instrs = scanInstructions(content);
  if (instrs.length === 0) return EMPTY;

  const stages: DockerStage[] = [];
  const ports: DockerPort[] = [];
  const args: DockerKeyValue[] = [];
  let entrypoint = '';
  let cmd = '';
  const exposeSet = new Set<string>();

  let current: DockerStage | null = null;

  for (const ins of instrs) {
    switch (ins.keyword) {
      case 'FROM': {
        const { base, name } = parseFrom(ins.rest);
        current = {
          index: stages.length + 1,
          base,
          name,
          line: ins.line,
          runCount: 0,
          copyCount: 0,
          steps: [],
          from: [],
          ports: [],
          entrypoint: '',
          cmd: '',
        };
        stages.push(current);
        break;
      }
      case 'RUN':
        if (current) {
          current.runCount++;
          current.steps.push({ keyword: 'RUN', text: clip(ins.rest, 80), line: ins.line });
        }
        break;
      case 'COPY':
      case 'ADD': {
        if (current) {
          current.copyCount++;
          current.steps.push({ keyword: ins.keyword, text: clip(ins.rest, 80), line: ins.line });
          const fromRef = /--from=(\S+)/.exec(ins.rest);
          if (fromRef && !current.from.includes(fromRef[1])) current.from.push(fromRef[1]);
        }
        break;
      }
      case 'EXPOSE':
        for (const p of ins.rest.trim().split(/\s+/).filter(Boolean)) {
          ports.push({ port: p, line: ins.line });
          exposeSet.add(p);
          if (current) current.ports.push(p);
        }
        break;
      case 'ARG':
        // Only ARGs before the first FROM are global build args worth a row.
        if (!current) {
          const kv = parseKeyValue(ins.rest);
          args.push({ key: kv.key, value: kv.value, line: ins.line });
        }
        break;
      case 'WORKDIR':
      case 'USER':
      case 'VOLUME':
      case 'ENV':
      case 'LABEL':
      case 'HEALTHCHECK':
        if (current) current.steps.push({ keyword: ins.keyword, text: clip(ins.rest, 80), line: ins.line });
        break;
      case 'ENTRYPOINT':
        entrypoint = normalizeExec(ins.rest);
        if (current) current.entrypoint = entrypoint;
        break;
      case 'CMD':
        cmd = normalizeExec(ins.rest);
        if (current) current.cmd = cmd;
        break;
    }
  }

  return {
    ok: true,
    stages,
    ports,
    args,
    entrypoint,
    cmd,
    exposeRaw: [...exposeSet],
  };
}

function scanInstructions(content: string): Instruction[] {
  const raws = content.split('\n');
  const out: Instruction[] = [];
  let i = 0;
  while (i < raws.length) {
    let line = raws[i];
    const startLine = i + 1;
    const trimmed = line.trim();
    // Skip blanks, comments, and parser directives (`# syntax=…`).
    if (trimmed.length === 0 || trimmed.startsWith('#')) {
      i++;
      continue;
    }
    // Join backslash continuations into a single logical instruction.
    let joined = line.replace(/\\\s*$/, '');
    while (/\\\s*$/.test(line) && i + 1 < raws.length) {
      i++;
      line = raws[i];
      // Drop comment-only continuation lines (legal inside RUN blocks).
      const cont = line.replace(/\\\s*$/, '');
      if (line.trim().startsWith('#')) continue;
      joined += ' ' + cont.trim();
    }
    i++;

    const m = /^\s*([A-Za-z]+)\s*(.*)$/s.exec(joined);
    if (!m) continue;
    out.push({ keyword: m[1].toUpperCase(), rest: m[2].trim(), line: startLine });
  }
  return out;
}

function parseFrom(rest: string): { base: string; name: string } {
  // `FROM image[:tag][@digest] [AS name]` — `--platform=…` flag tolerated.
  const tokens = rest.split(/\s+/).filter(Boolean);
  let base = '';
  let name = '';
  for (let k = 0; k < tokens.length; k++) {
    const tok = tokens[k];
    if (tok.startsWith('--')) continue; // build flag
    if (!base) {
      base = tok;
      continue;
    }
    if (/^as$/i.test(tok) && tokens[k + 1]) {
      name = tokens[k + 1];
      break;
    }
  }
  return { base, name };
}

function parseKeyValue(rest: string): { key: string; value: string } {
  const eq = rest.indexOf('=');
  if (eq < 0) return { key: rest.trim(), value: '' };
  return { key: rest.slice(0, eq).trim(), value: stripQuotes(rest.slice(eq + 1).trim()) };
}

function normalizeExec(rest: string): string {
  // Exec form `["a","b"]` → `a b`; shell form passes through. Keep it short.
  const t = rest.trim();
  if (t.startsWith('[')) {
    try {
      const arr = JSON.parse(t) as unknown;
      if (Array.isArray(arr)) return clip(arr.join(' '), 80);
    } catch {
      // fall through to raw clip
    }
  }
  return clip(t, 80);
}

function stripQuotes(s: string): string {
  if ((s.startsWith('"') && s.endsWith('"')) || (s.startsWith("'") && s.endsWith("'"))) {
    return s.slice(1, -1);
  }
  return s;
}

function clip(s: string, max: number): string {
  if (s.length <= max) return s;
  return s.slice(0, max) + '…';
}
