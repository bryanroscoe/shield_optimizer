<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { recordUnknownDiagnostics } from "../lib/unknownDiagnostics";
  import packageMetadata from "../../package.json";
  import type { Screen } from "../lib/router.svelte";
  import type { AppUsage, OtherPackage, Safety } from "../lib/types";
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
  let activeFilter = $state<"all" | "enabled" | "disabled">("all");
  let showSystemApps = $state(false);
  let selectedApp = $state<OtherPackage | null>(null);
  // The sheet stays open after a successful uninstall so Reinstall is offered.
  let sheetUninstalled = $state(false);
  let showPaywall = $state(false);

  type ListIdentity = { serial: string; generation: number; token: number };
  type RemovalAction = "disable" | "uninstall";
  type RemovalIntent = ListIdentity & {
    action: RemovalAction;
    app: OtherPackage;
    verdict: Safety;
  };

  let listIdentity = $state<ListIdentity | null>(null);
  let removedIdentity = $state<(ListIdentity & { app: OtherPackage }) | null>(null);
  let removalIntent = $state<RemovalIntent | null>(null);
  let loadRequest = 0;
  let enrichmentRequest = 0;
  let removalRequest = 0;
  let actionRequest = 0;
  let destroyed = false;
  let observedSession = "";

  let apps = $state<OtherPackage[]>([]);
  // Lazily-loaded, best-effort enrichment maps for the detail sheet.
  let memoryMap = $state<Record<string, number>>({});
  let usageMap = $state<Record<string, AppUsage>>({});
  let lookupBusy = $state("");
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

  function sessionCurrent(serial: string, generation: number): boolean {
    return (
      !destroyed &&
      session.serial === serial &&
      session.generation === generation &&
      session.isConnected
    );
  }

  function listCurrent(identity: ListIdentity | null = listIdentity): identity is ListIdentity {
    return (
      identity !== null &&
      listIdentity !== null &&
      identity.serial === listIdentity.serial &&
      identity.generation === listIdentity.generation &&
      identity.token === listIdentity.token &&
      sessionCurrent(identity.serial, identity.generation)
    );
  }

  function currentTarget(input: OtherPackage): OtherPackage | null {
    if (!listCurrent()) return null;
    return apps.find((candidate) => candidate.package === input.package) ?? null;
  }

  function removedTargetCurrent(input: OtherPackage): boolean {
    return (
      removedIdentity !== null &&
      removedIdentity.app.package === input.package &&
      sessionCurrent(removedIdentity.serial, removedIdentity.generation)
    );
  }

  function invalidateActions() {
    ++removalRequest;
    ++actionRequest;
    removalIntent = null;
    lookupBusy = "";
    busyAction = "";
    selectedApp = null;
    sheetUninstalled = false;
    removedIdentity = null;
  }

  // Keep stale rows visible while refreshing, but make them inert until a
  // successful list load is bound to this exact connection generation.
  async function loadApps(force = false) {
    const device = session.connectedDevice;
    const serial = session.serial;
    const generation = session.generation;
    if (!device || !serial || !session.isConnected) {
      ++loadRequest;
      listIdentity = null;
      loaded = false;
      error = "No TV connected. Go back and connect first.";
      loading = false;
      return;
    }
    if (loaded && !force && listCurrent()) {
      loading = false;
      return;
    }
    const request = ++loadRequest;
    listIdentity = null;
    invalidateActions();
    if (apps.length === 0) loading = true;
    error = "";
    try {
      const result = await api.listOtherPackages(serial);
      if (request !== loadRequest || !sessionCurrent(serial, generation)) return;
      apps = result;
      loaded = true;
      const identity = { serial, generation, token: request };
      listIdentity = identity;
      recordUnknownDiagnostics(
        result.map((app) => ({
          kind: "installed_package",
          token: app.package,
          reason: "uncatalogued_package",
          appVersion: packageMetadata.version,
          registryVersion: null,
          deviceFamily: device?.device_type ?? "unknown",
          deviceOs: device?.properties?.android_release || null,
        })),
        () => listCurrent(identity),
      );
      void loadEnrichment(identity);
    } catch (e) {
      if (request !== loadRequest || !sessionCurrent(serial, generation)) return;
      error = String(e);
      loaded = false;
    } finally {
      if (request === loadRequest && sessionCurrent(serial, generation)) loading = false;
    }
  }

  // Memory + usage power the detail sheet's "142 MB" / "never opened" signals.
  // Best-effort: on failure the sheet simply omits those rows (no fabrication).
  async function loadEnrichment(identity: ListIdentity) {
    const request = ++enrichmentRequest;
    try {
      const memory = await api.appMemoryMap(identity.serial);
      if (request === enrichmentRequest && listCurrent(identity)) memoryMap = memory;
    } catch {
      // leave empty
    }
    try {
      const usage = await api.appUsageMap(identity.serial);
      if (request === enrichmentRequest && listCurrent(identity)) usageMap = usage;
    } catch {
      // leave empty
    }
  }

  // generation is a plain getter, so the reactive device and liveness reads
  // are intentional: same-serial reconnects still invalidate the list.
  $effect(() => {
    const device = session.connectedDevice;
    const liveness = session.liveness;
    const generation = session.generation;
    const key = `${device?.serial ?? ""}|${generation}|${liveness}`;
    if (key === observedSession) return;
    observedSession = key;
    untrack(() => {
      ++loadRequest;
      ++enrichmentRequest;
      listIdentity = null;
      loaded = false;
      invalidateActions();
      memoryMap = {};
      usageMap = {};
      if (device && liveness === "live") void loadApps(true);
      else {
        loading = false;
        error = "No TV connected. Go back and connect first.";
      }
    });
  });

  $effect(() => {
    const q = searchQuery;
    clearTimeout(searchTimer);
    searchTimer = setTimeout(() => (debouncedQuery = q.trim().toLowerCase()), 120);
    return () => clearTimeout(searchTimer);
  });

  onDestroy(() => {
    destroyed = true;
    ++loadRequest;
    ++enrichmentRequest;
    invalidateActions();
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

  function matchesStatus(app: OtherPackage): boolean {
    if (activeFilter === "enabled") return app.enabled;
    if (activeFilter === "disabled") return !app.enabled;
    return true;
  }

  const visibleApps = $derived.by(() =>
    apps.filter((app) => (showSystemApps || !app.system) && matchesStatus(app)),
  );

  const filteredApps = $derived.by(() => {
    const q = debouncedQuery;
    return visibleApps.filter(
      (app) => !q || (haystacks.get(app.package) ?? "").includes(q),
    );
  });

  const systemAppCount = $derived(apps.filter((app) => app.system).length);
  const hiddenSystemMatches = $derived.by(() => {
    if (showSystemApps) return 0;
    const q = debouncedQuery;
    return apps.filter(
      (app) =>
        app.system &&
        matchesStatus(app) &&
        (!q || (haystacks.get(app.package) ?? "").includes(q)),
    ).length;
  });
  const countText = $derived(
    debouncedQuery
      ? `${filteredApps.length} ${filteredApps.length === 1 ? "result" : "results"}`
      : `${visibleApps.length} of ${apps.length} visible`,
  );

  /// Friendly name when the backend actually has one, otherwise the package.
  /// Never the package's last segment — that invents names like "Tv".
  function label(app: OtherPackage): string {
    return app.name || app.package;
  }

  function patch(pkg: string, enabled: boolean, identity: ListIdentity) {
    if (!listCurrent(identity)) return;
    apps = apps.map((a) => (a.package === pkg ? { ...a, enabled } : a));
  }

  function actionCurrent(request: number, identity: ListIdentity): boolean {
    return request === actionRequest && listCurrent(identity);
  }

  async function enableCurrent(input: OtherPackage) {
    if (busyAction) return;
    const app = currentTarget(input);
    const identity = listIdentity;
    if (!app || !identity || app.enabled) return;
    const request = ++actionRequest;
    busyAction = app.package;
    patch(app.package, true, identity);
    selectedApp = null;
    try {
      const r = await api.enablePackage(identity.serial, app.package);
      if (!actionCurrent(request, identity)) return;
      if (r.ok) {
        showToast(`Enabled ${label(app)}`, "success");
        session.invalidateAll();
      } else {
        patch(app.package, false, identity);
        showToast(r.message || "Action failed.", "error");
      }
    } catch (e) {
      if (!actionCurrent(request, identity)) return;
      patch(app.package, false, identity);
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  async function requestRemoval(input: OtherPackage, action: RemovalAction) {
    if (busyAction || lookupBusy) return;
    const app = currentTarget(input);
    const identity = listIdentity;
    if (!app || !identity || (action === "disable" && !app.enabled)) {
      showToast("This app list is no longer current. Refresh and try again.", "error");
      return;
    }
    const request = ++removalRequest;
    removalIntent = null;
    lookupBusy = app.package;
    try {
      const verdict = await api.safetyInfo(app.package);
      if (request !== removalRequest || !listCurrent(identity) || !currentTarget(app)) return;
      if (verdict.kind === "never_disable") {
        showToast(`Protected: ${verdict.reason}`, "error");
        return;
      }
      removalIntent = { ...identity, action, app, verdict };
      selectedApp = null;
    } catch {
      if (request !== removalRequest || !listCurrent(identity)) return;
      showToast("Safety unavailable. Retry before removing this app.", "error");
    } finally {
      if (request === removalRequest) lookupBusy = "";
    }
  }

  function handleToggle(app: OtherPackage) {
    if (app.enabled) void requestRemoval(app, "disable");
    else void enableCurrent(app);
  }

  function requestUninstall(app: OtherPackage) {
    void requestRemoval(app, "uninstall");
  }

  async function handleForceStop(input: OtherPackage) {
    if (busyAction) return;
    const app = currentTarget(input);
    const identity = listIdentity;
    if (!app || !identity) return;
    const request = ++actionRequest;
    busyAction = app.package;
    try {
      const r = await api.forceStop(identity.serial, app.package);
      if (!actionCurrent(request, identity)) return;
      showToast(r.ok ? `Stopped ${label(app)}.` : r.message || "Couldn't stop the app.", r.ok ? "success" : "error");
      if (r.ok) session.invalidateHealth();
    } catch (e) {
      if (!actionCurrent(request, identity)) return;
      showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  async function handlePlayStore(input: OtherPackage) {
    if (busyAction) return;
    const app = sheetUninstalled && removedTargetCurrent(input) ? input : currentTarget(input);
    const serial = sheetUninstalled ? removedIdentity?.serial : listIdentity?.serial;
    const generation = sheetUninstalled ? removedIdentity?.generation : listIdentity?.generation;
    if (!app || !serial || generation == null || !sessionCurrent(serial, generation)) return;
    const request = ++actionRequest;
    busyAction = app.package;
    try {
      const r = await api.openPlayStore(serial, app.package);
      if (request !== actionRequest || !sessionCurrent(serial, generation)) return;
      showToast(r.ok ? `Opened the Play Store for ${label(app)} on the TV.` : r.message, r.ok ? "success" : "error");
    } catch (e) {
      if (request !== actionRequest || !sessionCurrent(serial, generation)) return;
      showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  async function confirmRemoval() {
    const intent = removalIntent;
    removalIntent = null;
    if (!intent || busyAction || !listCurrent(intent) || !currentTarget(intent.app)) return;
    const request = ++actionRequest;
    busyAction = intent.app.package;
    try {
      let fresh: Safety;
      try {
        fresh = await api.safetyInfo(intent.app.package);
      } catch {
        if (actionCurrent(request, intent)) {
          showToast("Safety unavailable. Retry before removing this app.", "error");
        }
        return;
      }
      if (!actionCurrent(request, intent) || !currentTarget(intent.app)) return;
      if (fresh.kind === "never_disable") {
        showToast(`Protected: ${fresh.reason}`, "error");
        return;
      }
      if (fresh.kind !== intent.verdict.kind || fresh.reason !== intent.verdict.reason) {
        removalIntent = { ...intent, verdict: fresh };
        showToast("Safety guidance changed. Review the updated warning before confirming.", "info");
        return;
      }

      if (intent.action === "disable") {
        patch(intent.app.package, false, intent);
        selectedApp = null;
      }
      const r =
        intent.action === "disable"
          ? await api.disablePackage(intent.serial, intent.app.package)
          : await api.uninstallPackage(intent.serial, intent.app.package);
      if (!actionCurrent(request, intent)) return;
      if (r.ok) {
        if (intent.action === "uninstall") {
          apps = apps.filter((a) => a.package !== intent.app.package);
          removedIdentity = { ...intent, app: intent.app };
          selectedApp = intent.app;
          sheetUninstalled = true;
        }
        showToast(`${intent.action === "disable" ? "Disabled" : "Uninstalled"} ${label(intent.app)}.`, "success");
        session.invalidateAll();
      } else {
        if (intent.action === "disable") patch(intent.app.package, true, intent);
        showToast(r.message || `${intent.action === "disable" ? "Disable" : "Uninstall"} failed.`, "error");
      }
    } catch (e) {
      if (!actionCurrent(request, intent)) return;
      if (intent.action === "disable") patch(intent.app.package, true, intent);
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  // `install-existing` restores an APK that is still on the TV (the usual case
  // for a system app removed with `pm uninstall --user 0`). It cannot conjure
  // an app that was never installed — that's what the Play Store button is for.
  async function handleReinstall(app: OtherPackage) {
    if (busyAction || !removedTargetCurrent(app) || !removedIdentity) return;
    const identity = removedIdentity;
    const request = ++actionRequest;
    busyAction = app.package;
    try {
      const r = await api.reinstallExisting(identity.serial, app.package);
      if (request !== actionRequest || !sessionCurrent(identity.serial, identity.generation)) return;
      if (r.ok) {
        showToast(`Reinstalled ${label(app)}.`, "success");
        selectedApp = null;
        sheetUninstalled = false;
        removedIdentity = null;
        session.invalidateAll();
        await loadApps(true);
      } else {
        showToast(r.message || "Reinstall failed. Try the Play Store.", "error");
      }
    } catch (e) {
      if (request !== actionRequest || !sessionCurrent(identity.serial, identity.generation)) return;
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  function closeDetail() {
    ++removalRequest;
    lookupBusy = "";
    selectedApp = null;
    sheetUninstalled = false;
    removedIdentity = null;
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
    <span class="mono apps-count-badge" aria-live="polite">{countText}</span>
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
  </div>

  <button
    class="system-toggle"
    role="switch"
    aria-checked={showSystemApps}
    onclick={() => (showSystemApps = !showSystemApps)}
  >
    <span class="system-toggle-copy">
      <span class="system-toggle-title">Show system apps</span>
      <span class="system-toggle-detail">
        {showSystemApps ? "Included in the current view" : `${systemAppCount} hidden`}
      </span>
    </span>
    <span class="switch-track" aria-hidden="true"><span class="switch-knob"></span></span>
  </button>

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
            if (!listCurrent()) return;
            sheetUninstalled = false;
            removedIdentity = null;
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
      {#if filteredApps.length === 0}
        <div class="empty-state">
          <div class="empty-message" role="status">
            <h4>{debouncedQuery ? "No matching apps" : "No apps to show"}</h4>
            <p>
              {#if debouncedQuery}
                No {activeFilter === "all" ? "" : `${activeFilter} `}apps match “{searchQuery.trim()}” in this view.
              {:else if showSystemApps && activeFilter === "all"}
                No apps are available in this non-curated list.
              {:else if showSystemApps}
                No {activeFilter} apps are available in this non-curated list.
              {:else if activeFilter === "all"}
                No user-installed apps are available in this non-curated list.
              {:else}
                No {activeFilter} user-installed apps are available in this non-curated list.
              {/if}
            </p>
          </div>
          <div class="empty-actions">
            {#if debouncedQuery}
              <button class="ghost small" onclick={() => (searchQuery = "")}>Clear search</button>
            {/if}
            {#if hiddenSystemMatches > 0}
              <button class="primary" onclick={() => (showSystemApps = true)}>
                {debouncedQuery ? "Show matching system apps" : "Show system apps"}
              </button>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <div class="spacer"></div>

  <AppDetailSheet
    app={selectedApp}
    memoryMb={selectedApp ? (memoryMap[selectedApp.package] ?? null) : null}
    usage={selectedApp ? (usageMap[selectedApp.package] ?? null) : null}
    busy={busyAction !== "" || lookupBusy !== ""}
    uninstalled={sheetUninstalled}
    onClose={closeDetail}
    onToggle={handleToggle}
    onForceStop={handleForceStop}
    onUninstall={requestUninstall}
    onPlayStore={handlePlayStore}
    onReinstall={handleReinstall}
  />

  <ConfirmDialog
    open={removalIntent !== null}
    danger
    icon={removalIntent?.action === "disable" ? "block" : "delete"}
    title={`${removalIntent?.action === "disable" ? "Disable" : "Uninstall"} ${removalIntent ? label(removalIntent.app) : "app"}?`}
    warning={removalIntent?.verdict.reason ?? ""}
    message={removalIntent?.action === "disable"
      ? `${removalIntent.verdict.kind === "unknown" ? "The removal impact is Unknown." : "The safety engine marked this Caution."} Disable is reversible with Enable.`
      : `${removalIntent?.verdict.kind === "unknown" ? "The removal impact is Unknown." : "The safety engine marked this Caution."} Uninstall removes the app for this TV's current user. Reinstall works only while its APK remains on the TV; otherwise use the Play Store.`}
    confirmLabel={removalIntent?.action === "disable" ? "Disable" : "Uninstall"}
    onConfirm={confirmRemoval}
    onCancel={() => {
      ++removalRequest;
      removalIntent = null;
    }}
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
    margin-bottom: 10px;
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

  .system-toggle {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 48px;
    padding: 8px 12px;
    margin-bottom: 14px;
    border: 1px solid var(--line);
    border-radius: 13px;
    background: var(--surface);
    color: var(--text);
    font-family: var(--sans);
    text-align: left;
    width: 100%;
    cursor: pointer;
  }
  .system-toggle-copy {
    display: flex;
    flex: 1;
    min-width: 0;
    flex-direction: column;
    gap: 2px;
  }
  .system-toggle-title {
    font-size: 13px;
    font-weight: 600;
  }
  .system-toggle-detail {
    color: var(--muted);
    font-size: 10px;
  }
  .switch-track {
    width: 42px;
    height: 24px;
    padding: 2px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    box-sizing: border-box;
    flex: none;
    transition: background 0.15s ease, border-color 0.15s ease;
  }
  .switch-knob {
    display: block;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: var(--muted);
    transition: transform 0.15s ease, background 0.15s ease;
  }
  .system-toggle[aria-checked="true"] .switch-track {
    background: var(--accent);
    border-color: var(--accent);
  }
  .system-toggle[aria-checked="true"] .switch-knob {
    background: var(--accent-ink);
    transform: translateX(18px);
  }
  .system-toggle:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
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
  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
    padding: 28px 18px;
    border: 1px dashed var(--line);
    border-radius: 14px;
    color: var(--muted);
    text-align: center;
  }
  .empty-state h4,
  .empty-state p {
    margin: 0;
  }
  .empty-message {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 8px;
  }
  .empty-state h4 {
    color: var(--text);
    font-size: 15px;
  }
  .empty-state p {
    max-width: 280px;
    font-size: 12px;
    line-height: 1.45;
  }
  .empty-actions {
    display: flex;
    justify-content: center;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .empty-actions button {
    width: auto;
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
