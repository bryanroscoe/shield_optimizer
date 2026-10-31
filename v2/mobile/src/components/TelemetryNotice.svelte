<script lang="ts">
  // One-time, non-blocking first-launch notice about anonymous reports. Only
  // shown by a build that can actually send them, and never as a modal: the
  // app stays usable underneath and the notice goes away for good on dismiss.
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { NOTICE_TEXT } from "../../../shared/telemetry";

  let { onOpenSettings }: { onOpenSettings?: () => void } = $props();
  let visible = $state(false);

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
</script>

{#if visible}
  <div class="telemetry-notice" role="status" aria-live="polite">
    <span class="msr" aria-hidden="true">info</span>
    <div class="tn-body">
      <span class="tn-text">{NOTICE_TEXT}</span>
      <div class="tn-actions">
        <button type="button" class="tn-ok" onclick={() => dismiss(false)}>OK</button>
        {#if onOpenSettings}
          <button type="button" class="tn-link" onclick={() => dismiss(true)}>Settings</button>
        {/if}
      </div>
    </div>
  </div>
{/if}

<style>
  .telemetry-notice {
    position: fixed;
    left: 16px;
    right: 16px;
    bottom: calc(env(safe-area-inset-bottom) + 88px);
    z-index: 250;
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
  .telemetry-notice > .msr {
    color: var(--accent);
    font-size: 20px;
    flex: none;
  }
  .tn-body {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .tn-actions {
    display: flex;
    gap: 16px;
    align-items: center;
  }
  .tn-ok {
    background: var(--accent);
    color: var(--accent-ink);
    border: none;
    border-radius: 999px;
    padding: 6px 16px;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
  .tn-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
</style>
