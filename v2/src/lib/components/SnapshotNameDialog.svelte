<script lang="ts">
  import { dialogFocus } from "$lib/dialog-focus";

  // Names a snapshot before it is saved. The desktop app can't use prompt():
  // macOS's web view has no prompt dialog and returns null at once.
  let {
    onSave,
    onCancel,
  }: {
    onSave: (label: string) => void;
    onCancel: () => void;
  } = $props();

  let input: HTMLInputElement;
  let label = $state("");
  const titleId = $props.id();

  function submit(e: SubmitEvent) {
    e.preventDefault();
    onSave(label.trim());
  }
</script>

<div class="name-backdrop" role="presentation" onclick={onCancel}></div>
<div
  class="name-dialog"
  role="dialog"
  aria-modal="true"
  aria-labelledby={titleId}
  tabindex="-1"
  use:dialogFocus={{ onClose: onCancel, initial: () => input }}
  data-snapshot-name-dialog
>
  <form onsubmit={submit}>
    <h2 id={titleId}>Save snapshot</h2>
    <label class="field">
      Name (optional)
      <input bind:this={input} bind:value={label} maxlength="80" autocomplete="off" spellcheck="false" />
    </label>
    <div class="actions">
      <button type="button" onclick={onCancel}>Cancel</button>
      <button type="submit" class="primary">Save snapshot</button>
    </div>
  </form>
</div>

<style>
  .name-backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    background: var(--scrim);
  }
  .name-dialog {
    position: fixed;
    z-index: 41;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    box-sizing: border-box;
    width: min(24rem, calc(100vw - 32px));
    max-height: calc(100vh - 32px);
    overflow-y: auto;
    padding: 1.25rem;
    border: 1px solid var(--border-strong);
    border-radius: 14px;
    background: var(--bg-surface);
    box-shadow: var(--shadow-modal);
    text-align: left;
  }
  .name-dialog:focus {
    outline: none;
  }
  form {
    display: flex;
    flex-direction: column;
    gap: 0.9rem;
    margin: 0;
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    color: var(--fg-secondary);
    font-size: 0.9rem;
  }
  .field input {
    font: inherit;
    font-size: 1rem;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 0.5rem;
  }
</style>
