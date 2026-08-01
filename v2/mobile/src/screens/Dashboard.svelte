<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { listSavedDevices } from "../lib/savedDevices";
  import type { Screen } from "../lib/router.svelte";
  import type { SavedDevice } from "../lib/types";
  import BottomTabs from "../components/BottomTabs.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Toast from "../components/Toast.svelte";

  let { onDisconnect, navigate }: {
    onDisconnect: () => void;
    navigate: (screen: Screen) => void;
  } = $props();

  let showDeviceMenu = $state(false);
  let busyAction = $state("");
  let rebootConfirm = $state(false);
  let reconnecting = $state(false);
  let switchingHost = $state("");
  let savedTvs = $state<SavedDevice[]>([]);

  let toastMessage = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "success") {
    toastMessage = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toastMessage = ""), 4000);
  }

  onMount(() => {
    savedTvs = listSavedDevices();
    session.loadHealth();
    session.loadBloat();
    session.checkLiveness();
  });

  const previousTvs = $derived(
    savedTvs.filter(
      (d) => !(d.host === session.host && d.connectPort === session.connectPort),
    ),
  );

  // Score comes from a REAL signal (count of enabled recommended-debloat
  // packages). null when that signal failed to load — we render "—", never a
  // fabricated 100/healthy device.
  const score = $derived(
    session.bloatLoaded && !session.bloatError
      ? Math.max(60, 100 - session.bloatCount * 3)
      : null,
  );
  const strokeDashoffset = $derived(289 * (1 - (score ?? 0) / 100));

  const ramFreeText = $derived(
    session.health?.ram?.free_mb != null
      ? `${(session.health.ram.free_mb / 1024).toFixed(1)} GB`
      : "—",
  );
  const storageUsedText = $derived(
    session.health?.storage?.used_percent != null
      ? `${session.health.storage.used_percent}%`
      : "—",
  );
  const tempText = $derived(
    session.health?.temperature_c != null
      ? `${Math.round(session.health.temperature_c)}°C`
      : "—",
  );

  async function handleDisconnect() {
    showDeviceMenu = false;
    onDisconnect();
  }

  async function handleReconnect() {
    if (reconnecting) return;
    reconnecting = true;
    try {
      const r = await session.reconnect();
      if (r.ok) {
        showToast("Reconnected.", "success");
        session.loadHealth(true);
        session.loadBloat(true);
      } else {
        showToast(r.message || "Couldn't reconnect.", "error");
      }
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      reconnecting = false;
    }
  }

  async function switchDevice(device: SavedDevice) {
    if (switchingHost) return;
    showDeviceMenu = false;
    switchingHost = device.host;
    try {
      const result = await session.connect(device.host, device.connectPort);
      if (result.ok) {
        savedTvs = listSavedDevices();
        showToast(`Connected to ${session.deviceLabel}.`, "success");
        session.loadHealth(true);
        session.loadBloat(true);
      } else {
        showToast(result.message || `Couldn't connect to ${device.name}.`, "error");
      }
    } catch (error) {
      showToast(String(error), "error");
    } finally {
      switchingHost = "";
    }
  }

  async function retryHealth() {
    await Promise.all([session.loadHealth(true), session.loadBloat(true)]);
  }

  async function handleTrimCaches() {
    if (busyAction) return;
    busyAction = "trim_caches";
    try {
      const result = await api.trimCaches(session.serial);
      showToast(result.message || (result.ok ? "Caches trimmed." : "Failed to trim caches."), result.ok ? "success" : "error");
      if (result.ok) {
        session.invalidateHealth();
        session.loadHealth(true);
      }
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }

  async function handleScreenshot() {
    if (busyAction) return;
    busyAction = "screenshot";
    try {
      await api.takeScreenshot(session.serial);
      showToast("Screenshot captured.", "success");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }

  async function doReboot() {
    rebootConfirm = false;
    if (busyAction) return;
    busyAction = "reboot";
    try {
      const r = await api.rebootDevice(session.serial, "normal");
      showToast(r.ok ? "Reboot command sent." : r.message || "Reboot failed.", r.ok ? "success" : "error");
      if (r.ok) {
        // Device drops the ADB socket on reboot; tear down and return to scan.
        setTimeout(() => onDisconnect(), 1500);
      }
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="device-menu-wrap">
      <button class="device-selector" onclick={() => (showDeviceMenu = !showDeviceMenu)}>
        <span class="d-dot" class:lost={session.liveness === "lost"}></span>
        <div class="device-details">
          <span class="device-name">{session.deviceLabel}</span>
          <span class="mono device-ip">{session.host}</span>
        </div>
        <span class="msr">expand_more</span>
      </button>

      {#if showDeviceMenu}
        <div class="device-dropdown">
          {#if previousTvs.length > 0}
            <span class="dropdown-label">Previous TVs</span>
            {#each previousTvs as device (device.host)}
              <button
                class="dropdown-item saved-tv"
                disabled={switchingHost !== ""}
                onclick={() => switchDevice(device)}
              >
                <span class="msr">tv</span>
                <span class="dropdown-device-copy">
                  <span>{device.name}</span>
                  <span class="mono">{device.host}</span>
                </span>
              </button>
            {/each}
            <span class="dropdown-divider"></span>
          {/if}
          <button class="dropdown-item" onclick={() => { showDeviceMenu = false; navigate("devices"); }}>
            <span class="msr">devices_other</span>Manage devices
          </button>
          <button class="dropdown-item danger" onclick={handleDisconnect}>
            <span class="msr">power_settings_new</span>Disconnect
          </button>
        </div>
      {/if}
    </div>

    <div class="header-actions">
      <FindRemoteButton />
      <button class="iconbtn" onclick={() => navigate("more")} aria-label="Settings">
        <span class="msr">settings</span>
      </button>
    </div>
  </div>

  {#if session.liveness === "lost"}
    <div class="reconnect-banner">
      <span class="msr">wifi_off</span>
      <span class="rb-text">Connection lost. The TV is no longer reachable.</span>
      <button class="rb-btn" disabled={reconnecting} onclick={handleReconnect}>
        {reconnecting ? "Reconnecting…" : "Reconnect"}
      </button>
    </div>
  {/if}

  {#if session.healthLoading && !session.health && !session.healthError}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading stats…</span>
    </div>
  {:else}
    {#if session.healthError}
      <div class="error-card">
        <span class="msr">error</span>
        <div class="ec-body">
          <span class="ec-title">Couldn't read device health</span>
          <span class="ec-desc mono">{session.healthError}</span>
        </div>
        <button class="rb-btn" onclick={retryHealth}>Retry</button>
      </div>
    {:else}
      <!-- Health Ring Card -->
      <div class="health-card">
        <div class="ring-container">
          <svg width="104" height="104" viewBox="0 0 104 104" class="svg-ring">
            <circle cx="52" cy="52" r="46" fill="none" stroke="rgba(255,255,255,0.09)" stroke-width="9"></circle>
            {#if score !== null}
              <circle
                cx="52" cy="52" r="46" fill="none" stroke="var(--accent)"
                stroke-width="9" stroke-linecap="round" stroke-dasharray="289"
                stroke-dashoffset={strokeDashoffset} class="progress-circle"
              ></circle>
            {/if}
          </svg>
          <div class="ring-text">
            <span class="mono score-value">{score ?? "—"}</span>
            <span class="score-label">score</span>
          </div>
        </div>

        <div class="health-details">
          {#if score === null}
            <span class="health-title">Couldn't assess</span>
            <span class="health-desc">{session.bloatError || "App status unavailable."}</span>
            <button class="optimize-link" onclick={retryHealth}>
              Retry<span class="msr">refresh</span>
            </button>
          {:else if session.bloatCount > 0}
            <span class="health-title">Room to optimize</span>
            <span class="health-desc">
              <span class="accent-text">{session.bloatCount}</span>
              {session.bloatCount === 1 ? "bloat app is" : "bloat apps are"} still active.
            </span>
            <button class="optimize-link" onclick={() => navigate("optimize")}>
              Run optimize<span class="msr">arrow_forward</span>
            </button>
          {:else}
            <span class="health-title">System optimized</span>
            <span class="health-desc">Running clean — 0 active recommended-bloat apps.</span>
            <button class="optimize-link" onclick={() => navigate("optimize")}>
              Review apps<span class="msr">arrow_forward</span>
            </button>
          {/if}
        </div>
      </div>

      <!-- Stat Tiles -->
      <div class="stats-grid">
        <button class="stat-tile" onclick={() => navigate("diagnostics")}>
          <span class="msr memory-icon">memory</span>
          <span class="mono stat-value">{ramFreeText}</span>
          <span class="stat-desc">RAM free</span>
        </button>
        <button class="stat-tile" onclick={() => navigate("diagnostics")}>
          <span class="msr storage-icon">database</span>
          <span class="mono stat-value">{storageUsedText}</span>
          <span class="stat-desc">Storage used</span>
        </button>
        <button class="stat-tile" onclick={() => navigate("diagnostics")}>
          <span class="msr temp-icon">thermostat</span>
          <span class="mono stat-value">{tempText}</span>
          <span class="stat-desc">Temp</span>
        </button>
      </div>
    {/if}

    <!-- Quick Actions -->
    <div class="quick-actions-section">
      <span class="section-label">Quick actions</span>
      <div class="actions-grid">
        <button class="action-btn" onclick={() => navigate("optimize")}>
          <span class="msr action-icon primary-color">auto_fix_high</span>
          <span class="action-text">Optimize</span>
        </button>

        <button class="action-btn" disabled={busyAction !== ""} onclick={handleTrimCaches}>
          {#if busyAction === "trim_caches"}
            <span class="pdot blink"></span><span class="action-text">Trimming…</span>
          {:else}
            <span class="msr action-icon">cleaning_services</span><span class="action-text">Trim caches</span>
          {/if}
        </button>

        <button class="action-btn" disabled={busyAction !== ""} onclick={handleScreenshot}>
          {#if busyAction === "screenshot"}
            <span class="pdot blink"></span><span class="action-text">Capturing…</span>
          {:else}
            <span class="msr action-icon">screenshot_monitor</span><span class="action-text">Screenshot</span>
          {/if}
        </button>

        <button class="action-btn" disabled={busyAction !== ""} onclick={() => (rebootConfirm = true)}>
          {#if busyAction === "reboot"}
            <span class="pdot blink"></span><span class="action-text">Rebooting…</span>
          {:else}
            <span class="msr action-icon">restart_alt</span><span class="action-text">Reboot</span>
          {/if}
        </button>
      </div>
    </div>

    <div class="callout teal bottom-callout">
      <span class="msr">verified_user</span>
      <span class="callout-text">Tuned for maximum performance — every change is reversible.</span>
    </div>
  {/if}

  <ConfirmDialog
    open={rebootConfirm}
    icon="restart_alt"
    danger
    title="Reboot the TV?"
    message="The TV will restart and this connection will drop. You can reconnect once it's back."
    confirmLabel="Reboot"
    onConfirm={doReboot}
    onCancel={() => (rebootConfirm = false)}
  />

  <Toast message={toastMessage} type={toastType} />

  <div class="spacer"></div>
  <BottomTabs active="dashboard" {navigate} />
</div>

<style>
  .header-actions {
    display: flex;
    gap: 8px;
    align-items: center;
  }

  /* Device Menu & Selector */
  .device-menu-wrap {
    position: relative;
  }
  .device-selector {
    display: flex;
    align-items: center;
    gap: 10px;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 9px 14px 9px 11px;
    cursor: pointer;
    text-align: left;
    color: var(--text);
  }
  .device-selector:active {
    opacity: 0.9;
  }
  .d-dot {
    width: 9px;
    height: 9px;
    border-radius: 50%;
    background: var(--teal);
    box-shadow: 0 0 8px var(--teal);
    flex-shrink: 0;
  }
  .d-dot.lost {
    background: var(--danger);
    box-shadow: 0 0 8px var(--danger);
  }
  .device-details {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 1px;
  }
  .device-name {
    font-size: 14px;
    font-weight: 600;
  }
  .device-ip {
    font-size: 10px;
    color: var(--muted);
  }
  .device-selector .msr {
    font-size: 20px;
    color: var(--muted);
    margin-left: 2px;
  }
  .device-dropdown {
    position: absolute;
    top: 54px;
    left: 0;
    width: min(260px, 78vw);
    max-height: min(65vh, 520px);
    overflow-y: auto;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 10px 25px rgba(0, 0, 0, 0.5);
    z-index: 20;
    overflow: hidden;
  }
  .dropdown-item {
    width: 100%;
    background: transparent;
    border: none;
    padding: 12px 16px;
    text-align: left;
    font-family: var(--sans);
    font-size: 14px;
    font-weight: 500;
    color: var(--text);
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .dropdown-item:active {
    background: rgba(255, 255, 255, 0.05);
  }
  .dropdown-item:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .dropdown-label {
    display: block;
    padding: 10px 16px 5px;
    font-family: var(--mono);
    font-size: 9px;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--muted);
  }
  .dropdown-device-copy {
    min-width: 0;
    display: flex;
    flex: 1;
    flex-direction: column;
    gap: 2px;
  }
  .dropdown-device-copy > span:first-child {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .dropdown-device-copy .mono {
    color: var(--muted);
    font-size: 9px;
  }
  .dropdown-divider {
    display: block;
    height: 1px;
    background: var(--line);
  }
  .dropdown-item.danger,
  .dropdown-item.danger .msr {
    color: var(--danger);
  }

  /* Reconnect banner */
  .reconnect-banner {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 14px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--danger) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
    margin-bottom: 12px;
  }
  .reconnect-banner .msr {
    color: var(--danger);
    font-size: 20px;
    flex: none;
  }
  .rb-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-soft);
  }
  .rb-btn {
    flex: none;
    background: var(--accent);
    color: var(--accent-ink);
    border: none;
    border-radius: 10px;
    padding: 8px 14px;
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }
  .rb-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }

  /* Error card (health load failed) */
  .error-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 18px;
    border-radius: 22px;
    background: color-mix(in srgb, var(--danger) 8%, #141519);
    border: 1px solid color-mix(in srgb, var(--danger) 26%, transparent);
    margin-bottom: 8px;
  }
  .error-card > .msr {
    color: var(--danger);
    font-size: 28px;
    flex: none;
  }
  .ec-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .ec-title {
    font-size: 15px;
    font-weight: 700;
  }
  .ec-desc {
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
    word-break: break-word;
  }

  /* Health Card */
  .health-card {
    display: flex;
    gap: 18px;
    align-items: center;
    padding: 20px;
    border-radius: 22px;
    background: linear-gradient(160deg, color-mix(in srgb, var(--accent) 10%, #141519), #141519);
    border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
    margin-bottom: 8px;
  }
  .ring-container {
    position: relative;
    width: 104px;
    height: 104px;
    flex: none;
  }
  .svg-ring {
    display: block;
    transform: rotate(-90deg);
  }
  .progress-circle {
    transition: stroke-dashoffset 0.4s ease;
  }
  .ring-text {
    position: absolute;
    inset: 0;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
  }
  .score-value {
    font-size: 26px;
    font-weight: 600;
    color: var(--accent);
    line-height: 1;
  }
  .score-label {
    font-size: 9px;
    color: var(--muted);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    margin-top: 2px;
  }
  .health-details {
    display: flex;
    flex-direction: column;
    gap: 8px;
    min-width: 0;
  }
  .health-title {
    font-size: 17px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .health-desc {
    font-size: 13px;
    color: var(--text-soft);
    line-height: 1.4;
  }
  .accent-text {
    color: var(--accent);
    font-weight: 600;
  }
  .optimize-link {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 0;
    text-align: left;
    margin-top: 2px;
    align-self: flex-start;
  }
  .optimize-link .msr {
    font-size: 15px;
  }

  /* Stats Grid */
  .stats-grid {
    display: flex;
    gap: 10px;
    margin-top: 8px;
  }
  .stat-tile {
    flex: 1;
    padding: 14px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 7px;
    cursor: pointer;
    text-align: left;
    color: var(--text);
  }
  .stat-tile:active {
    background: var(--surface-2);
  }
  .stat-tile .msr {
    font-size: 20px;
  }
  .memory-icon { color: var(--teal); }
  .storage-icon { color: var(--amber); }
  .temp-icon { color: var(--teal); }
  .stat-value {
    font-size: 18px;
    font-weight: 600;
  }
  .stat-desc {
    font-size: 11px;
    color: var(--muted);
  }

  /* Quick Actions */
  .quick-actions-section {
    display: flex;
    flex-direction: column;
    gap: 11px;
    margin-top: 12px;
  }
  .actions-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .action-btn {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 14px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--line);
    cursor: pointer;
    text-align: left;
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .action-btn:active {
    background: var(--surface-2);
  }
  .action-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
  }
  .action-icon {
    font-size: 22px;
    color: var(--text-soft);
  }
  .action-icon.primary-color {
    color: var(--accent);
  }
  .action-text {
    font-size: 13px;
    font-weight: 600;
  }
  .bottom-callout {
    margin-top: 14px;
  }
  .callout-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.45;
  }
</style>
