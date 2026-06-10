<script lang="ts">
  import {
    extractSqlInsights,
    type SqlStatementKind,
  } from '../lib/sql-insights';

  let {
    content,
    onJump,
  }: {
    content: string;
    onJump?: (line: number) => void;
  } = $props();

  let insights = $derived(extractSqlInsights(content));

  // Short glyph per statement kind. Three-letter caps keep the column
  // alignable without needing icons; CREATE variants are abbreviated to
  // keep the row compact and readable next to the target name.
  function kindGlyph(k: SqlStatementKind): string {
    switch (k) {
      case 'create_table': return 'TBL';
      case 'create_index': return 'IDX';
      case 'create_view':
      case 'create_materialized_view': return 'VW';
      case 'create_function': return 'FN';
      case 'create_procedure': return 'PROC';
      case 'create_trigger': return 'TRG';
      case 'create_type': return 'TYP';
      case 'create_schema': return 'SCM';
      case 'create_extension': return 'EXT';
      case 'create_sequence': return 'SEQ';
      case 'create_database': return 'DB';
      case 'create_role': return 'ROL';
      case 'create_other': return 'NEW';
      case 'alter': return 'ALT';
      case 'drop': return 'DRP';
      case 'insert': return 'INS';
      case 'update': return 'UPD';
      case 'delete': return 'DEL';
      case 'select': return 'SEL';
      case 'with': return 'CTE';
      case 'merge': return 'MRG';
      case 'truncate': return 'TRU';
      case 'comment': return '#';
      case 'grant': return 'GRT';
      case 'revoke': return 'RVK';
      case 'begin': return 'BEG';
      case 'commit': return 'COM';
      case 'rollback': return 'RBK';
      case 'savepoint': return 'SVP';
      case 'set': return 'SET';
      case 'do': return 'DO';
      case 'use': return 'USE';
      case 'call': return 'CALL';
      case 'analyze': return 'ANA';
      case 'vacuum': return 'VAC';
      case 'explain': return 'EXP';
      case 'pragma': return 'PRG';
      case 'other': return '·';
    }
  }
</script>

<div class="insights">
  {#if !insights.ok}
    <section>
      <p class="muted">No SQL statements found.</p>
    </section>
  {:else}
    {#if insights.statements.length > 0}
      <section aria-label="SQL statements">
        <h3>
          Statements <span class="count muted">{insights.statements.length}{insights.truncated ? `/${insights.totalStatements}` : ''}</span>
        </h3>
        <ul>
          {#each insights.statements as s, i (`${s.line}:${i}`)}
            <li>
              <button
                type="button"
                class="row stmt-row"
                title={s.target || s.kind}
                aria-label={`Jump to ${s.kind} ${s.target} on line ${s.line}`}
                onclick={() => onJump?.(s.line)}
              >
                <span class="glyph muted" aria-hidden="true">{kindGlyph(s.kind)}</span>
                <span class="target mono">{s.target || '—'}</span>
                <span class="line muted">L{s.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.tables.length > 0}
      <section aria-label="tables">
        <h3>Tables <span class="count muted">{insights.tables.length}</span></h3>
        <ul>
          {#each insights.tables as t, i (`${t.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row name-row"
                aria-label={`Jump to table ${t.name} on line ${t.line}`}
                onclick={() => onJump?.(t.line)}
              >
                <span class="target mono">{t.name}</span>
                <span class="line muted">L{t.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.indexes.length > 0}
      <section aria-label="indexes">
        <h3>Indexes <span class="count muted">{insights.indexes.length}</span></h3>
        <ul>
          {#each insights.indexes as idx, i (`${idx.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row dep-row"
                title={idx.target ? `ON ${idx.target}` : undefined}
                aria-label={`Jump to index ${idx.name} on line ${idx.line}`}
                onclick={() => onJump?.(idx.line)}
              >
                <span class="target mono">{idx.name}</span>
                <span class="value muted mono">{idx.target ? `ON ${idx.target}` : ''}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.views.length > 0}
      <section aria-label="views">
        <h3>Views <span class="count muted">{insights.views.length}</span></h3>
        <ul>
          {#each insights.views as v, i (`${v.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row name-row"
                aria-label={`Jump to view ${v.name} on line ${v.line}`}
                onclick={() => onJump?.(v.line)}
              >
                <span class="target mono">{v.name}</span>
                <span class="line muted">L{v.line}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}

    {#if insights.routines.length > 0}
      <section aria-label="routines">
        <h3>Routines <span class="count muted">{insights.routines.length}</span></h3>
        <ul>
          {#each insights.routines as r, i (`${r.name}:${i}`)}
            <li>
              <button
                type="button"
                class="row dep-row"
                aria-label={`Jump to ${r.kind} ${r.name} on line ${r.line}`}
                onclick={() => onJump?.(r.line)}
              >
                <span class="target mono">{r.name}</span>
                <span class="value muted mono">{r.kind}</span>
              </button>
            </li>
          {/each}
        </ul>
      </section>
    {/if}
  {/if}
</div>

<style>
  .insights {
    display: flex;
    flex-direction: column;
  }
  section {
    padding: 10px 12px;
    border-block-start: 1px solid var(--ctx-border);
  }
  section:first-child {
    border-block-start: 0;
  }
  h3 {
    margin: 0 0 8px;
    font-size: 11px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    padding: 0;
  }
  .row {
    display: grid;
    align-items: baseline;
    gap: 6px;
    width: 100%;
    padding: 3px 4px;
    font-size: 11px;
    text-align: start;
    border: 0;
    background: transparent;
    color: var(--ctx-fg);
    border-radius: 3px;
    cursor: pointer;
  }
  .row:hover {
    background: var(--ctx-bg-elev);
  }
  .row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  /* Statement row: glyph | target | line. */
  .stmt-row {
    grid-template-columns: 32px 1fr auto;
  }
  .stmt-row .glyph {
    font-size: 10px;
    text-align: center;
    font-weight: 600;
  }
  .stmt-row .target,
  .name-row .target,
  .dep-row .target {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .stmt-row .line {
    font-size: 10px;
  }
  /* Name-only rows (tables, views): target | line. */
  .name-row {
    grid-template-columns: 1fr auto;
  }
  .name-row .line {
    font-size: 10px;
  }
  /* Two-column rows (indexes, routines): target | qualifier. */
  .dep-row {
    grid-template-columns: 1fr 1fr;
  }
  .dep-row .value {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 10px;
    text-align: end;
  }
</style>
