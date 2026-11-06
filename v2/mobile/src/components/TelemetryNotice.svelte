<script lang="ts">
  // One-time, non-blocking first-launch notice about anonymous reports. Only
  // shown by a build that can actually send them, and never as a modal: the
  // app stays usable underneath and the notice goes away for good on dismiss.
  // A failed scan or connect can send a report before any TV is connected,
  // so onboarding shows it too, in the page flow, with an opt-out in place
  // (More isn't reachable from there).
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { MOBILE_NOTICE_TEXT } from "../../../shared/telemetry";

  let {
    onOpenSettings,
    inline = false,
  }: {
    onOpenSettings?: () => void;
    /// In the page flow instead of the floating bottom stack.
    inline?: boolean;
  } = $props();
  let visible = $state(false);
  let turnedOff = $state(false);
  let turningOff = $state(false);
  let turnOffFailed = $state(false);

  onMount(() => {
    api
      .telemetrySettings()
      .then((s) => (visible = s.available && !s.notice_seen))
      .catch(() => (visible = false));
  });

  async function dismiss(openSettings: boolean) {
    visible = false;
    try {
      await api.dismissTelemetryNotice();
    } catch {
      // Shown again next launch at worst.
    }
    if (openSettings) onOpenSettings?.();
  }

  async function turnOff() {
    if (turningOff) return;
    turningOff = true;
    turnOffFailed = false;
    try {
      const settings = await api.setTelemetryEnabled(false);
      if (settings.enabled) throw new Error("still enabled");
      turnedOff = true;
      try {
        await api.dismissTelemetryNotice();
      } catch {
        // Reports are off either way; the notice may show once more.
      }
    } catch {
      turnOffFailed = true;
    } finally {
      turningOff = false;
    }
  }
</script>

{#if visible}
  <div class="telemetry-notice" class:inline role="status" aria-live="polite">
    <span class="msr" aria-hidden="true">{turnedOff ? "check_circle" : "info"}</span>
    <div class="tn-body">
      {#if turnedOff}
        <span class="tn-text">Anonymous reports are off. You can turn them back on in More.</span>
        <div class="tn-actions">
          <button type="button" class="tn-ok" onclick={() => (visible = false)}>OK</button>
        </div>
      {:else}
        <span class="tn-text">{MOBILE_NOTICE_TEXT}</span>
        {#if turnOffFailed}
          <span class="tn-error">Couldn't turn reports off. Try again.</span>
        {/if}
        <div class="tn-actions">
          <button type="button" class="tn-ok" onclick={() => dismiss(false)}>OK</button>
          {#if onOpenSettings}
            <button type="button" class="tn-link" onclick={() => dismiss(true)}>Settings</button>
          {:else}
            <button type="button" class="tn-link" disabled={turningOff} onclick={turnOff}>
              Turn off reports
            </button>
          {/if}
        </div>
      {/if}
    </div>
  </div>
{/if}

<style>
  .telemetry-notice {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 14px 16px;
    border-radius: 16px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    box-shadow: 0 8px 20px rgba(0, 0, 0, 0.5);
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-soft);
  }
  .telemetry-notice.inline {
    box-shadow: none;
    margin-bottom: 16px;
  }
  .telemetry-notice > .msr {
    color: var(--accent);
    font-size: 20px;
    flex: none;
  }
  .tn-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .tn-error {
    color: var(--danger);
  }
  .tn-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    align-items: center;
  }
  .tn-ok {
    background: var(--accent);
    color: var(--accent-ink);
    border: none;
    border-radius: 999px;
    padding: 6px 16px;
    min-height: 36px;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .tn-link {
    background: none;
    border: none;
    padding: 0;
    min-height: 36px;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
  .tn-link:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
