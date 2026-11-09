<script lang="ts">
  import { modal } from "../lib/modal";
  // In-app styled confirm dialog, used instead of native confirm(), which is
  // unreliable/ugly in the Android webview.
  let {
    open,
    title,
    message,
    warning = "",
    supportWarning = "",
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
    warning?: string;
    /// The TV's support-tier caution (`destructiveWarning`). Informational:
    /// it never disables Confirm.
    supportWarning?: string | null;
    confirmLabel?: string;
    cancelLabel?: string;
    danger?: boolean;
    icon?: string;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  // The page behind is scroll-locked, so long content scrolls inside the card
  // while the actions stay pinned below it. A divider marks the cut-off.
  let body = $state<HTMLElement>();
  let content = $state<HTMLElement>();
  let overflowing = $state(false);
  $effect(() => {
    if (!body || !content) return;
    const scroller = body;
    const update = () => (overflowing = scroller.scrollHeight > scroller.clientHeight + 1);
    const observer = new ResizeObserver(update);
    observer.observe(scroller);
    observer.observe(content);
    update();
    return () => observer.disconnect();
  });
</script>

{#if open}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="dialog-overlay" use:modal={{ onClose: onCancel }} onclick={onCancel}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="dialog-card" role="dialog" aria-modal="true" aria-label={title} tabindex="-1" onclick={(e) => e.stopPropagation()}>
      <div class="dialog-body" bind:this={body}>
        <div class="dialog-content" bind:this={content}>
          {#if icon}
            <span class="dialog-icon msr" class:danger aria-hidden="true">{icon}</span>
          {/if}
          <h3>{title}</h3>
          {#if warning}
            <p class="dialog-warning" role="alert"><span class="msr" aria-hidden="true">warning</span>{warning}</p>
          {/if}
          {#if supportWarning}
            <p class="dialog-warning" data-testid="support-warning"><span class="msr" aria-hidden="true">warning</span>{supportWarning}</p>
          {/if}
          <p class="dialog-message">{message}</p>
        </div>
      </div>
      <div class="dialog-actions" class:divided={overflowing}>
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
    display: flex;
    align-items: center;
    justify-content: center;
    padding: max(16px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right))
      max(16px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left));
    box-sizing: border-box;
    overscroll-behavior: contain;
    z-index: 400;
  }
  .dialog-card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 22px;
    width: 100%;
    max-width: 320px;
    max-height: 100%;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.7);
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
    overflow: hidden;
    animation: dialogIn 0.2s ease-out;
  }
  .dialog-body {
    flex: 0 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 24px 24px 12px;
    border-radius: 21px 21px 0 0;
  }
  .dialog-content {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
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
    margin: 0;
    font-size: 13px;
    color: var(--muted);
    line-height: 1.45;
  }
  .dialog-warning {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 4px 0;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--amber) 42%, transparent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--amber) 10%, transparent);
    color: var(--amber);
    font-size: 12px;
    line-height: 1.4;
    text-align: left;
  }
  .dialog-warning .msr {
    flex: none;
    font-size: 17px;
  }
  .dialog-actions {
    flex: none;
    display: flex;
    gap: 10px;
    padding: 8px 24px 24px;
    border-top: 1px solid transparent;
  }
  .dialog-actions.divided {
    border-top-color: var(--line);
    padding-top: 16px;
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
