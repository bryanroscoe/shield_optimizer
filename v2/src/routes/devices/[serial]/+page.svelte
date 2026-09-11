<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { Channel } from "@tauri-apps/api/core";
  import { api } from "$lib/api";
  import type {
    Device,
    HealthReport,
    LauncherStatus,
    CurrentLauncher,
    AppEntry,
    SnapshotFile,
    SnapshotApplyPlan,
    ApplyResult,
    RecoveryResult,
    RebootMode,
    OtherPackage,
    ScreenshotResult,
    Safety,
  } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";
  import RamBadge from "$lib/components/RamBadge.svelte";
  import UsageBadge from "$lib/components/UsageBadge.svelte";
  import StateBadge from "$lib/components/StateBadge.svelte";
  import AppRow from "$lib/components/AppRow.svelte";
  import FilesTab from "$lib/components/FilesTab.svelte";
  import TweaksTab from "$lib/components/TweaksTab.svelte";
  import SideloadTab from "$lib/components/SideloadTab.svelte";
  import RemoteTab from "$lib/components/RemoteTab.svelte";
  import OptimizeTab from "$lib/components/OptimizeTab.svelte";

  let serial = $derived(decodeURIComponent($page.params.serial ?? ""));

  type Tab = "overview" | "health" | "launcher" | "apps" | "optimize" | "tweaks" | "remote" | "files" | "snapshot" | "sideload";
  let activeTab = $state<Tab>("overview");

  let device = $state<Device | null>(null);
  let deviceErr = $state<string | null>(null);

  let report = $state<HealthReport | null>(null);
  let reportLoading = $state(false);
  let reportErr = $state<string | null>(null);
  // Set when a device-state change in another tab makes the health report
  // stale, so it re-fetches (in the background, keeping current data on screen)
  // the next time the Memory tab is opened.
  let healthStale = $state(false);
  let reportLastRefreshed = $state<Date | null>(null);
  let liveRefresh = $state(false);
  let liveRefreshTimer: ReturnType<typeof setInterval> | null = null;
  const LIVE_REFRESH_INTERVAL_MS = 3000;
  type PackageState = "enabled" | "disabled" | "missing";
  type SafetyStatus =
    | { status: "checking" }
    | { status: "ready"; verdict: Safety }
    | { status: "unavailable"; reason: string };
  type PageContext = { serial: string; epoch: number };
  let memorySafety = $state<Record<string, SafetyStatus>>({});
  let packageSafety = $state<Record<string, SafetyStatus>>({});
  let pageEpoch = $state(0);
  let deviceRequest = 0;
  let healthRequest = 0;
  let appsRequest = 0;
  let otherRequest = 0;
  let enrichmentRequest = 0;
  let mutationRequest = 0;
  let destroyed = false;

  function capturePageContext(): PageContext {
    return { serial, epoch: pageEpoch };
  }

  function pageContextIsCurrent(context: PageContext): boolean {
    return !destroyed && context.serial === serial && context.epoch === pageEpoch;
  }

  function safetyLabel(safety: SafetyStatus | undefined): string {
    if (!safety || safety.status === "unavailable") return "Unavailable";
    if (safety.status === "checking") return "Checking";
    if (safety.verdict.kind === "never_disable") return "Protected";
    if (safety.verdict.kind === "caution") return "Caution";
    return "Unknown";
  }

  function safetyReason(safety: SafetyStatus | undefined): string {
    if (!safety) return "Safety lookup has not completed.";
    if (safety.status === "checking") return "Safety lookup is in progress.";
    if (safety.status === "unavailable") return `Safety lookup failed: ${safety.reason}`;
    return safety.verdict.reason;
  }

  let renaming = $state(false);
  let renameValue = $state("");
  let renameBusy = $state(false);

  let screenshotBusy = $state(false);
  let screenshot = $state<ScreenshotResult | null>(null);

  let trimBusy = $state(false);
  let trimMessage = $state("");

  let launchers = $state<LauncherStatus[]>([]);
  // "Loaded" flags track load *attempts* — an empty result is a valid loaded
  // state. Guarding the lazy-load effect on `length === 0` instead re-fetches
  // forever when a list is legitimately empty (e.g. zero snapshots).
  let launchersLoaded = $state(false);
  let currentLauncher = $state<CurrentLauncher | null>(null);
  let channelDisabled = $state<boolean | null>(null);
  let launcherLoading = $state(false);
  let launcherErr = $state<string | null>(null);
  let launcherActionBusy = $state<string | null>(null); // package id currently being acted on
  let launcherActionMessage = $state("");
  let launcherProgress = $state(""); // live per-step status while a switch is in flight

  let apps = $state<AppEntry[]>([]);
  let appsLoaded = $state(false);
  let appsLoading = $state(false);
  let appsErr = $state<string | null>(null);
  /// Missing or invalid entries stay absent: absence means inventory unavailable.
  let appStates = $state<Record<string, PackageState>>({});
  let catalogInventoryVersion = 0;
  let appSearch = $state("");
  // Default on: the catalog lists ~70 known apps, most not present on any given
  // device, so an unfiltered list is mostly un-actionable "Missing" rows. Start
  // focused on what's installed; unticking reveals the full catalog.
  let hideNotInstalled = $state(true);
  let showSystemOthers = $state(false);
  /// Installed packages not in the curated catalog (sideloaded apps like
  /// SmartTube + system internals). Loaded lazily on the Apps tab.
  let otherPackages = $state<OtherPackage[]>([]);
  let othersLoading = $state(false);
  let othersLoaded = $state(false);
  let othersErr = $state<string | null>(null);
  let otherInventoryVersion = 0;
  /// package → resident RAM (MB) for apps running right now. Lazy-loaded after
  /// the list paints; most apps aren't here (not running), so a value means the
  /// app is actively holding RAM — the cue for "disable this unused app".
  let appMemory = $state<Record<string, number>>({});
  /// package → last-used / launch count, lazy-loaded alongside RAM. Powers the
  /// "remove if unused" signal (never opened / months idle).
  let appUsage = $state<Record<string, import("$lib/types").AppUsage>>({});

  function matchesSearch(name: string, pkg: string): boolean {
    const q = appSearch.trim().toLowerCase();
    if (!q) return true;
    return name.toLowerCase().includes(q) || pkg.toLowerCase().includes(q);
  }

  let visibleApps = $derived(
    apps.filter((a) => {
      if (hideNotInstalled && appStates[a.package] === "missing") return false;
      return matchesSearch(a.name, a.package);
    }),
  );
  let visibleOthers = $derived(
    otherPackages.filter((o) => {
      if (!showSystemOthers && o.system) return false;
      return matchesSearch(o.name ?? o.package, o.package);
    }),
  );
  let appActionBusy = $state<string | null>(null);
  let appActionMessage = $state("");
  let appMutationInFlight = $state(false);
  /// Package the "Copy to another device" panel is open for, plus targets.
  let clonePkg = $state<string | null>(null);
  let cloneTargets = $state<Device[]>([]);
  let cloneBusy = $state(false);

  let snapshots = $state<SnapshotFile[]>([]);
  let snapshotsLoaded = $state(false);
  let snapshotsErr = $state<string | null>(null);
  let saveBusy = $state(false);
  let saveResult = $state<string>("");
  let previewPath = $state<string | null>(null);
  let preview = $state<SnapshotApplyPlan | null>(null);
  let previewBusy = $state(false);
  let previewErr = $state<string | null>(null);

  // Header actions: reboot menu visibility, disconnect/reboot status, recovery.
  let rebootMenuOpen = $state(false);
  let rebootBusy = $state(false);
  let headerActionMsg = $state<string>("");
  let disconnectBusy = $state(false);

  let recoveryBusy = $state(false);
  let recoveryResult = $state<RecoveryResult | null>(null);
  let recoveryErr = $state<string | null>(null);

  // Apply snapshot (confirm step after preview).
  let applyBusy = $state(false);
  let applyResult = $state<ApplyResult | null>(null);
  let applyErr = $state<string | null>(null);

  // OptimizeTab caches a plan keyed on the installed/disabled sets. Bumping
  // this token (after an App List action, snapshot apply, or panic recovery)
  // tells the tab to drop that stale plan and reload fresh next run.
  let optimizeResetToken = $state(0);

  async function loadDevice() {
    const context = capturePageContext();
    const request = ++deviceRequest;
    deviceErr = null;
    try {
      const nextDevice = await api.deviceProfile(context.serial);
      if (!pageContextIsCurrent(context) || request !== deviceRequest) return;
      device = nextDevice;
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== deviceRequest) return;
      deviceErr = String(e);
    }
  }

  async function clearCaches() {
    trimBusy = true;
    trimMessage = "";
    try {
      const r = await api.trimCaches(serial);
      trimMessage = r.ok ? "App caches cleared." : r.message.trim();
      if (r.ok) await loadHealth();
    } catch (e) {
      trimMessage = String(e);
    } finally {
      trimBusy = false;
    }
  }

  async function loadHealth() {
    const context = capturePageContext();
    const request = ++healthRequest;
    reportLoading = true;
    reportErr = null;
    memorySafety = {};
    try {
      const nextReport = await api.healthReport(context.serial);
      if (!pageContextIsCurrent(context) || request !== healthRequest) return;
      report = nextReport;
      reportLastRefreshed = new Date();
      const pkgs = nextReport.top_memory.map((m) => m.package);
      memorySafety = Object.fromEntries(pkgs.map((pkg) => [pkg, { status: "checking" }]));
      const results = await Promise.allSettled(pkgs.map((pkg) => api.safetyInfo(pkg)));
      if (!pageContextIsCurrent(context) || request !== healthRequest || report !== nextReport) return;
      memorySafety = Object.fromEntries(pkgs.map((pkg, index) => {
        const result = results[index];
        return result.status === "fulfilled"
          ? [pkg, { status: "ready", verdict: result.value } satisfies SafetyStatus]
          : [pkg, { status: "unavailable", reason: String(result.reason) } satisfies SafetyStatus];
      }));
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== healthRequest) return;
      reportErr = String(e);
    } finally {
      if (pageContextIsCurrent(context) && request === healthRequest) reportLoading = false;
    }
  }

  function toggleLiveRefresh() {
    liveRefresh = !liveRefresh;
    if (liveRefresh) {
      // Don't double-fire if a manual refresh is in flight; the interval also
      // checks before firing.
      liveRefreshTimer = setInterval(() => {
        if (!reportLoading) loadHealth();
      }, LIVE_REFRESH_INTERVAL_MS);
    } else if (liveRefreshTimer) {
      clearInterval(liveRefreshTimer);
      liveRefreshTimer = null;
    }
  }

  function relativeRefreshLabel(d: Date | null): string {
    if (!d) return "never";
    const secs = Math.max(0, Math.floor((Date.now() - d.getTime()) / 1000));
    if (secs < 5) return "just now";
    if (secs < 60) return `${secs}s ago`;
    if (secs < 3600) return `${Math.floor(secs / 60)}m ago`;
    return `${Math.floor(secs / 3600)}h ago`;
  }

  // Force the relative-time label to re-render every second when live-refresh
  // is on. The derived value depends on `now` so the template reactively
  // updates without us re-binding anything.
  let now = $state(Date.now());
  let nowTicker: ReturnType<typeof setInterval> | null = null;
  let refreshLabel = $derived.by(() => {
    void now; // touch to register reactivity
    return reportLastRefreshed ? `Updated ${relativeRefreshLabel(reportLastRefreshed)}` : "Not loaded";
  });
  $effect(() => {
    if (liveRefresh && !nowTicker) {
      nowTicker = setInterval(() => (now = Date.now()), 1000);
    } else if (!liveRefresh && nowTicker) {
      clearInterval(nowTicker);
      nowTicker = null;
    }
  });

  onDestroy(() => {
    destroyed = true;
    pageEpoch++;
    deviceRequest++;
    healthRequest++;
    appsRequest++;
    otherRequest++;
    enrichmentRequest++;
    mutationRequest++;
    if (liveRefreshTimer) clearInterval(liveRefreshTimer);
    if (nowTicker) clearInterval(nowTicker);
  });

  async function loadLauncher() {
    launcherLoading = true;
    launcherErr = null;
    try {
      const [list, cur, chan] = await Promise.all([
        api.listLaunchers(serial),
        api.currentLauncher(serial),
        api.channelProviderDisabled(serial),
      ]);
      launchers = list;
      currentLauncher = cur;
      channelDisabled = chan;
    } catch (e) {
      launcherErr = String(e);
    } finally {
      launcherLoading = false;
      launchersLoaded = true;
    }
  }

  async function loadApps() {
    if (!device) return;
    const context = capturePageContext();
    const request = ++appsRequest;
    otherRequest++;
    enrichmentRequest++;
    mutationRequest++;
    catalogInventoryVersion++;
    otherInventoryVersion++;
    const deviceType = device.device_type;
    appsLoading = true;
    appsErr = null;
    appsLoaded = false;
    apps = [];
    appStates = {};
    packageSafety = {};
    otherPackages = [];
    othersLoaded = false;
    othersLoading = false;
    othersErr = null;
    appMemory = {};
    appUsage = {};
    if (appMutationInFlight) appActionBusy = null;
    appActionMessage = "";
    try {
      const list = await api.appListForDevice(deviceType);
      if (!pageContextIsCurrent(context) || request !== appsRequest) return;
      apps = list;
      const packages = list.map((a) => a.package);
      packageSafety = Object.fromEntries(packages.map((pkg) => [pkg, { status: "checking" }]));
      const [stateResult, safetyResults] = await Promise.all([
        api.packageStates(context.serial, packages),
        Promise.allSettled(packages.map((pkg) => api.safetyInfo(pkg))),
      ]);
      if (!pageContextIsCurrent(context) || request !== appsRequest) return;
      appStates = validatedPackageStates(packages, stateResult);
      catalogInventoryVersion++;
      const unavailableCount = packages.length - Object.keys(appStates).length;
      if (unavailableCount > 0) appsErr = `State unavailable for ${unavailableCount} package(s). Refresh before taking action.`;
      packageSafety = Object.fromEntries(packages.map((pkg, index) => {
        const result = safetyResults[index];
        return result.status === "fulfilled"
          ? [pkg, { status: "ready", verdict: result.value } satisfies SafetyStatus]
          : [pkg, { status: "unavailable", reason: String(result.reason) } satisfies SafetyStatus];
      }));
      appsLoaded = true;
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== appsRequest) return;
      appsErr = `Inventory unavailable: ${e}. Refresh to retry.`;
      appsLoaded = true;
    } finally {
      if (pageContextIsCurrent(context) && request === appsRequest) appsLoading = false;
    }
    if (pageContextIsCurrent(context) && request === appsRequest) {
      void loadOtherPackages();
      void loadAppMemory();
    }
  }

  /// Lazy RAM + last-used annotations: one `dumpsys meminfo` and one
  /// `dumpsys usagestats`, mapped onto the rows. Run after the list paints and
  /// never block it — a failure just leaves those cues off.
  async function loadAppMemory() {
    const context = capturePageContext();
    const request = ++enrichmentRequest;
    const [mem, usage] = await Promise.allSettled([
      api.appMemoryMap(context.serial),
      api.appUsageMap(context.serial),
    ]);
    if (!pageContextIsCurrent(context) || request !== enrichmentRequest) return;
    appMemory = mem.status === "fulfilled" ? mem.value : {};
    appUsage = usage.status === "fulfilled" ? usage.value : {};
  }

  // Everything installed that isn't in the curated catalog — sideloaded apps
  // (SmartTube etc.) plus system internals. Loaded after the catalog so the
  // curated list paints first; failures here don't block the main list.
  async function loadOtherPackages() {
    const context = capturePageContext();
    const request = ++otherRequest;
    mutationRequest++;
    otherInventoryVersion++;
    if (appMutationInFlight) appActionBusy = null;
    appActionMessage = "";
    othersLoading = true;
    othersLoaded = false;
    othersErr = null;
    otherPackages = [];
    try {
      const list = await api.listOtherPackages(context.serial);
      if (!pageContextIsCurrent(context) || request !== otherRequest) return;
      otherPackages = list;
      othersLoaded = true;
      otherInventoryVersion++;
      const packages = list.map((entry) => entry.package);
      const next = { ...packageSafety };
      packages.forEach((pkg) => (next[pkg] = { status: "checking" }));
      packageSafety = next;
      const results = await Promise.allSettled(packages.map((pkg) => api.safetyInfo(pkg)));
      if (!pageContextIsCurrent(context) || request !== otherRequest) return;
      const resolved = { ...packageSafety };
      packages.forEach((pkg, index) => {
        const result = results[index];
        resolved[pkg] = result.status === "fulfilled"
          ? { status: "ready", verdict: result.value }
          : { status: "unavailable", reason: String(result.reason) };
      });
      packageSafety = resolved;
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== otherRequest) return;
      othersErr = `Other-package inventory unavailable: ${e}. Refresh to retry.`;
    } finally {
      if (pageContextIsCurrent(context) && request === otherRequest) othersLoading = false;
    }
  }

  function patchOtherState(pkg: string, enabled: boolean | "removed") {
    if (enabled === "removed") {
      otherPackages = otherPackages.filter((o) => o.package !== pkg);
    } else {
      otherPackages = otherPackages.map((o) => (o.package === pkg ? { ...o, enabled } : o));
    }
    otherInventoryVersion++;
  }

  function catalogStateIsCurrent(pkg: string, state: PackageState, version: number): boolean {
    return version === catalogInventoryVersion
      && apps.some((entry) => entry.package === pkg)
      && appStates[pkg] === state;
  }

  function otherStateIsCurrent(pkg: string, enabled: boolean, version: number): boolean {
    return version === otherInventoryVersion
      && othersLoaded
      && otherPackages.some((entry) => entry.package === pkg && entry.enabled === enabled);
  }

  async function disableOther(pkg: string) {
    await removeInstalledPackage("other", pkg, "disable");
  }

  async function enableOther(pkg: string) {
    if (appActionBusy || appMutationInFlight) return;
    const context = capturePageContext();
    const request = ++mutationRequest;
    const inventoryVersion = otherInventoryVersion;
    if (!otherStateIsCurrent(pkg, false, inventoryVersion)) {
      appActionMessage = `${pkg}: current disabled-state evidence is unavailable. Refresh to retry.`;
      return;
    }
    appMutationInFlight = true;
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.enablePackage(context.serial, pkg);
      if (!pageContextIsCurrent(context) || request !== mutationRequest || !otherStateIsCurrent(pkg, false, inventoryVersion)) return;
      appActionMessage = `${pkg}: ${r.message.trim() || (r.ok ? "enabled" : "failed")}`;
      if (r.ok) {
        appActionBusy = null;
        patchOtherState(pkg, true);
        invalidateDeviceCaches();
      }
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== mutationRequest || !otherStateIsCurrent(pkg, false, inventoryVersion)) return;
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appMutationInFlight = false;
      if (pageContextIsCurrent(context)
        && request === mutationRequest
        && otherStateIsCurrent(pkg, false, inventoryVersion)
        && appActionBusy === pkg) appActionBusy = null;
    }
  }

  async function uninstallOther(pkg: string) {
    await removeInstalledPackage("other", pkg, "uninstall");
  }

  // Real state per package — one batched backend call (pm list packages +
  // pm list packages -d in parallel) so we can show Enabled/Disabled/Missing.
  function validatedPackageStates(packages: string[], value: unknown): Record<string, PackageState> {
    const result: Record<string, PackageState> = {};
    if (!value || typeof value !== "object" || Array.isArray(value)) return result;
    const record = value as Record<string, unknown>;
    for (const pkg of packages) {
      const state = record[pkg];
      if (state === "enabled" || state === "disabled" || state === "missing") result[pkg] = state;
    }
    return result;
  }

  async function fetchAppStates(context: PageContext, packages: string[]): Promise<Record<string, PackageState>> {
    if (packages.length === 0) return {};
    return validatedPackageStates(packages, await api.packageStates(context.serial, packages));
  }

  /// Re-sync the App List's cached states after the Optimize wizard runs —
  /// it cached states before the run, same as executeOptimize used to do inline.
  async function resyncAppStates() {
    const context = capturePageContext();
    const request = ++appsRequest;
    if (apps.length > 0) {
      try {
        const packages = apps.map((a) => a.package);
        const next = await fetchAppStates(context, packages);
        if (!pageContextIsCurrent(context) || request !== appsRequest) return;
        appStates = next;
        catalogInventoryVersion++;
        appsErr = Object.keys(next).length === packages.length
          ? null
          : "Some package states are unavailable. Refresh before taking action.";
      } catch (e) {
        if (!pageContextIsCurrent(context) || request !== appsRequest) return;
        appStates = {};
        appsErr = `Inventory unavailable: ${e}. Refresh to retry.`;
      }
    }
    // The Optimize wizard can disable launchers and many packages — mark the
    // Launcher and Memory caches stale so they reload fresh on next visit.
    invalidateDeviceCaches();
  }

  type RemovalSource = "catalog" | "other";
  type RemovalAction = "disable" | "uninstall";

  function removalSourceIsCurrent(source: RemovalSource, pkg: string, version: number): boolean {
    if (source === "catalog") {
      return version === catalogInventoryVersion
        && apps.some((entry) => entry.package === pkg)
        && (appStates[pkg] === "enabled" || appStates[pkg] === "disabled");
    }
    return version === otherInventoryVersion
      && othersLoaded
      && otherPackages.some((entry) => entry.package === pkg);
  }

  function removalName(source: RemovalSource, pkg: string): string {
    return source === "catalog"
      ? apps.find((entry) => entry.package === pkg)?.name ?? pkg
      : otherPackages.find((entry) => entry.package === pkg)?.name ?? pkg;
  }

  async function readRemovalEvidence(context: PageContext, pkg: string): Promise<{ state: PackageState; safety: Safety }> {
    const [rawStates, safety] = await Promise.all([
      api.packageStates(context.serial, [pkg]),
      api.safetyInfo(pkg),
    ]);
    const state = validatedPackageStates([pkg], rawStates)[pkg];
    if (state !== "enabled" && state !== "disabled") {
      throw new Error("Current installed-state evidence is unavailable");
    }
    return { state, safety };
  }

  async function removeInstalledPackage(source: RemovalSource, pkg: string, action: RemovalAction) {
    if (appActionBusy || appMutationInFlight) return;
    const displayedSafety = packageSafety[pkg];
    if (displayedSafety?.status !== "ready") {
      appActionMessage = `${pkg}: safety unavailable. Refresh before ${action}.`;
      return;
    }
    if (displayedSafety.verdict.kind === "never_disable") {
      appActionMessage = `${pkg}: protected — ${displayedSafety.verdict.reason}`;
      return;
    }
    const context = capturePageContext();
    const request = ++mutationRequest;
    const inventoryVersion = source === "catalog" ? catalogInventoryVersion : otherInventoryVersion;
    if (!removalSourceIsCurrent(source, pkg, inventoryVersion)) {
      appActionMessage = `${pkg}: current inventory evidence is unavailable. Refresh to retry.`;
      return;
    }
    const name = removalName(source, pkg);
    appMutationInFlight = true;
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const before = await readRemovalEvidence(context, pkg);
      if (!pageContextIsCurrent(context)
        || request !== mutationRequest
        || !removalSourceIsCurrent(source, pkg, inventoryVersion)) return;
      if (before.safety.kind === "never_disable") {
        appActionMessage = `${pkg}: protected — ${before.safety.reason}`;
        return;
      }
      if (before.safety.kind !== displayedSafety.verdict.kind
        || before.safety.reason !== displayedSafety.verdict.reason) {
        appActionMessage = `${pkg}: safety changed. Refresh and review before ${action}.`;
        return;
      }
      const approved = confirm(
        `${action.toUpperCase()} ${name}\nPackage: ${pkg}\nSafety: ${before.safety.kind === "caution" ? "Caution" : "Unknown"}\nReason: ${before.safety.reason}\n\nProceed?`,
      );
      if (!approved
        || !pageContextIsCurrent(context)
        || request !== mutationRequest
        || !removalSourceIsCurrent(source, pkg, inventoryVersion)) return;
      const after = await readRemovalEvidence(context, pkg);
      if (!pageContextIsCurrent(context)
        || request !== mutationRequest
        || !removalSourceIsCurrent(source, pkg, inventoryVersion)) return;
      if (after.safety.kind === "never_disable"
        || after.safety.kind !== before.safety.kind
        || after.safety.reason !== before.safety.reason) {
        appActionMessage = `${pkg}: safety changed after confirmation. No action was taken; refresh and review again.`;
        return;
      }
      const result = action === "disable"
        ? await api.disablePackage(context.serial, pkg)
        : await api.uninstallPackage(context.serial, pkg);
      if (!pageContextIsCurrent(context)
        || request !== mutationRequest
        || !removalSourceIsCurrent(source, pkg, inventoryVersion)) return;
      appActionMessage = `${pkg}: ${result.message.trim() || (result.ok ? action === "disable" ? "disabled" : "uninstalled" : "failed")}`;
      if (!result.ok) return;
      appActionBusy = null;
      if (source === "catalog") setCatalogState(pkg, action === "disable" ? "disabled" : "missing");
      else patchOtherState(pkg, action === "disable" ? false : "removed");
      invalidateDeviceCaches();
    } catch (e) {
      if (!pageContextIsCurrent(context)
        || request !== mutationRequest
        || !removalSourceIsCurrent(source, pkg, inventoryVersion)) return;
      appActionMessage = `${pkg}: ${e}. Refresh to retry; no removal was dispatched without current evidence.`;
    } finally {
      appMutationInFlight = false;
      if (pageContextIsCurrent(context)
        && request === mutationRequest
        && removalSourceIsCurrent(source, pkg, inventoryVersion)
        && appActionBusy === pkg) appActionBusy = null;
    }
  }

  /// Record a curated app's new on-device state and keep the two tabs in
  /// parity: the Optimize plan baked in the old installed/disabled sets, so drop
  /// it — it reloads fresh next time the Optimize tab is opened.
  function setCatalogState(pkg: string, state: "enabled" | "disabled" | "missing") {
    appStates[pkg] = state;
    catalogInventoryVersion++;
    optimizeResetToken++;
  }

  async function disableApp(pkg: string) {
    await removeInstalledPackage("catalog", pkg, "disable");
  }

  async function enableApp(pkg: string) {
    if (appActionBusy || appMutationInFlight) return;
    const context = capturePageContext();
    const request = ++mutationRequest;
    const inventoryVersion = catalogInventoryVersion;
    if (!catalogStateIsCurrent(pkg, "disabled", inventoryVersion)) {
      appActionMessage = `${pkg}: current disabled-state evidence is unavailable. Refresh to retry.`;
      return;
    }
    appMutationInFlight = true;
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.enablePackage(context.serial, pkg);
      if (!pageContextIsCurrent(context) || request !== mutationRequest || !catalogStateIsCurrent(pkg, "disabled", inventoryVersion)) return;
      appActionMessage = `${pkg}: ${r.message.trim()}`;
      if (r.ok) {
        appActionBusy = null;
        setCatalogState(pkg, "enabled");
        invalidateDeviceCaches();
      }
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== mutationRequest || !catalogStateIsCurrent(pkg, "disabled", inventoryVersion)) return;
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appMutationInFlight = false;
      if (pageContextIsCurrent(context)
        && request === mutationRequest
        && catalogStateIsCurrent(pkg, "disabled", inventoryVersion)
        && appActionBusy === pkg) appActionBusy = null;
    }
  }

  async function uninstallApp(pkg: string) {
    await removeInstalledPackage("catalog", pkg, "uninstall");
  }

  // Best-effort system-app re-install via `cmd package install-existing` —
  // works only for apps still present on /system. For third-party uninstalls
  // we route through the Play Store instead.
  async function reinstallApp(pkg: string) {
    if (appActionBusy || appMutationInFlight) return;
    const context = capturePageContext();
    const request = ++mutationRequest;
    const inventoryVersion = catalogInventoryVersion;
    if (!catalogStateIsCurrent(pkg, "missing", inventoryVersion)) {
      appActionMessage = `${pkg}: current missing-state evidence is unavailable. Refresh to retry.`;
      return;
    }
    appMutationInFlight = true;
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.reinstallExisting(context.serial, pkg);
      if (!pageContextIsCurrent(context) || request !== mutationRequest || !catalogStateIsCurrent(pkg, "missing", inventoryVersion)) return;
      appActionMessage = `${pkg}: ${r.message.trim()}`;
      if (r.ok) {
        appActionBusy = null;
        setCatalogState(pkg, "enabled");
      }
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== mutationRequest || !catalogStateIsCurrent(pkg, "missing", inventoryVersion)) return;
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appMutationInFlight = false;
      if (pageContextIsCurrent(context)
        && request === mutationRequest
        && catalogStateIsCurrent(pkg, "missing", inventoryVersion)
        && appActionBusy === pkg) appActionBusy = null;
    }
  }

  type Recommendation =
    | { kind: "done"; label: string }
    | { kind: "act"; label: string; action: "disable" | "uninstall" }
    | { kind: "review"; label: string; action: "disable" | "uninstall" }
    | { kind: "restore"; label: string }
    | { kind: "unavailable"; label: string }
    | { kind: "keep" };

  /// The action actually safe to offer — mirrors the engine's AppEntry::
  /// safe_method: never uninstall an app you can't get back (not on the Play
  /// Store and not defunct), downgrade to the reversible disable instead.
  function effectiveMethod(a: AppEntry): "disable" | "uninstall" {
    return a.method === "uninstall" && !(a.play_store || a.defunct) ? "disable" : a.method;
  }

  /// What the wizard would suggest for this row, given its current on-device
  /// state. `act` = a recommended (default) action. `review` = a "remove if you
  /// don't use it" candidate (optional, never a default). `restore` = the app
  /// is gone and would be brought back. `done`/`keep` = nothing to do.
  function recommendation(a: AppEntry, state: PackageState | null, safety: SafetyStatus | undefined): Recommendation {
    if (state === null) return { kind: "unavailable", label: "State unavailable" };
    if (state === "missing") {
      if (a.default_restore) return { kind: "restore", label: "Reinstall" };
      if (a.default_optimize && a.method === "uninstall") return { kind: "done", label: "Already uninstalled" };
      return { kind: "keep" };
    }
    const method = effectiveMethod(a);
    if (safety?.status !== "ready") return { kind: "unavailable", label: "Safety unavailable" };
    if (safety.verdict.kind === "never_disable") return { kind: "done", label: "Protected" };
    if (safety.verdict.kind === "unknown") {
      return state === "enabled"
        ? { kind: "review", label: `${method === "disable" ? "Disable" : "Remove"} after review`, action: method }
        : { kind: "keep" };
    }
    if (a.default_optimize) {
      if (method === "disable") {
        return state === "disabled"
          ? { kind: "done", label: "Already disabled" }
          : { kind: "act", label: "Disable", action: "disable" };
      }
      return { kind: "act", label: "Uninstall", action: "uninstall" };
    }
    if (a.review && state === "enabled") {
      return method === "disable"
        ? { kind: "review", label: "Disable if unused", action: "disable" }
        : { kind: "review", label: "Remove if unused", action: "uninstall" };
    }
    return { kind: "keep" };
  }

  function applyRecommendation(pkg: string, action: "disable" | "uninstall") {
    if (action === "disable") return disableApp(pkg);
    return uninstallApp(pkg);
  }

  async function backupApkFor(pkg: string) {
    const folder = await openDialog({ directory: true, title: "Choose a folder for the APK backup" });
    if (!folder) return;
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.backupApk(serial, pkg, folder as string);
      appActionMessage = r.message;
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  async function startClone(pkg: string) {
    appActionMessage = "";
    try {
      const all = await api.listDevices();
      cloneTargets = all.filter((d) => d.status === "device" && d.serial !== serial);
    } catch (e) {
      appActionMessage = String(e);
      return;
    }
    if (cloneTargets.length === 0) {
      appActionMessage = "No other connected device to copy to — connect the target device first.";
      return;
    }
    clonePkg = pkg;
  }

  async function cloneTo(target: Device) {
    if (!clonePkg) return;
    const pkg = clonePkg;
    if (
      !confirm(
        `Copy ${pkg} to ${target.name} (${target.serial})?\n\nApp data does not transfer, and DRM/licensed apps may refuse to run. Paid apps should be installed via the Play Store instead.`,
      )
    )
      return;
    cloneBusy = true;
    appActionBusy = pkg;
    appActionMessage = `Copying ${pkg} to ${target.serial}… (this pulls the APK and can take a minute)`;
    try {
      const r = await api.cloneApp(serial, target.serial, pkg);
      appActionMessage = r.hint ? `${r.message}\n→ ${r.hint}` : r.message;
      if (r.ok) clonePkg = null;
    } catch (e) {
      appActionMessage = String(e);
    } finally {
      cloneBusy = false;
      appActionBusy = null;
    }
  }

  async function openInPlayStore(pkg: string) {
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.openPlayStore(serial, pkg);
      appActionMessage = r.ok
        ? `Opened Play Store on device for ${pkg} — confirm install on the TV.`
        : `${pkg}: ${r.message.trim()}`;
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  async function installLauncherFromStore(pkg: string) {
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    try {
      const r = await api.openPlayStore(serial, pkg);
      launcherActionMessage = r.ok
        ? `Opened Play Store on device for ${pkg} — confirm install on the TV, then click Refresh.`
        : `${pkg}: ${r.message.trim()}`;
    } catch (e) {
      launcherActionMessage = String(e);
    } finally {
      launcherActionBusy = null;
    }
  }

  async function enableLauncher(pkg: string) {
    const name = launchers.find((l) => l.entry.package === pkg)?.entry.name ?? pkg;
    const prevDefault = currentLauncher?.package ?? null;
    const prevName = prevDefault
      ? (launchers.find((l) => l.entry.package === prevDefault)?.entry.name ?? prevDefault)
      : null;
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    launcherProgress = "Enabling this launcher";
    try {
      const r = await api.enablePackage(serial, pkg);
      if (!r.ok) {
        launcherActionMessage = `Couldn't enable ${name}: ${r.message.trim() || "failed"}`;
        return;
      }
      launcherProgress = "Refreshing the launcher list";
      await loadLauncher();
      // Android clears its preferred-HOME record when a launcher package's
      // state changes, so a freshly re-enabled launcher (especially stock)
      // can steal the active-launcher slot. Enabling ≠ switching — put the
      // user's previous default back.
      if (prevDefault && prevDefault !== pkg && currentLauncher?.package === pkg) {
        launcherProgress = `Restoring ${prevName} as default`;
        const back = await api.setDefaultLauncher(serial, prevDefault);
        await loadLauncher();
        launcherActionMessage = back.ok
          ? `Enabled ${name}. Android made it the active launcher, so ${prevName} was re-set as your default.`
          : back.stock_takeover_available
            ? `Enabled ${name} — it also took over HOME, and this build can't hand HOME back without disabling it again. Use "Set as default" on ${prevName} if you want it back.`
            : `Enabled ${name} — Android made it the active launcher, and re-setting ${prevName} failed` +
              `${back.last_error ? `: ${back.last_error}` : ""}. Use "Set as default" on your preferred launcher.`;
      } else {
        launcherActionMessage = `${name} enabled.`;
      }
      // A launcher's enabled state changed — the Memory tab's report is now stale.
      invalidateDeviceCaches();
    } catch (e) {
      launcherActionMessage = String(e);
    } finally {
      launcherActionBusy = null;
      launcherProgress = "";
    }
  }

  async function disableLauncher(pkg: string) {
    const name = launchers.find((l) => l.entry.package === pkg)?.entry.name ?? pkg;
    const advice = launchers.find((l) => l.entry.package === pkg)?.other
      ? " Tip: save a snapshot first (Snapshot tab) so you have a record of today's state."
      : "";
    if (!confirm(`Disable ${name}? You'll lose access to it as a HOME app until you re-enable.${advice}`)) return;
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    launcherProgress = "Disabling this launcher";
    try {
      const r = await api.disableLauncher(serial, pkg);
      if (r.ok) {
        launcherProgress = "Refreshing the launcher list";
        await loadLauncher();
        invalidateDeviceCaches();
        launcherActionMessage = `${name} disabled.`;
      } else {
        launcherActionMessage = `Couldn't disable ${name}: ${r.message.trim() || "failed"}`;
      }
    } catch (e) {
      launcherActionMessage = String(e);
    } finally {
      launcherActionBusy = null;
      launcherProgress = "";
    }
  }

  async function setDefaultLauncher(pkg: string) {
    const name = launchers.find((l) => l.entry.package === pkg)?.entry.name ?? pkg;
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    launcherProgress = "";
    // The backend works through several strategies (enable → role → set-home-
    // activity → verify) that can take a few seconds; narrate each step so the
    // user knows it's working rather than hung.
    const onProgress = new Channel<string>();
    onProgress.onmessage = (step) => {
      if (launcherActionBusy === pkg) launcherProgress = step;
    };
    try {
      let r = await api.setDefaultLauncher(serial, pkg, false, onProgress);
      if (!r.ok && r.stock_takeover_available) {
        // The only working method on this build disables the stock launcher.
        // Never do that silently — ask, then retry with the opt-in flag.
        launcherProgress = "";
        const proceed = confirm(
          `${r.last_error ?? "This device ignores the standard launcher-switch commands."}\n\nDisable the stock launcher and switch to ${name}? You can re-enable it from this list at any time.`,
        );
        if (proceed) r = await api.setDefaultLauncher(serial, pkg, true, onProgress);
      }
      if (r.ok) {
        launcherActionMessage =
          r.strategy === "disable_stock_takeover"
            ? `${name} is now your default launcher — the stock launcher was disabled to hand it over. Re-enable it from this list any time.`
            : `${name} is now your default launcher.`;
      } else {
        // Backend messages are full sentences (including the "device accepted
        // the change — press Home" case) — render them verbatim rather than
        // prefixing "Failed:", which once produced "Failed: Success".
        launcherActionMessage =
          r.last_error ?? "Could not set default launcher. Try disabling other launchers first.";
      }
      // Always re-read state: the switch can land a beat after the backend's
      // own poll window, and the takeover path flips enabled/disabled badges —
      // so the list should redraw without the user hitting Refresh. This reload
      // is itself a few ADB queries, so keep the row's status line alive for it
      // rather than leaving the spinner frozen on the last backend step.
      if (launcherActionBusy === pkg) launcherProgress = "Refreshing the launcher list";
      await loadLauncher();
      // The takeover may have disabled stock; either way launcher state changed,
      // so the Memory tab's report is stale.
      invalidateDeviceCaches();
    } catch (e) {
      launcherActionMessage = String(e);
    } finally {
      launcherActionBusy = null;
      launcherProgress = "";
    }
  }

  function snapTimestamp(iso: string): string {
    return iso.replace("T", " ").replace("Z", " UTC");
  }

  async function loadSnapshots() {
    snapshotsErr = null;
    try {
      snapshots = await api.listSnapshots();
    } catch (e) {
      snapshotsErr = String(e);
    } finally {
      snapshotsLoaded = true;
    }
  }

  async function saveSnapshot() {
    if (!device) return;
    const label = (prompt("Name this snapshot (optional):", "") ?? "").trim();
    saveBusy = true;
    saveResult = "";
    try {
      const result = await api.saveSnapshot(serial, device.name, label || null);
      saveResult = `Saved ${result.label ?? result.filename} — ${result.disabled_count} disabled packages captured.`;
      await loadSnapshots();
    } catch (e) {
      saveResult = `Failed: ${e}`;
    } finally {
      saveBusy = false;
    }
  }

  async function previewSnapshot(path: string) {
    previewBusy = true;
    previewErr = null;
    preview = null;
    previewPath = path;
    // Clear any previous apply result when switching snapshots.
    applyResult = null;
    applyErr = null;
    try {
      preview = await api.previewApply(serial, path);
    } catch (e) {
      previewErr = String(e);
    } finally {
      previewBusy = false;
    }
  }

  /// Bulk mutations (snapshot apply, panic recovery) change package and
  /// launcher state behind the App List / Optimize / Launcher caches — re-sync
  /// them the same way executeOptimize does after a run. A partial failure
  /// still changed state, so callers resync unconditionally.
  async function resyncAfterBulkChange() {
    optimizeResetToken++;
    launchersLoaded = false;
    healthStale = true;
    if (apps.length === 0) return;
    const context = capturePageContext();
    try {
      appStates = await fetchAppStates(context, apps.map((a) => a.package));
      if (!pageContextIsCurrent(context)) return;
      catalogInventoryVersion++;
    } catch {
      if (!pageContextIsCurrent(context)) return;
      appStates = {};
      appsLoaded = false; // fall back to the lazy reload next tab visit
    }
  }

  async function applySnapshot() {
    if (!previewPath || !preview) return;
    const total =
      preview.packages_to_disable.length +
      Object.keys(preview.settings_to_write).length +
      (preview.launcher_to_set ? 1 : 0);
    if (!confirm(`Apply this snapshot? ${total} change(s) will be made to the device. Disabled packages can be re-enabled later via Recovery.`)) return;
    applyBusy = true;
    applyErr = null;
    applyResult = null;
    try {
      applyResult = await api.applySnapshot(serial, previewPath);
    } catch (e) {
      applyErr = String(e);
    } finally {
      applyBusy = false;
    }
    await resyncAfterBulkChange();
  }

  async function runRecovery() {
    if (!confirm("Re-enable every disabled package on this device? This is the panic button — use it if something went wrong.")) return;
    recoveryBusy = true;
    recoveryResult = null;
    recoveryErr = null;
    try {
      recoveryResult = await api.panicRecovery(serial);
    } catch (e) {
      recoveryErr = String(e);
    } finally {
      recoveryBusy = false;
    }
    await resyncAfterBulkChange();
  }

  async function rebootDevice(mode: RebootMode) {
    const label = mode === "normal" ? "" : ` into ${mode}`;
    if (!confirm(`Reboot the device${label}? You will lose ADB connection briefly.`)) return;
    rebootMenuOpen = false;
    rebootBusy = true;
    headerActionMsg = "";
    try {
      const r = await api.rebootDevice(serial, mode);
      headerActionMsg = r.message;
    } catch (e) {
      headerActionMsg = String(e);
    } finally {
      rebootBusy = false;
    }
  }

  function startRename() {
    renameValue = device?.properties?.friendly_name ?? device?.name ?? "";
    renaming = true;
  }

  async function saveRename() {
    if (!renameValue.trim()) return;
    renameBusy = true;
    headerActionMsg = "";
    try {
      const r = await api.renameDevice(serial, renameValue.trim());
      headerActionMsg = r.message;
      if (r.ok) {
        renaming = false;
        await loadDevice();
      }
    } catch (e) {
      headerActionMsg = String(e);
    } finally {
      renameBusy = false;
    }
  }

  async function takeScreenshot() {
    screenshotBusy = true;
    headerActionMsg = "";
    try {
      screenshot = await api.takeScreenshot(serial);
    } catch (e) {
      headerActionMsg = `Screenshot failed: ${e}`;
    } finally {
      screenshotBusy = false;
    }
  }

  async function revealScreenshot() {
    if (!screenshot) return;
    try {
      await revealItemInDir(screenshot.path);
    } catch (e) {
      headerActionMsg = `Open folder failed: ${e}`;
    }
  }

  async function disconnectAndLeave() {
    if (device?.connection === "usb") {
      if (!confirm("This is a USB device — disconnect will only forget it from the ADB server until you replug. Continue?")) return;
    }
    disconnectBusy = true;
    headerActionMsg = "";
    try {
      const r = await api.disconnectDevice(serial);
      if (r.ok) {
        pageEpoch++;
        mutationRequest++;
        goto("/");
      } else {
        headerActionMsg = `Disconnect failed: ${r.message}`;
      }
    } catch (e) {
      headerActionMsg = String(e);
    } finally {
      disconnectBusy = false;
    }
  }

  // Extracted tabs (Files, …) mount once on first visit and stay mounted
  // (hidden while inactive) so their state and fetched data survive tab
  // switches — they load their own data in onMount. `visited` gates that
  // first mount; clearing it (on a serial change) unmounts them.
  let visited = $state<Record<string, boolean>>({});

  // Lazy-load each non-extracted tab the first time it's opened. Extracted
  // tabs (tweaks/files/sideload/…) load their own data in onMount.
  $effect(() => {
    visited[activeTab] = true;
    if (activeTab === "health") {
      if ((report === null || healthStale) && !reportLoading) {
        healthStale = false;
        loadHealth();
      }
      // Preload the app tab so its own inventory is ready when visited.
      if (!appsLoaded && !appsLoading) loadApps();
    }
    if (activeTab === "launcher" && !launchersLoaded && !launcherLoading) loadLauncher();
    if (activeTab === "apps" && !appsLoaded && !appsLoading) loadApps();
    if (activeTab === "snapshot" && !snapshotsLoaded) loadSnapshots();
  });

  // A device-state change (enable/disable/uninstall/launcher switch) in one tab
  // leaves the other tabs' cached snapshots stale — e.g. disabling a launcher
  // from the Memory tab must show up on the Launchers tab. Mark the *other*
  // data tabs for a fresh load on their next visit; the tab the action happened
  // on refreshes itself inline. Existing data stays on screen until each reload
  // finishes, so there's no flash of empty state.
  function invalidateDeviceCaches() {
    if (activeTab !== "launcher") launchersLoaded = false;
    if (activeTab !== "health") healthStale = true;
  }

  /// Wipe all per-device state. Used if the route's serial changes under a
  /// live component (today the only way off this page is "← Back to devices",
  /// so this is defensive — but it guarantees no device's data or in-flight
  /// timer can leak onto another if a device→device link is ever added).
  function resetDeviceState() {
    pageEpoch++;
    deviceRequest++;
    healthRequest++;
    appsRequest++;
    otherRequest++;
    enrichmentRequest++;
    mutationRequest++;
    if (liveRefreshTimer) {
      clearInterval(liveRefreshTimer);
      liveRefreshTimer = null;
    }
    liveRefresh = false;
    activeTab = "overview";
    // Unmount the extracted tab components — their state dies with them.
    visited = {};
    device = null; deviceErr = null;
    report = null; reportErr = null; reportLastRefreshed = null; memorySafety = {};
    launchers = []; launchersLoaded = false; currentLauncher = null; channelDisabled = null;
    launcherErr = null; launcherActionMessage = "";
    apps = []; appsLoaded = false; appsErr = null; appStates = {}; packageSafety = {}; appActionBusy = null; appActionMessage = "";
    otherPackages = []; othersLoaded = false; othersErr = null; appMemory = {}; appUsage = {}; appSearch = ""; hideNotInstalled = true; showSystemOthers = false;
    clonePkg = null; cloneTargets = [];
    snapshots = []; snapshotsLoaded = false; snapshotsErr = null; preview = null; previewPath = null; previewErr = null; saveResult = "";
    headerActionMsg = ""; recoveryResult = null; recoveryErr = null; screenshot = null;
    renaming = false; renameValue = "";
    trimMessage = "";
    applyResult = null; applyErr = null;
  }

  // Track the serial so a *change* (not the initial mount) resets and reloads.
  // onMount handles the first load; this only fires if serial changes live.
  let loadedSerial: string | null = null;
  $effect(() => {
    const s = serial;
    if (loadedSerial !== null && loadedSerial !== s) {
      resetDeviceState();
      loadDevice();
    }
    loadedSerial = s;
  });

  onMount(loadDevice);
</script>

<div class="back-row">
  <button onclick={() => goto("/")}>← Back to devices</button>
</div>

{#if deviceErr}
  <div class="error">{deviceErr}</div>
{:else if !device}
  <div class="muted">Loading device…</div>
{:else}
  <header class="device-header">
    <div class="device-title-row">
      <div>
        {#if renaming}
          <div class="rename-row">
            <input
              bind:value={renameValue}
              maxlength={64}
              onkeydown={(e) => {
                if (e.key === "Enter") saveRename();
                if (e.key === "Escape") renaming = false;
              }}
            />
            <button class="primary small-action" onclick={saveRename} disabled={renameBusy || !renameValue.trim()}>
              {renameBusy ? "Saving…" : "Save"}
            </button>
            <button class="small-action subtle" onclick={() => (renaming = false)} disabled={renameBusy}>
              Cancel
            </button>
          </div>
        {:else}
          <h1>
            {device.name}
            <button
              class="small-action subtle rename-button"
              onclick={startRename}
              title="Rename this device (settings put global device_name — what Cast / Google Home display)"
            >
              Rename
            </button>
          </h1>
        {/if}
        <div class="device-meta">
          <span>{deviceTypeLabel(device.device_type)}</span>
          {#if device.model}<span>· {device.model}</span>{/if}
          <span class="serial">· {device.serial}</span>
          {#if device.properties?.android_release}
            <span>· Android {device.properties.android_release}</span>
          {/if}
        </div>
      </div>
      <div class="device-header-actions">
        <div class="reboot-wrap">
          <button
            onclick={() => (rebootMenuOpen = !rebootMenuOpen)}
            disabled={rebootBusy}
            aria-haspopup="menu"
            aria-expanded={rebootMenuOpen}
          >
            {rebootBusy ? "Rebooting…" : "Reboot ▾"}
          </button>
          {#if rebootMenuOpen}
            <div class="reboot-menu" role="menu">
              <button role="menuitem" onclick={() => rebootDevice("normal")}>Normal</button>
              <button role="menuitem" onclick={() => rebootDevice("recovery")}>Recovery</button>
              <button role="menuitem" onclick={() => rebootDevice("bootloader")}>Bootloader</button>
            </div>
          {/if}
        </div>
        <button
          onclick={takeScreenshot}
          disabled={screenshotBusy}
          title="Capture the TV screen (screencap) and save it as a PNG on this computer"
        >
          {screenshotBusy ? "Capturing…" : "Screenshot"}
        </button>
        <button
          class="small-action subtle"
          onclick={disconnectAndLeave}
          disabled={disconnectBusy}
          title="Drop the ADB connection to this device. Useful for network devices you don't want auto-reconnecting on Refresh."
        >
          {disconnectBusy ? "Disconnecting…" : "Disconnect"}
        </button>
      </div>
    </div>
    {#if headerActionMsg}
      <p class="muted small mono action-message">{headerActionMsg}</p>
    {/if}
    {#if screenshot}
      <div class="screenshot-preview">
        <img src={`data:image/png;base64,${screenshot.base64}`} alt="TV screenshot" />
        <div class="screenshot-meta">
          <span class="muted small mono">{screenshot.path}</span>
          <button class="small-action" onclick={revealScreenshot}>Open folder</button>
          <button class="small-action subtle" onclick={() => (screenshot = null)}>Dismiss</button>
        </div>
      </div>
    {/if}
  </header>

  <div class="tabs" role="tablist" aria-label="Device sections">
    {#each [
      { id: "overview", label: "Overview" },
      { id: "health", label: "Health" },
      { id: "launcher", label: "Launcher" },
      { id: "apps", label: "App List" },
      { id: "optimize", label: "Optimize" },
      { id: "tweaks", label: "Tweaks" },
      { id: "remote", label: "Remote" },
      { id: "files", label: "Files" },
      { id: "sideload", label: "Install APK" },
      { id: "snapshot", label: "Snapshot" },
    ] as t (t.id)}
      <button
        role="tab"
        aria-selected={activeTab === t.id}
        aria-controls={`tabpanel-${t.id}`}
        id={`tab-${t.id}`}
        class:active={activeTab === t.id}
        onclick={() => (activeTab = t.id as Tab)}
      >
        {t.label}
      </button>
    {/each}
  </div>

  {#if activeTab === "overview"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-overview" aria-labelledby="tab-overview">
      <h2>Profile</h2>
      {#if device.properties}
        <dl class="kv">
          <dt>Friendly name</dt>
          <dd>{device.properties.friendly_name ?? "—"}</dd>
          <dt>Brand</dt><dd>{device.properties.brand}</dd>
          <dt>Model</dt><dd>{device.properties.model}</dd>
          <dt>Codename</dt><dd>{device.properties.device_codename}</dd>
          <dt>Manufacturer</dt><dd>{device.properties.manufacturer}</dd>
          <dt>Android version</dt><dd>{device.properties.android_release} (SDK {device.properties.sdk_level})</dd>
          <dt>Build ID</dt><dd>{device.properties.build_id}</dd>
          <dt>Board platform</dt><dd>{device.properties.board_platform}</dd>
        </dl>
      {/if}

      <div class="recovery-section">
        <h3>Emergency Recovery</h3>
        <p class="muted small">
          If something broke after disabling a package, re-enable everything that's
          currently disabled in one shot. Equivalent to v1's <code>Run-PanicRecovery</code>.
        </p>
        <button
          class="danger-button"
          onclick={runRecovery}
          disabled={recoveryBusy}
          title="pm enable every package currently in `pm list packages -d`"
        >
          {recoveryBusy ? "Restoring…" : "Re-enable all disabled packages"}
        </button>
        {#if recoveryErr}
          <div class="error">{recoveryErr}</div>
        {/if}
        {#if recoveryResult}
          <div class="recovery-result">
            <p><strong>{recoveryResult.message}</strong></p>
            {#if recoveryResult.failed.length > 0}
              <details>
                <summary>{recoveryResult.failed.length} package(s) failed</summary>
                <ul class="mono small">
                  {#each recoveryResult.failed as f}
                    <li>{f.package}: {f.error}</li>
                  {/each}
                </ul>
              </details>
            {/if}
          </div>
        {/if}
      </div>
    </div>
  {:else if activeTab === "health"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-health" aria-labelledby="tab-health">
      <div class="card-header">
        <h2>Health Report</h2>
        <div class="header-actions">
          <label class="live-refresh">
            <input type="checkbox" checked={liveRefresh} onchange={toggleLiveRefresh} />
            Live refresh
          </label>
          <span class="muted small" title={reportLastRefreshed?.toISOString() ?? ""}>
            {refreshLabel}
          </span>
          <button
            onclick={clearCaches}
            disabled={trimBusy}
            title="pm trim-caches — clears every app's cache; caches rebuild on next launch"
          >
            {trimBusy ? "Clearing…" : "Clear caches"}
          </button>
          <button onclick={loadHealth} disabled={reportLoading}>
            {reportLoading ? "Loading…" : "Refresh"}
          </button>
        </div>
      </div>
      {#if trimMessage}
        <p class="muted small mono">{trimMessage}</p>
      {/if}
      {#if reportErr}
        <div class="error">{reportErr}</div>
      {:else if !report}
        <div class="muted">{reportLoading ? "Querying…" : "—"}</div>
      {:else}
        <h3>Vitals</h3>
        <dl class="kv">
          <dt>Temperature</dt>
          <dd>{report.temperature_c != null ? `${report.temperature_c.toFixed(1)}°C` : "—"}</dd>
          {#if report.ram.total_mb != null}
            <dt>RAM</dt>
            <dd>
              {report.ram.used_mb ?? "?"} / {report.ram.total_mb} MB
              {#if report.ram.total_mb && report.ram.used_mb != null}
                ({Math.round((report.ram.used_mb / report.ram.total_mb) * 100)}%)
              {/if}
            </dd>
          {/if}
          {#if report.ram.swap_mb != null}
            <dt>Swap</dt><dd>{report.ram.swap_mb} MB</dd>
          {/if}
          {#if report.storage.total}
            <dt>Storage</dt>
            <dd>
              {report.storage.used ?? "?"} / {report.storage.total}
              {#if report.storage.used_percent != null}({report.storage.used_percent}%){/if}
            </dd>
          {/if}
        </dl>

        <h3>Display & Audio</h3>
        <dl class="kv">
          <dt>Resolution</dt><dd>{report.display.resolution ?? "—"}</dd>
          <dt>Refresh</dt><dd>{report.display.refresh_hz ? `${report.display.refresh_hz} Hz` : "—"}</dd>
          <dt>HDR</dt><dd>{report.display.hdr_types.length ? report.display.hdr_types.join(", ") : "SDR only"}</dd>
          <dt>Audio out</dt><dd>{report.audio_device ?? "—"}</dd>
        </dl>

        <h3>Top Memory Users</h3>
        {#if report.top_memory.length === 0}
          <p class="muted">No process data.</p>
        {:else}
          <table class="mem-table">
            <thead>
              <tr><th>PSS</th><th>Reported package field</th><th class="center">Rule for field</th><th>Use</th></tr>
            </thead>
            <tbody>
              {#each report.top_memory as m}
                {@const safety = memorySafety[m.package]}
                <tr>
                  <td
                    class="num"
                    class:warn={m.mb >= 200}
                    class:caution={m.mb >= 100 && m.mb < 200}
                  >
                    {m.mb.toFixed(1)} MB
                  </td>
                  <td class="pkg">
                    {m.package}
                    <div class="muted small">Identity unverified</div>
                  </td>
                  <td class="center" title={safetyReason(safety)}>
                    {safetyLabel(safety).toUpperCase()}
                  </td>
                  <td>
                    <span class="muted small" title="Memory rows are diagnostics only because the reported package field is not verified ownership.">Inspect only</span>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if appActionMessage}
            <p class="muted small mono">
              {appActionMessage}
              <button class="dismiss" onclick={() => (appActionMessage = "")} title="Dismiss">✕</button>
            </p>
          {/if}
        {/if}
      {/if}
    </div>
  {:else if activeTab === "launcher"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-launcher" aria-labelledby="tab-launcher">
      <div class="card-header">
        <h2>Launchers</h2>
        <button onclick={loadLauncher} disabled={launcherLoading}>
          {launcherLoading ? "Loading…" : "Refresh"}
        </button>
      </div>
      {#if launcherErr}
        <div class="error">{launcherErr}</div>
      {:else}
        {#if currentLauncher?.package}
          <p>Currently active: <strong>{currentLauncher.package}</strong></p>
        {/if}
        {#if channelDisabled}
          <div class="warning">
            ⚠ <code>com.android.providers.tv</code> is disabled on this device. Watch Next / Continue
            Watching rows from Apple TV, Netflix, Disney+ etc. will be empty until you re-enable it.
          </div>
        {/if}
        {#if launchers.length === 0 && !launcherLoading}
          <p class="muted">No launchers loaded.</p>
        {:else}
          <ul class="launcher-list">
            {#each launchers as l}
              {@const isCurrent = currentLauncher?.package === l.entry.package}
              {@const busy = launcherActionBusy === l.entry.package}
              <li>
                <div>
                  <div class="launcher-name">
                    {l.entry.name}
                    {#if isCurrent}
                      <span class="tag installed">ACTIVE</span>
                    {/if}
                  </div>
                  <div class="muted small mono">{l.entry.package}</div>
                  {#if busy && launcherProgress}
                    <div class="launcher-progress" role="status" aria-live="polite">
                      <span class="spinner" aria-hidden="true"></span>{launcherProgress}…
                    </div>
                  {/if}
                </div>
                <div class="row-actions">
                  <div class="tags">
                    {#if l.stock}
                      <span class="tag stock">STOCK</span>
                    {:else if l.other}
                      <span class="tag stock">HOME APP</span>
                    {:else if l.installed}
                      <span class="tag installed">INSTALLED</span>
                    {:else}
                      <span class="tag missing">MISSING</span>
                    {/if}
                    {#if l.installed && !l.enabled}
                      <span class="tag disabled">DISABLED</span>
                    {/if}
                  </div>
                  {#if !l.installed}
                    <button
                      class="small-action"
                      onclick={() => installLauncherFromStore(l.entry.package)}
                      disabled={launcherActionBusy !== null}
                      title="Open the Play Store on the device to install {l.entry.name}"
                    >
                      {busy ? "Opening…" : "Install"}
                    </button>
                  {:else}
                    {#if !l.enabled}
                      <button
                        class="small-action"
                        onclick={() => enableLauncher(l.entry.package)}
                        disabled={launcherActionBusy !== null}
                        title="pm enable {l.entry.package}"
                      >
                        {busy ? "Enabling…" : "Enable"}
                      </button>
                    {/if}
                    {#if !isCurrent}
                      <button
                        class="primary small-action"
                        onclick={() => setDefaultLauncher(l.entry.package)}
                        disabled={launcherActionBusy !== null}
                        title={l.enabled
                          ? "Make this the default launcher (role API / set-home-activity)"
                          : "Enable this launcher, then make it the default"}
                      >
                        {busy ? "Setting…" : l.enabled ? "Set as default" : "Enable & set default"}
                      </button>
                    {/if}
                    {#if !isCurrent && l.enabled}
                      <button
                        class="small-action subtle"
                        onclick={() => disableLauncher(l.entry.package)}
                        disabled={launcherActionBusy !== null}
                        title="pm disable-user --user 0 {l.entry.package}"
                      >{busy ? "Disabling…" : "Disable"}</button>
                    {:else if isCurrent}
                      <span
                        class="muted small"
                        title="Disabling the launcher you're currently using would leave the TV with no Home screen"
                      >
                        Set another launcher as default to disable this one
                      </span>
                    {/if}
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
          {#if launcherActionMessage}
            <p class="muted small mono action-message">{launcherActionMessage}</p>
          {/if}
        {/if}
      {/if}
    </div>
  {:else if activeTab === "apps"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-apps" aria-labelledby="tab-apps">
      <div class="card-header">
        <h2>App List for {deviceTypeLabel(device.device_type)}</h2>
        <div class="header-actions">
          <span class="muted">{apps.length} curated · {otherPackages.length} other</span>
          <button onclick={loadApps} disabled={appsLoading}>
            {appsLoading ? "Loading…" : "Refresh"}
          </button>
        </div>
      </div>
      <div class="app-toolbar">
        <input
          class="app-search"
          placeholder="Search apps by name or package…"
          bind:value={appSearch}
        />
        <label class="inline-check">
          <input type="checkbox" bind:checked={hideNotInstalled} />
          Hide not installed
        </label>
        <label class="inline-check">
          <input type="checkbox" bind:checked={showSystemOthers} />
          Show system packages
        </label>
      </div>
      {#if appsErr}
        <div class="error">{appsErr}</div>
      {/if}
      {#if appsLoading && apps.length === 0}
        <div class="muted">Loading…</div>
      {:else}
        <p class="muted small legend">
          <strong>State</strong> is what the device reports right now.
          <strong>Safety</strong> is the canonical engine verdict. Unknown removals need explicit review;
          unavailable safety or state blocks removal. <strong>Tools</strong> has the Play Store link
          plus APK backup and copy-to-another-device.
        </p>
        {#if appActionMessage}
          <p class="muted small mono action-message">
            {appActionMessage}
            <button class="dismiss" onclick={() => (appActionMessage = "")} title="Dismiss">✕</button>
          </p>
        {/if}
        {#if appMutationInFlight && !appActionBusy}
          <p class="muted small mono action-message">Finishing the previous app action…</p>
        {/if}
        {#if clonePkg}
          <div class="clone-panel">
            <span>Copy <code>{clonePkg}</code> to:</span>
            {#each cloneTargets as t (t.serial)}
              <button class="small-action" onclick={() => cloneTo(t)} disabled={cloneBusy}>
                {cloneBusy ? "Copying…" : `${t.name} (${t.serial})`}
              </button>
            {/each}
            <button class="small-action subtle" onclick={() => (clonePkg = null)} disabled={cloneBusy}>
              Cancel
            </button>
          </div>
        {/if}
        <table class="app-table">
          <thead>
            <tr>
              <th>App</th>
              <th class="center">State</th>
              <th class="center">Safety</th>
              <th>Action</th>
              <th class="center">Tools</th>
            </tr>
          </thead>
          <tbody>
            {#each visibleApps as a (a.package)}
              {@const state = appStates[a.package] ?? null}
              {@const safety = packageSafety[a.package]}
              {@const rec = recommendation(a, state, safety)}
              {@const canRemove = (state === "enabled" || state === "disabled") && safety?.status === "ready" && safety.verdict.kind !== "never_disable"}
              <AppRow
                name={a.name}
                description={a.optimize_description}
                package={a.package}
                review={a.review}
                {state}
                mb={appMemory[a.package]}
                usage={appUsage[a.package]}
                showUsage={state !== "missing"}
                safety={safety?.status === "ready" ? safety.verdict : null}
                safetyStatus={safety?.status ?? "unavailable"}
              >
                {#snippet actions()}
                <td class="rec-cell">
                  {#if rec.kind === "act"}
                    <button
                      class="small-action recommended"
                      class:danger={rec.action === "uninstall"}
                      onclick={() => applyRecommendation(a.package, rec.action)}
                      disabled={appActionBusy === a.package || appMutationInFlight}
                      title={a.optimize_description}
                    >
                      {appActionBusy === a.package ? "…" : rec.label}
                    </button>
                  {:else if rec.kind === "review"}
                    <button
                      class="small-action review-action"
                      class:danger={rec.action === "uninstall"}
                      onclick={() => applyRecommendation(a.package, rec.action)}
                      disabled={appActionBusy === a.package || appMutationInFlight}
                      title="You may not use this one — check the last-used cue, then {rec.action} if so."
                    >
                      {appActionBusy === a.package ? "…" : rec.label}
                    </button>
                  {:else if rec.kind === "restore"}
                    <button
                      class="small-action recommended"
                      onclick={() => reinstallApp(a.package)}
                      disabled={appActionBusy === a.package || appMutationInFlight}
                      title="cmd package install-existing — works for system apps still on /system"
                    >
                      {appActionBusy === a.package ? "…" : rec.label}
                    </button>
                  {:else if rec.kind === "done"}
                    <span class="muted small done">✓ {rec.label}</span>
                  {:else if rec.kind === "unavailable"}
                    <span class="muted small">{rec.label} — refresh to retry</span>
                  {:else}
                    <span class="muted small">Keep</span>
                  {/if}

                  {#if state === "enabled" && canRemove && rec.kind !== "act" && !(rec.kind === "review" && rec.action === "disable")}
                    <button
                      class="small-action subtle"
                      onclick={() => disableApp(a.package)}
                      disabled={appActionBusy === a.package || appMutationInFlight}
                      title="pm disable-user --user 0"
                    >Disable</button>
                  {/if}
                  {#if state === "disabled"}
                    <button
                      class="small-action subtle"
                      onclick={() => enableApp(a.package)}
                      disabled={appActionBusy === a.package || appMutationInFlight}
                      title="pm enable"
                    >Enable</button>
                  {/if}
                </td>
                <td class="center tools-cell">
                  {#if a.play_store}
                    <button
                      class="small-action"
                      onclick={() => openInPlayStore(a.package)}
                      disabled={appActionBusy === a.package}
                      title="Open {a.name} on the Play Store on the device"
                    >
                      Play Store
                    </button>
                  {/if}
                  {#if state !== "missing"}
                    <button
                      class="small-action subtle"
                      onclick={() => backupApkFor(a.package)}
                      disabled={appActionBusy === a.package}
                      title="Save this app's APK(s) to a folder on this computer"
                    >
                      Backup
                    </button>
                    <button
                      class="small-action subtle"
                      onclick={() => startClone(a.package)}
                      disabled={appActionBusy === a.package}
                      title="Install this app onto another connected device (app data does not transfer)"
                    >
                      Copy to…
                    </button>
                  {:else if !a.play_store}
                    <span class="muted small">—</span>
                  {/if}
                </td>
                {/snippet}
              </AppRow>
            {/each}
            {#if visibleApps.length === 0}
              <tr><td colspan="5" class="muted">No curated apps match your filters.</td></tr>
            {/if}
          </tbody>
        </table>

        <div class="other-apps">
          <h3>Everything else {othersLoaded ? `(${visibleOthers.length})` : ""}</h3>
          <p class="muted small">
            Installed apps that aren't in the curated list — sideloaded apps (SmartTube etc.)
            get the same <strong>Backup</strong> and <strong>Copy to…</strong> tools.
            Every removal uses the canonical safety verdict and current inventory evidence.
            {showSystemOthers ? "Showing system packages too." : "System packages are hidden; tick \"Show system packages\" to include them."}
          </p>
          {#if othersErr}
            <div class="error">
              {othersErr}
              <button class="small-action" onclick={loadOtherPackages} disabled={othersLoading}>Retry other packages</button>
            </div>
          {:else if othersLoading}
            <div class="muted">Loading installed packages…</div>
          {:else if !othersLoaded}
            <p class="muted">Other-package inventory is unavailable. Retry to load it.</p>
          {:else if visibleOthers.length === 0}
            <p class="muted">{otherPackages.length === 0 ? "No non-catalog packages found." : "Nothing matches your filters."}</p>
          {:else}
            <table class="app-table">
              <thead>
                <tr><th>Package</th><th class="center">Type</th><th class="center">State</th><th class="center">Safety</th><th>Actions</th><th class="center">Tools</th></tr>
              </thead>
              <tbody>
                {#each visibleOthers as o (o.package)}
                  {@const safety = packageSafety[o.package]}
                  {@const canRemove = othersLoaded && safety?.status === "ready" && safety.verdict.kind !== "never_disable"}
                  <tr>
                    <td class="app-cell">
                      {#if o.name}
                        <div class="app-name-row">{o.name}</div>
                        <div class="muted small mono pkg-id">{o.package}</div>
                      {:else}
                        <div class="mono small">{o.package}</div>
                      {/if}
                    </td>
                    <td class="center type-cell">
                      <span class={`tag ${o.system ? "missing" : "installed"}`}>{o.system ? "SYSTEM" : "3RD-PARTY"}</span>
                    </td>
                    <td class="center">
                      <StateBadge state={o.enabled ? "enabled" : "disabled"} />
                      {#if appMemory[o.package]}
                        <div class="cell-cue"><RamBadge mb={appMemory[o.package]} /></div>
                      {/if}
                      {#if appUsage[o.package]}
                        <div class="cell-cue"><UsageBadge usage={appUsage[o.package]} /></div>
                      {/if}
                    </td>
                    <td class="center" title={safetyReason(safety)}>{safetyLabel(safety).toUpperCase()}</td>
                    <td class="rec-cell">
                      {#if o.enabled}
                        <button class="small-action subtle" onclick={() => disableOther(o.package)} disabled={appActionBusy === o.package || appMutationInFlight || !canRemove} title="Canonical safety and fresh inventory are required">Disable</button>
                        <button class="small-action subtle danger" onclick={() => uninstallOther(o.package)} disabled={appActionBusy === o.package || appMutationInFlight || !canRemove} title="Canonical safety and fresh inventory are required">Uninstall</button>
                      {:else}
                        <button class="small-action subtle" onclick={() => enableOther(o.package)} disabled={appActionBusy === o.package || appMutationInFlight} title="pm enable">Enable</button>
                      {/if}
                    </td>
                    <td class="center tools-cell">
                      <button class="small-action subtle" onclick={() => backupApkFor(o.package)} disabled={appActionBusy === o.package} title="Save this app's APK(s) to a folder on this computer">Backup</button>
                      <button class="small-action subtle" onclick={() => startClone(o.package)} disabled={appActionBusy === o.package} title="Install this app onto another connected device">Copy to…</button>
                    </td>
                  </tr>
                {/each}
              </tbody>
            </table>
          {/if}
        </div>
      {/if}
    </div>
  {:else if activeTab === "snapshot"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-snapshot" aria-labelledby="tab-snapshot">
      <div class="card-header">
        <h2>Snapshots</h2>
        <button class="primary" onclick={saveSnapshot} disabled={saveBusy}>
          {saveBusy ? "Saving…" : "Save current state"}
        </button>
      </div>
      {#if saveResult}<p class="muted small">{saveResult}</p>{/if}
      {#if snapshotsErr}<div class="error">{snapshotsErr}</div>{/if}
      {#if snapshots.length === 0}
        <p class="muted">No snapshots yet. Use the button above to save one.</p>
      {:else}
        <ul class="snap-list">
          {#each snapshots as s (s.path)}
            <li>
              <div class="snap-main">
                <div class="snap-title">
                  <strong>{s.label ?? s.device_name}</strong>
                  <span class="tag installed">{deviceTypeLabel(s.device_type).toUpperCase()}</span>
                  {#if s.label}<span class="muted small">{s.device_name}</span>{/if}
                </div>
                <div class="muted small">
                  {snapTimestamp(s.saved_at)} ·
                  {s.disabled_count} disabled,
                  {s.settings_count} settings,
                  launcher {s.launcher ?? "—"}
                </div>
              </div>
              <div class="snap-actions">
                <button class="small-action" onclick={() => previewSnapshot(s.path)}>Preview apply</button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
      {#if previewBusy}
        <p class="muted">Computing plan…</p>
      {:else if previewErr}
        <div class="error">{previewErr}</div>
      {:else if preview && previewPath}
        <div class="preview-box">
          <h3>Plan preview</h3>
          {#if preview.cross_device_warning}
            <div class="warning">{preview.cross_device_warning}</div>
          {/if}
          <ul>
            <li><strong>{preview.packages_to_disable.length}</strong> packages will be disabled</li>
            <li><strong>{preview.packages_already_disabled.length}</strong> already disabled (no-op)</li>
            <li><strong>{preview.packages_not_installed.length}</strong> not present on device</li>
            <li>Launcher: <code>{preview.launcher_to_set ?? "(unchanged)"}</code></li>
            <li><strong>{Object.keys(preview.settings_to_write).length}</strong> settings will be written
              {#if preview.settings_already_set.length > 0}
                <span class="muted">({preview.settings_already_set.length} already set, no-op)</span>
              {/if}
            </li>
          </ul>
          <div class="apply-row">
            <button
              class="primary"
              onclick={applySnapshot}
              disabled={applyBusy || applyResult !== null}
            >
              {applyBusy ? "Applying…" : applyResult ? "Applied" : "Apply this snapshot"}
            </button>
            <span class="muted small">
              Disable is reversible via Emergency Recovery on the Overview tab.
            </span>
          </div>
          {#if applyErr}
            <div class="error">{applyErr}</div>
          {/if}
          {#if applyResult}
            <div class="apply-result">
              <p><strong>{applyResult.summary}</strong></p>
              <ul>
                <li><strong>{applyResult.packages_disabled.length}</strong> packages disabled</li>
                {#if applyResult.packages_failed.length > 0}
                  <li class="warn-text"><strong>{applyResult.packages_failed.length}</strong> failed: {applyResult.packages_failed.join(", ")}</li>
                {/if}
                {#if applyResult.launcher_message}
                  <li>Launcher: {applyResult.launcher_message}</li>
                {/if}
                <li><strong>{applyResult.settings_written.length}</strong> settings written</li>
                {#if applyResult.settings_failed.length > 0}
                  <li class="warn-text">Settings failed: {applyResult.settings_failed.join("; ")}</li>
                {/if}
              </ul>
            </div>
          {/if}
        </div>
      {/if}
    </div>
  {/if}

  <!-- Extracted tabs: mount once on first visit, then toggle visibility so
       their state and fetched data persist across tab switches. -->
  {#if visited.tweaks}
    <div hidden={activeTab !== "tweaks"}>
      <TweaksTab {serial} />
    </div>
  {/if}
  {#if visited.files}
    <div hidden={activeTab !== "files"}>
      <FilesTab {serial} />
    </div>
  {/if}
  {#if visited.sideload}
    <div hidden={activeTab !== "sideload"}>
      <SideloadTab {serial} />
    </div>
  {/if}
  {#if visited.remote}
    <div hidden={activeTab !== "remote"}>
      <RemoteTab {serial} />
    </div>
  {/if}
  {#if visited.optimize}
    <div hidden={activeTab !== "optimize"}>
      <OptimizeTab
        {serial}
        deviceType={device.device_type}
        {appUsage}
        resetToken={optimizeResetToken}
        {pageEpoch}
        onStatesChanged={resyncAppStates}
        onPlanLoaded={loadAppMemory}
      />
    </div>
  {/if}
{/if}

<style>
  .back-row {
    margin-bottom: 1rem;
  }
  .device-header {
    margin-bottom: 1.2rem;
  }
  .device-header h1 {
    margin: 0;
    font-size: 1.5rem;
  }
  .device-meta {
    color: var(--fg-muted);
    font-size: 0.9rem;
    margin-top: 0.3rem;
    display: flex;
    gap: 0.4rem;
    flex-wrap: wrap;
  }
  .serial {
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  .tabs {
    display: flex;
    gap: 0.4rem;
    margin-bottom: 1rem;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0;
  }
  .tabs button {
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    padding: 0.5rem 0.8rem;
  }
  .tabs button.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.2rem;
  }
  .card h2 {
    margin: 0 0 0.8rem;
    font-size: 1.1rem;
  }
  .card h3 {
    margin: 1rem 0 0.4rem;
    font-size: 1rem;
    color: var(--fg-secondary);
  }
  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .kv {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.4rem 1.5rem;
    margin: 0;
    font-size: 0.9rem;
  }
  .kv dt {
    color: var(--fg-muted);
  }
  .kv dd {
    margin: 0;
    font-family: ui-monospace, monospace;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  th, td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--bg-button);
    vertical-align: middle;
  }
  th.center, td.center {
    text-align: center;
  }
  .app-table .app-cell {
    line-height: 1.3;
    /* Long system package ids (com.google.android.overlay.modules.…) are one
       unbreakable token; without this they force the column — and the whole
       table — wider than the viewport, pushing the action buttons off-screen.
       `anywhere` (not `break-word`) also shrinks the column's min-content width
       so the table stops overflowing. Inherited by the child name/pkg rows. */
    overflow-wrap: anywhere;
  }
  .app-table .rec-cell,
  .app-table .tools-cell {
    /* Keep the action/tool buttons from being squeezed once the name column
       can shrink — they stay on one line at their natural width. */
    white-space: nowrap;
    width: 1%;
  }
  .app-name-row {
    font-size: 0.95rem;
    font-weight: 500;
  }
  .app-table .app-desc {
    margin-top: 0.15rem;
    font-size: 0.82rem;
    max-width: 42rem;
  }
  .app-table .pkg-id {
    margin-top: 0.1rem;
    font-size: 0.78rem;
    opacity: 0.7;
  }
  /* Small stacked cue (RAM / last-used badge) under a row's state badge. */
  .cell-cue {
    margin-top: 0.2rem;
  }
  .app-table .rec-cell {
    /* Keep button + subtle override on one row when possible. */
    white-space: nowrap;
  }
  .app-table .rec-cell .small-action {
    margin-right: 0.3rem;
  }
  .app-table .rec-cell .done {
    display: inline-block;
    margin-right: 0.5rem;
  }
  th {
    color: var(--fg-muted);
    font-weight: 500;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  td.num {
    font-family: ui-monospace, monospace;
    text-align: right;
    width: 100px;
  }
  td.num.warn { color: var(--danger-strong); }
  td.num.caution { color: var(--warn); }
  td.pkg {
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .launcher-list {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0 0;
  }
  .launcher-list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    padding: 0.7rem 0;
    border-bottom: 1px solid var(--bg-button);
  }
  .launcher-name {
    font-weight: 500;
  }
  .tags {
    display: flex;
    gap: 0.4rem;
  }
  .tag {
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    letter-spacing: 0.04em;
  }
  .tag.installed { background: var(--ok-surface); color: var(--ok); }
  .tag.review { background: var(--warn-surface-2); color: var(--warn); }
  .tag.stock { background: var(--bg-muted); color: var(--accent); }
  .tag.missing { background: var(--bg-muted); color: var(--fg-faint); }
  .tag.disabled { background: var(--warn-surface-2); color: var(--warn); }
  .warning {
    background: var(--warn-surface);
    border: 1px solid var(--warn-border);
    color: var(--warn);
    padding: 0.7rem 1rem;
    border-radius: 6px;
    margin: 0.8rem 0;
    font-size: 0.9rem;
  }
  .warning code {
    background: var(--bg-inset);
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  .snap-list {
    list-style: none;
    padding: 0;
    margin: 0.6rem 0 0;
  }
  .snap-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.7rem 1rem;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    margin-bottom: 0.5rem;
  }
  .snap-main { flex: 1; min-width: 0; }
  .snap-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-bottom: 0.2rem;
  }
  .snap-actions { display: flex; gap: 0.4rem; align-items: center; flex-shrink: 0; }
  .preview-box {
    margin-top: 1rem;
    padding: 1rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .preview-box ul {
    margin: 0.4rem 0;
    padding-left: 1.2rem;
  }
  .preview-disclaimer {
    margin-top: 0 !important;
  }
  .header-actions {
    display: flex;
    gap: 0.8rem;
    align-items: center;
  }
  .live-refresh {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    font-size: 0.85rem;
    color: var(--fg-secondary);
    cursor: pointer;
  }
  .row-actions {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .small-action {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
  }
  .small-action.danger {
    background: var(--bg-button);
    border-color: var(--danger-surface);
    color: var(--danger-strong);
  }
  .small-action.danger:hover {
    background: var(--danger-surface);
    color: var(--danger-surface-text);
    border-color: var(--danger-strong);
  }
  .small-action.recommended {
    background: var(--accent-strong);
    color: #fff;
    border-color: var(--accent);
    font-weight: 500;
  }
  .small-action.recommended:hover:not(:disabled) {
    background: var(--accent-strong-hover);
  }
  .small-action.recommended.danger {
    background: var(--danger-surface);
    border-color: var(--danger-strong);
    color: var(--danger-surface-text);
  }
  .small-action.recommended.danger:hover:not(:disabled) {
    background: var(--danger-border);
  }
  /* Review (remove-if-unused): optional, not a default — outlined in the warn
     color so it reads as "consider this", lighter than a recommended action. */
  .small-action.review-action {
    border-color: var(--warn);
    color: var(--warn);
  }
  .small-action.review-action.danger {
    border-color: var(--danger-strong);
    color: var(--danger-strong);
  }
  .small-action.subtle {
    background: transparent;
    border-color: var(--border);
    color: var(--fg-muted);
  }
  .small-action.subtle:hover:not(:disabled) {
    background: var(--bg-button);
    color: var(--fg-secondary);
  }
  .state-badge {
    display: inline-block;
    font-size: 0.74rem;
    padding: 0.15rem 0.55rem;
    border-radius: 4px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    font-family: ui-monospace, monospace;
  }
  .state-badge.state-enabled {
    background: var(--ok-surface);
    color: var(--ok);
  }
  .state-badge.state-disabled {
    background: var(--warn-surface-2);
    color: var(--warn);
  }
  .state-badge.state-missing {
    background: var(--bg-muted);
    color: var(--fg-faint);
  }
  .action-message {
    margin-top: 0.4rem;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    word-break: break-word;
  }
  .launcher-progress {
    display: flex;
    align-items: center;
    gap: 0.45rem;
    margin-top: 0.35rem;
    color: var(--accent);
    font-size: 0.8rem;
    font-weight: 500;
  }
  .spinner {
    width: 0.8rem;
    height: 0.8rem;
    flex: none;
    display: inline-block;
    vertical-align: -0.12em;
    border: 2px solid var(--border);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: launcher-spin 0.7s linear infinite;
  }
  /* Inline wrapper so a spinner + label sit centred inside a button. */
  .busy {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }
  @keyframes launcher-spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
  .dismiss {
    margin-left: 0.5rem;
    padding: 0 0.3rem;
    background: none;
    border: none;
    color: var(--fg-muted);
    cursor: pointer;
    font-size: 0.75rem;
  }
  .dismiss:hover {
    color: var(--fg-primary);
  }
  .device-title-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 1rem;
  }
  .device-header-actions {
    display: flex;
    gap: 0.5rem;
    align-items: flex-start;
  }
  .reboot-wrap {
    position: relative;
  }
  .reboot-menu {
    position: absolute;
    top: 100%;
    right: 0;
    margin-top: 0.3rem;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    padding: 0.3rem;
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    min-width: 9rem;
    z-index: 5;
  }
  .reboot-menu button {
    text-align: left;
    background: transparent;
    border: none;
  }
  .reboot-menu button:hover {
    background: var(--bg-button);
  }
  .recovery-section {
    margin-top: 1.5rem;
    padding-top: 1.2rem;
    border-top: 1px solid var(--border);
  }
  .danger-button {
    background: var(--danger-surface);
    border-color: var(--danger-border);
    color: var(--danger-surface-text);
    margin-top: 0.6rem;
  }
  .danger-button:hover:not(:disabled) {
    background: var(--danger-border);
  }
  .recovery-result {
    margin-top: 0.8rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .recovery-result ul {
    margin: 0.4rem 0 0;
    padding-left: 1.2rem;
  }
  .apply-row {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    margin: 0.8rem 0 0.4rem;
    flex-wrap: wrap;
  }
  .apply-result {
    margin-top: 0.6rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
  }
  .apply-result ul {
    margin: 0.3rem 0 0;
    padding-left: 1.2rem;
  }
  .warn-text {
    color: var(--warn);
  }
  .screenshot-preview {
    margin-top: 0.8rem;
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
  }
  .screenshot-preview img {
    max-width: 480px;
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .screenshot-meta {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  .clone-panel {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin: 0.4rem 0;
    padding: 0.5rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.9rem;
  }
  .tools-cell {
    white-space: nowrap;
  }
  .rename-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    max-width: 420px;
  }
  .rename-row input {
    flex: 1;
    font-size: 1.1rem;
  }
  h1 .rename-button {
    vertical-align: middle;
    margin-left: 0.5rem;
  }
  .app-toolbar {
    display: flex;
    align-items: center;
    gap: 1rem;
    flex-wrap: wrap;
    margin: 0.6rem 0;
  }
  .app-search {
    flex: 1;
    min-width: 220px;
    max-width: 380px;
  }
  .inline-check {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.9rem;
    white-space: nowrap;
  }
  .other-apps {
    margin-top: 1.6rem;
    padding-top: 1.2rem;
    border-top: 1px solid var(--border);
  }
  .type-cell { white-space: nowrap; }
  .type-cell .tag { white-space: nowrap; }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    color: var(--fg-secondary);
    cursor: pointer;
  }
  .legend {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 0.4rem;
    margin: 0 0 0.8rem;
    padding: 0.5rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    line-height: 1.4;
  }
  .install-output {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.7rem 1rem;
    margin: 0.8rem 0;
    font-family: ui-monospace, monospace;
    font-size: 0.82rem;
    white-space: pre-wrap;
    word-break: break-word;
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
  }
</style>
