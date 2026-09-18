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
    ResourceSample,
    Safety,
  } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";
  import { getKeptPackages, setPackageKept } from "$lib/prefs";
  import Icon from "$lib/components/Icon.svelte";
  import { isBlocked, safetyClass, type SafetyStatus } from "$lib/safety";
  import RamBadge from "$lib/components/RamBadge.svelte";
  import UsageBadge from "$lib/components/UsageBadge.svelte";
  import StateBadge from "$lib/components/StateBadge.svelte";
  import AppRow from "$lib/components/AppRow.svelte";
  import FilesTab from "$lib/components/FilesTab.svelte";
  import TweaksTab from "$lib/components/TweaksTab.svelte";
  import SideloadTab from "$lib/components/SideloadTab.svelte";
  import RemoteTab from "$lib/components/RemoteTab.svelte";
  import OptimizeTab from "$lib/components/OptimizeTab.svelte";
  import MediaTab from "$lib/components/MediaTab.svelte";
  import ShellTab from "$lib/components/ShellTab.svelte";

  let serial = $derived(decodeURIComponent($page.params.serial ?? ""));

  type Tab = "overview" | "health" | "media" | "launcher" | "apps" | "optimize" | "tweaks" | "remote" | "files" | "snapshot" | "sideload" | "shell";
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
  type PageContext = { serial: string; epoch: number };
  let memorySafety = $state<Record<string, SafetyStatus>>({});
  /// Process names confirmed to be installed packages on this device.
  let memoryConfirmed = $state<Set<string>>(new Set());
  /// Package whose safety detail is expanded in the App List, or null. One at a
  /// time: the verdict is a single line, and the reasoning behind it is worth
  /// reading properly rather than skimming twelve at once.
  let expandedSafety = $state<string | null>(null);
  let packageSafety = $state<Record<string, SafetyStatus>>({});
  /// Packages the user has decided to keep on THIS device. An opinion, not
  /// device state — it never touches a snapshot, and it is keyed by hardware
  /// id so it cannot leak between two TVs that swapped addresses.
  let keptPackages = $state<Set<string>>(new Set());
  /// Hides rows that are already decided — kept, or already disabled/removed —
  /// so the list works as a shrinking worklist.
  let hideDecided = $state(false);

  const hardwareId = $derived(device?.properties?.serial_number ?? null);

  function toggleKept(pkg: string) {
    keptPackages = new Set(setPackageKept(hardwareId, pkg, !keptPackages.has(pkg)));
  }
  let pageEpoch = $state(0);
  let deviceRequest = 0;
  let healthRequest = 0;
  let appsRequest = 0;
  let otherRequest = 0;
  let enrichmentRequest = 0;
  let mutationRequest = 0;
  let destroyed = false;

  let profileCopied = $state(false);

  /// The whole spec sheet as text, for pasting into a bug report.
  async function copyProfile() {
    const p = device?.properties;
    if (!p) return;
    const lines = [
      `Friendly name: ${shown(p.friendly_name)}`,
      `Brand: ${shown(p.brand)}`,
      `Model: ${shown(p.model)}`,
      `Codename: ${shown(p.device_codename)}`,
      `Manufacturer: ${shown(p.manufacturer)}`,
      `Android version: ${shown(p.android_release)} (SDK ${shown(p.sdk_level)})`,
      `Build ID: ${shown(p.build_id)}`,
      `Board platform: ${shown(p.board_platform)}`,
      `Hardware ID: ${shown(p.serial_number)}`,
    ];
    try {
      await navigator.clipboard.writeText(lines.join("\n"));
      profileCopied = true;
      setTimeout(() => (profileCopied = false), 2000);
    } catch {
      /* clipboard unavailable — the values are all on screen anyway */
    }
  }

  /// Safety for one row of the memory report.
  ///
  /// Confirming the name against the installed list is allowed to make a
  /// verdict *more* cautious, never less. An app can declare
  /// `android:process` as any string — including another package's name — so
  /// a matching name is strong evidence of ownership but not proof. Being
  /// wrong about Protected or Caution costs a needless warning; being wrong
  /// about Safe tells someone it is fine to remove something on the strength
  /// of a string, which is the one claim this app must never make from an
  /// unverified name. tests/memory-safety.mjs pins that.
  async function memorySafetyFor(pkg: string, installed: Set<string>): Promise<Safety> {
    if (!installed.has(pkg)) return api.processSafetyInfo(pkg);
    const verdict = await api.safetyInfo(pkg);
    return verdict.kind === "safe" ? api.processSafetyInfo(pkg) : verdict;
  }

  function capturePageContext(): PageContext {
    return { serial, epoch: pageEpoch };
  }

  function pageContextIsCurrent(context: PageContext): boolean {
    return !destroyed && context.serial === serial && context.epoch === pageEpoch;
  }

  /// One word for a verdict kind. Kept separate from `safetyLabel` because the
  /// confirm dialogs have a bare `Safety` value, not a load status.
  function safetyKindLabel(kind: Safety["kind"]): string {
    if (kind === "never_disable") return "Protected";
    if (kind === "caution") return "Caution";
    if (kind === "safe") return "Safe";
    return "Unknown";
  }

  function safetyLabel(safety: SafetyStatus | undefined): string {
    if (!safety || safety.status === "unavailable") return "Unavailable";
    if (safety.status === "checking") return "Checking";
    if (safety.verdict.kind === "never_disable") return "Protected";
    if (safety.verdict.kind === "caution") return "Caution";
    // This function feeds the memory table AND "Everything else". Process rows
    // never produce `safe`, but package rows do — without this branch a package
    // we reviewed and vouched for was displayed as "Unknown".
    if (safety.verdict.kind === "safe") return "Safe";
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
  /// Per-stage record from the last failed launcher switch, offered as a copy
  /// button. Kept out of the message itself — it is for a bug report, not for
  /// reading on screen.
  let launcherDiagnostics = $state<string[]>([]);
  let launcherDiagnosticsCopied = $state(false);
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
      // "Decided" is kept-by-you or already off the device; hiding both turns
      // the list into a worklist that shrinks as you work.
      if (hideDecided) {
        const st = appStates[a.package];
        if (keptPackages.has(a.package) || st === "disabled" || st === "missing") return false;
      }
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
  let mediaResetToken = $state(0);
  let shellAcknowledged = $state(false);

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

  // CPU + network rates, fetched separately from the health report: the
  // sample needs a one-second device-side window, and making every refresh
  // wait for it would make the whole tab feel slow.
  let resource = $state<ResourceSample | null>(null);
  let resourceLoading = $state(false);

  /// Fastest inbound interface in the last sample — the one worth a tile.
  /// Ties and empty samples give null, which the tile renders as a dash.
  const busiest = $derived(
    (resource?.interfaces ?? [])
      .filter((n) => n.rx_bytes_per_s != null)
      .sort((a, b) => (b.rx_bytes_per_s ?? 0) - (a.rx_bytes_per_s ?? 0))[0] ?? null,
  );
  const busiestRate = $derived(busiest?.rx_bytes_per_s ?? null);
  const busiestName = $derived(busiest?.name ?? null);

  /// Shared by the RAM tile and the RAM meter, which now live in two separate
  /// cards and must not drift apart.
  const ramPct = $derived(
    report?.ram.total_mb && report.ram.used_mb != null
      ? Math.round((report.ram.used_mb / report.ram.total_mb) * 100)
      : null,
  );
  const ramFreeMb = $derived(
    report?.ram.total_mb != null && report?.ram.used_mb != null
      ? report.ram.total_mb - report.ram.used_mb
      : null,
  );

  let resourceErr = $state<string | null>(null);
  let resourceRequest = 0;

  async function loadResourceSample() {
    if (resourceLoading) return;
    const context = capturePageContext();
    const request = ++resourceRequest;
    resourceLoading = true;
    resourceErr = null;
    try {
      const sample = await api.resourceSample(context.serial);
      if (!pageContextIsCurrent(context) || request !== resourceRequest) return;
      resource = sample;
    } catch (e) {
      if (!pageContextIsCurrent(context) || request !== resourceRequest) return;
      resource = null;
      resourceErr = String(e);
    } finally {
      if (pageContextIsCurrent(context) && request === resourceRequest) resourceLoading = false;
    }
  }

  /// Colour a usage meter by how full it is. Thresholds are the same ones the
  /// memory table already uses for its PSS column, so "amber means getting
  /// full" reads consistently across the Health tab.
  function meterTone(percent: number | null): string {
    if (percent == null) return "";
    if (percent >= 90) return "danger";
    if (percent >= 75) return "warn";
    return "ok";
  }

  /// Bytes/s → the largest unit that keeps the number readable.
  function formatRate(bytesPerSecond: number | null): string {
    if (bytesPerSecond == null) return "—";
    if (bytesPerSecond < 1024) return `${bytesPerSecond} B/s`;
    if (bytesPerSecond < 1024 * 1024) return `${(bytesPerSecond / 1024).toFixed(1)} KB/s`;
    return `${(bytesPerSecond / (1024 * 1024)).toFixed(2)} MB/s`;
  }

  async function loadHealth() {
    const context = capturePageContext();
    const request = ++healthRequest;
    reportLoading = true;
    reportErr = null;
    // Kick the sample off in parallel and let it land on its own.
    void loadResourceSample();
    // Deliberately NOT clearing memorySafety here: Live Refresh ticks every
    // three seconds, and wiping the map made the whole Safety column blank to
    // "Checking" and back twenty times a minute. A process's verdict does not
    // change between ticks; the effect below refetches when the *set* of
    // processes changes.
    try {
      const nextReport = await api.healthReport(context.serial);
      if (!pageContextIsCurrent(context) || request !== healthRequest) return;
      report = nextReport;
      reportLastRefreshed = new Date();
      const pkgs = nextReport.top_memory.map((m) => m.package);
      memorySafety = Object.fromEntries(pkgs.map((pkg) => [pkg, { status: "checking" }]));
      // These are process names from `dumpsys meminfo`, and a process can be
      // named anything, so a bare name must not inherit a curated verdict.
      // But a name that matches a package the device reports as installed IS
      // that package — Android names a process after its package by default —
      // and for those the catalog applies legitimately. So: confirm against
      // the installed list first, and only then ask the catalog-aware lookup.
      // Anything we cannot confirm still gets the catalog-free classification.
      let installed: Set<string>;
      try {
        installed = new Set(
          (await api.listInstalledPackages(context.serial)).map((p) => p.package),
        );
      } catch {
        // No installed list means nothing is confirmed; fall back to treating
        // every row as an unverified process rather than guessing.
        installed = new Set();
      }
      if (!pageContextIsCurrent(context) || request !== healthRequest) return;
      memoryConfirmed = installed;
      const results = await Promise.allSettled(
        pkgs.map((pkg) => memorySafetyFor(pkg, installed)),
      );
      // Deliberately not comparing `report` to `nextReport`: `report` is
      // $state, so assigning an object stores a deep proxy and the identity
      // check is always true — which bailed out here every time and left every
      // row stuck on "Checking". The request token already proves this load is
      // the current one.
      if (!pageContextIsCurrent(context) || request !== healthRequest) return;
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
      keptPackages = getKeptPackages(hardwareId);
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
      appActionMessage = `${pkg}: we couldn't read the current disabled state. Refresh to retry.`;
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
    if (isBlocked(displayedSafety.verdict)) {
      appActionMessage = `${pkg}: protected — ${displayedSafety.verdict.reason}`;
      return;
    }
    const context = capturePageContext();
    const request = ++mutationRequest;
    const inventoryVersion = source === "catalog" ? catalogInventoryVersion : otherInventoryVersion;
    if (!removalSourceIsCurrent(source, pkg, inventoryVersion)) {
      appActionMessage = `${pkg}: we couldn't read the current package list. Refresh to retry.`;
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
      if (isBlocked(before.safety)) {
        appActionMessage = `${pkg}: protected — ${before.safety.reason}`;
        return;
      }
      if (before.safety.kind !== displayedSafety.verdict.kind
        || before.safety.reason !== displayedSafety.verdict.reason) {
        appActionMessage = `${pkg}: safety changed. Refresh and review before ${action}.`;
        return;
      }
      const approved = confirm(
        `${action.toUpperCase()} ${name}\nPackage: ${pkg}\nSafety: ${safetyKindLabel(before.safety.kind)}\nReason: ${before.safety.reason}\n\nProceed?`,
      );
      if (!approved
        || !pageContextIsCurrent(context)
        || request !== mutationRequest
        || !removalSourceIsCurrent(source, pkg, inventoryVersion)) return;
      const after = await readRemovalEvidence(context, pkg);
      if (!pageContextIsCurrent(context)
        || request !== mutationRequest
        || !removalSourceIsCurrent(source, pkg, inventoryVersion)) return;
      if (isBlocked(after.safety)
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
      appActionMessage = `${pkg}: ${e}. Refresh to retry; nothing was removed without a current reading.`;
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
      appActionMessage = `${pkg}: we couldn't read the current disabled state. Refresh to retry.`;
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
    if (isBlocked(safety.verdict)) return { kind: "done", label: "Protected" };
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

  async function copyLauncherDiagnostics() {
    const report = launcherDiagnostics.join("\n");
    try {
      await navigator.clipboard.writeText(report);
      launcherDiagnosticsCopied = true;
    } catch {
      // Clipboard can be refused; show the text so it can still be selected.
      launcherActionMessage = report;
    }
  }

  async function setDefaultLauncher(pkg: string) {
    const name = launchers.find((l) => l.entry.package === pkg)?.entry.name ?? pkg;
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    launcherDiagnostics = [];
    launcherDiagnosticsCopied = false;
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
        // Only on failure: this is the record a reporter can paste back, and
        // it is the only thing that distinguishes "the device refused the
        // command" from "the device accepted it and ignored it".
        launcherDiagnostics = r.diagnostics ?? [];
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
      preview.settings_to_delete.length +
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
    // Health reads it too, for the "every change is reversible" callout.
    if ((activeTab === "snapshot" || activeTab === "health") && !snapshotsLoaded) loadSnapshots();
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
    mediaResetToken++;
  }

  function shellExecuted() {
    invalidateDeviceCaches();
    appsRequest++;
    otherRequest++;
    enrichmentRequest++;
    appsLoaded = false;
    appsLoading = false;
    othersLoaded = false;
    othersLoading = false;
    preview = null;
    previewPath = null;
    optimizeResetToken++;
    visited.tweaks = false;
    void loadDevice();
  }

  /// Wipe all per-device state. Used if the route's serial changes under a
  /// live component (today the only way off this page is "← Back to devices",
  /// so this is defensive — but it guarantees no device's data or in-flight
  /// timer can leak onto another if a device→device link is ever added).
  function resetDeviceState() {
    pageEpoch++;
    deviceRequest++;
    healthRequest++;
    resourceRequest++;
    resource = null;
    resourceLoading = false;
    resourceErr = null;
    shellAcknowledged = false;
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
    memoryConfirmed = new Set();
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

  /// A property the device never answered arrives as an empty string. Show an
  /// em dash rather than a blank cell, so "unreported" reads differently from
  /// "reported as empty".
  function shown(value: string | null | undefined): string {
    const trimmed = value?.trim() ?? "";
    return trimmed === "" || trimmed === "unknown" ? "—" : trimmed;
  }

  onMount(loadDevice);
</script>

<div class="back-row">
  <button class="back-btn" onclick={() => goto("/")}>
    <Icon name="arrow_back" size={16} /> Back to devices
  </button>
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
      </div>
      <div class="device-header-actions">
        <div class="reboot-wrap">
          <button
            class="reboot-btn"
            onclick={() => (rebootMenuOpen = !rebootMenuOpen)}
            disabled={rebootBusy}
            aria-haspopup="menu"
            aria-expanded={rebootMenuOpen}
          >
            {rebootBusy ? "Rebooting…" : "Reboot"}{#if !rebootBusy}<Icon name="expand_more" size={16} />{/if}
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
    <div class="device-meta">
      <span>{deviceTypeLabel(device.device_type)}</span>
      {#if device.model}<span>· {device.model}</span>{/if}
      <span class="serial">· {device.serial}</span>
      {#if device.properties?.android_release}
        <span>· Android {device.properties.android_release}</span>
      {/if}
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
    <!-- Ordered by the shape of the job rather than by history: the tabs you
         act in come first, in roughly the order a debloat happens, then the
         ones you only read, then Shell pushed to the far edge — it is the
         documented opt-in exception to the safety story, so it should not sit
         shoulder to shoulder with the curated actions. Grouping is carried by
         that gap alone; hairline separators between tabs read as rendering
         artifacts rather than as structure. -->
    {#each [
      { id: "overview", label: "Overview", far: false },
      { id: "optimize", label: "Optimize", far: false },
      { id: "apps", label: "App List", far: false },
      { id: "launcher", label: "Launcher", far: false },
      { id: "tweaks", label: "Tweaks", far: false },
      { id: "snapshot", label: "Snapshot", far: false },
      { id: "sideload", label: "Install APK", far: false },
      { id: "remote", label: "Remote", far: false },
      { id: "files", label: "Files", far: false },
      { id: "health", label: "Health", far: false },
      { id: "media", label: "Playback", far: false },
      { id: "shell", label: "Shell", far: true },
    ] as t (t.id)}
      <button
        role="tab"
        aria-selected={activeTab === t.id}
        aria-controls={`tabpanel-${t.id}`}
        id={`tab-${t.id}`}
        class:active={activeTab === t.id}
        class:far={t.far}
        onclick={() => (activeTab = t.id as Tab)}
      >
        {t.label}
      </button>
    {/each}
  </div>

  {#if activeTab === "overview"}
    <div class="profile-layout" role="tabpanel" tabindex={0} id="tabpanel-overview" aria-labelledby="tab-overview">
      <div class="card">
        <div class="card-header">
          <h2><Icon name="tv" size={17} /> Profile</h2>
          <div class="header-actions">
            <button class="small-action" onclick={copyProfile} disabled={!device.properties}>
              <Icon name="content_copy" size={14} /> {profileCopied ? "Copied" : "Copy all"}
            </button>
          </div>
        </div>
        {#if device.properties}
          <h3>Device properties</h3>
          <div class="prop-table">
            <div class="prop-row">
              <span class="prop-label">Friendly name</span>
              <span class="prop-value">{shown(device.properties.friendly_name)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Brand</span>
              <span class="prop-value mono">{shown(device.properties.brand)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Model</span>
              <span class="prop-value mono">{shown(device.properties.model)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Codename</span>
              <span class="prop-value mono">{shown(device.properties.device_codename)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Manufacturer</span>
              <span class="prop-value mono">{shown(device.properties.manufacturer)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Build ID</span>
              <span class="prop-value mono">{shown(device.properties.build_id)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Board platform</span>
              <span class="prop-value mono">{shown(device.properties.board_platform)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Hardware ID</span>
              <span class="prop-value mono">{shown(device.properties.serial_number)}</span>
            </div>
            <div class="prop-row">
              <span class="prop-label">Android version</span>
              <span class="prop-value mono">
                {shown(device.properties.android_release)} · SDK {shown(device.properties.sdk_level)}
              </span>
            </div>
          </div>
          <p class="muted small prop-note">
            Read-only — sourced from <code>getprop</code>. The friendly name is the
            only field this app can change, with Rename above.
          </p>
        {:else}
          <p class="muted small">
            This device hasn't reported its details. It's usually still waiting on
            the debugging authorization prompt on the TV — over the network some
            TVs still title that "Allow USB debugging?".
          </p>
        {/if}
      </div>

      <aside class="profile-side">
        <h3 class="side-label">Emergency recovery</h3>
        <div class="card danger-card">
          <h2><Icon name="restore" size={17} /> Re-enable everything</h2>
          <!-- The copy says "everything currently disabled" rather than "what
               this app disabled", because that is what the command does:
               panic_recovery runs `pm enable` over `pm list packages -d`. There
               is no per-app change log, so a package you disabled by hand
               elsewhere is re-enabled too. -->
          <p class="muted small">
            Re-enables every package that is currently disabled on this TV — not
            only the ones changed here. Use it when the TV boots to a black
            screen, the launcher is gone, or an app you need has vanished.
          </p>
          <button
            class="danger-button recovery-run"
            onclick={runRecovery}
            disabled={recoveryBusy}
            title="pm enable every package currently in `pm list packages -d`"
          >
            {recoveryBusy ? "Restoring…" : "Run Emergency Recovery"}
          </button>
          <p class="muted recovery-caption">Asks for confirmation · cannot be undone from here</p>
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
        <div class="callout">
          <Icon name="info" size={16} />
          <span>
            Recovery cannot tell which app disabled a package. Anything disabled
            outside this app is re-enabled as well.
          </span>
        </div>
      </aside>
    </div>
  {:else if activeTab === "health"}
    <div class="health-stack" role="tabpanel" tabindex={0} id="tabpanel-health" aria-labelledby="tab-health">
      <div class="card">
        <div class="card-header">
          <h2><Icon name="monitor_heart" size={17} /> Vitals</h2>
          <div class="header-actions">
            <span class="muted small" title={reportLastRefreshed?.toISOString() ?? ""}>
              {refreshLabel}
            </span>
            <!-- A button, not a checkbox: it is the only control in a ruled
                 header that was not one. The dot carries the on state so the
                 lime fill is not doing status duty. -->
            <button
              class="live-toggle"
              class:on={liveRefresh}
              aria-pressed={liveRefresh}
              onclick={toggleLiveRefresh}
              title="Re-read the health report every few seconds"
            >
              <span class="live-dot" aria-hidden="true"></span> Live
            </button>
            <button onclick={loadHealth} disabled={reportLoading}>
              {reportLoading ? "Loading…" : "Refresh"}
            </button>
          </div>
        </div>
        {#if reportErr}
          <div class="error">{reportErr}</div>
        {:else if !report}
          <div class="muted">{reportLoading ? "Querying…" : "—"}</div>
        {:else}
        <div class="stat-tiles">
          <div class="stat-tile">
            <span class="stat-icon {ramPct != null ? meterTone(ramPct) : ''}">
              <Icon name="memory" size={20} />
            </span>
            <span class="stat-value">
              {#if ramFreeMb != null}
                {ramFreeMb >= 1024 ? (ramFreeMb / 1024).toFixed(1) : ramFreeMb}<span
                  class="stat-unit">{ramFreeMb >= 1024 ? "GB" : "MB"}</span
                >
              {:else}—{/if}
            </span>
            <span class="stat-caption">RAM free</span>
          </div>
          <div class="stat-tile">
            <span
              class="stat-icon {report.storage.used_percent != null
                ? meterTone(report.storage.used_percent)
                : ''}"
            >
              <Icon name="storage" size={20} />
            </span>
            <span class="stat-value">
              {#if report.storage.used_percent != null}
                {report.storage.used_percent}<span class="stat-unit">%</span>
              {:else}—{/if}
            </span>
            <span class="stat-caption">Storage used</span>
          </div>
          <div class="stat-tile">
            <span class="stat-icon">
              <Icon name="device_thermostat" size={20} />
            </span>
            <span class="stat-value">
              {#if report.temperature_c != null}
                {report.temperature_c.toFixed(0)}<span class="stat-unit">°C</span>
              {:else}—{/if}
            </span>
            <span class="stat-caption">Temp</span>
          </div>
          <div class="stat-tile">
            <span class="stat-icon">
              <Icon name="memory" size={20} />
            </span>
            <span class="stat-value">
              {#if resource?.cpu_percent != null}
                {resource.cpu_percent.toFixed(0)}<span class="stat-unit">%</span>
              {:else}<span class="stat-pending">{resourceLoading ? "…" : "—"}</span>{/if}
            </span>
            <span class="stat-caption">CPU</span>
          </div>
          <div class="stat-tile">
            <span class="stat-icon">
              <Icon name="arrow_downward" size={20} />
            </span>
            <span class="stat-value">
              {#if busiestRate != null}
                {formatRate(busiestRate)}
              {:else}<span class="stat-pending">{resourceLoading ? "…" : "—"}</span>{/if}
            </span>
            <span class="stat-caption">{busiestName ?? "Network"}</span>
          </div>
        </div>
        {/if}
      </div>

      {#if report && !reportErr}
      <div class="health-grid">
        <div class="health-col">
          <div class="card">
            <h2><Icon name="speed" size={17} /> Resources</h2>
            {#if resourceErr}<p class="error">Resource sample: {resourceErr}</p>{/if}
        <dl class="kv">
          <dt>CPU</dt>
          <dd>
            {#if resource?.cpu_percent != null}
              {resource.cpu_percent.toFixed(1)}%
              {#if resource.interval_ms != null}<span class="muted small">over {(resource.interval_ms / 1000).toFixed(2)}s</span>{/if}
              <button
                class="small-action subtle"
                onclick={loadResourceSample}
                disabled={resourceLoading}
                title="Re-read CPU and network without re-running the whole report"
              >{resourceLoading ? "Sampling…" : "Sample again"}</button>
            {:else}
              {resourceLoading ? "sampling…" : "—"}
            {/if}
          </dd>
          <dt>Network</dt>
          <dd>
            {#if resource?.interfaces.length}
              <div class="net-grid">
                {#each resource.interfaces as network (network.name)}
                  <span class="net-name">{network.name}</span>
                  <span><Icon name="arrow_downward" size={13} /> {formatRate(network.rx_bytes_per_s)}</span>
                  <span><Icon name="arrow_upward" size={13} /> {formatRate(network.tx_bytes_per_s)}</span>
                {/each}
              </div>
            {:else}
              {resourceLoading ? "sampling…" : "—"}
            {/if}
          </dd>
          <dt>Temperature</dt>
          <dd>{report.temperature_c != null ? `${report.temperature_c.toFixed(1)}°C` : "—"}</dd>
          {#if report.ram.total_mb != null}
            <dt>RAM</dt>
            <dd>
              <div class="meter-value">
                <span>{report.ram.used_mb ?? "?"} / {report.ram.total_mb} MB</span>
                {#if ramPct != null}<span class="muted">{ramPct}%</span>{/if}
              </div>
              {#if ramPct != null}
                <div class="meter" role="presentation">
                  <div class="meter-fill {meterTone(ramPct)}" style="width: {Math.min(100, ramPct)}%"></div>
                </div>
              {/if}
            </dd>
          {/if}
          {#if report.ram.swap_mb != null}
            <dt>Swap</dt><dd>{report.ram.swap_mb} MB</dd>
          {/if}
          {#if report.storage.total}
            <dt>Storage</dt>
            <dd>
              <div class="meter-value">
                <span>{report.storage.used ?? "?"} / {report.storage.total}</span>
                {#if report.storage.used_percent != null}
                  <span class="muted">{report.storage.used_percent}%</span>
                {/if}
                <!-- A device mutation that moves exactly this number, so it
                     belongs beside it rather than in the header looking like a
                     view control. -->
                <button
                  class="small-action subtle"
                  onclick={clearCaches}
                  disabled={trimBusy}
                  title="pm trim-caches — clears every app's cache; caches rebuild on next launch"
                >{trimBusy ? "Clearing…" : "Clear caches"}</button>
              </div>
              {#if trimMessage}
                <p class="muted small mono trim-note">{trimMessage}</p>
              {/if}
              {#if report.storage.used_percent != null}
                <div class="meter" role="presentation">
                  <div
                    class="meter-fill {meterTone(report.storage.used_percent)}"
                    style="width: {Math.min(100, report.storage.used_percent)}%"
                  ></div>
                </div>
              {/if}
            </dd>
          {/if}
        </dl>

          </div>

          <div class="card">
            <h2><Icon name="tv" size={17} /> Display &amp; Audio</h2>
            <!-- One card, two blocks. The board draws two cards, but the audio
                 side is a single string and a whole card for one value reads
                 as empty. The chips are facts, not verdicts, so they take no
                 status colour. -->
            <div class="av-grid">
              <div class="av-block">
                <h3>Display</h3>
                <div class="av-primary">{report.display.resolution ?? "—"}</div>
                <div class="av-chips">
                  {#if report.display.refresh_hz}
                    <span class="av-chip">{report.display.refresh_hz} Hz</span>
                  {/if}
                  {#if report.display.hdr_types.length}
                    {#each report.display.hdr_types as hdr (hdr)}
                      <span class="av-chip">{hdr}</span>
                    {/each}
                  {:else}
                    <span class="av-chip muted">SDR only</span>
                  {/if}
                </div>
              </div>
              <span class="av-rule" aria-hidden="true"></span>
              <div class="av-block">
                <h3>Audio out</h3>
                <!-- Printed whole. We cannot reliably split "Dolby Atmos" from
                     "over HDMI (eARC)", and guessing would be inventing data. -->
                <div class="av-primary av-audio">{report.audio_device ?? "—"}</div>
              </div>
            </div>
          </div>
        </div>

        <div class="card health-memory">
          <h2><Icon name="memory" size={17} /> Top memory users</h2>
        <p class="muted small consumers-note">
          Rows whose name matches an installed package are classified against the
          reviewed app list. The rest are process names we cannot tie to an app,
          so only the protected and caution rules apply to them. Inspection only
          — nothing here can be disabled from this table.
        </p>
        {#if report.top_memory.length === 0}
          <p class="muted">No process data.</p>
        {:else}
          <table class="mem-table">
            <thead>
              <tr><th>Memory</th><th>Process</th><th class="center">Safety</th></tr>
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
                    {#if !memoryConfirmed.has(m.package)}
                      <span
                        class="unconfirmed"
                        title="No installed package has this name, so this is a process we cannot tie to an app. Its verdict comes from the protected and caution rules only — the reviewed app list is not applied to unverified names."
                      >not a package</span>
                    {/if}
                  </td>
                  <td class="center" title={safetyReason(safety)}>
                    <span class={safetyClass(safety)}>{safetyLabel(safety)}</span>
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if appActionMessage}
            <p class="muted small mono">
              {appActionMessage}
              <button class="dismiss" onclick={() => (appActionMessage = "")} title="Dismiss" aria-label="Dismiss"><Icon name="close" size={15} /></button>
            </p>
          {/if}
        {/if}
        </div>
      </div>

      <!-- The one piece of genuine state this screen can lead with, in place
           of the board's invented score: whether the work is reversible. -->
      {#if snapshots.length > 0}
        {@const newest = snapshots[0]}
        <div class="callout callout-ok">
          <Icon name="check_circle" size={16} />
          <span>
            Snapshot saved {snapTimestamp(newest.saved_at)} — every change is reversible.
          </span>
          <button class="callout-link" onclick={() => (activeTab = "snapshot")}>
            Open snapshots
          </button>
        </div>
      {:else}
        <div class="callout callout-warn">
          <Icon name="warning" size={16} />
          <span>No snapshot saved yet — save one before you change anything.</span>
          <button class="callout-link" onclick={() => (activeTab = "snapshot")}>
            Save a snapshot
          </button>
        </div>
      {/if}
      {/if}
    </div>
  {:else if activeTab === "launcher"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-launcher" aria-labelledby="tab-launcher">
      <div class="card-header">
        <h2><Icon name="home" size={17} /> Launchers</h2>
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
            <Icon name="warning" size={15} /> <code>com.android.providers.tv</code> is disabled on this device. Watch Next / Continue
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
          {#if launcherDiagnostics.length > 0}
            <p class="muted small action-message">
              <button class="link-button" onclick={copyLauncherDiagnostics}>
                {launcherDiagnosticsCopied ? "Copied" : "Copy diagnostic details"}
              </button>
              — every command this attempt ran and what your TV replied. Paste it into a
              GitHub issue; it is what makes a launcher report fixable.
            </p>
          {/if}
        {/if}
      {/if}
    </div>
  {:else if activeTab === "apps"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-apps" aria-labelledby="tab-apps">
      <div class="card-header">
        <h2><Icon name="apps" size={17} /> App List for {deviceTypeLabel(device.device_type)}</h2>
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
          <input type="checkbox" bind:checked={hideDecided} />
          Hide decided
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
        {#if appActionMessage}
          <p class="muted small mono action-message">
            {appActionMessage}
            <button class="dismiss" onclick={() => (appActionMessage = "")} title="Dismiss" aria-label="Dismiss"><Icon name="close" size={15} /></button>
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
              <th class="center" title="What the device reports right now.">State</th>
              <th
                class="center"
                title="Our verdict on removing it — click a verdict for the reason and where it came from. Anything we can't vouch for needs an explicit tick before it can be removed."
              >Safety</th>
              <th class="controls-start">Action</th>
              <th
                class="center"
                title="Play Store link, APK backup, and copy to another device."
              >Tools</th>
            </tr>
          </thead>
          <tbody>
            {#each visibleApps as a (a.package)}
              {@const state = appStates[a.package] ?? null}
              {@const safety = packageSafety[a.package]}
              {@const rec = recommendation(a, state, safety)}
              {@const canRemove = (state === "enabled" || state === "disabled") && safety?.status === "ready" && !isBlocked(safety.verdict)}
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
                safetyUnavailableReason={safety?.status === "unavailable" ? safety.reason : undefined}
                detailOpen={expandedSafety === a.package}
                onToggleDetail={() =>
                  (expandedSafety = expandedSafety === a.package ? null : a.package)}
              >
                {#snippet actions()}
                <td class="rec-cell controls-start">
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
                  {:else if keptPackages.has(a.package)}
                    <!-- The user's own decision, shown exactly like "already
                         disabled": grey, because a decided row should recede.
                         Never teal or lime — those mean verdict and action. -->
                    <span class="muted small done"><Icon name="check" size={14} /> Kept</span>
                    <button
                      class="small-action subtle change-keep"
                      onclick={() => toggleKept(a.package)}
                      title="Undo keeping this app"
                    >Change</button>
                  {:else if rec.kind === "done"}
                    <span class="muted small done"><Icon name="check" size={14} /> {rec.label}</span>
                  {:else if rec.kind === "unavailable"}
                    <span class="muted small">{rec.label} — refresh to retry</span>
                  {:else}
                    <!-- No recommendation. This used to read "Keep", which now
                         collides with the Keep button one column over: the same
                         word meant both "we suggest keeping it" and "I have
                         decided to keep it". -->
                    <span class="muted small">No change needed</span>
                  {/if}

                  {#if !keptPackages.has(a.package) && state === "enabled" && (rec.kind === "act" || rec.kind === "review")}
                    <button
                      class="small-action subtle"
                      onclick={() => toggleKept(a.package)}
                      title="Mark this as one you use, so it stops being recommended for removal"
                    >Keep</button>
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
            Removals here still go through the same safety checks as the curated list.
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
                  {@const canRemove = othersLoaded && safety?.status === "ready" && !isBlocked(safety.verdict)}
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
                    <td class="center" title={safetyReason(safety)}>
                      <span class={safetyClass(safety)}>{safetyLabel(safety)}</span>
                    </td>
                    <td class="rec-cell">
                      {#if o.enabled}
                        <button class="small-action subtle" onclick={() => disableOther(o.package)} disabled={appActionBusy === o.package || appMutationInFlight || !canRemove} title="Needs a completed safety check and a current package list">Disable</button>
                        <button class="small-action subtle danger" onclick={() => uninstallOther(o.package)} disabled={appActionBusy === o.package || appMutationInFlight || !canRemove} title="Needs a completed safety check and a current package list">Uninstall</button>
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
        <h2><Icon name="history" size={17} /> Snapshots</h2>
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
            <li><strong>{preview.settings_to_delete.length}</strong> settings will be reset to device defaults
              {#each preview.settings_to_delete as key}<div><code>{key}</code></div>{/each}
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
      <TweaksTab {serial} onSettingsChanged={invalidateDeviceCaches} />
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
  {#if visited.media}
    <div hidden={activeTab !== "media"}>
      <MediaTab {serial} resetToken={mediaResetToken} active={activeTab === "media"} />
    </div>
  {/if}
  {#if visited.shell}
    <div hidden={activeTab !== "shell"}>
      <ShellTab {serial} bind:acknowledged={shellAcknowledged} onexecuted={shellExecuted} />
    </div>
  {/if}
  {#if visited.optimize}
    <div hidden={activeTab !== "optimize"}>
      <OptimizeTab
        {serial}
        deviceType={device.device_type}
        {appUsage}
        {keptPackages}
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
    font-family: var(--mono);
    font-size: 0.85rem;
  }
  /* Controls that now pair an icon with a label. */
  /* Scan layer for the Health tab: the three numbers people look for first,
     duplicated from the Vitals list below rather than moved out of it. The
     icon carries the threshold colour so the tone is readable before the
     number is. */
  /* Overview is two cards, not one: a read-only spec sheet and a destructive
     action. They were sharing a card, which made the recovery button read as
     a footnote on the device's build id. */
  .overview-stack {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  .danger-card {
    border-color: var(--danger-border);
    background: linear-gradient(
      var(--danger-surface),
      var(--danger-surface)
    ), var(--bg-surface);
  }
  .danger-card h2 :global(.msr) {
    color: var(--danger-text);
  }
  /* Board 11.1: the spec sheet takes the width it needs and recovery sits
     beside it, so a destructive action is never buried under a scroll of
     read-only values. */
  .profile-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) 22rem;
    gap: 1.25rem;
    align-items: start;
  }
  .profile-side {
    display: flex;
    flex-direction: column;
    gap: 0.75rem;
  }
  .side-label {
    margin: 0 0 0.1rem;
    font-family: var(--mono);
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.12em;
    text-transform: uppercase;
    color: var(--fg-muted);
  }
  /* Rows rather than a definition list: the values line up in one column and
     each property reads as its own line. */
  .prop-table {
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    overflow: hidden;
  }
  .prop-row {
    display: grid;
    grid-template-columns: 11rem minmax(0, 1fr);
    gap: 1rem;
    align-items: baseline;
    padding: 0.6rem 0.9rem;
  }
  .prop-row + .prop-row {
    border-top: 1px solid var(--border);
  }
  .prop-row:nth-child(odd) {
    background: var(--bg-surface-2);
  }
  .prop-label {
    color: var(--fg-muted);
    font-size: 0.85rem;
  }
  .prop-value {
    overflow-wrap: anywhere;
  }
  .prop-note {
    margin-top: 0.7rem;
  }
  .danger-card {
    border-color: var(--danger-border);
    background: linear-gradient(var(--danger-surface), var(--danger-surface)),
      var(--bg-surface);
  }
  .danger-card h2 :global(.msr) {
    color: var(--danger-text);
  }
  .recovery-run {
    width: 100%;
    margin-top: 0.9rem;
  }
  .recovery-caption {
    margin: 0.5rem 0 0;
    text-align: center;
    font-size: 0.74rem;
  }
  /* Neutral note, not a warning: it qualifies the scope of the action above
     rather than adding a second alarm. */
  .callout {
    display: flex;
    gap: 0.6rem;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-surface);
    color: var(--fg-secondary);
    font-size: 0.8rem;
    line-height: 1.45;
  }
  .callout :global(.msr) {
    color: var(--fg-muted);
    margin-top: 0.1rem;
  }
  @media (max-width: 1100px) {
    .profile-layout {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  /* Five tiles on a fill-first grid rather than three stretched across the
     full width — that stretch was most of the empty space. The icon sits on
     the number's line instead of on a row of its own. */
  /* Marks a row we could not match to an installed package, so a reader can
     tell "no rule covered this app" apart from "this is not an app". */
  .unconfirmed {
    margin-left: 0.45rem;
    padding: 0.05rem 0.35rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-surface-2);
    color: var(--fg-muted);
    font-family: var(--sans);
    font-size: 0.68rem;
    white-space: nowrap;
  }
  /* Hairline between tab groups — twelve tabs at this width have no room for
     captions, so the rule does the grouping. */
  /* Only on hover: an undo does not need to advertise itself on every
     decided row. */
  .change-keep {
    opacity: 0;
  }
  tr:hover .change-keep,
  .change-keep:focus-visible {
    opacity: 1;
  }
  .tabs button.far {
    margin-left: auto;
  }
  .health-stack {
    display: flex;
    flex-direction: column;
    gap: 1rem;
  }
  /* Memory takes the wide column, inverting the board: its Process cells carry
     forty-to-seventy-character package ids, which wrap on every row in a
     narrow rail. */
  .health-grid {
    display: grid;
    grid-template-columns: minmax(0, 5fr) minmax(0, 7fr);
    gap: 1.25rem;
    align-items: start;
  }
  .health-col {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    min-width: 0;
  }
  .health-memory {
    min-width: 0;
  }
  .av-grid {
    display: grid;
    grid-template-columns: 1fr 1px 1fr;
    gap: 1rem;
    align-items: start;
  }
  .av-block {
    min-width: 0;
  }
  .av-block h3 {
    margin-top: 0;
  }
  .av-rule {
    align-self: stretch;
    background: var(--border);
  }
  .av-primary {
    font-family: var(--mono);
    font-size: 1.3rem;
    font-weight: 600;
    line-height: 1.15;
  }
  .av-audio {
    font-size: 1rem;
    font-weight: 500;
    overflow-wrap: anywhere;
  }
  .av-chips {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem;
    margin-top: 0.45rem;
  }
  /* Facts about the panel, not verdicts — no status colour. */
  .av-chip {
    font-family: var(--mono);
    font-size: 0.72rem;
    padding: 0.1rem 0.45rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    background: var(--bg-surface-2);
    color: var(--fg-secondary);
    white-space: nowrap;
  }
  .live-toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .live-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--fg-muted);
  }
  .live-toggle.on {
    border-color: var(--ok);
    color: var(--fg-primary);
  }
  .live-toggle.on .live-dot {
    background: var(--ok);
  }
  .callout-link {
    margin-left: auto;
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font: inherit;
    white-space: nowrap;
    cursor: pointer;
  }
  .callout-ok {
    border-color: color-mix(in srgb, var(--ok) 35%, transparent);
  }
  .callout-ok :global(.msr) {
    color: var(--ok);
  }
  .callout-warn :global(.msr) {
    color: var(--warn);
  }
  .trim-note {
    margin: 0.3rem 0 0;
  }
  /* An outlier package id must never widen the table. */
  .mem-table .pkg {
    max-width: 0;
    overflow-wrap: anywhere;
  }
  @media (max-width: 1100px) {
    .health-grid {
      grid-template-columns: minmax(0, 1fr);
    }
  }
  .stat-tiles {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(9.5rem, 1fr));
    gap: 0.6rem;
    margin: 0 0 1.1rem;
  }
  .stat-tile {
    display: grid;
    grid-template-columns: auto minmax(0, 1fr);
    grid-template-rows: auto auto;
    column-gap: 0.55rem;
    row-gap: 0.1rem;
    align-items: center;
    padding: 0.7rem 0.85rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-surface-2);
  }
  .stat-icon {
    grid-row: 1 / span 2;
    display: inline-flex;
    color: var(--fg-muted);
  }
  .stat-pending {
    color: var(--fg-muted);
  }
  .stat-icon.ok {
    color: var(--ok);
  }
  .stat-icon.warn {
    color: var(--warn);
  }
  .stat-icon.danger {
    color: var(--danger-text);
  }
  .stat-value {
    grid-column: 2;
    font-family: var(--mono);
    font-size: 1.25rem;
    font-weight: 600;
    line-height: 1.15;
    white-space: nowrap;
  }
  .stat-unit {
    margin-left: 0.15rem;
    font-size: 0.8rem;
    font-weight: 500;
    color: var(--fg-muted);
  }
  .stat-caption {
    grid-column: 2;
    font-size: 0.72rem;
    color: var(--fg-muted);
  }
  .back-btn,
  .reboot-btn,
  .net-grid span,
  .done {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }
  .tabs {
    display: flex;
    gap: 0.4rem;
    margin-bottom: 1rem;
    border-bottom: 1px solid var(--border);
    padding-bottom: 0;
    /* Twelve tabs do not always fit. Scrolling the strip is the honest
       failure: wrapping "Install APK" onto two lines makes one tab twice the
       height of its neighbours and shoves the underline off the baseline. */
    overflow-x: auto;
    scrollbar-width: thin;
  }
  .tabs button {
    flex: none;
    white-space: nowrap;
    border: none;
    border-bottom: 2px solid transparent;
    border-radius: 0;
    background: transparent;
    padding: 0.5rem 0.7rem;
  }
  .tabs button.active {
    color: var(--accent);
    border-bottom-color: var(--accent);
  }
  /* Usage meters, lifted from the mobile Diagnostics screen: a number alone
     makes you do the arithmetic, a bar tells you at a glance. Desktop tokens
     rather than mobile's, so it matches the rest of this app. */
  .meter-value {
    display: flex;
    justify-content: space-between;
    gap: 1rem;
    align-items: baseline;
    max-width: 22rem;
  }
  .meter {
    max-width: 22rem;
    margin-top: 0.3rem;
    height: 6px;
    border-radius: var(--radius-xs);
    background: var(--bg-inset);
    overflow: hidden;
  }
  .meter-fill {
    height: 100%;
    border-radius: var(--radius-xs);
    transition: width 0.3s ease;
  }
  /* Solid, not a gradient. In dark --accent and --accent-strong are the same
     lime so the gradient was already flat; in light they are dark olive and
     lime, which made one bar look like two different states. --accent is the
     readable one against the track in both themes. */
  .meter-fill.ok {
    background: var(--accent);
  }
  .meter-fill.warn {
    background: var(--warn);
  }
  .meter-fill.danger {
    background: var(--danger);
  }
  /* One column per field instead of a ragged "name: down x / up y" line, so
     the rates line up when a device reports six interfaces. */
  .net-grid {
    display: grid;
    grid-template-columns: auto auto auto;
    gap: 0.1rem 1rem;
    justify-content: start;
  }
  .net-name {
    color: var(--fg-muted);
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
    font-family: var(--mono);
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  th, td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  th.center, td.center {
    text-align: center;
  }
  .app-table .app-cell {
    /* The flexible column. Every other cell is width:1% + nowrap, so this one
       takes the remainder — but a nowrap description makes its min-content the
       full sentence, which widens the table until State/Safety/Action fall off
       the right edge. max-width:0 lets it shrink to the space left over, which
       is what makes the ellipsis fire instead of the table growing. */
    max-width: 0;
    line-height: 1.3;
    /* Long system package ids (com.google.android.overlay.modules.…) are one
       unbreakable token; without this they force the column — and the whole
       table — wider than the viewport, pushing the action buttons off-screen.
       `anywhere` (not `break-word`) also shrinks the column's min-content width
       so the table stops overflowing. Inherited by the child name/pkg rows. */
    overflow-wrap: anywhere;
  }
  /* Everything right of this line does something; everything left of it tells
     you something. One rule down the whole table rather than a tinted column,
     which becomes a stripe over three hundred rows. */
  .app-table .controls-start {
    border-left: 1px solid var(--border);
    padding-left: 1rem;
  }
  /* Carries the eye from the app name across to its controls — the columns are
     far apart on a 1280px window. */
  .app-table tbody tr:hover td {
    background: var(--bg-inset);
  }
  .app-table .rec-cell,
  .app-table .tools-cell {
    /* Keep the action/tool buttons from being squeezed once the name column
       can shrink — they stay on one line at their natural width. */
    white-space: nowrap;
    width: 1%;
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
    font-family: var(--mono);
    text-align: right;
    width: 100px;
  }
  td.num.warn { color: var(--danger-strong); }
  td.num.caution { color: var(--warn); }
  td.pkg {
    font-family: var(--mono);
    font-size: 0.85rem;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
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
    border-bottom: 1px solid var(--border);
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
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-md);
    margin: 0.8rem 0;
    font-size: 0.9rem;
  }
  .warning code {
    background: var(--bg-inset);
    padding: 0.1rem 0.3rem;
    border-radius: var(--radius-xs);
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-family: var(--mono);
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
    border-radius: var(--radius-md);
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
    border-radius: var(--radius-md);
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
    color: var(--accent-ink);
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
  .action-message {
    margin-top: 0.4rem;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    word-break: break-word;
  }
  .link-button {
    background: none;
    border: none;
    padding: 0;
    font: inherit;
    color: var(--accent);
    text-decoration: underline;
    cursor: pointer;
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
  /* The meta line sits below this row rather than inside its left column, so
     the title and the actions are the only two things in it and can simply be
     centred on each other. Previously the actions top-aligned against the
     title-plus-meta block and read a few pixels low. */
  .device-title-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .device-header-actions {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-shrink: 0;
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
    border-radius: var(--radius-md);
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
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-md);
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
    border-radius: var(--radius-sm);
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

  .install-output {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0.7rem 1rem;
    margin: 0.8rem 0;
    font-family: var(--mono);
    font-size: 0.82rem;
    white-space: pre-wrap;
    word-break: break-word;
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 0.85em;
  }
</style>
