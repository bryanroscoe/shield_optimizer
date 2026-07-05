<script lang="ts">
  // In-app styled confirm dialog — replaces native confirm(), which is
  // unreliable/ugly in the Android webview.
  let {
    open,
    title,
    message,
    confirmLabel = "Confirm",
    cancelLabel = "Cancel",
    danger = false,
    icon,
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    title: string;
    message: string;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    icon?: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog-overlay" onclick={onCancel}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="dialog-card" onclick={(e) => e.stopPropagation()}>
      {#if icon}
        <span class="dialog-icon msr" class:danger>{icon}</span>
      {/if}
      <h3>{title}</h3>
      <p class="dialog-message">{message}</p>
      <div class="dialog-actions">
        <button class="ghost small" onclick={onCancel}>{cancelLabel}</button>
        <button class="primary small" class:danger-btn={danger} onclick={onConfirm}>
          {confirmLabel}
        </button>
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
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 22px;
    width: 100%;
    max-width: 320px;
    padding: 24px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.7);
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    box-sizing: border-box;
    animation: dialogIn 0.2s ease-out;
  }
  .dialog-icon {
    font-size: 40px;
    color: var(--accent);
    margin-bottom: 4px;
  }
  .dialog-icon.danger {
    color: var(--danger);
  }
  .dialog-card h3 {
    margin: 0;
    font-size: 18px;
    font-weight: 700;
  }
  .dialog-message {
    margin: 0 0 12px;
    font-size: 13px;
    color: var(--muted);
    line-height: 1.45;
  }
  .dialog-actions {
    display: flex;
    gap: 10px;
    width: 100%;
  }
  .dialog-actions .ghost,
  .dialog-actions .primary {
    flex: 1;
  }
  .primary.danger-btn {
    background: var(--danger);
    color: #2a0906;
  }
  @keyframes dialogIn {
    from {
      transform: scale(0.94);
      opacity: 0;
    }
    to {
      transform: scale(1);
      opacity: 1;
    }
  }
</style>
