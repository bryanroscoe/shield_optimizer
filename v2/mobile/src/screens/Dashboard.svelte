<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { router } from "../lib/router.svelte";
  import { listSavedDevices } from "../lib/savedDevices";
  import type { Screen } from "../lib/router.svelte";
  import type { SavedDevice, ScreenshotResult } from "../lib/types";
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
  let switchingHost = $state("");
  let savedTvs = $state<SavedDevice[]>([]);
  let screenshot = $state<ScreenshotResult | null>(null);

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

  const bloatKnown = $derived(
    session.bloatLoaded && !session.bloatLoading && !session.bloatError,
  );

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

  async function switchDevice(device: SavedDevice) {
    if (switchingHost) return;
    showDeviceMenu = false;
    switchingHost = device.host;
    try {
      const result = await session.connect(device.host, device.connectPort);
      if (result.ok) {
        savedTvs = listSavedDevices();
        // The global identity-note banner in App.svelte owns showing a
        // mismatch; suppress only the now-misleading success toast here.
        if (!session.identityNote) {
          showToast(`Connected to ${session.deviceLabel}.`, "success");
        }
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

  async function retryBloat() {
    await session.loadBloat(true);
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
      screenshot = await api.takeScreenshot(session.serial);
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
    const generation = session.generation;
    try {
      const r = await api.rebootDevice(session.serial, "normal");
      showToast(r.ok ? "Reboot command sent." : r.message || "Reboot failed.", r.ok ? "success" : "error");
      if (r.ok) {
        if (await session.finishReboot(generation)) router.reset("onboarding");
      }
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }
  // Surfaced next to the address so the TV's OS version is visible without
  // opening Diagnostics. Empty when the TV hasn't reported it — never guessed.
  const androidRelease = $derived(
    session.connectedDevice?.properties?.android_release?.trim() || "",
  );
</script>

<div class="screen">
  <div class="topline">
    <div class="device-menu-wrap">
      <button class="device-selector" onclick={() => (showDeviceMenu = !showDeviceMenu)}>
        <span class="d-dot" class:lost={session.liveness === "lost"} class:pending={session.liveness === "reconnecting"}></span>
        <div class="device-details">
          <span class="device-name">{session.deviceLabel}</span>
          <span class="mono device-ip">{session.host}{androidRelease ? ` · Android ${androidRelease}` : ""}</span>
        </div>
        <span class="msr">expand_more</span>
      </button>

      {#if showDeviceMenu}
        <button class="dropdown-backdrop" aria-label="Close menu" onclick={() => (showDeviceMenu = false)}></button>
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
      <button class="iconbtn" onclick={() => navigate("more")} aria-label="More">
        <span class="msr">settings</span>
      </button>
    </div>
  </div>

  {#if session.healthLoading && !session.health && !session.healthError}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading stats…</span>
    </div>
  {:else}
    <!-- Optimize call to action. Its count comes from the recommended-app
         read, not from health, so it stays usable when health fails. -->
    <div class="health-card">
      <div class="cta-top">
        <div class="review-count">
          <span class="mono count-value">{session.bloatLoading ? "…" : bloatKnown ? session.bloatCount : "—"}</span>
          <span class="count-label">Enabled</span>
        </div>

        <div class="health-details">
          <span class="cta-eyebrow">Optimize</span>
          <span class="health-title">Recommended app review</span>
          <span class="health-desc">
            {#if session.bloatLoading}
              Checking installed recommended apps…
            {:else if !bloatKnown}
              {session.bloatError || "App status unavailable."}
            {:else if session.bloatCount > 0}
              {session.bloatCount} of {session.bloatTotal} installed recommended apps are enabled.
            {:else if session.bloatTotal > 0}
              No recommended apps are enabled. You can still review optional apps.
            {:else}
              No apps from the recommended list are installed. You can still review optional apps.
            {/if}
          </span>
          {#if !session.bloatLoading && !bloatKnown}
            <button class="optimize-link" onclick={retryBloat}>
              Retry<span class="msr" aria-hidden="true">refresh</span>
            </button>
          {/if}
        </div>
      </div>

      <button class="cta-btn" onclick={() => navigate("optimize")}>
        <span class="msr" aria-hidden="true">auto_fix_high</span>
        <span class="cta-label">Review app choices</span>
        {#if !session.isPro}<span class="pro-marker">PRO</span>{/if}
        <span class="msr cta-arrow" aria-hidden="true">arrow_forward</span>
      </button>
    </div>

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

    <span class="section-label">Tune</span>
    <div class="tune-grid">
      <button class="tune-tile" onclick={() => navigate("tweaks")}>
        <span class="msr tune-icon" aria-hidden="true">tune</span>
        <span class="tune-title">Tweaks</span>
        <span class="tune-desc">CEC, frame rate, audio, screensaver, DNS</span>
      </button>
      <button class="tune-tile" onclick={() => navigate("launcher")}>
        <span class="msr tune-icon" aria-hidden="true">home</span>
        <span class="tune-title">
          Launcher
          {#if !session.isPro}<span class="pro-marker">PRO</span>{/if}
        </span>
        <span class="tune-desc">Set a custom home screen</span>
      </button>
    </div>

    <!-- Quick Actions -->
    <div class="quick-actions-section">
      <span class="section-label">Quick actions</span>
      <div class="actions-grid">
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
      <span class="callout-text">Apps your TV needs can't be disabled. Undo anything from Apps, or restore everything from More.</span>
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

  {#if screenshot}
    <div class="shot-sheet" role="dialog" aria-label="Screenshot">
      <div class="shot-head">
        <span class="shot-title">TV screenshot</span>
        <button class="iconbtn" aria-label="Close" onclick={() => (screenshot = null)}>
          <span class="msr">close</span>
        </button>
      </div>
      <img class="shot-img" src={`data:image/png;base64,${screenshot.base64}`} alt="Current TV screen" />
      <p class="shot-note mono">{screenshot.path}</p>
      <p class="shot-note">Saved in the app's private storage on this phone. Sharing and export are not available yet.</p>
    </div>
  {/if}

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
  .d-dot.pending {
    background: var(--accent);
    box-shadow: 0 0 8px var(--accent);
    animation: dot-pulse 1s ease-in-out infinite;
  }
  @keyframes dot-pulse {
    50% {
      opacity: 0.35;
    }
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
  }
  .dropdown-backdrop {
    position: fixed;
    inset: 0;
    background: transparent;
    border: none;
    z-index: 15;
  }
  .shot-sheet {
    position: fixed;
    inset: 0;
    z-index: 300;
    background: var(--bg, #0b0d10);
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: calc(env(safe-area-inset-top) + 16px) 16px calc(env(safe-area-inset-bottom) + 16px);
    overflow-y: auto;
  }
  .shot-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
  }
  .shot-title {
    font-size: 16px;
    font-weight: 700;
  }
  .shot-img {
    width: 100%;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: #000;
  }
  .shot-note {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
    word-break: break-all;
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


  /* Error card (health load failed) */
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
    min-height: 36px;
  }
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

  /* Optimize CTA card */
  .health-card {
    display: flex;
    flex-direction: column;
    gap: 16px;
    padding: 18px;
    border-radius: 22px;
    background: linear-gradient(160deg, color-mix(in srgb, var(--accent) 10%, #141519), #141519);
    border: 1px solid color-mix(in srgb, var(--accent) 18%, transparent);
    margin-bottom: 8px;
  }
  .cta-top {
    display: flex;
    gap: 16px;
    align-items: center;
  }
  .review-count {
    width: 80px;
    min-height: 80px;
    flex: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    border-radius: 18px;
    background: color-mix(in srgb, var(--accent) 11%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 25%, transparent);
  }
  .count-value {
    font-size: 26px;
    font-weight: 600;
    color: var(--accent);
    line-height: 1;
  }
  .count-label {
    font-size: 9px;
    color: var(--muted);
    letter-spacing: 0.1em;
    text-transform: uppercase;
    margin-top: 2px;
  }
  .health-details {
    display: flex;
    flex-direction: column;
    gap: 5px;
    min-width: 0;
  }
  .cta-eyebrow {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.14em;
    text-transform: uppercase;
    color: var(--accent);
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
  .cta-btn {
    display: flex;
    align-items: center;
    gap: 9px;
    width: 100%;
    min-height: 50px;
    padding: 0 16px;
    border: none;
    border-radius: 14px;
    background: var(--accent);
    color: var(--accent-ink);
    font-family: var(--sans);
    font-size: 15px;
    font-weight: 700;
    cursor: pointer;
    text-align: left;
  }
  .cta-btn:active {
    opacity: 0.9;
  }
  .cta-btn .msr {
    font-size: 20px;
  }
  .cta-label {
    flex: 1;
  }
  .cta-btn .pro-marker {
    background: color-mix(in srgb, var(--accent-ink) 14%, transparent);
    color: var(--accent-ink);
  }
  .pro-marker {
    padding: 2px 5px;
    border-radius: 5px;
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
    font-family: var(--mono);
    font-size: 8px;
    font-weight: 700;
    letter-spacing: 0.06em;
  }

  /* Tune tiles */
  .tune-grid {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .tune-tile {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 6px;
    padding: 14px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
    cursor: pointer;
    text-align: left;
    font-family: var(--sans);
    color: var(--text);
  }
  .tune-tile:active {
    background: var(--surface-2);
  }
  .tune-icon {
    font-size: 22px;
    color: var(--accent);
  }
  .tune-title {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 14px;
    font-weight: 600;
  }
  .tune-desc {
    font-size: 11px;
    line-height: 1.35;
    color: var(--muted);
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
  }
  .quick-actions-section .section-label {
    margin-bottom: 12px;
  }
  .actions-grid {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 10px;
  }
  .action-btn {
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    gap: 7px;
    min-height: 74px;
    padding: 12px 6px;
    text-align: center;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--line);
    cursor: pointer;
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
  .action-text {
    font-size: 12px;
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
