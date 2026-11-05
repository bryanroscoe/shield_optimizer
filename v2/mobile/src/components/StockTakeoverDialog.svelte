<script lang="ts">
  import { modal } from "../lib/modal";
  // The confirm for Launcher › Advanced "Disable stock launcher". Three ways
  // out, as on desktop: save a snapshot first, disable now, or cancel.
  let {
    open,
    target,
    turnsOffSetupHelper,
    supportWarning = null,
    onSaveFirst,
    onConfirm,
    onCancel,
  }: {
    open: boolean;
    target: string;
    turnsOffSetupHelper: boolean;
    /// The TV's support-tier caution. Never blocks the choice.
    supportWarning?: string | null;
    onSaveFirst: () => void;
    onConfirm: () => void;
    onCancel: () => void;
  } = $props();

  // As in ConfirmDialog: long content scrolls inside the card while the three
  // choices stay pinned below it, with a divider once anything is cut off.
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
    <div
      class="dialog-card"
      role="alertdialog"
      aria-modal="true"
      tabindex="-1"
      aria-label="Confirm disabling the stock launcher"
      onclick={(e) => e.stopPropagation()}
    >
      <div class="dialog-body" bind:this={body}>
        <div class="dialog-content" bind:this={content}>
          <span class="dialog-icon msr" aria-hidden="true">home</span>
          <h3>Disable the stock launcher?</h3>
          <p class="dialog-message">
            Disable the stock launcher and hand Home to <span class="mono">{target}</span>? If Home
            doesn't land on it, the stock launcher is re-enabled straight away. You can re-enable it
            from the launcher list at any time.
          </p>
          {#if turnsOffSetupHelper}
            <p class="dialog-warning">
              <span class="msr" aria-hidden="true">warning</span>
              <span>
                Also turns off Google TV's setup helper (Setup Wraith), or it takes the Home button
                back. You may need to turn it back on briefly to sign in to Google again or pair a
                remote.
              </span>
            </p>
          {/if}
          {#if supportWarning}
            <p class="dialog-warning" data-testid="support-warning">
              <span class="msr" aria-hidden="true">warning</span>
              <span>{supportWarning}</span>
            </p>
          {/if}
        </div>
      </div>
      <div class="dialog-actions" class:divided={overflowing}>
        <button class="primary small" onclick={onSaveFirst}>
          <span class="msr" aria-hidden="true">history</span>Save snapshot first
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
    width: 100%;
    max-width: 360px;
    max-height: 100%;
    overflow: hidden;
    box-sizing: border-box;
    border-radius: 20px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
  }
  .dialog-body {
    flex: 0 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 22px 20px 8px;
  }
  .dialog-content {
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
    flex: none;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 8px 20px 12px;
    border-top: 1px solid transparent;
  }
  .dialog-actions.divided {
    border-top-color: var(--line);
    padding-top: 12px;
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
