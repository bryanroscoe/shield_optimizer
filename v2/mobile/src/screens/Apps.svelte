<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { recordUnknownDiagnostics } from "../lib/unknownDiagnostics";
  import { isBlocked, safetyLabel, tierOf, verdictOf, verdictSummary, type SafetyStatus } from "../lib/safety";
  import { canOfferUninstall, recommendation, uninstallNote, type PackageState, type Recommendation } from "../lib/recommendation";
  import { isReportablePackage, reportFamily, type AppReportDevice, type AppReportState } from "../lib/app-report";
  import { hasStorage, idleMeasurements, type AppMeasurements, type MeasureStatus } from "../lib/app-details";
  import { normalizeHardwareId } from "../lib/identity";
  import {
    catalogItems,
    filterCatalog,
    filterOthers,
    hiddenMatches,
    normalizeQuery,
    onDeviceCount,
    otherItems,
    validatedStates,
    type AppItem,
    type AppMenuItem,
    type ListFilters,
    type StatusFilter,
  } from "../lib/appsList";
  import packageMetadata from "../../package.json";
  import type { Screen } from "../lib/router.svelte";
  import type { AppEntry, AppStorage, AppUsage, DeviceType, OtherPackage, Safety } from "../lib/types";
  import BottomTabs from "../components/BottomTabs.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import AppDetailSheet from "../components/AppDetailSheet.svelte";
  import AppActionMenu from "../components/AppActionMenu.svelte";
  import AppReportSheet from "../components/AppReportSheet.svelte";
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
  let activeFilter = $state<StatusFilter>("all");
  let showSystemApps = $state(false);
  // Desktop's default: a catalogue entry this TV doesn't have is noise until
  // the user asks for it.
  let hideNotInstalled = $state(true);
  let selectedPkg = $state<string | null>(null);
  // The sheet stays open after a successful uninstall so Reinstall is offered.
  let sheetUninstalled = $state(false);
  let showPaywall = $state(false);

  type ListIdentity = { serial: string; generation: number; token: number };
  type RemovalAction = "disable" | "uninstall";
  type RemovalIntent = ListIdentity & {
    action: RemovalAction;
    app: AppItem;
    verdict: Safety;
  };

  let listIdentity = $state<ListIdentity | null>(null);
  let removedIdentity = $state<(ListIdentity & { app: AppItem }) | null>(null);
  let removalIntent = $state<RemovalIntent | null>(null);
  let loadRequest = 0;
  let enrichmentRequest = 0;
  let safetyRequest = 0;
  let removalRequest = 0;
  let actionRequest = 0;
  let reportRequest = 0;
  let destroyed = false;
  let observedSession = "";

  let others = $state<OtherPackage[]>([]);
  let othersError = $state("");
  let catalog = $state<AppEntry[]>([]);
  let catalogStates = $state<Record<string, PackageState>>({});
  let catalogSafety = $state<Record<string, SafetyStatus>>({});
  let catalogError = $state("");
  // Lazily-loaded, best-effort enrichment maps for the detail sheet, each
  // stamped with when it was read.
  let memoryMap = $state<Record<string, number>>({});
  let usageMap = $state<Record<string, AppUsage>>({});
  let storageMap = $state<Record<string, AppStorage>>({});
  let measures = $state<AppMeasurements>(idleMeasurements());
  /// Verdicts the detail sheet resolved, so a report carries what was shown.
  let sheetVerdicts = $state<Record<string, Safety | null>>({});
  let lookupBusy = $state("");
  let busyAction = $state("");

  let menuApp = $state<AppItem | null>(null);
  let reportApp = $state<AppItem | null>(null);
  let reportVerdict = $state<Safety | null>(null);

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

  const catalogRows = $derived(catalogItems(catalog, catalogStates));
  const catalogPackages = $derived(new Set(catalogRows.map((i) => i.package)));
  const otherRows = $derived(otherItems(others, catalogPackages));
  const itemByPackage = $derived.by(() => {
    const map = new Map<string, AppItem>();
    for (const item of otherRows) map.set(item.package, item);
    for (const item of catalogRows) map.set(item.package, item);
    return map;
  });

  function currentTarget(input: AppItem): AppItem | null {
    if (!listCurrent()) return null;
    return itemByPackage.get(input.package) ?? null;
  }

  function removedTargetCurrent(input: AppItem): boolean {
    return (
      removedIdentity !== null &&
      removedIdentity.app.package === input.package &&
      sessionCurrent(removedIdentity.serial, removedIdentity.generation)
    );
  }

  function invalidateActions() {
    ++removalRequest;
    ++actionRequest;
    ++reportRequest;
    removalIntent = null;
    lookupBusy = "";
    busyAction = "";
    selectedPkg = null;
    sheetUninstalled = false;
    removedIdentity = null;
    menuApp = null;
    reportApp = null;
  }

  async function readCatalog(serial: string, deviceType: DeviceType) {
    const entries = await api.appListForDevice(deviceType);
    if (!Array.isArray(entries)) throw new Error("The recognised-app list is unavailable.");
    const packages = entries.map((entry) => entry.package);
    const raw = packages.length > 0 ? await api.packageStates(serial, packages) : {};
    return { entries, states: validatedStates(packages, raw) };
  }

  // Keep stale rows visible while refreshing, but make them inert until a
  // successful list load is bound to this exact connection generation. The
  // recognised list and everything else load together; either one failing
  // clears only its own section, so no stale row in it stays actionable.
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
    if (others.length === 0 && catalog.length === 0) loading = true;
    error = "";
    try {
      const [otherResult, catalogResult] = await Promise.allSettled([
        api.listOtherPackages(serial),
        readCatalog(serial, device.device_type),
      ]);
      if (request !== loadRequest || !sessionCurrent(serial, generation)) return;
      const identity = { serial, generation, token: request };

      if (otherResult.status === "fulfilled") {
        others = otherResult.value;
        othersError = "";
      } else {
        others = [];
        othersError = String(otherResult.reason);
      }
      if (catalogResult.status === "fulfilled") {
        catalog = catalogResult.value.entries;
        catalogStates = catalogResult.value.states;
        const unread = catalog.length - Object.keys(catalogStates).length;
        catalogError =
          unread > 0
            ? `State unavailable for ${unread} recognised ${unread === 1 ? "app" : "apps"}. Refresh before changing them.`
            : "";
      } else {
        catalog = [];
        catalogStates = {};
        catalogError = String(catalogResult.reason);
      }
      catalogSafety = {};

      if (otherResult.status === "rejected" && catalogResult.status === "rejected") {
        error = othersError;
        loaded = false;
        return;
      }
      loaded = true;
      listIdentity = identity;
      if (otherResult.status === "fulfilled") {
        const uncatalogued = otherResult.value.filter((app) => !catalogPackages.has(app.package));
        recordUnknownDiagnostics(
          uncatalogued.map((app) => ({
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
      }
      void loadCatalogSafety(identity);
      void loadEnrichment(identity);
    } finally {
      if (request === loadRequest && sessionCurrent(serial, generation)) loading = false;
    }
  }

  // Verdicts for the recognised rows' labels and suggestions. Display only:
  // every removal re-reads safety at the moment of the action.
  async function loadCatalogSafety(identity: ListIdentity) {
    const request = ++safetyRequest;
    const packages = catalog.map((entry) => entry.package);
    catalogSafety = Object.fromEntries(packages.map((pkg) => [pkg, { status: "checking" }]));
    const results = await Promise.allSettled(packages.map((pkg) => api.safetyInfo(pkg)));
    if (request !== safetyRequest || !listCurrent(identity)) return;
    catalogSafety = Object.fromEntries(
      packages.map((pkg, index) => {
        const result = results[index];
        return [
          pkg,
          result.status === "fulfilled"
            ? ({ status: "ready", verdict: result.value } satisfies SafetyStatus)
            : ({ status: "unavailable", reason: String(result.reason) } satisfies SafetyStatus),
        ];
      }),
    );
  }

  // Memory, usage and storage power the detail sheet's figures. Best-effort:
  // a failed read is shown as unavailable with when it failed, never as zero.
  async function loadEnrichment(identity: ListIdentity) {
    const request = ++enrichmentRequest;
    const current = () => request === enrichmentRequest && listCurrent(identity);
    measures = { memory: { status: "loading" }, usage: { status: "loading" }, storage: { status: "loading" } };
    const settle = (key: keyof AppMeasurements, status: MeasureStatus) => {
      if (current()) measures = { ...measures, [key]: status };
    };
    try {
      const memory = await api.appMemoryMap(identity.serial);
      if (current()) memoryMap = memory;
      settle("memory", { status: "ready", at: Date.now() });
    } catch (e) {
      settle("memory", { status: "unavailable", at: Date.now(), reason: String(e) });
    }
    try {
      const usage = await api.appUsageMap(identity.serial);
      if (current()) usageMap = usage;
      settle("usage", { status: "ready", at: Date.now() });
    } catch (e) {
      settle("usage", { status: "unavailable", at: Date.now(), reason: String(e) });
    }
    try {
      const storage = await api.appStorageMap(identity.serial);
      if (current()) storageMap = storage;
      settle("storage", { status: "ready", at: Date.now() });
    } catch (e) {
      settle("storage", { status: "unavailable", at: Date.now(), reason: String(e) });
    }
  }

  function remeasure() {
    const identity = listIdentity;
    if (listCurrent(identity)) void loadEnrichment(identity);
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
      ++safetyRequest;
      listIdentity = null;
      loaded = false;
      invalidateActions();
      memoryMap = {};
      usageMap = {};
      storageMap = {};
      measures = idleMeasurements();
      sheetVerdicts = {};
      catalogSafety = {};
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
    searchTimer = setTimeout(() => (debouncedQuery = normalizeQuery(q)), 120);
    return () => clearTimeout(searchTimer);
  });

  onDestroy(() => {
    destroyed = true;
    ++loadRequest;
    ++enrichmentRequest;
    ++safetyRequest;
    invalidateActions();
    clearTimeout(searchTimer);
    clearTimeout(toastTimer);
    clearTimeout(pressTimer);
  });

  const filters = $derived<ListFilters>({
    query: debouncedQuery,
    status: activeFilter,
    showSystem: showSystemApps,
    hideNotInstalled,
  });
  const visibleCatalog = $derived(filterCatalog(catalogRows, filters));
  const visibleOthers = $derived(filterOthers(otherRows, filters));
  const visibleCount = $derived(visibleCatalog.length + visibleOthers.length);
  const hidden = $derived(hiddenMatches(catalogRows, otherRows, filters));
  const systemAppCount = $derived(otherRows.filter((app) => app.system === true).length);
  const notInstalledCount = $derived(catalogRows.filter((app) => app.state === "missing").length);
  const countText = $derived(
    debouncedQuery
      ? `${visibleCount} ${visibleCount === 1 ? "result" : "results"}`
      : `${onDeviceCount(visibleCatalog) + onDeviceCount(visibleOthers)} of ${onDeviceCount(catalogRows) + onDeviceCount(otherRows)} visible`,
  );

  const sheetApp = $derived.by((): AppItem | null => {
    if (!selectedPkg) return null;
    if (sheetUninstalled && removedIdentity?.app.package === selectedPkg) return removedIdentity.app;
    return itemByPackage.get(selectedPkg) ?? null;
  });

  /// Friendly name when the backend actually has one, otherwise the package.
  /// Never the package's last segment — that invents names like "Tv".
  function label(app: AppItem): string {
    return app.name || app.package;
  }

  function rowRecommendation(app: AppItem): Recommendation | null {
    if (!app.entry) return null;
    const rec = recommendation(app.entry, app.state, catalogSafety[app.package]);
    // "No change needed" and "unavailable" on every row would drown out the
    // rows that do have a suggestion; the verdict chip already says the rest.
    // "Protected" is already the verdict chip.
    if (rec.kind === "keep" || rec.kind === "unavailable") return null;
    return rec.kind === "done" && isBlocked(verdictOf(catalogSafety[app.package])) ? null : rec;
  }

  function recText(rec: Recommendation): string {
    return rec.kind === "act" || rec.kind === "restore" ? `Suggested: ${rec.label}` : rec.label;
  }

  function patch(pkg: string, state: PackageState, identity: ListIdentity) {
    if (!listCurrent(identity)) return;
    if (catalogPackages.has(pkg)) {
      catalogStates = { ...catalogStates, [pkg]: state };
    } else if (state === "missing") {
      others = others.filter((a) => a.package !== pkg);
    } else {
      others = others.map((a) => (a.package === pkg ? { ...a, enabled: state === "enabled" } : a));
    }
  }

  function actionCurrent(request: number, identity: ListIdentity): boolean {
    return request === actionRequest && listCurrent(identity);
  }

  async function enableCurrent(input: AppItem) {
    if (busyAction) return;
    const app = currentTarget(input);
    const identity = listIdentity;
    if (!app || !identity || app.state !== "disabled") return;
    const request = ++actionRequest;
    busyAction = app.package;
    patch(app.package, "enabled", identity);
    selectedPkg = null;
    try {
      const r = await api.enablePackage(identity.serial, app.package);
      if (!actionCurrent(request, identity)) return;
      if (r.ok) {
        showToast(`Enabled ${label(app)}`, "success");
        session.invalidateAll();
      } else {
        // Not-ok can still have landed (stock enabled, its setup helper not),
        // so reload the real state instead of reverting the row.
        showToast(r.message || "Action failed.", "error");
        session.invalidateAll();
        await loadApps(true);
      }
    } catch (e) {
      if (!actionCurrent(request, identity)) return;
      patch(app.package, "disabled", identity);
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  function removalAllowed(app: AppItem, action: RemovalAction): boolean {
    if (action === "disable") return app.state === "enabled";
    if (app.state !== "enabled" && app.state !== "disabled") return false;
    return !app.entry || canOfferUninstall(app.entry);
  }

  async function requestRemoval(input: AppItem, action: RemovalAction) {
    if (busyAction || lookupBusy) return;
    const app = currentTarget(input);
    const identity = listIdentity;
    if (!app || !identity || !removalAllowed(app, action)) {
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
      selectedPkg = null;
    } catch {
      if (request !== removalRequest || !listCurrent(identity)) return;
      showToast("Safety unavailable. Retry before removing this app.", "error");
    } finally {
      if (request === removalRequest) lookupBusy = "";
    }
  }

  function handleToggle(app: AppItem) {
    if (app.state === "enabled") void requestRemoval(app, "disable");
    else if (app.state === "disabled") void enableCurrent(app);
  }

  function requestUninstall(app: AppItem) {
    void requestRemoval(app, "uninstall");
  }

  async function handleForceStop(input: AppItem) {
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

  async function handlePlayStore(input: AppItem) {
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
        patch(intent.app.package, "disabled", intent);
        selectedPkg = null;
      }
      const r =
        intent.action === "disable"
          ? await api.disablePackage(intent.serial, intent.app.package)
          : await api.uninstallPackage(intent.serial, intent.app.package);
      if (!actionCurrent(request, intent)) return;
      if (r.ok) {
        if (intent.action === "uninstall") {
          const recognised = catalogPackages.has(intent.app.package);
          patch(intent.app.package, "missing", intent);
          selectedPkg = intent.app.package;
          // A recognised row stays in the list as "not installed", which
          // already offers Reinstall; anything else leaves the list, so the
          // sheet keeps it to offer Reinstall here.
          if (!recognised) {
            removedIdentity = { ...intent, app: { ...intent.app, state: "missing" } };
            sheetUninstalled = true;
          }
        }
        showToast(`${intent.action === "disable" ? "Disabled" : "Uninstalled"} ${label(intent.app)}.`, "success");
        session.invalidateAll();
      } else {
        // A not-ok result can still have landed (stock disabled with its
        // setup helper left on), so reload the real state instead of
        // assuming nothing changed.
        showToast(r.message || `${intent.action === "disable" ? "Disable" : "Uninstall"} failed.`, "error");
        session.invalidateAll();
        await loadApps(true);
      }
    } catch (e) {
      if (!actionCurrent(request, intent)) return;
      if (intent.action === "disable") patch(intent.app.package, "enabled", intent);
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  // `install-existing` restores an APK that is still on the TV (the usual case
  // for a system app removed with `pm uninstall --user 0`). It cannot conjure
  // an app that was never installed — that's what the Play Store button is for.
  async function handleReinstall(app: AppItem) {
    if (busyAction) return;
    let identity: ListIdentity | null = null;
    if (sheetUninstalled && removedTargetCurrent(app) && removedIdentity) identity = removedIdentity;
    else if (currentTarget(app)?.state === "missing" && listIdentity) identity = listIdentity;
    if (!identity) return;
    const target = identity;
    const request = ++actionRequest;
    busyAction = app.package;
    try {
      const r = await api.reinstallExisting(target.serial, app.package);
      if (request !== actionRequest || !sessionCurrent(target.serial, target.generation)) return;
      if (r.ok) {
        showToast(`Reinstalled ${label(app)}.`, "success");
        selectedPkg = null;
        sheetUninstalled = false;
        removedIdentity = null;
        session.invalidateAll();
        await loadApps(true);
      } else {
        showToast(r.message || "Reinstall failed. Try the Play Store.", "error");
      }
    } catch (e) {
      if (request !== actionRequest || !sessionCurrent(target.serial, target.generation)) return;
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      if (request === actionRequest) busyAction = "";
    }
  }

  function openDetail(app: AppItem) {
    if (!listCurrent()) return;
    sheetUninstalled = false;
    removedIdentity = null;
    selectedPkg = app.package;
  }

  function closeDetail() {
    ++removalRequest;
    lookupBusy = "";
    selectedPkg = null;
    sheetUninstalled = false;
    removedIdentity = null;
  }

  function iconFor(app: AppItem): string {
    if (app.system) return "system_update";
    const n = (app.name ?? "").toLowerCase();
    return n.includes("video") || n.includes("tv") ? "smart_display" : "apps";
  }

  // ---- Report this app (#100) ----

  const reportDevice = $derived.by((): AppReportDevice | null => {
    const d = session.connectedDevice;
    if (!d) return null;
    return {
      family: reportFamily(d.device_type, d.tv_evidence),
      androidVersion: d.properties?.android_release || null,
      // A placeholder ro.serialno ("unknown") identifies nothing, and
      // scrubbing it would eat that word from the user's note.
      redact: [session.serial, d.serial, d.properties?.serial_number].filter(
        (id): id is string => !!normalizeHardwareId(id),
      ),
    };
  });

  function canReport(app: AppItem | null): boolean {
    return !!app && !!reportDevice && isReportablePackage(app.package);
  }

  function knownVerdict(pkg: string): Safety | null {
    return sheetVerdicts[pkg] ?? verdictOf(catalogSafety[pkg]) ?? null;
  }

  function openReport(app: AppItem) {
    if (!canReport(app)) return;
    const request = ++reportRequest;
    reportApp = app;
    reportVerdict = knownVerdict(app.package);
    if (reportVerdict) return;
    const serial = session.serial;
    const generation = session.generation;
    api.safetyInfo(app.package).then(
      (verdict) => {
        if (request === reportRequest && reportApp?.package === app.package && sessionCurrent(serial, generation)) {
          reportVerdict = verdict;
        }
      },
      () => {},
    );
  }

  function reportState(app: AppItem): AppReportState {
    const pkg = app.package;
    const memRead = measures.memory.status === "ready";
    const mb = memoryMap[pkg];
    const running = memRead ? mb !== undefined && mb > 0 : null;
    const stored = storageMap[pkg];
    return {
      status: app.state,
      running,
      ramMb: running ? (mb ?? null) : null,
      storage:
        measures.storage.status === "ready" && hasStorage(stored)
          ? { source: "diskstats", ...stored }
          : null,
    };
  }

  // ---- Long-press menu (#129's phone counterpart) ----

  let pressTimer: ReturnType<typeof setTimeout> | undefined;
  let pressOrigin: { x: number; y: number } | null = null;
  // The tap that ends a long-press must not also open the detail sheet.
  let suppressClickFor = "";

  function startPress(e: PointerEvent, app: AppItem) {
    suppressClickFor = "";
    if (e.pointerType === "mouse" && e.button !== 0) return;
    clearTimeout(pressTimer);
    pressOrigin = { x: e.clientX, y: e.clientY };
    pressTimer = setTimeout(() => {
      pressOrigin = null;
      suppressClickFor = app.package;
      openMenu(app);
    }, 480);
  }

  function movePress(e: PointerEvent) {
    if (pressOrigin && Math.hypot(e.clientX - pressOrigin.x, e.clientY - pressOrigin.y) > 10) cancelPress();
  }

  function cancelPress() {
    clearTimeout(pressTimer);
    pressOrigin = null;
  }

  function onRowContextMenu(e: MouseEvent, app: AppItem) {
    // The WebView's own long-press menu (and text selection) means nothing on
    // a row; ours replaces it.
    e.preventDefault();
    cancelPress();
    suppressClickFor = app.package;
    openMenu(app);
  }

  function onRowClick(app: AppItem) {
    if (suppressClickFor === app.package) {
      suppressClickFor = "";
      return;
    }
    openDetail(app);
  }

  function openMenu(app: AppItem) {
    if (menuApp?.package === app.package) return;
    try {
      navigator.vibrate?.(12);
    } catch {
      // Haptics are a nicety.
    }
    menuApp = app;
  }

  async function copyPackage(app: AppItem) {
    try {
      await navigator.clipboard.writeText(app.package);
      showToast("Copied package name.", "success");
    } catch {
      showToast("Couldn't reach the clipboard.", "error");
    }
  }

  const menuItems = $derived.by((): AppMenuItem[] => {
    const app = menuApp ? (itemByPackage.get(menuApp.package) ?? menuApp) : null;
    if (!app) return [];
    const live = listIdentity !== null && itemByPackage.has(app.package);
    const busy = busyAction !== "" || lookupBusy !== "";
    const verdict = verdictOf(catalogSafety[app.package]);
    const items: AppMenuItem[] = [
      { id: "details", label: "Details", icon: "info", disabled: !live, run: () => openDetail(app) },
    ];
    if (app.state === "enabled") {
      items.push({
        id: "disable",
        label: "Disable",
        icon: "block",
        danger: true,
        disabled: !live || busy || isBlocked(verdict),
        run: () => void requestRemoval(app, "disable"),
      });
    } else if (app.state === "disabled") {
      items.push({
        id: "enable",
        label: "Enable",
        icon: "check_circle",
        disabled: !live || busy,
        run: () => void enableCurrent(app),
      });
    }
    items.push({ id: "copy", label: "Copy package name", icon: "content_copy", run: () => void copyPackage(app) });
    if (canReport(app)) {
      items.push({ id: "report", label: "Report this app", icon: "warning", run: () => openReport(app) });
    }
    return items;
  });

  function rowSafety(app: AppItem): SafetyStatus | undefined {
    return catalogSafety[app.package];
  }

  function stateBadge(app: AppItem): { text: string; off: boolean } | null {
    if (app.state === "enabled") return { text: "ON", off: false };
    if (app.state === "disabled") return { text: "OFF", off: true };
    if (app.state === null) return { text: "?", off: true };
    return null;
  }

  const removalNote = $derived.by(() => {
    const intent = removalIntent;
    if (!intent || intent.action !== "uninstall") return "";
    return uninstallNote(intent.app.package, intent.app.entry) ?? "";
  });
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
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
      autocomplete="off"
      autocapitalize="off"
      spellcheck="false"
    />
  </div>

  <p class="search-hint">Search covers every app on the TV, by name or package. Long-press an app for quick actions.</p>

  <div class="filters-row">
    <button class="filter-chip" class:active={activeFilter === "all"} onclick={() => (activeFilter = "all")}>All</button>
    <button class="filter-chip" class:active={activeFilter === "enabled"} onclick={() => (activeFilter = "enabled")}>Enabled</button>
    <button class="filter-chip" class:active={activeFilter === "disabled"} onclick={() => (activeFilter = "disabled")}>Disabled</button>
  </div>

  <div class="toggles">
    <button
      class="system-toggle"
      role="switch"
      aria-checked={showSystemApps}
      onclick={() => (showSystemApps = !showSystemApps)}
    >
      <span class="system-toggle-copy">
        <span class="system-toggle-title">Show system apps</span>
        <span class="system-toggle-detail">
          {showSystemApps ? "Included in Everything else" : `${systemAppCount} hidden from Everything else`}
        </span>
      </span>
      <span class="switch-track" aria-hidden="true"><span class="switch-knob"></span></span>
    </button>
    <button
      class="system-toggle"
      role="switch"
      aria-checked={hideNotInstalled}
      onclick={() => (hideNotInstalled = !hideNotInstalled)}
    >
      <span class="system-toggle-copy">
        <span class="system-toggle-title">Hide not installed</span>
        <span class="system-toggle-detail">
          {hideNotInstalled
            ? `${notInstalledCount} recognised ${notInstalledCount === 1 ? "app isn't" : "apps aren't"} on this TV`
            : "Recognised apps missing from this TV are shown"}
        </span>
      </span>
      <span class="switch-track" aria-hidden="true"><span class="switch-knob"></span></span>
    </button>
  </div>

  {#if loading && others.length === 0 && catalog.length === 0}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading packages…</span>
    </div>
  {:else if error && others.length === 0 && catalog.length === 0}
    <p class="error">{error}</p>
    <button class="primary" onclick={() => loadApps(true)}>Retry</button>
  {:else}
    <div class="apps-list">
      {#snippet row(app: AppItem)}
        {@const safety = rowSafety(app)}
        {@const rec = rowRecommendation(app)}
        {@const badge = stateBadge(app)}
        <button
          class="app-row"
          class:selected={selectedPkg === app.package}
          class:missing={app.state === "missing"}
          data-package={app.package}
          onclick={() => onRowClick(app)}
          onpointerdown={(e) => startPress(e, app)}
          onpointermove={movePress}
          onpointerup={cancelPress}
          onpointercancel={cancelPress}
          onpointerleave={cancelPress}
          oncontextmenu={(e) => onRowContextMenu(e, app)}
        >
          <div class="app-avatar"><span class="msr">{iconFor(app)}</span></div>
          <div class="app-details">
            <span class="app-name-text" class:mono={!app.name}>{label(app)}</span>
            {#if app.name}
              <span class="mono app-pkg-text">{app.package}</span>
            {/if}
            {#if app.entry && app.description}
              <span class="app-desc">{app.description}</span>
            {/if}
            {#if app.entry}
              <span class="row-meta">
                {#if app.state === "missing"}
                  <span class="chip muted-chip">Not installed</span>
                {/if}
                <span
                  class="chip verdict-chip {safety?.status === 'ready' ? (tierOf(safety.verdict)?.cls ?? '') : safety?.status === 'checking' ? 'checking' : 'unavailable'}"
                >{safetyLabel(safety)}</span>
                {#if rec}
                  <span class="rec-text" class:act={rec.kind === "act" || rec.kind === "restore"} data-rec={rec.label}>{recText(rec)}</span>
                {/if}
              </span>
            {/if}
          </div>
          {#if badge}
            <span class="status-badge" class:off={badge.off}>{badge.text}</span>
          {/if}
          <span class="msr more-icon">more_vert</span>
        </button>
      {/snippet}

      {#if catalogError}
        <div class="section-error" role="alert">
          <span>{catalogError}</span>
          <button class="ghost small" onclick={() => loadApps(true)}>Retry</button>
        </div>
      {/if}
      {#if visibleCatalog.length > 0}
        <div class="section-head">
          <h4>Recognised apps <span class="mono">{visibleCatalog.length}</span></h4>
          <p>Apps we have notes on, with what removing each one costs.</p>
        </div>
        {#each visibleCatalog as app (app.package)}
          {@render row(app)}
        {/each}
      {/if}

      {#if othersError}
        <div class="section-error" role="alert">
          <span>{othersError}</span>
          <button class="ghost small" onclick={() => loadApps(true)}>Retry</button>
        </div>
      {/if}
      {#if visibleOthers.length > 0}
        <div class="section-head">
          <h4>Everything else <span class="mono">{visibleOthers.length}</span></h4>
          <p>Installed apps we have no notes on. Removals still go through the same safety checks.</p>
        </div>
        {#each visibleOthers as app (app.package)}
          {@render row(app)}
        {/each}
      {/if}

      {#if visibleCount === 0}
        <div class="empty-state">
          <div class="empty-message" role="status">
            <h4>{debouncedQuery ? "No matching apps" : "No apps to show"}</h4>
            <p>
              {#if debouncedQuery}
                No {activeFilter === "all" ? "" : `${activeFilter} `}apps match “{searchQuery.trim()}” in this view.
              {:else if activeFilter === "all"}
                No apps are available in this view.
              {:else}
                No {activeFilter} apps are available in this view.
              {/if}
            </p>
          </div>
          <div class="empty-actions">
            {#if debouncedQuery}
              <button class="ghost small" onclick={() => (searchQuery = "")}>Clear search</button>
            {/if}
            {#if hidden.system > 0}
              <button class="primary" onclick={() => (showSystemApps = true)}>
                {debouncedQuery ? "Show matching system apps" : "Show system apps"}
              </button>
            {/if}
            {#if hidden.notInstalled > 0}
              <button class="primary" onclick={() => (hideNotInstalled = false)}>
                Show {hidden.notInstalled} not installed
              </button>
            {/if}
          </div>
        </div>
      {/if}
    </div>
  {/if}

  <div class="spacer"></div>

  <AppDetailSheet
    app={sheetApp}
    memoryMb={sheetApp ? (memoryMap[sheetApp.package] ?? null) : null}
    usage={sheetApp ? (usageMap[sheetApp.package] ?? null) : null}
    storage={sheetApp ? (storageMap[sheetApp.package] ?? null) : null}
    {measures}
    onRemeasure={remeasure}
    busy={busyAction !== "" || lookupBusy !== ""}
    uninstalled={sheetUninstalled}
    canReport={canReport(sheetApp)}
    onClose={closeDetail}
    onToggle={handleToggle}
    onForceStop={handleForceStop}
    onUninstall={requestUninstall}
    onPlayStore={handlePlayStore}
    onReinstall={handleReinstall}
    onReport={openReport}
    onVerdict={(pkg, verdict) => (sheetVerdicts = { ...sheetVerdicts, [pkg]: verdict })}
  />

  <AppActionMenu app={menuApp} items={menuItems} onClose={() => (menuApp = null)} />

  {#if reportApp && reportDevice}
    <AppReportSheet
      package={reportApp.package}
      appName={reportApp.name}
      verdict={reportVerdict}
      device={reportDevice}
      appState={reportState(reportApp)}
      appVersion={packageMetadata.version}
      onClose={() => {
        ++reportRequest;
        reportApp = null;
      }}
    />
  {/if}

  <ConfirmDialog
    open={removalIntent !== null}
    danger
    icon={removalIntent?.action === "disable" ? "block" : "delete"}
    title={`${removalIntent?.action === "disable" ? "Disable" : "Uninstall"} ${removalIntent ? label(removalIntent.app) : "app"}?`}
    warning={removalIntent?.verdict.reason ?? ""}
    message={removalIntent?.action === "disable"
      ? `${verdictSummary(removalIntent.verdict)} Disable is reversible with Enable.`
      : `${verdictSummary(removalIntent?.verdict)} Uninstall removes the app for this TV's current user. Reinstall works only while its APK remains on the TV; otherwise use the Play Store.${removalNote ? ` ${removalNote}` : ""}`}
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
    flex-wrap: wrap;
    gap: 8px;
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

  .toggles {
    display: flex;
    flex-direction: column;
    margin-bottom: 14px;
    border: 1px solid var(--line);
    border-radius: 13px;
    background: var(--surface);
    overflow: hidden;
  }
  .system-toggle + .system-toggle {
    border-top: 1px solid var(--line);
  }
  .system-toggle {
    display: flex;
    align-items: center;
    gap: 12px;
    min-height: 46px;
    padding: 6px 12px;
    border: 0;
    border-radius: 0;
    background: transparent;
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
  .app-row {
    /* A long-press opens the action menu; it must not start a text
       selection or the WebView's own callout. */
    user-select: none;
    -webkit-user-select: none;
    -webkit-touch-callout: none;
    -webkit-tap-highlight-color: transparent;
  }
  .app-row.selected {
    border-color: var(--accent);
  }
  .app-row.missing .app-avatar,
  .app-row.missing .app-name-text {
    opacity: 0.6;
  }
  .app-desc {
    font-size: 11px;
    line-height: 1.35;
    color: var(--text-soft);
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    margin-top: 2px;
  }
  .row-meta {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 6px;
    margin-top: 4px;
    min-width: 0;
  }
  .chip {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 6px;
    white-space: nowrap;
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }
  .verdict-chip.safe {
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
  }
  .verdict-chip.caution,
  .verdict-chip.unavailable {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .verdict-chip.blocked {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .verdict-chip.checking {
    font-weight: 500;
  }
  .rec-text {
    font-size: 11px;
    color: var(--muted);
    min-width: 0;
  }
  .rec-text.act {
    color: var(--accent);
    font-weight: 600;
  }
  .section-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    margin: 6px 2px 0;
  }
  .section-head h4 {
    margin: 0;
    font-size: 13px;
    font-weight: 700;
    display: flex;
    align-items: baseline;
    gap: 6px;
  }
  .section-head h4 .mono {
    font-size: 11px;
    font-weight: 500;
    color: var(--muted);
  }
  .section-head p {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }
  .section-error {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid color-mix(in srgb, var(--amber) 40%, transparent);
    font-size: 12px;
    color: var(--amber);
  }
  .section-error span {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .section-error button {
    width: auto;
    flex: none;
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
