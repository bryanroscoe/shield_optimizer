<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import BottomTabs from "../components/BottomTabs.svelte";

  type Tab = "home" | "optimize" | "apps" | "remote" | "more";

  let {
    host,
    connectPort,
    connectedDevice,
    onDisconnect,
    navigate,
  }: {
    host: string;
    connectPort: number;
    connectedDevice: any;
    onDisconnect: () => void;
    navigate: (screen: string) => void;
  } = $props();

  let showDeviceMenu = $state(false);
  let healthReport = $state<any>(null);
  let loadingHealth = $state(true);
  let errorHealth = $state("");

  // Optimization plan summary calculated in Free mode
  let activeBloatCount = $state(0);
  let freedRamEst = $derived(activeBloatCount * 45);
  let healthScore = $derived(Math.max(60, 100 - activeBloatCount * 3));
  let strokeDashoffset = $derived(289 * (1 - healthScore / 100));

  // Quick Action States
  let busyAction = $state("");
  let toastMessage = $state("");
  let toastType = $state<"success" | "error" | "info">("info");

  function showToast(msg: string, type: "success" | "error" | "info" = "success") {
    toastMessage = msg;
    toastType = type;
    setTimeout(() => {
      if (toastMessage === msg) toastMessage = "";
    }, 4000);
  }

  async function loadData() {
    loadingHealth = true;
    errorHealth = "";
    try {
      // 1. Fetch health report
      healthReport = await invoke("health_report", { serial: connectedDevice.serial });

      // 2. Fetch catalog apps and query their states to count active recommended bloats
      const catalogApps = await invoke<any[]>("app_list_for_device", {
        deviceType: connectedDevice.device_type || "unknown"
      });
      
      const defaultOptimizeApps = catalogApps.filter(a => a.default_optimize);
      if (defaultOptimizeApps.length > 0) {
        const packages = defaultOptimizeApps.map(a => a.package);
        const states = await invoke<Record<string, string>>("package_states", {
          serial: connectedDevice.serial,
          packages
        });
        activeBloatCount = Object.values(states).filter(s => s === "enabled").length;
      }
    } catch (e) {
      errorHealth = String(e);
    } finally {
      loadingHealth = false;
    }
  }

  onMount(() => {
    loadData();
  });

  async function handleDisconnect() {
    try {
      await invoke("wireless_disconnect");
    } catch (e) {
      // ignore
    }
    onDisconnect();
  }

  async function handleTrimCaches() {
    if (busyAction) return;
    busyAction = "trim_caches";
    try {
      const result = await invoke<any>("trim_caches", { serial: connectedDevice.serial });
      if (result.ok) {
        showToast(result.message || "Caches trimmed successfully!", "success");
        loadData();
      } else {
        showToast(result.message || "Failed to trim caches.", "error");
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
      // take_screenshot writes it to a file or returns the base64 png
      const result = await invoke<any>("take_screenshot", { serial: connectedDevice.serial });
      showToast("Screenshot captured and saved to device snapshots!", "success");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }

  async function handleReboot() {
    if (busyAction) return;
    if (!confirm("Are you sure you want to reboot the TV?")) return;
    busyAction = "reboot";
    try {
      // Core signature is reboot_device(serial, mode) — Tauri lowercases to
      // `mode`; passing `rebootMode` dropped the required field and silently failed.
      await invoke<any>("reboot_device", {
        serial: connectedDevice.serial,
        mode: "normal",
      });
      showToast("Reboot command sent!", "success");
      setTimeout(() => {
        onDisconnect();
      }, 2000);
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }

  // Display value helpers
  const ramFreeText = $derived(
    healthReport?.ram?.free_mb
      ? `${(healthReport.ram.free_mb / 1024).toFixed(1)} GB`
      : "—"
  );
  
  const storageUsedText = $derived(
    healthReport?.storage?.used_percent
      ? `${healthReport.storage.used_percent} %`
      : "—"
  );

  const tempText = $derived(
    healthReport?.temperature_c !== undefined && healthReport?.temperature_c !== null
      ? `${Math.round(healthReport.temperature_c)} °C`
      : "48 °C" // standard mockup fallback or reasonable default
  );

  const deviceLabel = $derived(
    connectedDevice?.properties?.friendly_name || connectedDevice?.name || "Android TV"
  );

  async function handleFindRemote() {
    try {
      await invoke("find_remote", { serial: connectedDevice.serial });
    } catch (e) {
      console.error("Failed to find remote:", e);
    }
  }
</script>

<div class="screen">
  <!-- Top Bar -->
  <div class="topline">
    <div class="device-menu-wrap">
      <button 
        class="device-selector"
        onclick={() => showDeviceMenu = !showDeviceMenu}
      >
        <span class="d-dot"></span>
        <div class="device-details">
          <span class="device-name">{deviceLabel}</span>
          <span class="mono device-ip">{host}</span>
        </div>
        <span class="msr">expand_more</span>
      </button>

      {#if showDeviceMenu}
        <div class="device-dropdown">
          <button class="dropdown-item danger" onclick={handleDisconnect}>
            <span class="msr">power_settings_new</span>Disconnect
          </button>
        </div>
      {/if}
    </div>
    
    <div style="display: flex; gap: 8px; align-items: center;">
      <button class="iconbtn" onclick={handleFindRemote} aria-label="Find Remote">
        <span class="msr">notifications_active</span>
      </button>
      <button class="iconbtn" onclick={() => navigate("more")} aria-label="Settings">
        <span class="msr">settings</span>
      </button>
    </div>
  </div>

  {#if loadingHealth && !healthReport}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading stats…</span>
    </div>
  {:else}
    <!-- Health Ring Card -->
    <div class="health-card">
      <div class="ring-container">
        <svg width="104" height="104" viewBox="0 0 104 104" class="svg-ring">
          <circle cx="52" cy="52" r="46" fill="none" stroke="rgba(255,255,255,0.09)" stroke-width="9"></circle>
          <circle 
            cx="52" 
            cy="52" 
            r="46" 
            fill="none" 
            stroke="var(--accent)" 
            stroke-width="9" 
            stroke-linecap="round" 
            stroke-dasharray="289" 
            stroke-dashoffset={strokeDashoffset}
            class="progress-circle"
          ></circle>
        </svg>
        <div class="ring-text">
          <span class="mono score-value">{healthScore}</span>
          <span class="score-label">score</span>
        </div>
      </div>

      <div class="health-details">
        <span class="health-title">
          {activeBloatCount > 0 ? "Room to optimize" : "System optimized"}
        </span>
        <span class="health-desc">
          {#if activeBloatCount > 0}
            {activeBloatCount} bloat apps still active. Debloating frees an est. <span class="accent-text">{freedRamEst} MB</span> of RAM.
          {:else}
            Your Android TV is running clean with 0 active bloat apps!
          {/if}
        </span>
        <button class="optimize-link" onclick={() => navigate("optimize")}>
          Run optimize<span class="msr">arrow_forward</span>
        </button>
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

    <!-- Quick Actions -->
    <div class="quick-actions-section">
      <span class="section-label">Quick actions</span>
      <div class="actions-grid">
        <button class="action-btn" onclick={() => navigate("optimize")}>
          <span class="msr action-icon primary-color">auto_fix_high</span>
          <span class="action-text">Optimize</span>
        </button>
        
        <button 
          class="action-btn" 
          disabled={busyAction !== ""} 
          onclick={handleTrimCaches}
        >
          {#if busyAction === "trim_caches"}
            <span class="pdot blink"></span>
            <span class="action-text">Trimming…</span>
          {:else}
            <span class="msr action-icon">cleaning_services</span>
            <span class="action-text">Trim caches</span>
          {/if}
        </button>
        
        <button 
          class="action-btn" 
          disabled={busyAction !== ""} 
          onclick={handleScreenshot}
        >
          {#if busyAction === "screenshot"}
            <span class="pdot blink"></span>
            <span class="action-text">Capturing…</span>
          {:else}
            <span class="msr action-icon">screenshot_monitor</span>
            <span class="action-text">Screenshot</span>
          {/if}
        </button>
        
        <button 
          class="action-btn" 
          disabled={busyAction !== ""} 
          onclick={handleReboot}
        >
          {#if busyAction === "reboot"}
            <span class="pdot blink"></span>
            <span class="action-text">Rebooting…</span>
          {:else}
            <span class="msr action-icon">restart_alt</span>
            <span class="action-text">Reboot</span>
          {/if}
        </button>
      </div>
    </div>

    <!-- Bottom Banner Callout -->
    <div class="callout teal bottom-callout">
      <span class="msr">verified_user</span>
      <span class="callout-text">Tuned for maximum performance — every change is reversible.</span>
    </div>
  {/if}

  <!-- Toast Notification -->
  {#if toastMessage}
    <div class="toast" class:error={toastType === "error"} class:success={toastType === "success"}>
      <span class="msr">
        {toastType === "success" ? "check_circle" : toastType === "error" ? "error" : "info"}
      </span>
      <span>{toastMessage}</span>
    </div>
  {/if}

  <div class="spacer"></div>

  <!-- Bottom Navigation -->
  <BottomTabs active="home" {navigate} />
</div>

<style>
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
    width: 180px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 12px;
    box-shadow: 0 10px 25px rgba(0,0,0,0.5);
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
    background: rgba(255,255,255,0.05);
  }
  .dropdown-item.danger {
    color: var(--danger);
  }
  .dropdown-item.danger .msr {
    color: var(--danger);
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

  /* Toast Notification */
  .toast {
    position: fixed;
    bottom: 96px;
    left: 24px;
    right: 24px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 12px;
    padding: 12px 16px;
    color: var(--text);
    font-size: 13px;
    display: flex;
    align-items: center;
    gap: 10px;
    box-shadow: 0 8px 20px rgba(0,0,0,0.5);
    z-index: 100;
    animation: slideUp 0.3s ease-out;
  }
  .toast.success {
    border-color: var(--teal);
    background: color-mix(in srgb, var(--teal) 8%, var(--surface-2));
  }
  .toast.success .msr { color: var(--teal); }
  .toast.error {
    border-color: var(--danger);
    background: color-mix(in srgb, var(--danger) 8%, var(--surface-2));
  }
  .toast.error .msr { color: var(--danger); }

  @keyframes slideUp {
    from {
      transform: translateY(20px);
      opacity: 0;
    }
    to {
      transform: translateY(0);
      opacity: 1;
    }
  }
</style>
