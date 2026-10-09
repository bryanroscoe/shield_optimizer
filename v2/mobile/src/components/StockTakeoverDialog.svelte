<script lang="ts">
  // The confirm for Launcher › Advanced "Disable stock launcher". Three ways
  // out, as on desktop: save a snapshot first, disable now, or cancel.
  let {
    open,
    target,
    turnsOffSetupHelper,
    onSaveFirst,
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    target: string;
    turnsOffSetupHelper: boolean;
    onSaveFirst: () => void;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog-overlay" onclick={onCancel}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div
      class="dialog-card"
      role="alertdialog"
      tabindex="-1"
      aria-label="Confirm disabling the stock launcher"
      onclick={(e) => e.stopPropagation()}
    >
      <span class="dialog-icon msr">home</span>
      <h3>Disable the stock launcher?</h3>
      <p class="dialog-message">
        Disable the stock launcher and hand Home to <span class="mono">{target}</span>? If Home
        doesn't land on it, the stock launcher is re-enabled straight away. You can re-enable it
        from the launcher list at any time.
      </p>
      {#if turnsOffSetupHelper}
        <p class="dialog-warning">
          <span class="msr">warning</span>
          <span>
            Also turns off Google TV's setup helper (Setup Wraith), or it takes the Home button
            back. You may need to turn it back on briefly to sign in to Google again or pair a
            remote.
          </span>
        </p>
      {/if}
      <div class="dialog-actions">
        <button class="primary small" onclick={onSaveFirst}>
          <span class="msr">history</span>Save snapshot first
        </button>
        <button class="ghost small danger-ghost" onclick={onConfirm}>Disable stock launcher</button>
        <button class="ghost-link" onclick={onCancel}>Cancel</button>
      </div>
    </div>
  </div>
{/if}

<style>
  .dialog-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.72);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 400;
  }
  .dialog-card {
    width: 100%;
    max-width: 360px;
    box-sizing: border-box;
    padding: 22px 20px 12px;
    border-radius: 20px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .dialog-icon {
    font-size: 28px;
    color: var(--amber);
  }
  h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
  }
  .dialog-message {
    margin: 0;
    font-size: 13px;
    line-height: 1.5;
    color: var(--text-soft);
  }
  .dialog-message .mono {
    font-family: var(--mono);
    font-size: 12px;
    overflow-wrap: anywhere;
  }
  .dialog-warning {
    margin: 0;
    display: flex;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 12px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
    background: color-mix(in srgb, var(--amber) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--amber) 25%, transparent);
  }
  .dialog-warning .msr {
    font-size: 18px;
    color: var(--amber);
    flex: none;
  }
  .dialog-actions {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 4px;
  }
  .dialog-actions .primary.small,
  .dialog-actions .ghost.small {
    flex: none;
    width: 100%;
  }
  .danger-ghost {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .dialog-actions .ghost-link {
    margin-top: 0;
  }
</style>
