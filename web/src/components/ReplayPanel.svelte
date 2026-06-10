<script lang="ts">
  import {
    fetchReplayList,
    fetchReplayShow,
    fetchReplayDiff,
    verifyReplayResponse,
    type EvidenceVerifyOK,
    type EvidenceVerifyResponse,
    type EvidenceVerifyViolation,
    type ReplayListResponse,
    type ReplayManifest,
    type ReplayDiffResponse,
  } from '../lib/api';
  import { formatTokens } from '../lib/format';
  import { route, navigate, toFileHash, toReplayHash } from '../lib/router.svelte';
  import { announce } from '../lib/announce.svelte';

  // -- list state -----------------------------------------------------------
  let list = $state<ReplayListResponse | null>(null);
  let listLoading = $state(false);
  let listError = $state<string | null>(null);

  // -- detail state ---------------------------------------------------------
  let detail = $state<ReplayManifest | null>(null);
  let detailLoading = $state(false);
  let detailError = $state<string | null>(null);
  let detailHeader: HTMLElement | null = $state(null);

  // -- diff state -----------------------------------------------------------
  // Diff is lazily fetched on first expand. Cache is keyed by snapshot id so
  // collapsing/re-expanding the same snapshot does not re-fire the request;
  // the strict toggle invalidates the cached entry. Switching to another
  // snapshot resets all diff state via the $effect below.
  let diffExpanded = $state(false);
  let diffStrict = $state(false);
  let diffLoading = $state(false);
  let diffError = $state<string | null>(null);
  let diffData = $state<ReplayDiffResponse | null>(null);
  // snapshot id this diff state belongs to (so cache is correct after route change)
  let diffOwner = $state<string>('');

  // -- response verification state -----------------------------------------
  let verifyExpanded = $state(false);
  let verifyResponse = $state('');
  let verifyCheckWorktree = $state(true);
  let verifyStrict = $state(false);
  let verifyLoading = $state(false);
  let verifyError = $state<string | null>(null);
  let verifyData = $state<EvidenceVerifyResponse | null>(null);
  let loadedReplayRoute = $state<string | null>(null);

  // route.path === '' -> list, otherwise -> detail for id = route.path
  let activeId = $derived(route.name === 'replay' ? route.path : '');

  function loadList() {
    listLoading = true;
    listError = null;
    fetchReplayList()
      .then((r) => {
        list = r;
        announce(
          r.snapshots.length === 1
            ? '1 snapshot'
            : `${r.snapshots.length} snapshots`,
        );
      })
      .catch((e: unknown) => {
        listError = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        listLoading = false;
      });
  }

  function loadDetail(id: string) {
    if (!id) return;
    detailLoading = true;
    detailError = null;
    detail = null;
    fetchReplayShow(id)
      .then((r) => {
        detail = r;
        announce(`Showing snapshot ${r.id}`);
        queueMicrotask(() => detailHeader?.focus());
      })
      .catch((e: unknown) => {
        detailError = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        detailLoading = false;
      });
  }

  function resetDiff(): void {
    diffExpanded = false;
    diffStrict = false;
    diffLoading = false;
    diffError = null;
    diffData = null;
    diffOwner = '';
  }

  function resetVerify(): void {
    verifyExpanded = false;
    verifyResponse = '';
    verifyCheckWorktree = true;
    verifyStrict = false;
    verifyLoading = false;
    verifyError = null;
    verifyData = null;
  }

  function loadDiff(id: string, strict: boolean): void {
    if (!id) return;
    diffLoading = true;
    diffError = null;
    diffOwner = id;
    announce(`Loading diff for ${id}`);
    fetchReplayDiff(id, { strict })
      .then((r) => {
        // Drop stale responses if the user navigated away mid-flight.
        if (diffOwner !== id) return;
        diffData = r;
        const added = r.changes.filter((c) => c.kind === 'added').length;
        const modified = r.changes.filter((c) => c.kind === 'modified').length;
        const removed = r.changes.filter((c) => c.kind === 'removed').length;
        announce(
          `Diff: ${added} added, ${modified} modified, ${removed} removed`,
        );
      })
      .catch((e: unknown) => {
        if (diffOwner !== id) return;
        diffError = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (diffOwner === id) diffLoading = false;
      });
  }

  function toggleDiff(): void {
    if (!activeId) return;
    if (diffExpanded) {
      diffExpanded = false;
      return;
    }
    diffExpanded = true;
    // First expand for this snapshot → fetch. Subsequent expands reuse cache.
    if (diffData === null && !diffLoading) {
      loadDiff(activeId, diffStrict);
    }
  }

  function toggleStrict(): void {
    diffStrict = !diffStrict;
    // Strict change invalidates the cached diff.
    diffData = null;
    diffError = null;
    if (diffExpanded && activeId) loadDiff(activeId, diffStrict);
  }

  function runReplayVerify(): void {
    if (!activeId || verifyLoading || !verifyResponse.trim()) return;
    const id = activeId;
    verifyLoading = true;
    verifyError = null;
    verifyReplayResponse({
      id,
      response: verifyResponse,
      check_worktree: verifyCheckWorktree,
      strict: verifyStrict,
    })
      .then((r) => {
        if (activeId !== id) return;
        verifyData = r;
        const issues = r.violations.length;
        announce(issues === 0 ? 'Response matches snapshot evidence' : `${issues} snapshot evidence issue${issues === 1 ? '' : 's'}`);
      })
      .catch((e: unknown) => {
        if (activeId !== id) return;
        verifyError = e instanceof Error ? e.message : String(e);
        verifyData = null;
      })
      .finally(() => {
        if (activeId === id) verifyLoading = false;
      });
  }

  function diffSummary(d: ReplayDiffResponse): string {
    const added = d.changes.filter((c) => c.kind === 'added').length;
    const modified = d.changes.filter((c) => c.kind === 'modified').length;
    const removed = d.changes.filter((c) => c.kind === 'removed').length;
    const delta = d.total_token_delta;
    const sign = delta > 0 ? '+' : delta < 0 ? '−' : '±';
    return `+${added} added · ${modified} modified · ${removed} removed · unchanged: ${d.unchanged_count} · total token delta: ${sign}${formatTokens(Math.abs(delta))}`;
  }

  function signedTokens(n: number): string {
    if (n === 0) return '±0';
    const sign = n > 0 ? '+' : '−';
    return `${sign}${formatTokens(Math.abs(n))}`;
  }

  function refLabel(ref: EvidenceVerifyOK | EvidenceVerifyViolation): string {
    if (ref.path) {
      if (ref.line_start) {
        const end = ref.line_end && ref.line_end !== ref.line_start ? `-${ref.line_end}` : '';
        return `${ref.path}:${ref.line_start}${end}`;
      }
      return ref.path;
    }
    if (ref.symbol) return ref.symbol;
    return ref.kind;
  }

  function hiddenCount(total: number, shown: number): number {
    return Math.max(0, total - shown);
  }

  function deltaColor(n: number): string {
    if (n > 0) return 'var(--ctx-git-added)';
    if (n < 0) return 'var(--ctx-git-deleted)';
    return 'var(--ctx-fg-dim)';
  }

  // Load (or reload) when the route segment under #/replay changes.
  $effect(() => {
    if (route.name !== 'replay') return;
    const id = activeId;
    if (loadedReplayRoute === id) return;
    loadedReplayRoute = id;
    if (id === '') {
      // list view — only fetch on first mount or after explicit reload
      if (list === null && !listLoading && !listError) loadList();
      // leaving detail view → drop diff state so it never bleeds across snapshots
      resetDiff();
      resetVerify();
    } else {
      loadDetail(id);
      // Switching snapshots invalidates the previous diff.
      resetDiff();
      resetVerify();
    }
  });

  // ---------------------------------------------------------------------------
  // helpers
  // ---------------------------------------------------------------------------

  /** Relative time formatter. Falls back to absolute YYYY-MM-DD for old dates. */
  function relTime(iso: string): string {
    const t = Date.parse(iso);
    if (Number.isNaN(t)) return iso;
    const diffMs = Date.now() - t;
    const sec = Math.floor(diffMs / 1000);
    if (sec < 60) return 'just now';
    const min = Math.floor(sec / 60);
    if (min < 60) return min === 1 ? '1 minute ago' : `${min} minutes ago`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return hr === 1 ? '1 hour ago' : `${hr} hours ago`;
    const day = Math.floor(hr / 24);
    if (day < 30) return day === 1 ? '1 day ago' : `${day} days ago`;
    const d = new Date(t);
    const y = d.getFullYear();
    const m = String(d.getMonth() + 1).padStart(2, '0');
    const dd = String(d.getDate()).padStart(2, '0');
    return `${y}-${m}-${dd}`;
  }

  function shortHash(h: string): string {
    if (!h) return '';
    return h.length > 8 ? `${h.slice(0, 8)}…` : h;
  }

  function goBack() {
    navigate(toReplayHash());
  }

  function goDetail(id: string) {
    navigate(toReplayHash(id));
  }
</script>

{#if activeId === ''}
  <!-- ============================ LIST VIEW ============================ -->
  <section class="replay" aria-label="replay snapshots">
    <header>
      <h2>
        Replay snapshots
        {#if list}<span class="muted count">({list.snapshots.length})</span>{/if}
      </h2>
      <button onclick={loadList} aria-label="reload snapshot list">Reload</button>
    </header>

    {#if list}
      <p class="muted store mono" title={list.store_path}>store: {list.store_path}</p>
    {/if}

    {#if listLoading && !list}
      <p class="muted" aria-busy="true">Loading…</p>
    {:else if listError}
      <div class="error">
        <p>Snapshot list load failed.</p>
        <code class="mono">{listError}</code>
        <button onclick={loadList}>Retry</button>
      </div>
    {:else if list}
      {#if list.snapshots.length === 0}
        <p class="muted empty">
          No snapshots found. Run <code class="mono">ctx pack . --snapshot &lt;id&gt;</code> to create one.
        </p>
      {:else}
        <ul class="snapshots" role="list">
          {#each list.snapshots as s, si (`${si}:${s.id}`)}
            <li>
              <button
                type="button"
                class="snap-row"
                onclick={() => goDetail(s.id)}
                aria-label={`Open snapshot ${s.id}`}
              >
                <span class="snap-id mono">{s.id}</span>
                <span class="snap-time muted" title={s.created_at}>{relTime(s.created_at)}</span>
                {#if s.goal}
                  <span class="snap-goal" title={s.goal}>{s.goal}</span>
                {:else}
                  <span class="snap-goal muted">(no goal)</span>
                {/if}
                <span class="snap-meta muted">
                  {s.file_count} files · {formatTokens(s.used)} / {formatTokens(s.budget)} tokens
                </span>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>
{:else}
  <!-- ============================ DETAIL VIEW ========================== -->
  <section class="replay" aria-label={`snapshot ${activeId}`}>
    <header class="detail-head">
      <button type="button" class="back" onclick={goBack} aria-label="Back to snapshot list">
        ← Back
      </button>
      <h2
        bind:this={detailHeader}
        tabindex="-1"
        class="mono"
      >Snapshot {activeId}</h2>
      {#if detail}
        <span class="muted" title={detail.created_at}>{relTime(detail.created_at)}</span>
      {/if}
    </header>

    {#if detailLoading && !detail}
      <p class="muted" aria-busy="true">Loading snapshot…</p>
    {:else if detailError}
      <div class="error">
        <p>Snapshot load failed.</p>
        <code class="mono">{detailError}</code>
        <button onclick={() => loadDetail(activeId)}>Retry</button>
      </div>
    {:else if detail}
      <dl class="meta" aria-label="snapshot metadata">
        <div><dt>schema</dt><dd>{detail.schema_version}</dd></div>
        <div><dt>ctx version</dt><dd class="mono">{detail.ctx_version}</dd></div>
        <div><dt>format</dt><dd>{detail.format}</dd></div>
        {#if detail.preset}
          <div><dt>preset</dt><dd>{detail.preset}</dd></div>
        {/if}
        <div><dt>budget</dt><dd>{formatTokens(detail.budget)}</dd></div>
        <div><dt>used</dt><dd>{formatTokens(detail.used)}</dd></div>
        <div class="wide"><dt>root</dt><dd class="mono" title={detail.root}>{detail.root}</dd></div>
        {#if detail.goal}
          <div class="wide"><dt>goal</dt><dd>{detail.goal}</dd></div>
        {/if}
        {#if detail.out_sha256}
          <div class="wide">
            <dt>out sha256</dt>
            <dd class="mono" title={detail.out_sha256}>{shortHash(detail.out_sha256)}</dd>
          </div>
        {/if}
      </dl>

      <section class="entries-wrap" aria-label="snapshot entries">
        <h3>Entries <span class="muted">({detail.entries.length})</span></h3>
        <div class="scroll">
          <table class="entries">
            <thead>
              <tr>
                <th scope="col" class="col-path">path</th>
                <th scope="col" class="col-tokens">tokens</th>
                <th scope="col" class="col-rel">relevance</th>
                <th scope="col" class="col-score">score</th>
                <th scope="col" class="col-reason">reason</th>
                <th scope="col" class="col-sha">sha256</th>
              </tr>
            </thead>
            <tbody>
              {#each detail.entries as e, ei (`${ei}:${e.path}`)}
                <tr>
                  <td class="col-path">
                    <button
                      type="button"
                      class="path-btn mono"
                      onclick={() => navigate(toFileHash(e.path))}
                      aria-label={`Open ${e.path}`}
                    >{e.path}</button>
                  </td>
                  <td class="col-tokens mono">{formatTokens(e.tokens)}</td>
                  <td class="col-rel">{e.relevance ?? '—'}</td>
                  <td class="col-score mono">{e.score !== undefined ? e.score.toFixed(2) : '—'}</td>
                  <td class="col-reason muted">{e.reason ?? ''}</td>
                  <td class="col-sha mono" title={e.sha256}>{shortHash(e.sha256)}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        </div>
      </section>

      {#if detail.skipped && detail.skipped.length > 0}
        <section class="entries-wrap" aria-label="snapshot skipped">
          <h3>Skipped <span class="muted">({detail.skipped.length})</span></h3>
          <div class="scroll skipped">
            <table class="entries">
              <thead>
                <tr>
                  <th scope="col" class="col-path">path</th>
                  <th scope="col" class="col-reason">reason</th>
                </tr>
              </thead>
              <tbody>
                {#each detail.skipped as s, ski (`${ski}:${s.path}`)}
                  <tr>
                    <td class="col-path mono">{s.path}</td>
                    <td class="col-reason muted">{s.reason}</td>
                  </tr>
                {/each}
              </tbody>
            </table>
          </div>
        </section>
      {/if}

      <section class="entries-wrap verify-wrap" aria-label="verify response against snapshot">
        <div class="diff-head">
          <button
            type="button"
            class="diff-toggle"
            onclick={() => (verifyExpanded = !verifyExpanded)}
            aria-expanded={verifyExpanded}
            aria-controls="verify-body"
          >
            <span class="caret" aria-hidden="true">{verifyExpanded ? '▾' : '▸'}</span>
            Verify response against snapshot
          </button>
          {#if verifyData}
            <span class="verify-pill" class:verify-pass={verifyData.exit_code === 0} class:verify-fail={verifyData.exit_code !== 0}>
              {verifyData.violations.length} issues
            </span>
          {/if}
        </div>

        {#if verifyExpanded}
          <div id="verify-body" class="verify-body" aria-busy={verifyLoading}>
            <form class="verify-form" onsubmit={(event) => { event.preventDefault(); runReplayVerify(); }}>
              <label>
                <span>LLM response</span>
                <textarea bind:value={verifyResponse} rows="5" spellcheck="false"></textarea>
              </label>
              <div class="verify-actions">
                <div class="verify-checks">
                  <label>
                    <input type="checkbox" bind:checked={verifyCheckWorktree} />
                    <span>worktree</span>
                  </label>
                  <label>
                    <input type="checkbox" bind:checked={verifyStrict} />
                    <span>strict</span>
                  </label>
                </div>
                <div class="verify-buttons">
                  <button type="button" disabled={verifyLoading} onclick={resetVerify}>Clear</button>
                  <button type="submit" disabled={verifyLoading || !verifyResponse.trim()}>
                    {verifyLoading ? 'Verifying…' : 'Verify'}
                  </button>
                </div>
              </div>
            </form>

            {#if verifyError}
              <div class="error compact">
                <p>Verification failed.</p>
                <code class="mono">{verifyError}</code>
              </div>
            {/if}

            {#if verifyData}
              <div class="verify-result" class:verify-pass={verifyData.exit_code === 0} class:verify-fail={verifyData.exit_code !== 0}>
                <p class="verify-summary muted">
                  {verifyData.violations.length} issues · {verifyData.ok.length} ok · {verifyData.stale_files.length} stale · {verifyData.references_found} refs · {verifyData.total_files_in_contract} files
                </p>
                {#if verifyData.violations.length > 0}
                  <ul class="verify-list">
                    {#each verifyData.violations.slice(0, 6) as v}
                      <li>
                        <span class="kind-badge kind-modified">{v.kind}</span>
                        <span class="mono">{refLabel(v)}</span>
                        {#if v.message}<span class="muted">{v.message}</span>{/if}
                      </li>
                    {/each}
                  </ul>
                  {#if hiddenCount(verifyData.violations.length, 6)}
                    <p class="muted verify-more">+{hiddenCount(verifyData.violations.length, 6)} more issues</p>
                  {/if}
                {:else if verifyData.ok.length > 0}
                  <ul class="verify-list">
                    {#each verifyData.ok.slice(0, 6) as ok}
                      <li>
                        <span class="kind-badge kind-added">{ok.kind}</span>
                        <span class="mono">{refLabel(ok)}</span>
                      </li>
                    {/each}
                  </ul>
                  {#if hiddenCount(verifyData.ok.length, 6)}
                    <p class="muted verify-more">+{hiddenCount(verifyData.ok.length, 6)} more ok</p>
                  {/if}
                {/if}
                {#if verifyData.repack_suggestions.length > 0}
                  <p class="muted verify-more">
                    Repack: {verifyData.repack_suggestions.slice(0, 4).join(', ')}
                    {#if hiddenCount(verifyData.repack_suggestions.length, 4)}
                      +{hiddenCount(verifyData.repack_suggestions.length, 4)}
                    {/if}
                  </p>
                {/if}
              </div>
            {/if}
          </div>
        {/if}
      </section>

      <!-- ============================ DIFF SECTION ===================== -->
      <section
        class="entries-wrap diff-wrap"
        aria-label="diff against current working tree"
      >
        <div class="diff-head">
          <button
            type="button"
            class="diff-toggle"
            onclick={toggleDiff}
            aria-expanded={diffExpanded}
            aria-controls="diff-body"
          >
            <span class="caret" aria-hidden="true">{diffExpanded ? '▾' : '▸'}</span>
            Compare with current working tree
          </button>
          <label class="strict-toggle" title="Compare file content sha256 strictly (vs. token-only).">
            <input
              type="checkbox"
              checked={diffStrict}
              onchange={toggleStrict}
              aria-label="strict diff mode"
            />
            <span>strict</span>
          </label>
        </div>

        {#if diffExpanded}
          <div id="diff-body" class="diff-body" aria-busy={diffLoading}>
            {#if diffLoading}
              <p class="muted" aria-busy="true">Loading diff…</p>
            {:else if diffError}
              <div class="error">
                <p>Diff load failed.</p>
                <code class="mono">{diffError}</code>
                <button onclick={() => loadDiff(activeId, diffStrict)}>Retry</button>
              </div>
            {:else if diffData}
              <p class="diff-summary muted">{diffSummary(diffData)}</p>
              {#if diffData.truncated}
                <p class="diff-trunc warn">
                  Showing first {diffData.changes.length} of many changes.
                  Run <code class="mono">ctx replay diff {activeId}</code> in CLI for the full diff.
                </p>
              {/if}

              {#if diffData.changes.length === 0}
                <p class="muted empty">
                  No differences. Snapshot matches the current working tree.
                </p>
              {:else}
                <div class="scroll">
                  <table class="entries diff-table">
                    <thead>
                      <tr>
                        <th scope="col" class="col-kind">kind</th>
                        <th scope="col" class="col-path">path</th>
                        <th scope="col" class="col-tokens">tokens</th>
                        <th scope="col" class="col-delta">Δ</th>
                      </tr>
                    </thead>
                    <tbody>
                      {#each diffData.changes as c, ci (`${ci}:${c.path}:${c.kind}`)}
                        <tr class="diff-row">
                          <td class="col-kind">
                            <span class="kind-badge kind-{c.kind}">{c.kind}</span>
                          </td>
                          <td class="col-path">
                            {#if c.kind === 'removed'}
                              <span
                                class="path-disabled mono"
                                aria-disabled="true"
                                title="File no longer exists in the working tree"
                              >{c.path}</span>
                            {:else}
                              <button
                                type="button"
                                class="path-btn mono"
                                onclick={() => navigate(toFileHash(c.path))}
                                aria-label={`Open ${c.path}`}
                              >{c.path}</button>
                            {/if}
                          </td>
                          <td class="col-tokens mono">
                            <span class="tok-base">{formatTokens(c.base_tokens ?? 0)}</span>
                            <span class="tok-arrow" aria-hidden="true">→</span>
                            <span class="tok-curr">{formatTokens(c.current_tokens ?? 0)}</span>
                          </td>
                          <td
                            class="col-delta mono"
                            style="color: {deltaColor(c.tokens_delta)};"
                          >{signedTokens(c.tokens_delta)}</td>
                        </tr>
                      {/each}
                    </tbody>
                  </table>
                </div>
              {/if}
            {/if}
          </div>
        {/if}
      </section>
    {/if}
  </section>
{/if}

<style>
  .replay {
    padding: 16px 20px;
    display: flex;
    flex-direction: column;
    gap: 14px;
    max-width: 1200px;
  }
  header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    flex-wrap: wrap;
  }
  header.detail-head {
    justify-content: flex-start;
  }
  h2 {
    margin: 0;
    font-size: 18px;
  }
  h2:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 2px;
    border-radius: 3px;
  }
  h3 {
    margin: 0 0 6px;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  .count {
    font-size: 13px;
    font-weight: 400;
  }
  .store {
    margin: 0;
    font-size: 11px;
    word-break: break-all;
  }
  .empty code {
    background: var(--ctx-bg-elev);
    border-radius: 3px;
    padding: 1px 6px;
  }

  /* list */
  .snapshots {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .snap-row {
    width: 100%;
    text-align: left;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 8px 12px;
    display: grid;
    grid-template-columns: minmax(180px, auto) auto 1fr auto;
    gap: 4px 14px;
    background: var(--ctx-bg-panel);
    color: inherit;
    cursor: pointer;
    font-size: 12px;
    align-items: baseline;
  }
  .snap-row:hover {
    background: var(--ctx-bg-elev);
  }
  .snap-row:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  .snap-id {
    color: var(--ctx-link);
    font-weight: 600;
    word-break: break-all;
  }
  .snap-time {
    font-size: 11px;
    white-space: nowrap;
  }
  .snap-goal {
    color: var(--ctx-fg);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    min-width: 0;
  }
  .snap-meta {
    font-size: 11px;
    white-space: nowrap;
    justify-self: end;
  }

  /* detail */
  .detail-head {
    align-items: center;
    gap: 10px;
  }
  .back {
    font-size: 12px;
    padding: 2px 10px;
  }
  .meta {
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 8px 16px;
    padding: 10px 12px;
    background: var(--ctx-bg-panel);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
  }
  .meta > div {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
  }
  .meta > div.wide {
    grid-column: 1 / -1;
  }
  .meta dt {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  .meta dd {
    margin: 0;
    color: var(--ctx-fg);
    word-break: break-all;
    font-size: 12px;
  }

  .entries-wrap {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .scroll {
    max-height: 420px;
    overflow: auto;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    background: var(--ctx-bg-panel);
  }
  .scroll.skipped {
    max-height: 220px;
  }
  table.entries {
    width: 100%;
    border-collapse: collapse;
    font-size: 12px;
  }
  table.entries th,
  table.entries td {
    text-align: left;
    padding: 4px 8px;
    vertical-align: top;
    border-bottom: 1px solid var(--ctx-border);
  }
  table.entries thead th {
    position: sticky;
    top: 0;
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg-dim);
    text-transform: uppercase;
    letter-spacing: 0.05em;
    font-size: 10px;
    font-weight: 500;
    z-index: 1;
  }
  table.entries tbody tr:hover {
    background: var(--ctx-bg-elev);
  }
  .path-btn {
    border: 0;
    background: transparent;
    color: var(--ctx-link);
    padding: 0;
    cursor: pointer;
    text-align: left;
    word-break: break-all;
    font-size: 12px;
  }
  .path-btn:hover {
    text-decoration: underline;
  }
  .path-btn:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  .col-tokens,
  .col-score {
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .col-sha {
    white-space: nowrap;
    color: var(--ctx-fg-dim);
  }
  .col-reason {
    font-size: 11px;
  }

  .error {
    padding: 12px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
  }
  .error code {
    display: block;
    margin: 6px 0;
    color: var(--ctx-err);
    font-size: 11px;
    word-break: break-all;
  }

  /* diff section */
  .diff-head {
    display: flex;
    align-items: center;
    gap: 12px;
    flex-wrap: wrap;
  }
  .diff-toggle {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 1px solid var(--ctx-border);
    background: var(--ctx-bg-panel);
    color: inherit;
    padding: 4px 12px;
    border-radius: 4px;
    cursor: pointer;
    font: inherit;
    font-size: 12px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
  }
  .diff-toggle:hover {
    background: var(--ctx-bg-elev);
  }
  .diff-toggle:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  .caret {
    color: var(--ctx-fg-dim);
    font-size: 10px;
  }
  .strict-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    font-size: 11px;
    color: var(--ctx-fg-dim);
    cursor: pointer;
    user-select: none;
  }
  .strict-toggle input {
    cursor: pointer;
  }
  .strict-toggle:focus-within {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 2px;
    border-radius: 3px;
  }
  .diff-body {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .verify-body {
    margin-top: 8px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    background: var(--ctx-bg-panel);
    padding: 10px 12px;
  }
  .verify-form {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .verify-form label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 11px;
    color: var(--ctx-fg-dim);
  }
  .verify-form textarea {
    box-sizing: border-box;
    width: 100%;
    min-height: 96px;
    resize: vertical;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 8px;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    font: 12px/1.4 var(--ctx-mono);
  }
  .verify-form textarea:focus {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .verify-actions {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
  }
  .verify-checks,
  .verify-buttons {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .verify-checks label {
    flex-direction: row;
    align-items: center;
    gap: 4px;
    cursor: pointer;
    user-select: none;
  }
  .verify-checks input {
    margin: 0;
  }
  .verify-buttons button {
    border: 1px solid var(--ctx-border);
    background: var(--ctx-bg-elev);
    color: inherit;
    border-radius: 4px;
    padding: 4px 10px;
    font-size: 12px;
    cursor: pointer;
  }
  .verify-buttons button:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .verify-result {
    border-left: 2px solid var(--ctx-border);
    padding-left: 10px;
  }
  .verify-result.verify-pass {
    border-left-color: var(--ctx-accent);
  }
  .verify-result.verify-fail {
    border-left-color: var(--ctx-warn);
  }
  .verify-summary {
    margin: 0 0 6px;
    font-size: 11px;
  }
  .verify-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .verify-list li {
    display: grid;
    grid-template-columns: auto minmax(160px, 1fr) minmax(120px, 2fr);
    align-items: baseline;
    gap: 8px;
    font-size: 11px;
  }
  .verify-more {
    margin: 6px 0 0;
    font-size: 11px;
  }
  .verify-pill {
    border: 1px solid var(--ctx-border);
    border-radius: 999px;
    padding: 1px 8px;
    font-size: 11px;
    color: var(--ctx-fg-dim);
  }
  .verify-pill.verify-pass {
    color: var(--ctx-accent);
    border-color: color-mix(in srgb, var(--ctx-accent) 50%, var(--ctx-border));
  }
  .verify-pill.verify-fail {
    color: var(--ctx-warn);
    border-color: color-mix(in srgb, var(--ctx-warn) 55%, var(--ctx-border));
  }
  .error.compact {
    padding: 8px 10px;
  }
  .error.compact p {
    margin: 0;
  }
  .diff-summary {
    font-size: 11px;
    margin: 0;
  }
  .diff-trunc.warn {
    font-size: 11px;
    color: var(--ctx-warn);
    margin: 0;
  }
  .diff-trunc code {
    background: var(--ctx-bg-elev);
    border-radius: 3px;
    padding: 0 4px;
  }
  .empty {
    font-size: 12px;
  }
  .diff-table .col-kind {
    width: 92px;
    white-space: nowrap;
  }
  .diff-table .col-delta {
    width: 84px;
    text-align: right;
    white-space: nowrap;
    font-variant-numeric: tabular-nums;
  }
  .kind-badge {
    display: inline-block;
    padding: 1px 6px;
    border-radius: 3px;
    font-size: 10px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    line-height: 1.4;
  }
  .kind-added {
    color: var(--ctx-bg);
    background: var(--ctx-git-added);
  }
  .kind-modified {
    color: var(--ctx-bg);
    background: var(--ctx-git-modified);
  }
  .kind-removed {
    color: var(--ctx-bg);
    background: var(--ctx-git-deleted);
  }
  .path-disabled {
    color: var(--ctx-fg-dim);
    text-decoration: line-through;
    word-break: break-all;
    font-size: 12px;
    cursor: not-allowed;
  }
  .tok-arrow {
    color: var(--ctx-fg-dim);
    margin: 0 4px;
  }
  .tok-base {
    color: var(--ctx-fg-dim);
  }

  @media (max-width: 600px) {
    .snap-row {
      grid-template-columns: 1fr;
      gap: 2px;
    }
    .snap-meta {
      justify-self: start;
    }
    .meta {
      grid-template-columns: 1fr;
    }
    .diff-head {
      align-items: flex-start;
    }
    .diff-table .col-kind {
      width: auto;
    }
    /* On narrow screens, let the table rows wrap into a vertical stack so
       long paths and the tokens before→after column do not overflow. */
    .diff-table,
    .diff-table thead,
    .diff-table tbody,
    .diff-table tr,
    .diff-table td {
      display: block;
      width: 100%;
    }
    .diff-table thead {
      display: none;
    }
    .diff-table tr.diff-row {
      border-bottom: 1px solid var(--ctx-border);
      padding: 6px 8px;
    }
    .diff-table td {
      border: 0;
      padding: 2px 0;
    }
    .diff-table .col-delta {
      text-align: left;
    }
    .verify-actions {
      align-items: flex-start;
    }
    .verify-list li {
      grid-template-columns: 1fr;
      gap: 2px;
    }
  }
</style>
