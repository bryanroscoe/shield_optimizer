<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { OtherPackage } from "../lib/types";
  import BottomTabs from "../components/BottomTabs.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import Toast from "../components/Toast.svelte";

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  let loading = $state(true);
  let loaded = $state(false);
  let error = $state("");
  let searchQuery = $state("");
  let activeFilter = $state<"all" | "enabled" | "disabled" | "system">("all");
  let selectedApp = $state<OtherPackage | null>(null);

  let apps = $state<OtherPackage[]>([]);
  let busyAction = $state("");

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(message: string, type: "success" | "error" | "info" = "info") {
    toast = message;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 2800);
  }

  // Lazy load guarded by a loaded-flag (not length===0) and keeping stale rows
  // on screen during a refresh — no empty flash.
  async function loadApps(force = false) {
    if (!session.serial) {
      error = "No TV connected. Go back and connect first.";
      loading = false;
      return;
    }
    if (loaded && !force) {
      loading = false;
      return;
    }
    if (apps.length === 0) loading = true;
    error = "";
    try {
      apps = await api.listOtherPackages(session.serial);
      loaded = true;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(() => loadApps());

  const filteredApps = $derived.by(() => {
    const q = searchQuery.toLowerCase();
    return apps.filter((app) => {
      const matchesSearch =
        (app.name && app.name.toLowerCase().includes(q)) ||
        app.package.toLowerCase().includes(q);
      if (!matchesSearch) return false;
      if (activeFilter === "enabled") return app.enabled;
      if (activeFilter === "disabled") return !app.enabled;
      if (activeFilter === "system") return app.system;
      return true;
    });
  });

  function label(app: OtherPackage): string {
    return app.name || app.package.split(".").pop() || app.package;
  }

  function patch(pkg: string, enabled: boolean) {
    apps = apps.map((a) => (a.package === pkg ? { ...a, enabled } : a));
  }

  // Optimistic enable/disable: flip the row immediately, revert on failure.
  async function handleToggle(app: OtherPackage) {
    if (busyAction || !session.serial) return;
    const enabling = !app.enabled;
    const pkg = app.package;
    busyAction = pkg;
    patch(pkg, enabling);
    selectedApp = null;
    try {
      const r = enabling
        ? await api.enablePackage(session.serial, pkg)
        : await api.disablePackage(session.serial, pkg);
      if (r.ok) {
        showToast(`${enabling ? "Enabled" : "Disabled"} ${label(app)}`, "success");
        session.invalidateAll();
      } else {
        patch(pkg, !enabling);
        showToast(r.message || "Action failed.", "error");
      }
    } catch (e) {
      patch(pkg, !enabling);
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }

  function handleUninstall() {
    // Uninstall (with snapshot rollback) is a Pro feature not wired on mobile
    // yet — route to the upsell rather than performing anything.
    selectedApp = null;
    showToast("Uninstall is a Pro feature — upgrade in the More tab.", "info");
  }

  function iconFor(app: OtherPackage): string {
    if (app.system) return "system_update";
    const n = (app.name ?? "").toLowerCase();
    return n.includes("video") || n.includes("tv") ? "smart_display" : "apps";
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
    <h3 class="header-title">Apps</h3>
    <span class="mono apps-count-badge">{filteredApps.length} found</span>
  </div>

  <div class="search-box">
    <span class="msr search-icon">search</span>
    <input
      type="text"
      placeholder="Search apps or packages"
      bind:value={searchQuery}
      class="search-input"
    />
  </div>

  <div class="filters-row">
    <button class="filter-chip" class:active={activeFilter === "all"} onclick={() => (activeFilter = "all")}>All</button>
    <button class="filter-chip" class:active={activeFilter === "enabled"} onclick={() => (activeFilter = "enabled")}>Enabled</button>
    <button class="filter-chip" class:active={activeFilter === "disabled"} onclick={() => (activeFilter = "disabled")}>Disabled</button>
    <button class="filter-chip" class:active={activeFilter === "system"} onclick={() => (activeFilter = "system")}>System</button>
  </div>

  {#if loading && apps.length === 0}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading packages…</span>
    </div>
  {:else if error && apps.length === 0}
    <p class="error">{error}</p>
    <button class="primary" onclick={() => loadApps(true)}>Retry</button>
  {:else}
    <div class="apps-list">
      {#each filteredApps as app (app.package)}
        <button
          class="app-row"
          class:selected={selectedApp?.package === app.package}
          onclick={() => (selectedApp = app)}
        >
          <div class="app-avatar"><span class="msr">{iconFor(app)}</span></div>
          <div class="app-details">
            <span class="app-name-text">{label(app)}</span>
            <span class="mono app-pkg-text">{app.package}</span>
          </div>
          <span class="status-badge" class:off={!app.enabled}>{app.enabled ? "ON" : "OFF"}</span>
          <span class="msr more-icon">more_vert</span>
        </button>
      {/each}
    </div>
  {/if}

  <div class="spacer"></div>

  {#if selectedApp}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="action-drawer-overlay" onclick={() => (selectedApp = null)}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="action-drawer" onclick={(e) => e.stopPropagation()}>
        <div class="selected-app-header">
          <div class="app-avatar"><span class="msr">{iconFor(selectedApp)}</span></div>
          <div class="app-details">
            <span class="app-name-text">{label(selectedApp)}</span>
            <span class="mono app-pkg-text">{selectedApp.package}</span>
          </div>
          <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
          <span class="close-drawer msr" onclick={() => (selectedApp = null)}>close</span>
        </div>
        <div class="drawer-actions">
          <button class="drawer-btn" disabled={busyAction !== ""} onclick={() => selectedApp && handleToggle(selectedApp)}>
            {selectedApp.enabled ? "Disable" : "Enable"}
          </button>
          <button class="drawer-btn danger" onclick={handleUninstall}>
            Uninstall <span class="pro-badge">PRO</span>
          </button>
        </div>
      </div>
    </div>
  {/if}

  <Toast message={toast} type={toastType} />

  <BottomTabs active="apps" {navigate} />
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
  .apps-count-badge {
    font-size: 12px;
    color: var(--muted);
    margin-left: auto;
  }

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

  .apps-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
    overflow-y: auto;
    flex: 1;
    min-height: 0;
    padding-bottom: 12px;
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

  .action-drawer-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
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
  .drawer-btn:disabled {
    opacity: 0.6;
    cursor: not-allowed;
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
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
</style>
