<script lang="ts">
  import type { SetupHelperState } from "../lib/launcherView";

  // Google TV's setup helper (Setup Wraith) status. Same three states and copy
  // as the desktop Launcher tab (#122): it always says whether the helper is on
  // or off, warns when it is on with stock off, and offers the matching fix.
  let {
    state,
    pkg,
    disabled = false,
    progress = "",
    onTurnOff,
    onReenable,
  }: {
    state: SetupHelperState;
    pkg: string;
    disabled?: boolean;
    /// Live status while this card's own action runs.
    progress?: string;
    onTurnOff: () => void;
    onReenable: () => void;
  } = $props();
</script>

<div
  class="wraith-card"
  class:risk={state === "risk"}
  class:off={state === "off"}
  role="status"
  data-setup-helper={state}
>
  <div class="wraith-head">
    <span class="msr">{state === "risk" ? "warning" : "info"}</span>
    <span class="wraith-title">Setup Wraith</span>
    <span class="wraith-pill">{state === "off" ? "OFF" : "ON"}</span>
  </div>
  <p class="wraith-text">
    {#if state === "off"}
      Setup Wraith is off. Turn this back on if Google asks you to sign in again or you need to
      pair a remote; turn it off again after.
    {:else if state === "on"}
      Google TV's setup helper (Setup Wraith) is on. It can take the Home button back after you
      switch launchers. Disable stock launcher turns it off too.
    {:else}
      Google TV's setup helper (Setup Wraith) is on while the stock launcher is off. It will likely
      grab the Home button.
    {/if}
  </p>
  <span class="wraith-pkg mono">{pkg}</span>
  {#if progress}
    <span class="wraith-progress" aria-live="polite"><span class="pdot blink"></span>{progress}…</span>
  {:else if state === "off"}
    <button class="wraith-btn" {disabled} onclick={onReenable}>
      <span class="msr">restart_alt</span>Re-enable Setup Wraith
    </button>
  {:else if state === "risk"}
    <button class="wraith-btn warn" {disabled} onclick={onTurnOff}>
      <span class="msr">power_settings_new</span>Turn it off
    </button>
  {/if}
</div>

<style>
  .wraith-card {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 14px 16px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .wraith-card.risk {
    background: color-mix(in srgb, var(--amber) 8%, transparent);
    border-color: color-mix(in srgb, var(--amber) 30%, transparent);
  }
  .wraith-head {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .wraith-head .msr {
    font-size: 20px;
    color: var(--muted);
  }
  .risk .wraith-head .msr {
    color: var(--amber);
  }
  .wraith-title {
    flex: 1;
    font-size: 14px;
    font-weight: 700;
  }
  .wraith-pill {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 700;
    padding: 3px 8px;
    border-radius: 7px;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
  }
  .off .wraith-pill {
    color: var(--muted);
    background: var(--surface-2);
  }
  .risk .wraith-pill {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 16%, transparent);
  }
  .wraith-text {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
  }
  .wraith-pkg {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--dim);
    overflow-wrap: anywhere;
  }
  .wraith-btn {
    align-self: flex-start;
    min-height: 40px;
    padding: 0 14px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    display: flex;
    align-items: center;
    gap: 6px;
    cursor: pointer;
  }
  .wraith-btn .msr {
    font-size: 16px;
  }
  .wraith-btn.warn {
    color: var(--amber);
    border-color: color-mix(in srgb, var(--amber) 35%, transparent);
  }
  .wraith-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .wraith-progress {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted);
  }
</style>
