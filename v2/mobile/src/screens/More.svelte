<script lang="ts">
  import { onMount } from "svelte";
  import { session } from "../lib/session.svelte";
  import { frontendLogText } from "../lib/log";
  import { api } from "../lib/api";
  import type { Screen } from "../lib/router.svelte";
  import BottomTabs from "../components/BottomTabs.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import Toast from "../components/Toast.svelte";

  let { onDisconnect, navigate }: {
    onDisconnect: () => void;
    navigate: (screen: Screen) => void;
  } = $props();

  let licenseKey = $state("");
  let activating = $state(false);

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3500);
  }

  onMount(() => session.loadEntitlement());

  // Real Pro activation — calls the backend activate_license and updates the
  // shared entitlement. No more cosmetic `=== "pro"` string match.
  async function activate() {
    const key = licenseKey.trim();
    if (!key || activating) return;
    activating = true;
    try {
      const ent = await session.activatePro(key);
      if (ent === "pro") {
        showToast("Pro unlocked — thank you!", "success");
        licenseKey = "";
      } else {
        showToast("That key wasn't accepted. Check and try again.", "error");
      }
    } catch (e) {
      // Backend rejects invalid keys with an error string.
      showToast(String(e).replace(/^.*Error:\s*/, ""), "error");
    } finally {
      activating = false;
    }
  }

  function proFeature(name: string) {
    if (session.isPro) {
      showToast(`${name} is coming in a later update.`, "info");
    } else {
      showToast(`${name} is a Pro feature — activate a license above.`, "info");
    }
  }

  // Debug log — the native tail plus the frontend call ring buffer, both
  // copyable for support.
  let showLog = $state(false);
  let nativeLog = $state("");
  let logLoading = $state(false);
  let logError = $state("");

  async function loadLog() {
    showLog = true;
    logLoading = true;
    logError = "";
    try {
      nativeLog = await api.readDebugLog();
    } catch (e) {
      logError = String(e);
    } finally {
      logLoading = false;
    }
  }

  const combinedLog = $derived(
    `# Frontend calls\n${frontendLogText()}\n\n# Native log\n${nativeLog}`,
  );

  async function copyLog() {
    try {
      await navigator.clipboard.writeText(combinedLog);
      showToast("Debug log copied.", "success");
    } catch {
      showToast("Couldn't copy to clipboard.", "error");
    }
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={() => navigate("dashboard")} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">More Options</h3>
  </div>

  <div class="more-content">
    <!-- Licensing -->
    <div class="more-card">
      <span class="card-label">Licensing</span>
      <div class="license-status-row">
        <span class="license-title">Current plan:</span>
        <span class="license-badge" class:pro={session.isPro}>{session.isPro ? "Pro" : "Free"}</span>
      </div>

      {#if !session.isPro}
        <p class="license-desc">Activate Pro to unlock Curated Debloat, Snapshots, and Launcher takeover.</p>
        <div class="license-input-row">
          <input
            type="text"
            bind:value={licenseKey}
            placeholder="Enter license key…"
            class="license-input mono"
            onkeydown={(e) => e.key === "Enter" && activate()}
          />
          <button class="primary small-btn" disabled={!licenseKey || activating} onclick={activate}>
            {activating ? "…" : "Activate"}
          </button>
        </div>
      {:else}
        <p class="license-desc success-color">Thank you for supporting ATV Optimizer Pro!</p>
      {/if}
    </div>

    <!-- Snapshots -->
    <div class="more-card">
      <span class="card-label">Snapshots</span>
      <p class="card-desc">Restore a previously saved system state or back up the current package configuration.</p>
      <div class="btn-row">
        <button class="ghost-btn" onclick={() => proFeature("Snapshots")}>
          <span class="msr">settings_backup_restore</span>Restore snapshot
        </button>
        <button class="ghost-btn" onclick={() => proFeature("Snapshots")}>
          <span class="msr">add_circle</span>Create snapshot
        </button>
      </div>
    </div>

    <!-- System Tweaks -->
    <div class="more-card">
      <span class="card-label">System Tweaks{session.isPro ? "" : " (Pro)"}</span>
      <button class="setting-row" onclick={() => proFeature("System Tweaks")}>
        <div class="setting-info">
          <span class="setting-title">HDMI-CEC control</span>
          <span class="setting-desc">One remote for TV + soundbar</span>
        </div>
        {#if !session.isPro}<span class="msr lock-icon">lock</span>{/if}
      </button>
      <button class="setting-row" onclick={() => proFeature("System Tweaks")}>
        <div class="setting-info">
          <span class="setting-title">Long-press timeout</span>
          <span class="setting-desc">Remote hold delay</span>
        </div>
        {#if !session.isPro}<span class="msr lock-icon">lock</span>{/if}
      </button>
      <button class="setting-row" onclick={() => proFeature("System Tweaks")}>
        <div class="setting-info">
          <span class="setting-title">Disable Assistant mic button</span>
          <span class="setting-desc">Revoke remote mic access</span>
        </div>
        {#if !session.isPro}<span class="msr lock-icon">lock</span>{/if}
      </button>
    </div>

    <!-- Debug log -->
    <div class="more-card">
      <span class="card-label">Debug log</span>
      <p class="card-desc">Recent app activity and the device transport log — useful for support.</p>
      {#if !showLog}
        <button class="ghost-btn" onclick={loadLog}>
          <span class="msr">description</span>Show debug log
        </button>
      {:else}
        <div class="log-toolbar">
          <button class="ghost-btn compact" onclick={loadLog} disabled={logLoading}>
            <span class="msr" class:blink={logLoading}>refresh</span>Refresh
          </button>
          <button class="ghost-btn compact" onclick={copyLog}>
            <span class="msr">content_copy</span>Copy
          </button>
        </div>
        {#if logError}
          <p class="log-error mono">{logError}</p>
        {/if}
        <pre class="log-view mono">{combinedLog}</pre>
      {/if}
    </div>

    <!-- Disconnect -->
    <button class="ghost danger-btn" onclick={onDisconnect}>
      <span class="msr">power_settings_new</span>Disconnect from TV
    </button>
  </div>

  <Toast message={toast} type={toastType} />

  <div class="spacer"></div>
  <BottomTabs active="more" {navigate} />
</div>

<style>
  .header-left {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .more-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .more-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card-label {
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
  }
  .card-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    margin: 0;
  }

  .license-status-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .license-title {
    font-size: 14px;
    font-weight: 600;
  }
  .license-badge {
    font-size: 11px;
    font-weight: 700;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.06);
    padding: 4px 9px;
    border-radius: 7px;
    text-transform: uppercase;
  }
  .license-badge.pro {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .license-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
    margin: 0;
  }
  .success-color {
    color: var(--teal);
  }
  .license-input-row {
    display: flex;
    gap: 8px;
  }
  .license-input {
    flex: 1;
    min-height: 40px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 0 10px;
    color: var(--text);
    font-size: 13px;
  }
  .license-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .small-btn {
    min-height: 40px;
    padding: 0 16px;
    width: auto;
    font-size: 13px;
    border-radius: 9px;
  }

  .btn-row {
    display: flex;
    gap: 10px;
  }
  .ghost-btn {
    flex: 1;
    min-height: 44px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .ghost-btn:active {
    background: var(--surface);
  }
  .ghost-btn:disabled {
    opacity: 0.6;
  }
  .ghost-btn .msr {
    font-size: 16px;
  }
  .ghost-btn.compact {
    flex: none;
    padding: 0 14px;
  }

  .setting-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    width: 100%;
    background: transparent;
    border: none;
    padding: 4px 0;
    cursor: pointer;
    text-align: left;
    color: var(--text);
    font-family: var(--sans);
  }
  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .setting-title {
    font-size: 14px;
    font-weight: 600;
  }
  .setting-desc {
    font-size: 12px;
    color: var(--muted);
  }
  .lock-icon {
    font-size: 18px;
    color: var(--dim);
    flex: none;
  }

  .log-toolbar {
    display: flex;
    gap: 8px;
  }
  .log-view {
    max-height: 260px;
    overflow: auto;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px;
    font-size: 10px;
    line-height: 1.5;
    color: var(--text-soft);
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
  }
  .log-error {
    font-size: 11px;
    color: var(--danger);
    margin: 0;
  }

  .danger-btn {
    min-height: 50px;
    gap: 8px;
    border-color: rgba(251, 107, 95, 0.25);
    background: rgba(251, 107, 95, 0.08);
    color: var(--danger);
  }
  .danger-btn:active {
    background: rgba(251, 107, 95, 0.05);
  }
  .danger-btn .msr {
    font-size: 20px;
  }
</style>
