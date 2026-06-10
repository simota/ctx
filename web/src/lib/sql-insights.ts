// SQL insight extraction for the FileDetail sidebar.
//
// Approach: a single linear scan that tokenises statement boundaries
// while honouring SQL's string, comment and dollar-quote rules, then
// classifies each statement by its leading keyword(s). Same minimalist
// philosophy as yaml-insights / xml-insights — no full parser, just
// enough structure to orient the reader and jump between statements.
//
// Dialect notes:
//   - Postgres dollar quotes (`$tag$ … $tag$`, bare `$$`) terminate
//     statements only at the matching tag, which keeps function bodies
//     containing `;` from being split mid-block.
//   - MySQL backtick identifiers and SQL Server `[bracket]` identifiers
//     are recognised so `;` inside them is ignored.
//   - SQL's `''` and `""` doubling escapes are handled.

export type SqlStatementKind =
  | 'create_table'
  | 'create_index'
  | 'create_view'
  | 'create_materialized_view'
  | 'create_function'
  | 'create_procedure'
  | 'create_trigger'
  | 'create_type'
  | 'create_schema'
  | 'create_extension'
  | 'create_sequence'
  | 'create_database'
  | 'create_role'
  | 'create_other'
  | 'alter'
  | 'drop'
  | 'insert'
  | 'update'
  | 'delete'
  | 'select'
  | 'with'
  | 'merge'
  | 'truncate'
  | 'comment'
  | 'grant'
  | 'revoke'
  | 'begin'
  | 'commit'
  | 'rollback'
  | 'savepoint'
  | 'set'
  | 'do'
  | 'use'
  | 'call'
  | 'analyze'
  | 'vacuum'
  | 'explain'
  | 'pragma'
  | 'other';

export interface SqlStatement {
  kind: SqlStatementKind;
  target: string;
  line: number;
}

export interface SqlTable {
  name: string;
  line: number;
}

export interface SqlIndex {
  name: string;
  target: string;
  line: number;
}

export interface SqlView {
  name: string;
  line: number;
}

export interface SqlRoutine {
  name: string;
  kind: 'function' | 'procedure' | 'trigger';
  line: number;
}

export interface SqlInsights {
  ok: boolean;
  statements: SqlStatement[];
  totalStatements: number;
  truncated: boolean; // outline was capped
  tables: SqlTable[];
  indexes: SqlIndex[];
  views: SqlView[];
  routines: SqlRoutine[];
}

const OUTLINE_CAP = 80;

export function extractSqlInsights(content: string): SqlInsights {
  const empty: SqlInsights = {
    ok: false,
    statements: [],
    totalStatements: 0,
    truncated: false,
    tables: [],
    indexes: [],
    views: [],
    routines: [],
  };

  const ranges = tokenizeStatements(content);
  if (ranges.length === 0) return empty;

  const statements: SqlStatement[] = [];
  const tables: SqlTable[] = [];
  const indexes: SqlIndex[] = [];
  const views: SqlView[] = [];
  const routines: SqlRoutine[] = [];

  for (const r of ranges) {
    const text = content.slice(r.start, r.end);
    const cls = classifyStatement(text);
    const stmt: SqlStatement = { kind: cls.kind, target: cls.target, line: r.line };
    statements.push(stmt);
    switch (cls.kind) {
      case 'create_table':
        if (cls.target) tables.push({ name: cls.target, line: r.line });
        break;
      case 'create_index':
        if (cls.target) indexes.push({ name: cls.target, target: cls.extra ?? '', line: r.line });
        break;
      case 'create_view':
      case 'create_materialized_view':
        if (cls.target) views.push({ name: cls.target, line: r.line });
        break;
      case 'create_function':
        if (cls.target) routines.push({ name: cls.target, kind: 'function', line: r.line });
        break;
      case 'create_procedure':
        if (cls.target) routines.push({ name: cls.target, kind: 'procedure', line: r.line });
        break;
      case 'create_trigger':
        if (cls.target) routines.push({ name: cls.target, kind: 'trigger', line: r.line });
        break;
    }
  }

  const total = statements.length;
  const outline = statements.slice(0, OUTLINE_CAP);

  return {
    ok: true,
    statements: outline,
    totalStatements: total,
    truncated: total > OUTLINE_CAP,
    tables,
    indexes,
    views,
    routines,
  };
}

interface StatementRange {
  start: number;
  end: number;  // exclusive; index of `;` if present
  line: number; // 1-based line of the first non-whitespace char
}

function tokenizeStatements(content: string): StatementRange[] {
  const out: StatementRange[] = [];
  const len = content.length;
  let i = 0;
  let line = 1;
  let stmtStart = -1;
  let stmtLine = 1;

  const beginStatement = () => {
    if (stmtStart === -1) {
      stmtStart = i;
      stmtLine = line;
    }
  };
  const endStatement = (endIdx: number) => {
    if (stmtStart !== -1) {
      out.push({ start: stmtStart, end: endIdx, line: stmtLine });
      stmtStart = -1;
    }
  };

  while (i < len) {
    const c = content[i];

    if (c === '\n') {
      line++;
      i++;
      continue;
    }

    // Whitespace between statements: don't begin a statement yet
    if (stmtStart === -1 && /\s/.test(c)) {
      i++;
      continue;
    }

    // Line comment `--`
    if (c === '-' && content[i + 1] === '-') {
      // Inside a statement, the comment is part of it; outside, skip without
      // beginning a statement. Either way, advance to end of line.
      const eol = content.indexOf('\n', i);
      i = eol < 0 ? len : eol;
      continue;
    }
    // Block comment `/* ... */` (does not nest in standard SQL)
    if (c === '/' && content[i + 1] === '*') {
      i += 2;
      while (i < len) {
        if (content[i] === '\n') line++;
        if (content[i] === '*' && content[i + 1] === '/') {
          i += 2;
          break;
        }
        i++;
      }
      continue;
    }

    // From here on, the character belongs to a statement.
    beginStatement();

    if (c === "'") {
      i = skipQuoted(content, i, "'", true, () => line++);
      continue;
    }
    if (c === '"') {
      i = skipQuoted(content, i, '"', true, () => line++);
      continue;
    }
    if (c === '`') {
      i = skipQuoted(content, i, '`', false, () => line++);
      continue;
    }
    if (c === '[') {
      // SQL Server bracketed identifier. Treat `]]` as escaped `]`.
      i++;
      while (i < len) {
        if (content[i] === '\n') line++;
        if (content[i] === ']') {
          if (content[i + 1] === ']') { i += 2; continue; }
          i++;
          break;
        }
        i++;
      }
      continue;
    }
    if (c === '$') {
      // Postgres dollar quote — only if it looks like $tag$.
      const m = /^\$([A-Za-z_]\w*)?\$/.exec(content.slice(i));
      if (m) {
        const tag = m[0];
        i += tag.length;
        while (i < len) {
          if (content[i] === '\n') line++;
          if (content.startsWith(tag, i)) {
            i += tag.length;
            break;
          }
          i++;
        }
        continue;
      }
      // Otherwise `$` is just a parameter prefix etc.
      i++;
      continue;
    }
    if (c === ';') {
      endStatement(i);
      i++;
      continue;
    }
    i++;
  }
  // Trailing statement without a terminating semicolon
  endStatement(len);
  return out;
}

function skipQuoted(
  content: string,
  start: number,
  quote: string,
  doubled: boolean,
  onNewline: () => void,
): number {
  // Advance past `quote`...`quote`, handling SQL's doubled-quote escape
  // (`''` in single, `""` in double identifier). Returns the index just
  // after the closing quote.
  let i = start + 1;
  const len = content.length;
  while (i < len) {
    if (content[i] === '\n') onNewline();
    if (content[i] === quote) {
      if (doubled && content[i + 1] === quote) {
        i += 2;
        continue;
      }
      return i + 1;
    }
    i++;
  }
  return len;
}

interface Classification {
  kind: SqlStatementKind;
  target: string;
  extra?: string; // e.g., ON <table> for indexes
}

function classifyStatement(text: string): Classification {
  // Strip leading comments and whitespace from the statement body so the
  // first token is the actual keyword.
  const body = stripLeadingNoise(text);
  if (!body) return { kind: 'other', target: '' };

  const kw = firstWord(body);
  const kw2 = secondWord(body);

  if (kw === 'CREATE') return classifyCreate(body);
  if (kw === 'ALTER') {
    const target = grabIdentifierAfterWords(body);
    return { kind: 'alter', target: prefixWithType(body, target, 'ALTER') };
  }
  if (kw === 'DROP') {
    const target = grabIdentifierAfterWords(body);
    return { kind: 'drop', target: prefixWithType(body, target, 'DROP') };
  }
  if (kw === 'INSERT') {
    const target = grabIdentifierAfter(body, /INSERT\s+INTO\s+/i);
    return { kind: 'insert', target };
  }
  if (kw === 'UPDATE') {
    const target = grabIdentifierAfter(body, /UPDATE\s+(ONLY\s+)?/i);
    return { kind: 'update', target };
  }
  if (kw === 'DELETE') {
    const target = grabIdentifierAfter(body, /DELETE\s+FROM\s+(ONLY\s+)?/i);
    return { kind: 'delete', target };
  }
  if (kw === 'SELECT') {
    const target = grabIdentifierAfter(body, /\bFROM\s+/i);
    return { kind: 'select', target };
  }
  if (kw === 'WITH') {
    // CTE: the leading WITH defines aliases; find the trailing
    // SELECT/INSERT/UPDATE/DELETE for a more useful label.
    const m = /\b(SELECT|INSERT|UPDATE|DELETE|MERGE)\b/i.exec(body);
    return { kind: 'with', target: m ? m[1].toUpperCase() : '' };
  }
  if (kw === 'MERGE') {
    const target = grabIdentifierAfter(body, /MERGE\s+INTO\s+/i);
    return { kind: 'merge', target };
  }
  if (kw === 'TRUNCATE') {
    const target = grabIdentifierAfter(body, /TRUNCATE\s+(TABLE\s+)?(ONLY\s+)?/i);
    return { kind: 'truncate', target };
  }
  if (kw === 'COMMENT') {
    // COMMENT ON <thing> <name> IS '…'
    const m = /COMMENT\s+ON\s+(\w+)\s+([^\s]+)/i.exec(body);
    return { kind: 'comment', target: m ? `${m[1].toLowerCase()} ${m[2]}` : '' };
  }
  if (kw === 'GRANT') return { kind: 'grant', target: extractGrantSubject(body) };
  if (kw === 'REVOKE') return { kind: 'revoke', target: extractGrantSubject(body) };
  if (kw === 'BEGIN' || (kw === 'START' && kw2 === 'TRANSACTION')) {
    return { kind: 'begin', target: '' };
  }
  if (kw === 'COMMIT') return { kind: 'commit', target: '' };
  if (kw === 'ROLLBACK') return { kind: 'rollback', target: '' };
  if (kw === 'SAVEPOINT') {
    const target = grabIdentifierAfter(body, /SAVEPOINT\s+/i);
    return { kind: 'savepoint', target };
  }
  if (kw === 'SET') {
    const m = /SET\s+([\w.]+)/i.exec(body);
    return { kind: 'set', target: m ? m[1] : '' };
  }
  if (kw === 'DO') return { kind: 'do', target: '' };
  if (kw === 'USE') {
    const target = grabIdentifierAfter(body, /USE\s+/i);
    return { kind: 'use', target };
  }
  if (kw === 'CALL' || kw === 'EXEC' || kw === 'EXECUTE') {
    const target = grabIdentifierAfter(body, /(?:CALL|EXEC(?:UTE)?)\s+/i);
    return { kind: 'call', target };
  }
  if (kw === 'ANALYZE') {
    const target = grabIdentifierAfter(body, /ANALYZE\s+/i);
    return { kind: 'analyze', target };
  }
  if (kw === 'VACUUM') {
    const target = grabIdentifierAfter(body, /VACUUM\s+(?:\([^)]*\)\s+)?(?:FULL\s+|FREEZE\s+|VERBOSE\s+|ANALYZE\s+)*/i);
    return { kind: 'vacuum', target };
  }
  if (kw === 'EXPLAIN') return { kind: 'explain', target: '' };
  if (kw === 'PRAGMA') {
    const target = grabIdentifierAfter(body, /PRAGMA\s+/i);
    return { kind: 'pragma', target };
  }
  return { kind: 'other', target: kw };
}

function classifyCreate(body: string): Classification {
  // Strip modifiers between CREATE and the kind keyword so we can switch
  // on the next bare keyword cleanly.
  const stripped = body.replace(
    /^CREATE\s+(OR\s+REPLACE\s+)?(GLOBAL\s+|LOCAL\s+|TEMPORARY\s+|TEMP\s+|UNLOGGED\s+)?/i,
    '',
  );
  // Pull the first one or two bare keywords so we can match exactly without
  // catching prefix collisions (e.g. `TABLE` vs `TABLESPACE`, `USER` vs
  // `USER MAPPING`). Identifiers come after these.
  const km = /^([A-Za-z]+)(?:\s+([A-Za-z]+))?/.exec(stripped);
  const kw1 = (km?.[1] ?? '').toUpperCase();
  const kw2 = (km?.[2] ?? '').toUpperCase();

  if (kw1 === 'UNIQUE' && kw2 === 'INDEX') {
    const noUnique = stripped.replace(/^UNIQUE\s+/i, '');
    const afterIndex = noUnique.replace(/^INDEX\s+(CONCURRENTLY\s+)?(IF\s+NOT\s+EXISTS\s+)?/i, '');
    const name = extractIdentifier(afterIndex);
    const onMatch = /\bON\s+(?:ONLY\s+)?([\w."`[\]]+(?:\.[\w."`[\]]+)*)/i.exec(noUnique);
    return { kind: 'create_index', target: name, extra: onMatch ? onMatch[1] : '' };
  }
  if (kw1 === 'INDEX') {
    const afterIndex = stripped.replace(/^INDEX\s+(CONCURRENTLY\s+)?(IF\s+NOT\s+EXISTS\s+)?/i, '');
    const name = extractIdentifier(afterIndex);
    const onMatch = /\bON\s+(?:ONLY\s+)?([\w."`[\]]+(?:\.[\w."`[\]]+)*)/i.exec(stripped);
    return { kind: 'create_index', target: name, extra: onMatch ? onMatch[1] : '' };
  }
  if (kw1 === 'TABLE') {
    const after = stripped.replace(/^TABLE\s+(IF\s+NOT\s+EXISTS\s+)?/i, '');
    return { kind: 'create_table', target: extractIdentifier(after) };
  }
  if (kw1 === 'MATERIALIZED' && kw2 === 'VIEW') {
    const after = stripped.replace(/^MATERIALIZED\s+VIEW\s+(IF\s+NOT\s+EXISTS\s+)?/i, '');
    return { kind: 'create_materialized_view', target: extractIdentifier(after) };
  }
  if (kw1 === 'VIEW') {
    const after = stripped.replace(/^VIEW\s+(IF\s+NOT\s+EXISTS\s+)?/i, '');
    return { kind: 'create_view', target: extractIdentifier(after) };
  }
  if (kw1 === 'FUNCTION') {
    const after = stripped.replace(/^FUNCTION\s+(IF\s+NOT\s+EXISTS\s+)?/i, '');
    return { kind: 'create_function', target: extractRoutineName(after) };
  }
  if (kw1 === 'PROCEDURE') {
    const after = stripped.replace(/^PROCEDURE\s+(IF\s+NOT\s+EXISTS\s+)?/i, '');
    return { kind: 'create_procedure', target: extractRoutineName(after) };
  }
  if (kw1 === 'TRIGGER') {
    const after = stripped.replace(/^TRIGGER\s+(IF\s+NOT\s+EXISTS\s+)?/i, '');
    return { kind: 'create_trigger', target: extractIdentifier(after) };
  }
  if (kw1 === 'TYPE') {
    return { kind: 'create_type', target: extractIdentifier(stripped.replace(/^TYPE\s+/i, '')) };
  }
  if (kw1 === 'SCHEMA') {
    return {
      kind: 'create_schema',
      target: extractIdentifier(stripped.replace(/^SCHEMA\s+(IF\s+NOT\s+EXISTS\s+)?(AUTHORIZATION\s+)?/i, '')),
    };
  }
  if (kw1 === 'EXTENSION') {
    return {
      kind: 'create_extension',
      target: extractIdentifier(stripped.replace(/^EXTENSION\s+(IF\s+NOT\s+EXISTS\s+)?/i, '')),
    };
  }
  if (kw1 === 'SEQUENCE') {
    return {
      kind: 'create_sequence',
      target: extractIdentifier(stripped.replace(/^SEQUENCE\s+(IF\s+NOT\s+EXISTS\s+)?/i, '')),
    };
  }
  if (kw1 === 'DATABASE') {
    return {
      kind: 'create_database',
      target: extractIdentifier(stripped.replace(/^DATABASE\s+(IF\s+NOT\s+EXISTS\s+)?/i, '')),
    };
  }
  if (kw1 === 'ROLE' || (kw1 === 'USER' && kw2 !== 'MAPPING')) {
    return {
      kind: 'create_role',
      target: extractIdentifier(stripped.replace(/^(ROLE|USER)\s+/i, '')),
    };
  }
  // Unknown CREATE flavour — keep the first two tokens as the label so the
  // outline still says something useful.
  return { kind: 'create_other', target: [kw1, kw2].filter(Boolean).join(' ') };
}

function stripLeadingNoise(text: string): string {
  let i = 0;
  const len = text.length;
  while (i < len) {
    const c = text[i];
    if (/\s/.test(c)) { i++; continue; }
    if (c === '-' && text[i + 1] === '-') {
      const eol = text.indexOf('\n', i);
      i = eol < 0 ? len : eol;
      continue;
    }
    if (c === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      i = end < 0 ? len : end + 2;
      continue;
    }
    break;
  }
  return text.slice(i);
}

function firstWord(body: string): string {
  const m = /^([A-Za-z]+)/.exec(body);
  return m ? m[1].toUpperCase() : '';
}

function secondWord(body: string): string {
  const m = /^[A-Za-z]+\s+([A-Za-z]+)/.exec(body);
  return m ? m[1].toUpperCase() : '';
}

function grabIdentifierAfter(body: string, prefixRe: RegExp): string {
  const re = new RegExp(prefixRe.source, prefixRe.flags.includes('i') ? prefixRe.flags : prefixRe.flags + 'i');
  const m = re.exec(body);
  if (!m) return '';
  return extractIdentifier(body.slice(m.index + m[0].length));
}

function grabIdentifierAfterWords(body: string): string {
  // For ALTER/DROP: drop the leading verb, then the object kind
  // (TABLE/INDEX/VIEW/...), plus optional modifiers, and extract the
  // next identifier.
  const rest = body.replace(/^\w+\s+/, '');
  const m = /^(\w+)\s+(?:IF\s+(?:NOT\s+)?EXISTS\s+)?(?:CONCURRENTLY\s+)?(?:ONLY\s+)?(.*)/i.exec(rest);
  if (!m) return '';
  return extractIdentifier(m[2]);
}

function prefixWithType(body: string, name: string, verb: 'ALTER' | 'DROP'): string {
  // For alter/drop, prefix the captured name with the object kind so the
  // outline reads "TABLE users" instead of just "users".
  const re = new RegExp(`^${verb}\\s+(\\w+)`, 'i');
  const m = re.exec(body);
  const kind = m ? m[1].toUpperCase() : '';
  if (!kind && !name) return '';
  if (!name) return kind;
  return `${kind} ${name}`;
}

function extractGrantSubject(body: string): string {
  // GRANT … ON [TABLE] <name> TO …  or  GRANT <role> TO <user>
  const m = /\bON\s+(?:TABLE\s+|SCHEMA\s+|SEQUENCE\s+|FUNCTION\s+|DATABASE\s+|ALL\s+TABLES\s+IN\s+SCHEMA\s+)?([\w."`[\]]+(?:\.[\w."`[\]]+)*)/i.exec(body);
  return m ? m[1] : '';
}

function extractIdentifier(s: string): string {
  const trimmed = s.trimStart();
  if (!trimmed) return '';

  const first = trimmed[0];
  if (first === '"' || first === '`' || first === '[') {
    const close = first === '[' ? ']' : first;
    const result = readQuotedIdent(trimmed, first, close);
    if (!result) return '';
    const rest = trimmed.slice(result.consumed);
    if (rest.startsWith('.')) {
      const tail = extractIdentifier(rest.slice(1));
      return tail ? result.text + '.' + tail : result.text;
    }
    return result.text;
  }
  const m = /^([\w$]+(?:\s*\.\s*[\w$]+)*)/.exec(trimmed);
  if (!m) return '';
  return m[1].replace(/\s+/g, '');
}

function readQuotedIdent(s: string, open: string, close: string): { text: string; consumed: number } | null {
  // Returns the bracketed identifier text (with the original quotes
  // included so the outline reads as written) and how many characters of
  // the input we consumed. Handles `""` and `]]` escapes.
  let i = 1;
  const len = s.length;
  while (i < len) {
    if (s[i] === close) {
      if (s[i + 1] === close) { i += 2; continue; }
      return { text: s.slice(0, i + 1), consumed: i + 1 };
    }
    i++;
  }
  return null;
}

function extractRoutineName(s: string): string {
  // FUNCTION/PROCEDURE names are followed by `(arg, arg, …)`. Strip the
  // arg list so the outline shows just the qualified name.
  const ident = extractIdentifier(s);
  return ident;
}
