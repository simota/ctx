<script lang="ts">
  import { bounceDialog, closeBounceDialog } from '../lib/bounce-dialog.svelte';
  import { mixSelection, clearSelection } from '../lib/mix-selection.svelte';
  import { saveMix, refreshMixList } from '../lib/mix-store.svelte';
  import { announce } from '../lib/announce.svelte';
  import { MIX_NAME_MAX, MIX_GOAL_MAX, MIX_FILES_MAX } from '../lib/mix-api';

  let dialogEl: HTMLDivElement | null = $state(null);
  let nameEl: HTMLInputElement | null = $state(null);

  // Form fields
  let name = $state('');
  let goal = $state('');
  let plan = $state('');
  let limit = $state(50000);

  // Submission state
  let saving = $state(false);
  let saveError = $state<string | null>(null);

  // Reset form each time dialog opens and focus the name input.
  $effect(() => {
    if (bounceDialog.open) {
      name = `untitled-mix-${Date.now().toString(36)}`;
      goal = '';
      plan = '';
      limit = 50000;
      saving = false;
      saveError = null;
      const el = nameEl;
      if (el) queueMicrotask(() => { el.focus(); el.select(); });
    }
  });

  // Derived: files list from selection.
  let selectedFiles = $derived([...mixSelection.includedPaths].sort());

  // Validation flags
  let nameOk = $derived(name.trim().length > 0 && name.length <= MIX_NAME_MAX);
  let goalOk = $derived(goal.length <= MIX_GOAL_MAX);
  let filesOk = $derived(selectedFiles.length > 0 && selectedFiles.length <= MIX_FILES_MAX);
  let canBounce = $derived(nameOk && goalOk && filesOk && !saving);

  // Preview: show first 5 paths + "and N more".
  let previewPaths = $derived(selectedFiles.slice(0, 5));
  let previewRemainder = $derived(selectedFiles.length - previewPaths.length);

  // Focus trap + Escape
  function onKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
      e.preventDefault();
      closeBounceDialog();
      return;
    }
    if (e.key === 'Tab') {
      if (!dialogEl) { e.preventDefault(); return; }
      const focusables = Array.from(
        dialogEl.querySelectorAll<HTMLElement>(
          'button:not(:disabled), input:not(:disabled), textarea:not(:disabled), [tabindex]:not([tabindex="-1"])',
        ),
      ).filter((el) => el.offsetParent !== null);
      if (focusables.length === 0) { e.preventDefault(); return; }
      const first = focusables[0];
      const last = focusables[focusables.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    }
  }

  function onOverlayClick(e: MouseEvent) {
    if (e.target === e.currentTarget) closeBounceDialog();
  }

  async function onBounce() {
    if (!canBounce) return;
    saving = true;
    saveError = null;
    try {
      const mix = await saveMix({
        name: name.trim(),
        goal: goal.trim(),
        files: selectedFiles,
        budget: { plan: plan.trim() || undefined, limit },
      });
      await refreshMixList();
      announce(`Saved ${mix.name}`);
      clearSelection();
      closeBounceDialog();
    } catch (e: unknown) {
      saveError = e instanceof Error ? e.message : String(e);
    } finally {
      saving = false;
    }
  }
</script>

{#if bounceDialog.open}
  <!-- svelte-ignore a11y_click_events_have_key_events a11y_no_static_element_interactions -->
  <div
    class="overlay"
    onclick={onOverlayClick}
    onkeydown={onKey}
    role="presentation"
  >
    <div
      bind:this={dialogEl}
      class="modal"
      role="dialog"
      aria-modal="true"
      aria-labelledby="ctx-bounce-title"
      tabindex="-1"
    >
      <header class="head">
        <h2 id="ctx-bounce-title">Bounce Mix</h2>
        <button
          type="button"
          class="close"
          aria-label="Close bounce dialog"
          onclick={closeBounceDialog}
        >×</button>
      </header>

      <div class="body">
        {#if saveError}
          <div class="banner banner-error" role="alert">{saveError}</div>
        {/if}

        <label class="field">
          <span class="label-text">Name <span class="required" aria-hidden="true">*</span></span>
          <input
            bind:this={nameEl}
            bind:value={name}
            type="text"
            class="input"
            class:invalid={name.length > MIX_NAME_MAX}
            maxlength={MIX_NAME_MAX}
            aria-label="Mix name (required)"
            autocomplete="off"
            spellcheck="false"
          />
          {#if name.length > MIX_NAME_MAX}
            <span class="field-err" role="alert">Max {MIX_NAME_MAX} characters</span>
          {/if}
        </label>

        <label class="field">
          <span class="label-text">Goal <span class="muted">(optional)</span></span>
          <textarea
            bind:value={goal}
            class="textarea"
            class:invalid={goal.length > MIX_GOAL_MAX}
            rows="3"
            maxlength={MIX_GOAL_MAX}
            placeholder="What are you trying to accomplish with this mix?"
            aria-label="Mix goal"
          ></textarea>
          {#if goal.length > MIX_GOAL_MAX}
            <span class="field-err" role="alert">Max {MIX_GOAL_MAX} characters</span>
          {/if}
        </label>

        <label class="field">
          <span class="label-text">Budget Plan <span class="muted">(optional)</span></span>
          <input
            bind:value={plan}
            type="text"
            class="input"
            placeholder="e.g. high-signal only"
            aria-label="Budget plan"
            autocomplete="off"
          />
        </label>

        <label class="field field-inline">
          <span class="label-text">Budget Limit</span>
          <input
            bind:value={limit}
            type="number"
            class="input input-narrow"
            min="1"
            aria-label="Budget token limit"
          />
          <span class="muted unit">tokens</span>
        </label>

        <div class="field">
          <span class="label-text">Selected Files</span>
          {#if selectedFiles.length === 0}
            <p class="empty-files muted">No files selected. Close this dialog and click files in the tree.</p>
          {:else}
            <ul class="file-list mono" aria-label="Files in mix">
              {#each previewPaths as p (p)}
                <li>{p}</li>
              {/each}
              {#if previewRemainder > 0}
                <li class="muted">…and {previewRemainder} more</li>
              {/if}
            </ul>
            <span class="file-count muted">{selectedFiles.length} file{selectedFiles.length !== 1 ? 's' : ''}</span>
          {/if}
        </div>
      </div>

      <footer class="foot">
        <button
          type="button"
          class="btn btn-cancel"
          onclick={closeBounceDialog}
          disabled={saving}
        >Cancel</button>
        <button
          type="button"
          class="btn btn-bounce"
          onclick={onBounce}
          disabled={!canBounce}
          aria-busy={saving}
        >{saving ? 'Saving…' : 'Bounce'}</button>
      </footer>
    </div>
  </div>
{/if}

<style>
  .overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.35);
    display: flex;
    justify-content: center;
    align-items: flex-start;
    padding-top: 10vh;
    z-index: 1000;
    opacity: 1;
    transition: opacity var(--motion-fast, 120ms) ease-out;
    @starting-style { opacity: 0; }
  }
  :global(:root[data-theme='light']) .overlay {
    background: rgba(0, 0, 0, 0.18);
  }
  .modal {
    width: 100%;
    max-width: 540px;
    background: var(--ctx-bg-elev);
    border: 1px solid var(--ctx-border);
    border-radius: 6px;
    box-shadow: 0 12px 32px rgba(0, 0, 0, 0.45);
    display: flex;
    flex-direction: column;
    max-height: 80vh;
    outline: none;
    transform: translateY(0) scale(1);
    transition: transform var(--motion-base, 200ms) ease-out, opacity var(--motion-base, 200ms) ease-out;
    @starting-style { opacity: 0; transform: translateY(-4px) scale(0.98); }
  }
  .modal:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 8px 12px;
    border-bottom: 1px solid var(--ctx-border);
    flex: 0 0 auto;
  }
  .head h2 {
    margin: 0;
    font-size: 12px;
    font-weight: 600;
    color: var(--ctx-fg);
  }
  .close {
    border: 0;
    padding: 0 8px;
    font-size: 16px;
    line-height: 1;
    color: var(--ctx-fg-dim);
    background: transparent;
    cursor: pointer;
    border-radius: 3px;
  }
  .close:hover { color: var(--ctx-fg); }
  .close:focus-visible { outline: 2px solid var(--ctx-accent); outline-offset: -2px; }
  .body {
    padding: 12px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow-y: auto;
    min-height: 0;
  }
  .banner {
    padding: 6px 10px;
    border-radius: 4px;
    font-size: 11px;
  }
  .banner-error {
    background: rgba(220, 38, 38, 0.1);
    color: var(--ctx-err, #f87171);
    border: 1px solid rgba(220, 38, 38, 0.25);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .field-inline {
    flex-direction: row;
    align-items: center;
  }
  .field-inline .label-text {
    flex: 0 0 auto;
    margin-right: 8px;
  }
  .label-text {
    font-size: 11px;
    color: var(--ctx-fg-dim);
    font-weight: 500;
  }
  .required { color: var(--ctx-err, #f87171); }
  .input, .textarea {
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    color: var(--ctx-fg);
    font: inherit;
    font-size: 12px;
    padding: 4px 8px;
    outline: none;
    width: 100%;
    box-sizing: border-box;
  }
  .input:focus-visible, .textarea:focus-visible {
    outline: 2px solid var(--ctx-accent);
    outline-offset: -2px;
  }
  .input.invalid, .textarea.invalid {
    border-color: var(--ctx-err, #f87171);
  }
  .input-narrow {
    width: 100px;
    flex: 0 0 100px;
  }
  .textarea { resize: vertical; }
  .unit { margin-left: 6px; font-size: 11px; }
  .field-err {
    font-size: 11px;
    color: var(--ctx-err, #f87171);
  }
  .file-list {
    list-style: none;
    padding: 6px 8px;
    margin: 0;
    background: var(--ctx-bg);
    border: 1px solid var(--ctx-border);
    border-radius: 4px;
    font-size: 11px;
    line-height: 1.8;
    max-height: 120px;
    overflow-y: auto;
  }
  .file-count {
    font-size: 11px;
    margin-top: 2px;
  }
  .empty-files {
    font-size: 11px;
    margin: 0;
  }
  .foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 8px 12px;
    border-top: 1px solid var(--ctx-border);
    flex: 0 0 auto;
  }
  .btn {
    font: inherit;
    font-size: 12px;
    padding: 4px 14px;
    border-radius: 4px;
    border: 1px solid var(--ctx-border);
    cursor: pointer;
  }
  .btn:focus-visible { outline: 2px solid var(--ctx-accent); outline-offset: -2px; }
  .btn:disabled { opacity: 0.5; cursor: not-allowed; }
  .btn-cancel {
    background: transparent;
    color: var(--ctx-fg-dim);
  }
  .btn-cancel:hover:not(:disabled) { background: var(--ctx-bg-panel); color: var(--ctx-fg); }
  .btn-bounce {
    background: var(--ctx-accent);
    color: var(--ctx-bg);
    border-color: var(--ctx-accent);
    font-weight: 600;
  }
  .btn-bounce:hover:not(:disabled) { filter: brightness(1.1); }
  .muted { color: var(--ctx-fg-dim); }
  .mono { font-family: var(--ctx-font-mono); }
</style>
