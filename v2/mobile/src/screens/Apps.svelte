<script lang="ts">
  import { onDestroy, onMount } from "svelte";
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
  // Debounced copy of the query — filtering 300+ rows on every keystroke drops
  // frames on a phone webview.
  let debouncedQuery = $state("");
  let searchTimer: ReturnType<typeof setTimeout> | undefined;
  let activeFilter = $state<"all" | "enabled" | "disabled" | "system">("all");
  let selectedApp = $state<OtherPackage | null>(null);
  // The sheet stays open after a successful uninstall so Reinstall is offered.
  let sheetUninstalled = $state(false);
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

  $effect(() => {
    const q = searchQuery;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => (debouncedQuery = q.trim().toLowerCase()), 120);
    return () => clearTimeout(searchTimer);
  });

  onDestroy(() => {
    clearTimeout(searchTimer);
    clearTimeout(toastTimer);
  });

  // One lowercase haystack per app, rebuilt only when the list itself changes.
  const haystacks = $derived.by(() => {
    const map = new Map<string, string>();
    for (const app of apps) {
      map.set(app.package, `${app.name ?? ""} ${app.package}`.toLowerCase());
    }
    return map;
  });

  const filteredApps = $derived.by(() => {
    const q = debouncedQuery;
    return apps.filter((app) => {
      if (q && !(haystacks.get(app.package) ?? "").includes(q)) return false;
      if (activeFilter === "enabled") return app.enabled;
      if (activeFilter === "disabled") return !app.enabled;
      if (activeFilter === "system") return app.system;
      return true;
    });
  });

  /// Friendly name when the backend actually has one, otherwise the package.
  /// Never the package's last segment — that invents names like "Tv".
  function label(app: OtherPackage): string {
    return app.name || app.package;
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
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
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
        // Reopen the sheet so Reinstall (install-existing) is one tap away.
        selectedApp = app;
        sheetUninstalled = true;
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

  // `install-existing` restores an APK that is still on the TV (the usual case
  // for a system app removed with `pm uninstall --user 0`). It cannot conjure
  // an app that was never installed — that's what the Play Store button is for.
  async function handleReinstall(app: OtherPackage) {
    if (busyAction || !session.serial) return;
    busyAction = app.package;
    try {
      const r = await api.reinstallExisting(session.serial, app.package);
      if (r.ok) {
        showToast(`Reinstalled ${label(app)}.`, "success");
        selectedApp = null;
        sheetUninstalled = false;
        session.invalidateAll();
        await loadApps(true);
      } else {
        showToast(r.message || "Reinstall failed. Try the Play Store.", "error");
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

  <p class="search-hint">
    Every package on the TV outside the curated catalog. Curated bloat (Live Channels Provider,
    Google feedback, …) is handled in Optimize — searching for it here comes up empty.
  </p>

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
          onclick={() => {
            sheetUninstalled = false;
            selectedApp = app;
          }}
        >
          <div class="app-avatar"><span class="msr">{iconFor(app)}</span></div>
          <div class="app-details">
            <span class="app-name-text" class:mono={!app.name}>{label(app)}</span>
            {#if app.name}
              <span class="mono app-pkg-text">{app.package}</span>
            {/if}
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
    uninstalled={sheetUninstalled}
    onClose={() => {
      selectedApp = null;
      sheetUninstalled = false;
    }}
    onToggle={handleToggle}
    onForceStop={handleForceStop}
    onUninstall={requestUninstall}
    onPlayStore={handlePlayStore}
    onReinstall={handleReinstall}
  />

  <ConfirmDialog
    open={uninstallTarget !== null}
    danger
    icon="delete"
    title={`Uninstall ${uninstallTarget ? label(uninstallTarget) : "app"}?`}
    message="Removes the app for this TV's current user. To get it back, use the Reinstall button here (it runs install-existing on the APK still on the TV) or install it from the Play Store. Snapshots don't reinstall apps."
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

  .search-hint {
    margin: -4px 0 12px;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
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
    /* Long package lists: let the browser skip offscreen row layout. */
    content-visibility: auto;
    contain-intrinsic-size: auto 62px;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .app-name-text.mono {
    font-size: 12px;
    font-weight: 500;
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
