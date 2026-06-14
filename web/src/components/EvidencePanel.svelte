<script lang="ts">
  import {
    fetchEvidence,
    verifyEvidence,
    type EvidenceResponse,
    type EvidenceVerifyOK,
    type EvidenceVerifyResponse,
    type EvidenceVerifyViolation,
  } from '../lib/api';
  import { formatTokens } from '../lib/format';
  import { announce } from '../lib/announce.svelte';

  let { path }: { path: string } = $props();

  let data = $state<EvidenceResponse | null>(null);
  let loading = $state(false);
  let error = $state<string | null>(null);
  let packText = $state('');
  let responseText = $state('');
  let checkWorktree = $state(true);
  let strictMode = $state(false);
  let verifyLoading = $state(false);
  let verifyError = $state<string | null>(null);
  let verifyResult = $state<EvidenceVerifyResponse | null>(null);
  let verifyOpen = $state(false);

  $effect(() => {
    const target = path;
    if (!target) {
      data = null;
      return;
    }
    loading = true;
    error = null;
    let cancelled = false;
    fetchEvidence(target)
      .then((r) => {
        if (!cancelled) data = r;
      })
      .catch((e: unknown) => {
        if (!cancelled) error = e instanceof Error ? e.message : String(e);
      })
      .finally(() => {
        if (!cancelled) loading = false;
      });
    return () => {
      cancelled = true;
    };
  });

  function relTime(iso: string): string {
    const t = Date.parse(iso);
    if (Number.isNaN(t)) return iso;
    const diffMs = Date.now() - t;
    const min = Math.floor(diffMs / 60000);
    if (min < 1) return 'just now';
    if (min < 60) return `${min}m ago`;
    const hr = Math.floor(min / 60);
    if (hr < 24) return `${hr}h ago`;
    const day = Math.floor(hr / 24);
    if (day < 30) return `${day}d ago`;
    return new Date(t).toISOString().slice(0, 10);
  }

  function shortSHA(s?: string): string {
    if (!s) return '';
    return s.length > 8 ? s.slice(0, 8) : s;
  }

  function signedTokens(n?: number): string {
    if (!n) return '±0';
    const sign = n > 0 ? '+' : '−';
    return `${sign}${formatTokens(Math.abs(n))}`;
  }

  function statusLabel(status: string): string {
    switch (status) {
      case 'fresh':
        return 'verified';
      case 'stale':
        return 'stale';
      case 'missing':
        return 'missing';
      case 'no-store':
        return 'no replay';
      case 'no-evidence':
        return 'not packed';
      default:
        return status;
    }
  }

  function resetVerify() {
    packText = '';
    responseText = '';
    verifyError = null;
    verifyResult = null;
  }

  async function runVerify() {
    if (verifyLoading || !packText.trim() || !responseText.trim()) return;
    verifyLoading = true;
    verifyError = null;
    try {
      verifyResult = await verifyEvidence({
        pack: packText,
        response: responseText,
        check_worktree: checkWorktree,
        strict: strictMode,
      });
      const issues = verifyResult.violations.length;
      announce(issues === 0 ? 'Response verified' : `${issues} verification issue${issues === 1 ? '' : 's'}`);
    } catch (e: unknown) {
      verifyError = e instanceof Error ? e.message : String(e);
      verifyResult = null;
    } finally {
      verifyLoading = false;
    }
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
</script>

<div class="evidence" aria-label="evidence ledger">
  <section>
    <h3>
      Evidence
      {#if data}
        <span class={`pill ${data.status}`}>{statusLabel(data.status)}</span>
      {/if}
    </h3>

    {#if loading && !data}
      <p class="muted">Loading…</p>
    {:else if error}
      <p class="muted">Could not load evidence.</p>
      <code class="mono err">{error}</code>
    {:else if data}
      {#if data.snapshots.length === 0}
        <p class="muted">
          {data.status === 'no-store'
            ? 'No replay snapshots found.'
            : 'This file has not appeared in a replay snapshot yet.'}
        </p>
      {:else}
        <p class="summary muted">
          {data.snapshots.length}{data.total_snapshots > data.snapshots.length ? `/${data.total_snapshots}` : ''}
          pack evidence record{data.total_snapshots === 1 ? '' : 's'}
        </p>
        <ul>
          {#each data.snapshots as snap (snap.id)}
            <li class={snap.status}>
              <div class="row" title={snap.goal || snap.id}>
                <span class={`dot ${snap.status}`} aria-hidden="true"></span>
                <span class="main">
                  <span class="id mono">{snap.id}</span>
                  <span class="time muted">{relTime(snap.created_at)}</span>
                </span>
                <span class="status muted">{statusLabel(snap.status)}</span>
              </div>
              <div class="meta muted">
                <span class="mono">{shortSHA(snap.pack_sha256)}</span>
                {#if snap.current_sha256}
                  <span class="mono">→ {shortSHA(snap.current_sha256)}</span>
                {/if}
                <span>{formatTokens(snap.tokens)}</span>
                {#if snap.token_delta}
                  <span class:pos={snap.token_delta > 0} class:neg={snap.token_delta < 0}>
                    {signedTokens(snap.token_delta)}
                  </span>
                {/if}
              </div>
              {#if snap.goal || snap.reason || snap.message}
                <p class="note muted">
                  {snap.message || snap.reason || snap.goal}
                </p>
              {/if}
            </li>
          {/each}
        </ul>
      {/if}
    {/if}
  </section>

  <section>
    <button
      type="button"
      class="section-toggle"
      aria-expanded={verifyOpen}
      onclick={() => (verifyOpen = !verifyOpen)}
    >
      <h3>Verify response</h3>
      {#if verifyResult}
        <span class={`pill ${verifyResult.exit_code === 0 ? 'fresh' : 'stale'}`}>
          {verifyResult.violations.length} issues
        </span>
      {/if}
      <span class="chev" aria-hidden="true">{verifyOpen ? '−' : '+'}</span>
    </button>

    {#if verifyOpen}
      <form class="verify" onsubmit={(event) => { event.preventDefault(); void runVerify(); }}>
        <label>
          <span>Contract pack</span>
          <textarea bind:value={packText} rows="4" spellcheck="false"></textarea>
        </label>
        <label>
          <span>LLM response</span>
          <textarea bind:value={responseText} rows="5" spellcheck="false"></textarea>
        </label>
        <div class="verify-actions">
          <div class="checks">
            <label class="check">
              <input type="checkbox" bind:checked={checkWorktree} />
              <span>Worktree</span>
            </label>
            <label class="check">
              <input type="checkbox" bind:checked={strictMode} />
              <span>Strict</span>
            </label>
          </div>
          <div class="buttons">
            <button type="button" class="verify-btn subtle" disabled={verifyLoading} onclick={resetVerify}>
              Clear
            </button>
            <button type="submit" class="verify-btn" disabled={verifyLoading || !packText.trim() || !responseText.trim()}>
              {verifyLoading ? 'Verifying…' : 'Verify'}
            </button>
          </div>
        </div>
      </form>

      {#if verifyError}
        <p class="muted verify-error">{verifyError}</p>
      {/if}

      {#if verifyResult}
        <div class={`verify-result ${verifyResult.exit_code === 0 ? 'pass' : 'fail'}`}>
          <div class="metrics">
            <span>{verifyResult.violations.length} issues</span>
            <span>{verifyResult.ok.length} ok</span>
            <span>{verifyResult.stale_files.length} stale</span>
            <span>{verifyResult.references_found} refs</span>
            <span>{verifyResult.total_files_in_contract} files</span>
          </div>
          {#if verifyResult.violations.length > 0}
            <ul class="verify-list">
              {#each verifyResult.violations.slice(0, 5) as v}
                <li>
                  <span class="kind">{v.kind}</span>
                  <span class="mono">{refLabel(v)}</span>
                  {#if v.message}<span class="muted">{v.message}</span>{/if}
                </li>
              {/each}
            </ul>
            {#if hiddenCount(verifyResult.violations.length, 5)}
              <p class="muted more">+{hiddenCount(verifyResult.violations.length, 5)} more issues</p>
            {/if}
          {:else if verifyResult.ok.length > 0}
            <ul class="verify-list">
              {#each verifyResult.ok.slice(0, 5) as ok}
                <li>
                  <span class="kind">{ok.kind}</span>
                  <span class="mono">{refLabel(ok)}</span>
                </li>
              {/each}
            </ul>
            {#if hiddenCount(verifyResult.ok.length, 5)}
              <p class="muted more">+{hiddenCount(verifyResult.ok.length, 5)} more ok</p>
            {/if}
          {/if}
          {#if verifyResult.repack_suggestions.length > 0}
            <p class="muted repack">
              Repack: {verifyResult.repack_suggestions.slice(0, 3).join(', ')}
              {#if hiddenCount(verifyResult.repack_suggestions.length, 3)}
                +{hiddenCount(verifyResult.repack_suggestions.length, 3)}
              {/if}
            </p>
          {/if}
        </div>
      {/if}
    {/if}
  </section>
</div>

<style>
  .evidence {
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
    align-items: center;
    gap: 6px;
  }
  .section-toggle {
    display: grid;
    grid-template-columns: 1fr auto auto;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    text-align: start;
    cursor: pointer;
  }
  .section-toggle h3 {
    margin: 0;
  }
  .chev {
    width: 18px;
    text-align: center;
    color: var(--ctx-fg-dim);
    font-size: 14px;
  }
  .pill {
    margin-inline-start: auto;
    border: 1px solid var(--ctx-border);
    border-radius: 999px;
    padding: 1px 6px;
    font-size: 10px;
    font-weight: 500;
    text-transform: none;
    letter-spacing: 0;
  }
  .pill.fresh {
    color: var(--ctx-accent);
    border-color: color-mix(in srgb, var(--ctx-accent) 50%, var(--ctx-border));
  }
  .pill.stale,
  .pill.missing {
    color: var(--ctx-warn);
    border-color: color-mix(in srgb, var(--ctx-warn) 55%, var(--ctx-border));
  }
  .summary {
    margin: 0 0 8px;
    font-size: 10px;
  }
  ul {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  li {
    padding: 0 0 8px;
  }
  .row {
    display: grid;
    grid-template-columns: 10px 1fr auto;
    align-items: center;
    gap: 6px;
    width: 100%;
    padding: 3px 4px;
    border: 0;
    border-radius: 3px;
    background: transparent;
    color: var(--ctx-fg);
    text-align: start;
  }
  .dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--ctx-fg-dim);
  }
  .dot.fresh {
    background: var(--ctx-accent);
  }
  .dot.stale,
  .dot.missing {
    background: var(--ctx-warn);
  }
  .main {
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 1px;
  }
  .id {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 11px;
  }
  .time,
  .status,
  .meta,
  .note {
    font-size: 10px;
  }
  .status {
    text-transform: uppercase;
  }
  .meta {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
    padding-left: 20px;
    margin-top: 1px;
  }
  .note {
    margin: 3px 0 0;
    padding-left: 20px;
    line-height: 1.35;
  }
  .pos {
    color: var(--ctx-git-added);
  }
  .neg {
    color: var(--ctx-git-deleted);
  }
  .verify {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .verify label {
    display: flex;
    flex-direction: column;
    gap: 4px;
    font-size: 10px;
    color: var(--ctx-fg-dim);
  }
  textarea {
    box-sizing: border-box;
    width: 100%;
    min-height: 72px;
    resize: vertical;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 6px;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    font: 11px/1.35 var(--ctx-mono);
  }
  textarea:focus {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -1px;
  }
  .checks {
    display: flex;
    gap: 10px;
    align-items: center;
  }
  .verify-actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .buttons {
    display: flex;
    gap: 6px;
  }
  .verify .check {
    flex-direction: row;
    align-items: center;
    gap: 5px;
  }
  .check input {
    margin: 0;
  }
  .verify-btn {
    align-self: flex-start;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    padding: 4px 9px;
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
    font-size: 11px;
    cursor: pointer;
  }
  .verify-btn:disabled {
    opacity: 0.55;
    cursor: not-allowed;
  }
  .verify-btn.subtle {
    background: transparent;
  }
  .verify-error {
    margin: 8px 0 0;
    color: var(--ctx-warn);
  }
  .verify-result {
    margin-top: 10px;
    border-left: 2px solid var(--ctx-border);
    padding-left: 8px;
  }
  .verify-result.pass {
    border-left-color: var(--ctx-accent);
  }
  .verify-result.fail {
    border-left-color: var(--ctx-warn);
  }
  .metrics {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    font-size: 10px;
    color: var(--ctx-fg-dim);
  }
  .verify-list {
    margin-top: 8px;
  }
  .verify-list li {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-bottom: 6px;
    font-size: 10px;
  }
  .kind {
    color: var(--ctx-fg-dim);
    text-transform: uppercase;
  }
  .repack {
    margin: 4px 0 0;
    font-size: 10px;
  }
  .more {
    margin: 0;
    font-size: 10px;
  }
  .err {
    display: block;
    margin-top: 4px;
    color: var(--ctx-err);
    word-break: break-all;
    font-size: 10px;
  }
</style>
