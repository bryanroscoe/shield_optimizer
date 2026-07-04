<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  import BottomTabs from "../components/BottomTabs.svelte";

  let {
    host,
    connectPort,
    connectedDevice,
    navigate,
  }: {
    host: string;
    connectPort: number;
    connectedDevice: any;
    navigate: (screen: string) => void;
  } = $props();

  let loading = $state(true);
  let error = $state("");
  let searchQuery = $state("");
  let activeFilter = $state<"all" | "enabled" | "disabled" | "system">("all");
  let selectedApp = $state<any>(null);

  // App data list
  let apps = $state<any[]>([]);
  let busyAction = $state("");

  // Lightweight toast — replaces blocking alert()s for action feedback.
  let toast = $state("");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(message: string) {
    toast = message;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 2600);
  }

  async function loadApps() {
    if (!connectedDevice?.serial) {
      error = "No TV connected. Go back and connect first.";
      loading = false;
      return;
    }
    loading = true;
    error = "";
    try {
      const list = await invoke<any[]>("list_other_packages", {
        serial: connectedDevice.serial,
      });
      apps = list;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => {
    loadApps();
  });

  const filteredApps = $derived.by(() => {
    return apps.filter(app => {
      // Filter by search query
      const matchesSearch = 
        (app.name && app.name.toLowerCase().includes(searchQuery.toLowerCase())) ||
        app.package.toLowerCase().includes(searchQuery.toLowerCase());
      
      if (!matchesSearch) return false;

      // Filter by category
      if (activeFilter === "enabled") return app.enabled;
      if (activeFilter === "disabled") return !app.enabled;
      if (activeFilter === "system") return app.system;
      return true;
    });
  });

  async function handleDisable(app: any) {
    if (busyAction || !connectedDevice?.serial) return;
    const enabling = !app.enabled;
    busyAction = "disable";
    try {
      await invoke(enabling ? "enable_package" : "disable_package", {
        serial: connectedDevice.serial,
        package: app.package,
      });
      showToast(`${enabling ? "Enabled" : "Disabled"} ${app.name || app.package}`);
      selectedApp = null;
      await loadApps();
    } catch (e) {
      showToast(String(e));
    } finally {
      busyAction = "";
    }
  }

  function handleUninstall(_app: any) {
    showToast("Uninstall is a Pro feature — upgrade in the More tab.");
  }

  let findingRemote = $state(false);
  async function handleFindRemote() {
    if (findingRemote || !connectedDevice?.serial) return;
    findingRemote = true;
    showToast("Locating remote — listen for your Shield remote to beep…");
    try {
      const res = await invoke<{ ok: boolean; message: string }>("find_remote", {
        serial: connectedDevice.serial,
      });
      showToast(res.ok ? "Remote locator triggered on the TV." : res.message || "Couldn't trigger the remote locator.");
    } catch (e) {
      showToast(String(e));
    } finally {
      findingRemote = false;
    }
  }
</script>

<div class="screen">
  <!-- Header -->
  <div class="topline">
    <div style="display: flex; gap: 8px; align-items: center;">
      <button class="iconbtn" onclick={() => navigate("dashboard")} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <button
        class="iconbtn"
        class:busy={findingRemote}
        onclick={handleFindRemote}
        disabled={findingRemote}
        aria-label="Find remote"
        title="Find remote"
      >
        <span class="msr">settings_remote</span>
      </button>
    </div>
    <h3 class="header-title">Apps</h3>
    <span class="mono apps-count-badge">{filteredApps.length} found</span>
  </div>

  <!-- Search -->
  <div class="search-box">
    <span class="msr search-icon">search</span>
    <input 
      type="text" 
      placeholder="Search apps or packages" 
      bind:value={searchQuery}
      class="search-input"
    />
  </div>

  <!-- Filters -->
  <div class="filters-row">
    <button class="filter-chip" class:active={activeFilter === "all"} onclick={() => activeFilter = "all"}>
      All
    </button>
    <button class="filter-chip" class:active={activeFilter === "enabled"} onclick={() => activeFilter = "enabled"}>
      Enabled
    </button>
    <button class="filter-chip" class:active={activeFilter === "disabled"} onclick={() => activeFilter = "disabled"}>
      Disabled
    </button>
    <button class="filter-chip" class:active={activeFilter === "system"} onclick={() => activeFilter = "system"}>
      System
    </button>
  </div>

  <!-- Apps List -->
  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading packages…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={loadApps}>Retry</button>
  {:else}
    <div class="apps-list">
      {#each filteredApps as app}
        <button 
          class="app-row" 
          class:selected={selectedApp?.package === app.package}
          onclick={() => selectedApp = app}
        >
          <div class="app-avatar">
            <span class="msr">
              {app.system ? "system_update" : app.name?.toLowerCase().includes("video") || app.name?.toLowerCase().includes("tv") ? "smart_display" : "apps"}
            </span>
          </div>
          <div class="app-details">
            <span class="app-name-text">{app.name || app.package.split(".").pop()}</span>
            <span class="mono app-pkg-text">{app.package}</span>
          </div>
          <span class="status-badge" class:off={!app.enabled}>
            {app.enabled ? "ON" : "OFF"}
          </span>
          <span class="msr more-icon">more_vert</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="spacer"></div>

  <!-- Bottom Drawer for selected app -->
  {#if selectedApp}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="action-drawer-overlay" onclick={() => selectedApp = null}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="action-drawer" onclick={(e) => e.stopPropagation()}>
        <div class="selected-app-header">
          <div class="app-avatar">
            <span class="msr">
              {selectedApp.system ? "system_update" : "apps"}
            </span>
          </div>
          <div class="app-details">
            <span class="app-name-text">{selectedApp.name || selectedApp.package.split(".").pop()}</span>
            <span class="mono app-pkg-text">{selectedApp.package}</span>
          </div>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span class="close-drawer msr" onclick={() => selectedApp = null}>close</span>
        </div>
        
        <div class="drawer-actions">
          <button 
            class="drawer-btn" 
            disabled={busyAction !== ""}
            onclick={() => handleDisable(selectedApp)}
          >
            {selectedApp.enabled ? "Disable" : "Enable"}
          </button>
          
          <button 
            class="drawer-btn danger" 
            onclick={() => handleUninstall(selectedApp)}
          >
            Uninstall <span class="pro-badge">PRO</span>
          </button>
        </div>
      </div>
    </div>
  {/if}

  {#if toast}
    <div class="toast" role="status" aria-live="polite">{toast}</div>
  {/if}

  <BottomTabs active="apps" {navigate} />
</div>

<style>
  .toast {
    position: fixed;
    left: 16px;
    right: 16px;
    bottom: calc(env(safe-area-inset-bottom) + 92px);
    z-index: 50;
    padding: 13px 16px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--surface-2) 94%, transparent);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line);
    color: var(--text);
    font-size: 13px;
    line-height: 1.4;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  .iconbtn.busy {
    color: var(--accent);
    border-color: color-mix(in srgb, var(--accent) 40%, transparent);
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .apps-count-badge {
    font-size: 12px;
    color: var(--muted);
    margin-left: auto;
  }

  /* Search box */
  .search-box {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 2px 14px;
    border-radius: 13px;
    background: var(--surface);
    border: 1px solid var(--line);
    margin-bottom: 12px;
  }
  .search-icon {
    font-size: 20px;
    color: var(--muted);
  }
  .search-input {
    flex: 1;
    min-height: 44px;
    background: transparent;
    border: none;
    padding: 0;
    color: var(--text);
    font-family: var(--sans);
    font-size: 14px;
  }
  .search-input:focus {
    outline: none;
  }

  /* Filters row */
  .filters-row {
    display: flex;
    gap: 8px;
    overflow-x: auto;
    margin-bottom: 14px;
    padding-bottom: 2px;
  }
  .filter-chip {
    padding: 7px 14px;
    border-radius: 999px;
    background: var(--surface);
    border: 1px solid var(--line);
    color: var(--muted);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    white-space: nowrap;
  }
  .filter-chip.active {
    background: var(--accent);
    color: var(--accent-ink);
    border-color: transparent;
    font-weight: 600;
  }

  /* Apps list */
  .apps-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
    overflow-y: auto;
    max-height: 380px;
  }
  .app-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 11px 13px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    text-align: left;
    color: var(--text);
    cursor: pointer;
    width: 100%;
  }
  .app-row.selected {
    border-color: var(--accent);
  }
  .app-avatar {
    width: 40px;
    height: 40px;
    border-radius: 11px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    flex-shrink: 0;
  }
  .app-avatar .msr {
    font-size: 22px;
    color: var(--muted);
  }
  .app-details {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .app-name-text {
    font-size: 14px;
    font-weight: 600;
  }
  .app-pkg-text {
    font-size: 10px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .status-badge {
    font-size: 10px;
    font-weight: 700;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
    padding: 3px 8px;
    border-radius: 6px;
    flex-shrink: 0;
  }
  .status-badge.off {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }
  .more-icon {
    font-size: 22px;
    color: var(--dim);
    flex-shrink: 0;
  }

  /* Action drawer overlay */
  .action-drawer-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0,0,0,0.6);
    z-index: 100;
    display: flex;
    align-items: flex-end;
  }
  .action-drawer {
    width: 100%;
    background: var(--surface-2);
    border-top: 1px solid var(--line);
    border-radius: 20px 20px 0 0;
    padding: 20px 24px calc(env(safe-area-inset-bottom) + 20px);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 16px;
    animation: slideUpDrawer 0.25s ease-out;
  }
  .selected-app-header {
    display: flex;
    align-items: center;
    gap: 12px;
    position: relative;
  }
  .close-drawer {
    position: absolute;
    top: 0;
    right: 0;
    font-size: 20px;
    color: var(--muted);
    cursor: pointer;
  }
  .drawer-actions {
    display: flex;
    gap: 9px;
  }
  .drawer-btn {
    flex: 1;
    height: 44px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 13px;
    background: var(--surface);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .drawer-btn:active {
    background: var(--surface-2);
  }
  .drawer-btn.danger {
    border-color: rgba(251, 107, 95, 0.3);
    background: rgba(251, 107, 95, 0.1);
    color: var(--danger);
  }
  .pro-badge {
    font-size: 9px;
    font-weight: 700;
    background: var(--danger);
    color: var(--accent-ink);
    padding: 2px 5px;
    border-radius: 5px;
  }

  @keyframes slideUpDrawer {
    from { transform: translateY(100%); }
    to { transform: translateY(0); }
  }
</style>
