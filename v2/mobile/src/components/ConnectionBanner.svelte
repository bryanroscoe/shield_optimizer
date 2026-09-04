<script lang="ts">
  // Global connection strip. Lives in App so every connected screen shows the
  // same honest state during a drop: a silent redial first, then a banner with
  // the two things a user can do — retry this TV, or switch to another.
  import { session } from "../lib/session.svelte";

  let { onSwitch }: { onSwitch: () => void } = $props();

  let reconnecting = $state(false);

  async function reconnect() {
    if (reconnecting) return;
    reconnecting = true;
    try {
      await session.reconnect();
    } catch {
      // session.liveness already reflects the failure
    } finally {
      reconnecting = false;
    }
  }
</script>

{#if session.connectedDevice && session.liveness === "reconnecting"}
  <div class="conn-banner" role="status">
    <span class="msr pending">wifi_find</span>
    <span class="cb-text">{session.deviceLabel} stopped answering. Reconnecting…</span>
  </div>
{:else if session.connectedDevice && session.liveness === "lost"}
  <div class="conn-banner lost" role="alert">
    <span class="msr">wifi_off</span>
    <span class="cb-text">Lost {session.deviceLabel} ({session.host}).</span>
    <button class="cb-btn ghost" onclick={onSwitch}>Switch TV</button>
    <button class="cb-btn" disabled={reconnecting} onclick={reconnect}>
      {reconnecting ? "…" : "Retry"}
    </button>
  </div>
{/if}

<style>
  .conn-banner {
    position: fixed;
    left: 16px;
    right: 16px;
    bottom: calc(env(safe-area-inset-bottom) + 96px);
    z-index: 30;
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 14px;
    background: var(--surface-2);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.45);
  }
  .conn-banner.lost {
    border-color: color-mix(in srgb, var(--danger) 40%, transparent);
  }
  .conn-banner .msr {
    font-size: 20px;
    flex: none;
    color: var(--danger);
  }
  .conn-banner .msr.pending {
    color: var(--accent);
    animation: cb-pulse 1s ease-in-out infinite;
  }
  @keyframes cb-pulse {
    50% {
      opacity: 0.35;
    }
  }
  .cb-text {
    flex: 1;
    min-width: 0;
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-soft);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .cb-btn {
    flex: none;
    background: var(--accent);
    color: var(--accent-ink);
    border: none;
    border-radius: 10px;
    padding: 8px 12px;
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 700;
    min-height: 36px;
  }
  .cb-btn.ghost {
    background: transparent;
    color: var(--text);
    border: 1px solid var(--line);
  }
  .cb-btn:disabled {
    opacity: 0.6;
  }
</style>
