<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { AppUsage, OtherPackage } from "../lib/types";
  import BottomTabs from "../components/BottomTabs.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import AppDetailSheet from "../components/AppDetailSheet.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  let loading = $state(true);
  let loaded = $state(false);
  let error = $state("");
  let searchQuery = $state("");
  let activeFilter = $state<"all" | "enabled" | "disabled" | "system">("all");
  let selectedApp = $state<OtherPackage | null>(null);
  let showPaywall = $state(false);
  let uninstallTarget = $state<OtherPackage | null>(null);

  let apps = $state<OtherPackage[]>([]);
  // Lazily-loaded, best-effort enrichment maps for the detail sheet.
  let memoryMap = $state<Record<string, number>>({});
  let usageMap = $state<Record<string, AppUsage>>({});
  let busyAction = $state("");

  function isLocked(e: unknown): boolean {
    return String(e).includes("LOCKED:");
  }

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
      loadEnrichment();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  // Memory + usage power the detail sheet's "142 MB" / "never opened" signals.
  // Best-effort: on failure the sheet simply omits those rows (no fabrication).
  async function loadEnrichment() {
    if (!session.serial) return;
    try {
      memoryMap = await api.appMemoryMap(session.serial);
    } catch {
      // leave empty
    }
    try {
      usageMap = await api.appUsageMap(session.serial);
    } catch {
      // leave empty
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

  async function handleForceStop(app: OtherPackage) {
    if (busyAction || !session.serial) return;
    busyAction = app.package;
    try {
      const r = await api.forceStop(session.serial, app.package);
      showToast(r.ok ? `Stopped ${label(app)}.` : r.message || "Couldn't stop the app.", r.ok ? "success" : "error");
      if (r.ok) session.invalidateHealth();
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }

  async function handlePlayStore(app: OtherPackage) {
    if (busyAction || !session.serial) return;
    busyAction = app.package;
    try {
      const r = await api.openPlayStore(session.serial, app.package);
      showToast(r.ok ? `Opened the Play Store for ${label(app)} on the TV.` : r.message, r.ok ? "success" : "error");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
  }

  // Uninstall is Pro (Feature::CuratedDebloat). Confirm, then attempt — a
  // LOCKED response routes to the paywall instead of a raw error.
  function requestUninstall(app: OtherPackage) {
    selectedApp = null;
    uninstallTarget = app;
  }

  async function confirmUninstall() {
    const app = uninstallTarget;
    uninstallTarget = null;
    if (!app || !session.serial) return;
    busyAction = app.package;
    try {
      const r = await api.uninstallPackage(session.serial, app.package);
      if (r.ok) {
        apps = apps.filter((a) => a.package !== app.package);
        showToast(`Uninstalled ${label(app)}.`, "success");
        session.invalidateAll();
      } else {
        showToast(r.message || "Uninstall failed.", "error");
      }
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      busyAction = "";
    }
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

  <AppDetailSheet
    app={selectedApp}
    memoryMb={selectedApp ? (memoryMap[selectedApp.package] ?? null) : null}
    usage={selectedApp ? (usageMap[selectedApp.package] ?? null) : null}
    busy={busyAction !== ""}
    onClose={() => (selectedApp = null)}
    onToggle={handleToggle}
    onForceStop={handleForceStop}
    onUninstall={requestUninstall}
    onPlayStore={handlePlayStore}
  />

  <ConfirmDialog
    open={uninstallTarget !== null}
    danger
    icon="delete"
    title={`Uninstall ${uninstallTarget ? label(uninstallTarget) : "app"}?`}
    message="Removes it for the current user and frees storage. You can reinstall from the Play Store or a snapshot restore."
    confirmLabel="Uninstall"
    onConfirm={confirmUninstall}
    onCancel={() => (uninstallTarget = null)}
  />

  <PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />
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
</style>
