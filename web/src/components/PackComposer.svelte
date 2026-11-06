<script lang="ts">
  // Turns the pinned files into an LLM context pack (copy / download), saves
  // the selection as a reusable mix, and lists saved mixes so they can be
  // packed again or loaded back into the board. Mixes share the store with
  // `ctx mix` and `ctx pack --from-mix`, so a selection made here is usable
  // from the terminal and the MCP ctx_pack tool.
  import {
    packSelection,
    findRefs,
    fetchMixes,
    fetchMix,
    createMix,
    deleteMix,
    type PackResponse,
    type PackFormat,
    type MixSummary,
    type RefsResponse,
  } from '../lib/api';
  import { announce } from '../lib/announce.svelte';
  import { pinMany, markPinStale } from '../lib/pins.svelte';

  let { paths }: { paths: string[] } = $props();

  const PREVIEW_CHARS = 20000;

  let goal = $state('');
  let budget = $state(50000);
  let format = $state<PackFormat>('markdown');
  let packing = $state(false);
  let packError = $state<string | null>(null);
  let pack = $state<PackResponse | null>(null);
  /** What the current `pack` was built from, for labels. */
  let packSource = $state('');

  // "From text": paste a model reply / stack trace, find the files it cites.
  let refsText = $state('');
  let refsAround = $state(10);
  let refsBusy = $state(false);
  let refsError = $state<string | null>(null);
  let refs = $state<RefsResponse | null>(null);

  let mixName = $state('');
  let saving = $state(false);
  let saveError = $state<string | null>(null);
  let savedId = $state<string | null>(null);
  let savedSkipped = $state<string[]>([]);

  let mixes = $state<MixSummary[]>([]);
  let mixesLoading = $state(false);
  let mixesError = $state<string | null>(null);
  let confirmDelete = $state<string | null>(null);
  let busyMix = $state<string | null>(null);

  function message(e: unknown): string {
    return e instanceof Error ? e.message : String(e);
  }

  function budgetValue(): number {
    const n = Math.floor(Number(budget));
    return Number.isFinite(n) && n > 0 ? n : 0;
  }

  async function buildPack(): Promise<void> {
    if (paths.length === 0 || packing) return;
    packing = true;
    packError = null;
    try {
      pack = await packSelection({ files: paths, goal: goal.trim(), budget: budgetValue(), format });
      packSource = 'pinned files';
      for (const s of pack.skipped) {
        if (s.reason === 'not found' || s.reason === 'not a regular file') markPinStale(s.path);
      }
      announce(summary(pack));
    } catch (e) {
      packError = message(e);
      announce(`Pack failed: ${packError}`);
    } finally {
      packing = false;
    }
  }

  async function resolveRefs(): Promise<void> {
    if (!refsText.trim() || refsBusy) return;
    refsBusy = true;
    refsError = null;
    try {
      const around = Math.max(0, Math.floor(Number(refsAround)) || 0);
      refs = await findRefs(refsText, around);
      const n = refs.targets.length;
      const missed = refs.unresolved.length ? `, ${refs.unresolved.length} unresolved` : '';
      announce(`Found ${n} referenced file${n === 1 ? '' : 's'}${missed}`);
    } catch (e) {
      refsError = message(e);
      announce(`Finding references failed: ${refsError}`);
    } finally {
      refsBusy = false;
    }
  }

  async function packRefs(): Promise<void> {
    if (!refs || refs.targets.length === 0 || packing) return;
    packing = true;
    packError = null;
    try {
      const files = refs.targets.flatMap((t) => t.specs);
      pack = await packSelection({ files, goal: goal.trim(), budget: budgetValue(), format });
      packSource = 'referenced lines';
      announce(summary(pack));
    } catch (e) {
      packError = message(e);
      announce(`Pack failed: ${packError}`);
    } finally {
      packing = false;
    }
  }

  function pinRefs(): void {
    if (!refs) return;
    announce(pinMany(refs.targets.map((t) => t.path)).message);
  }

  function summary(p: PackResponse): string {
    const files = `${p.included.length} file${p.included.length === 1 ? '' : 's'}`;
    const budgetText = p.budget > 0 ? ` of ${p.budget.toLocaleString()}` : '';
    const skipped = p.skipped.length ? `, ${p.skipped.length} skipped` : '';
    return `Packed ${files}, ${p.used.toLocaleString()}${budgetText} tokens${skipped}`;
  }

  async function copyPack(): Promise<void> {
    if (!pack) return;
    try {
      await navigator.clipboard.writeText(pack.content);
      announce('Context pack copied to the clipboard');
    } catch {
      announce('Copy failed; use Download instead');
    }
  }

  function downloadPack(): void {
    if (!pack) return;
    const ext = pack.format === 'plain' ? 'txt' : 'md';
    const blob = new Blob([pack.content], { type: 'text/plain;charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = `context.${ext}`;
    document.body.appendChild(a);
    a.click();
    a.remove();
    setTimeout(() => URL.revokeObjectURL(url), 0);
    announce(`Downloaded context.${ext}`);
  }

  async function loadMixes(): Promise<void> {
    mixesLoading = true;
    mixesError = null;
    try {
      mixes = (await fetchMixes()).mixes ?? [];
    } catch (e) {
      mixesError = message(e);
    } finally {
      mixesLoading = false;
    }
  }

  $effect(() => {
    void loadMixes();
  });

  async function saveMix(): Promise<void> {
    const name = mixName.trim();
    if (!name || paths.length === 0 || saving) return;
    saving = true;
    saveError = null;
    savedId = null;
    try {
      const mix = await createMix({
        name,
        goal: goal.trim(),
        files: paths,
        budget: budgetValue(),
        skip_missing: true,
      });
      savedId = mix.id;
      savedSkipped = mix.skipped ?? [];
      for (const path of savedSkipped) markPinStale(path);
      mixName = '';
      const note = savedSkipped.length
        ? ` (${savedSkipped.length} missing file${savedSkipped.length === 1 ? '' : 's'} left out and marked stale)`
        : '';
      announce(`Saved mix ${mix.id} with ${mix.files.length} files${note}`);
      await loadMixes();
    } catch (e) {
      saveError = message(e);
      announce(`Save failed: ${saveError}`);
    } finally {
      saving = false;
    }
  }

  async function packMix(m: MixSummary): Promise<void> {
    busyMix = m.id;
    packError = null;
    try {
      pack = await packSelection({ mix: m.id, format });
      packSource = `mix ${m.name}`;
      announce(summary(pack));
    } catch (e) {
      packError = message(e);
      announce(`Pack failed: ${packError}`);
    } finally {
      busyMix = null;
    }
  }

  async function loadIntoPins(m: MixSummary): Promise<void> {
    busyMix = m.id;
    try {
      const mix = await fetchMix(m.id);
      const result = pinMany(mix.files);
      if (mix.goal) goal = mix.goal;
      if (mix.budget?.limit) budget = mix.budget.limit;
      announce(result.message);
    } catch (e) {
      announce(`Could not load mix: ${message(e)}`);
    } finally {
      busyMix = null;
    }
  }

  async function removeMix(m: MixSummary): Promise<void> {
    busyMix = m.id;
    try {
      await deleteMix(m.id);
      confirmDelete = null;
      if (savedId === m.id) savedId = null;
      announce(`Deleted mix ${m.name}`);
      await loadMixes();
    } catch (e) {
      announce(`Delete failed: ${message(e)}`);
    } finally {
      busyMix = null;
    }
  }

  async function copyCommand(id: string): Promise<void> {
    try {
      await navigator.clipboard.writeText(`ctx pack --from-mix ${id}`);
      announce('Command copied');
    } catch {
      announce('Copy failed');
    }
  }
</script>

<section class="composer" aria-label="Build a context pack">
  <h3 class="section-title">Context pack</h3>
  <form
    class="controls"
    onsubmit={(e) => {
      e.preventDefault();
      void buildPack();
    }}
  >
    <label class="field grow">
      <span class="label">Goal</span>
      <input type="text" maxlength="2000" placeholder="e.g. debug session expiry" bind:value={goal} />
    </label>
    <label class="field">
      <span class="label">Budget</span>
      <input type="number" min="0" step="1000" inputmode="numeric" aria-describedby="budget-hint" bind:value={budget} />
    </label>
    <label class="field">
      <span class="label">Format</span>
      <select bind:value={format}>
        <option value="markdown">Markdown</option>
        <option value="plain">Plain text</option>
      </select>
    </label>
    <button type="submit" class="primary" disabled={paths.length === 0 || packing}>
      {packing ? 'Packing…' : `Pack ${paths.length} file${paths.length === 1 ? '' : 's'}`}
    </button>
  </form>
  <p id="budget-hint" class="muted hint">
    Files are packed in board order; the budget (tokens, 0 = unlimited) drops files that no longer fit.
  </p>

  <details class="refs">
    <summary>From a reply or stack trace</summary>
    <form
      class="refs-form"
      onsubmit={(e) => {
        e.preventDefault();
        void resolveRefs();
      }}
    >
      <label class="field">
        <span class="label">Text that mentions files</span>
        <textarea
          rows="5"
          maxlength="1000000"
          placeholder={'Paste a model reply, stack trace or review comment — e.g. "the bug is in src/store.rs:120-180" or "at load (src/api/user.ts:42:7)"'}
          bind:value={refsText}
        ></textarea>
      </label>
      <div class="controls">
        <label class="field">
          <span class="label">Context lines</span>
          <input type="number" min="0" max="1000" step="1" inputmode="numeric" aria-describedby="refs-hint" bind:value={refsAround} />
        </label>
        <button type="submit" disabled={!refsText.trim() || refsBusy}>
          {refsBusy ? 'Finding…' : 'Find files'}
        </button>
      </div>
      <p id="refs-hint" class="muted hint">
        Paths, <code class="mono">file:line</code>, <code class="mono">#L10-L20</code> permalinks and unique file names are resolved against this repository; cited lines are packed with this many lines either side.
      </p>
    </form>
    {#if refsError}
      <p class="error" role="alert">{refsError}</p>
    {/if}
    {#if refs}
      <div class="result" aria-live="polite">
        {#if refs.targets.length === 0}
          <p class="muted hint">No file in this repository is mentioned in that text.</p>
        {:else}
          <div class="result-head">
            <strong>{refs.targets.length} referenced file{refs.targets.length === 1 ? '' : 's'}</strong>
            <span class="result-actions">
              <button type="button" class="primary" disabled={packing} onclick={packRefs}>Pack cited lines</button>
              <button type="button" onclick={pinRefs}>Pin {refs.targets.length} file{refs.targets.length === 1 ? '' : 's'}</button>
            </span>
          </div>
          <ul class="skipped" aria-label="Referenced files">
            {#each refs.targets as t (t.path)}
              <li>
                <span class="mono">{t.path}</span>
                <span class="muted">— {t.lines ? `lines ${t.lines}` : 'whole file'}; {t.reason}</span>
              </li>
            {/each}
          </ul>
        {/if}
        {#if refs.unresolved.length > 0}
          <ul class="skipped" aria-label="Unresolved mentions">
            {#each refs.unresolved as u (u.mention)}
              <li><span class="mono">{u.mention}</span> <span class="muted">— {u.reason} (line {u.line})</span></li>
            {/each}
          </ul>
        {/if}
      </div>
    {/if}
  </details>

  {#if packError}
    <p class="error" role="alert">{packError}</p>
  {/if}

  {#if pack}
    <div class="result" aria-live="polite">
      <div class="result-head">
        <span><strong>{summary(pack)}</strong> <span class="muted">from {packSource}</span></span>
        <span class="result-actions">
          <button type="button" onclick={copyPack}>Copy</button>
          <button type="button" onclick={downloadPack}>Download</button>
        </span>
      </div>
      {#if pack.skipped.length > 0}
        <ul class="skipped" aria-label="Skipped files">
          {#each pack.skipped as s (s.path)}
            <li><span class="mono">{s.path}</span> <span class="muted">— {s.reason}</span></li>
          {/each}
        </ul>
      {/if}
      <details>
        <summary>Preview</summary>
        <!-- svelte-ignore a11y_no_noninteractive_tabindex (a scrollable region must be keyboard-focusable) -->
        <pre class="preview mono" tabindex="0" aria-label="Pack preview">{pack.content.length > PREVIEW_CHARS
            ? `${pack.content.slice(0, PREVIEW_CHARS)}\n… (${(pack.content.length - PREVIEW_CHARS).toLocaleString()} more characters; use Copy or Download)`
            : pack.content}</pre>
      </details>
    </div>
  {/if}

  <form
    class="controls save"
    onsubmit={(e) => {
      e.preventDefault();
      void saveMix();
    }}
  >
    <label class="field grow">
      <span class="label">Save as mix</span>
      <input type="text" maxlength="200" placeholder="name, e.g. auth review" bind:value={mixName} />
    </label>
    <button type="submit" disabled={!mixName.trim() || paths.length === 0 || saving}>
      {saving ? 'Saving…' : 'Save'}
    </button>
  </form>
  {#if saveError}
    <p class="error" role="alert">{saveError}</p>
  {/if}
  {#if savedId}
    <p class="muted hint" role="status">
      Saved. Reuse it from the terminal with
      <code class="mono">ctx pack --from-mix {savedId}</code>
      <button type="button" class="link" onclick={() => copyCommand(savedId ?? '')}>Copy command</button>
    </p>
    {#if savedSkipped.length > 0}
      <p class="muted hint" role="status">
        Left out (not in this repository): <span class="mono">{savedSkipped.join(', ')}</span>
      </p>
    {/if}
  {/if}

  <div class="mixes">
    <h3 class="section-title">Saved mixes</h3>
    {#if mixesLoading && mixes.length === 0}
      <p class="muted hint" aria-busy="true">Loading…</p>
    {:else if mixesError}
      <p class="error">
        {mixesError}
        <button type="button" onclick={loadMixes}>Retry</button>
      </p>
    {:else if mixes.length === 0}
      <p class="muted hint">No saved mixes yet. Save the pinned files above, or run <code class="mono">ctx mix save NAME PATH...</code>.</p>
    {:else}
      <ul class="mix-list" aria-label="Saved mixes">
        {#each mixes as m (m.id)}
          <li>
            <span class="mix-name">
              <span>{m.name}</span>
              <span class="muted mono small">{m.id} · {m.file_count} file{m.file_count === 1 ? '' : 's'} · {m.created.slice(0, 10)}</span>
              {#if m.goal}<span class="muted small">{m.goal}</span>{/if}
            </span>
            <span class="result-actions" role="group" aria-label={`Actions for mix ${m.name}`}>
              {#if confirmDelete === m.id}
                <button type="button" class="danger" disabled={busyMix === m.id} onclick={() => removeMix(m)}>Confirm delete</button>
                <button type="button" onclick={() => (confirmDelete = null)}>Cancel</button>
              {:else}
                <button type="button" disabled={busyMix === m.id} onclick={() => packMix(m)}>Pack</button>
                <button type="button" disabled={busyMix === m.id} onclick={() => loadIntoPins(m)}>Add to pins</button>
                <button type="button" onclick={() => copyCommand(m.id)} aria-label={`Copy ctx pack command for ${m.name}`}>CLI</button>
                <button type="button" class="danger-text" onclick={() => (confirmDelete = m.id)}>Delete</button>
              {/if}
            </span>
          </li>
        {/each}
      </ul>
    {/if}
  </div>
</section>

<style>
  .composer {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 10px 12px;
    background: var(--ctx-bg-panel);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
  }
  .section-title {
    margin: 0;
    font-size: 13px;
    color: var(--ctx-fg);
  }
  .controls {
    display: flex;
    flex-wrap: wrap;
    align-items: flex-end;
    gap: 8px;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .field.grow {
    flex: 1 1 240px;
  }
  .label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.05em;
    color: var(--ctx-fg-dim);
  }
  input,
  select,
  textarea {
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 4px 7px;
    min-width: 0;
  }
  input[type='number'] {
    width: 110px;
  }
  input:focus-visible,
  select:focus-visible,
  textarea:focus-visible,
  pre:focus-visible,
  summary:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  button {
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg-elev);
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 3px 8px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    border-color: var(--ctx-accent);
  }
  button:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: 1px;
  }
  button:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  button.primary {
    border-color: var(--ctx-accent);
    font-weight: 600;
  }
  button.link {
    border: none;
    background: none;
    padding: 0 2px;
    text-decoration: underline;
    color: var(--ctx-accent-text);
  }
  .danger,
  .danger-text {
    color: var(--ctx-err);
  }
  .hint {
    margin: 0;
    font-size: 11px;
  }
  .error {
    margin: 0;
    color: var(--ctx-err);
    font-size: 12px;
  }
  .result {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 8px 10px;
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    background: var(--ctx-bg);
    font-size: 12px;
  }
  .result-head,
  .mix-list li {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 6px 10px;
  }
  .result-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 5px;
  }
  .skipped {
    margin: 0;
    padding-left: 18px;
    font-size: 11px;
  }
  .preview {
    margin: 6px 0 0;
    max-height: 320px;
    overflow: auto;
    white-space: pre-wrap;
    word-break: break-word;
    font-size: 11px;
    padding: 8px;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg-panel);
  }
  summary {
    cursor: pointer;
    font-size: 12px;
  }
  textarea {
    resize: vertical;
    font-family: var(--ctx-font-mono, monospace);
    width: 100%;
    box-sizing: border-box;
  }
  .refs {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .refs-form {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-top: 6px;
  }
  .save {
    padding-top: 6px;
    border-top: 1px solid var(--ctx-border);
  }
  .mixes {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding-top: 6px;
    border-top: 1px solid var(--ctx-border);
  }
  .mix-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .mix-list li {
    padding: 6px 8px;
    border: 1px solid var(--ctx-border);
    border-radius: 3px;
    background: var(--ctx-bg);
  }
  .mix-name {
    display: flex;
    flex-direction: column;
    gap: 1px;
    min-width: 0;
    font-size: 12px;
  }
  .small {
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  code {
    font-size: 11px;
  }
</style>
