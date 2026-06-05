<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl, revealItemInDir } from "@tauri-apps/plugin-opener";
  import sideloadCatalog from "$lib/sideload-catalog.json";
  import appFilesCatalog from "$lib/app-files-catalog.json";
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
    TweaksState,
    SettingNamespace,
    DisplayScalePreset,
    CurrentDisplayScaling,
    OptimizeMode,
    OptimizePlan,
    OptimizePlanItem,
    OtherPackage,
    ScreenshotResult,
    FileEntry,
    Safety,
  } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";

  let serial = $derived(decodeURIComponent($page.params.serial ?? ""));

  type Tab = "overview" | "health" | "launcher" | "apps" | "optimize" | "tweaks" | "remote" | "files" | "snapshot" | "sideload";
  let activeTab = $state<Tab>("overview");

  let device = $state<Device | null>(null);
  let deviceErr = $state<string | null>(null);

  let report = $state<HealthReport | null>(null);
  let reportLoading = $state(false);
  let reportErr = $state<string | null>(null);
  let reportLastRefreshed = $state<Date | null>(null);
  let liveRefresh = $state(false);
  let liveRefreshTimer: ReturnType<typeof setInterval> | null = null;
  const LIVE_REFRESH_INTERVAL_MS = 3000;
  /// Cached safety classifications for the visible memory-table rows. Populated
  /// in batch whenever the health report refreshes so each row knows whether
  /// the Disable button should be hard-blocked.
  let safetyMap = $state<Record<string, Safety>>({});

  let renaming = $state(false);
  let renameValue = $state("");
  let renameBusy = $state(false);

  /// Rolling echo of what live typing has sent (display only).
  let remoteEcho = $state("");
  let remoteMessage = $state("");
  let remoteCaptureFocused = $state(false);
  // Keystrokes are sent strictly in order through one promise chain — a
  // backspace must never overtake the characters typed before it.
  let remoteQueue: Promise<void> = Promise.resolve();
  let remoteBuffer = "";
  let remoteFlushTimer: ReturnType<typeof setTimeout> | null = null;

  let filesPath = $state("/sdcard");
  let filesEntries = $state<FileEntry[] | null>(null);
  let filesLoading = $state(false);
  let filesErr = $state<string | null>(null);
  let filesBusy = $state<string | null>(null); // entry name currently being acted on
  let filesMessage = $state("");
  /// package → found backup-file paths (null until that app was searched).
  let appFilesResults = $state<Record<string, string[] | null>>({});
  let appFilesBusy = $state<string | null>(null);
  /// File name the "copy to another device" picker is open for, plus targets.
  let fileCopyName = $state<string | null>(null);
  let fileCopyTargets = $state<Device[]>([]);
  let crumbs = $derived(
    filesPath
      .split("/")
      .filter(Boolean)
      .map((seg, i, all) => ({ label: seg, path: "/" + all.slice(0, i + 1).join("/") })),
  );

  let screenshotBusy = $state(false);
  let screenshot = $state<ScreenshotResult | null>(null);

  let sendTextValue = $state("");
  let sendTextBusy = $state(false);
  let sendTextMessage = $state("");
  let trimBusy = $state(false);
  let trimMessage = $state("");

  let launchers = $state<LauncherStatus[]>([]);
  let currentLauncher = $state<CurrentLauncher | null>(null);
  let channelDisabled = $state<boolean | null>(null);
  let launcherLoading = $state(false);
  let launcherErr = $state<string | null>(null);
  let launcherActionBusy = $state<string | null>(null); // package id currently being acted on
  let launcherActionMessage = $state("");

  let apps = $state<AppEntry[]>([]);
  let appsLoading = $state(false);
  let appsErr = $state<string | null>(null);
  /// package → 'enabled' | 'disabled' | 'missing' — refreshed alongside the app list.
  let appStates = $state<Record<string, "enabled" | "disabled" | "missing">>({});
  let appSearch = $state("");
  let hideNotInstalled = $state(false);
  let showSystemOthers = $state(false);
  /// Installed packages not in the curated catalog (sideloaded apps like
  /// SmartTube + system internals). Loaded lazily on the Apps tab.
  let otherPackages = $state<OtherPackage[]>([]);
  let othersLoading = $state(false);

  function matchesSearch(name: string, pkg: string): boolean {
    const q = appSearch.trim().toLowerCase();
    if (!q) return true;
    return name.toLowerCase().includes(q) || pkg.toLowerCase().includes(q);
  }

  let visibleApps = $derived(
    apps.filter((a) => {
      if (hideNotInstalled && (appStates[a.package] ?? "enabled") === "missing") return false;
      return matchesSearch(a.name, a.package);
    }),
  );
  let visibleOthers = $derived(
    otherPackages.filter((o) => {
      if (!showSystemOthers && o.system) return false;
      return matchesSearch(o.package, o.package);
    }),
  );
  let appActionBusy = $state<string | null>(null);
  let appActionMessage = $state("");
  /// Package the "Copy to another device" panel is open for, plus targets.
  let clonePkg = $state<string | null>(null);
  let cloneTargets = $state<Device[]>([]);
  let cloneBusy = $state(false);

  let snapshots = $state<SnapshotFile[]>([]);
  let snapshotsErr = $state<string | null>(null);
  let saveBusy = $state(false);
  let saveResult = $state<string>("");
  let previewPath = $state<string | null>(null);
  let preview = $state<SnapshotApplyPlan | null>(null);
  let previewBusy = $state(false);
  let previewErr = $state<string | null>(null);

  /// Path of the APK currently installing (null when idle) — per-path so a
  /// multi-APK list only shows the spinner on the row actually installing.
  let sideloadBusy = $state<string | null>(null);
  let sideloadResult = $state<string>("");
  let sideloadHint = $state<string | null>(null);
  // Auto-discovered APK list — re-scanned whenever the user picks a folder
  // (or after a successful install in case files were added/removed).
  let discoveredApks = $state<import("$lib/types").DiscoveredApk[]>([]);
  let discoveredFolder = $state<string | null>(null);
  let discoveryBusy = $state(false);

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

  // Tweaks tab.
  let tweaks = $state<TweaksState | null>(null);
  let tweaksLoading = $state(false);
  let tweaksErr = $state<string | null>(null);
  let tweaksActionBusy = $state<string | null>(null);
  let tweaksActionMessage = $state<string>("");
  let displayScaleBusy = $state<DisplayScalePreset | null>(null);
  let displayScaleMessage = $state<string>("");
  let currentDisplayScaling = $state<CurrentDisplayScaling | null>(null);

  // Optimize / Restore wizard.
  let optimizeMode = $state<OptimizeMode>("optimize");
  let optimizePlan = $state<OptimizePlan | null>(null);
  let optimizePlanLoading = $state(false);
  let optimizePlanErr = $state<string | null>(null);
  /// Per-package action override. A package absent from the map follows the
  /// plan's recommended action; a present value is the user's explicit pick
  /// from the per-row dropdown (including "skip"). The execute loop dispatches
  /// on effectiveAction(), so disable/uninstall/enable/skip all just work.
  type RowAction = "disable" | "uninstall" | "enable" | "skip";
  let optimizeOverrides = $state<Record<string, RowAction>>({});
  let optimizeRunning = $state(false);
  let optimizeCurrent = $state<string | null>(null); // package currently being acted on
  let optimizeProgress = $state<Record<string, "pending" | "done" | "skipped" | "failed">>({});
  let optimizeFailureMessages = $state<Record<string, string>>({});
  let optimizeAbort = $state(false);
  let optimizeSummary = $state<string>("");
  let optimizePerfApplied = $state<boolean>(false);

  async function loadDevice() {
    deviceErr = null;
    try {
      device = await api.deviceProfile(serial);
    } catch (e) {
      deviceErr = String(e);
    }
  }

  async function sendTextToTv() {
    if (!sendTextValue) return;
    sendTextBusy = true;
    sendTextMessage = "";
    try {
      const r = await api.sendText(serial, sendTextValue);
      sendTextMessage = r.message;
      if (r.ok) sendTextValue = "";
    } catch (e) {
      sendTextMessage = String(e);
    } finally {
      sendTextBusy = false;
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
    reportLoading = true;
    reportErr = null;
    try {
      report = await api.healthReport(serial);
      reportLastRefreshed = new Date();
      // Resolve safety for every visible row in parallel — single ms each,
      // pure lookup against the engine const list. Cached so re-renders
      // don't re-query.
      const pkgs = report.top_memory.map((m) => m.package);
      const results = await Promise.all(
        pkgs.map((p) =>
          safetyMap[p]
            ? Promise.resolve(safetyMap[p])
            : api.safetyInfo(p).catch(() => ({ kind: "safe" } as Safety)),
        ),
      );
      const next = { ...safetyMap };
      results.forEach((s, i) => (next[pkgs[i]] = s));
      safetyMap = next;
    } catch (e) {
      reportErr = String(e);
    } finally {
      reportLoading = false;
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
    }
  }

  async function loadApps() {
    if (!device) return;
    appsLoading = true;
    appsErr = null;
    try {
      // Pull the app list AND fresh enabled/disabled state for those packages.
      // The package state lets us render per-row Disable/Enable/Uninstall
      // buttons in their correct state.
      const list = await api.appListForDevice(device.device_type);
      apps = list;
      appStates = await fetchAppStates(list.map((a) => a.package));
    } catch (e) {
      appsErr = String(e);
    } finally {
      appsLoading = false;
    }
    loadOtherPackages();
  }

  // Everything installed that isn't in the curated catalog — sideloaded apps
  // (SmartTube etc.) plus system internals. Loaded after the catalog so the
  // curated list paints first; failures here don't block the main list.
  async function loadOtherPackages() {
    othersLoading = true;
    try {
      otherPackages = await api.listOtherPackages(serial);
    } catch (e) {
      appActionMessage = `Could not list other packages: ${e}`;
    } finally {
      othersLoading = false;
    }
  }

  function patchOtherState(pkg: string, enabled: boolean | "removed") {
    if (enabled === "removed") {
      otherPackages = otherPackages.filter((o) => o.package !== pkg);
    } else {
      otherPackages = otherPackages.map((o) => (o.package === pkg ? { ...o, enabled } : o));
    }
  }

  async function disableOther(pkg: string) {
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.disablePackage(serial, pkg);
      appActionMessage = `${pkg}: ${r.message.trim() || (r.ok ? "disabled" : "failed")}`;
      if (r.ok) patchOtherState(pkg, false);
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  async function enableOther(pkg: string) {
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.enablePackage(serial, pkg);
      appActionMessage = `${pkg}: ${r.message.trim() || (r.ok ? "enabled" : "failed")}`;
      if (r.ok) patchOtherState(pkg, true);
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  async function uninstallOther(pkg: string) {
    if (!confirm(`Uninstall ${pkg}? Semi-reversible (Play Store reinstall or pm install-existing).`)) return;
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.uninstallPackage(serial, pkg);
      appActionMessage = `${pkg}: ${r.message.trim()}`;
      if (r.ok) patchOtherState(pkg, "removed");
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  // Real state per package — one batched backend call (pm list packages +
  // pm list packages -d in parallel) so we can show Enabled/Disabled/Missing.
  async function fetchAppStates(packages: string[]): Promise<Record<string, "enabled" | "disabled" | "missing">> {
    if (packages.length === 0) return {};
    try {
      return await api.packageStates(serial, packages);
    } catch {
      const out: Record<string, "enabled" | "disabled" | "missing"> = {};
      for (const p of packages) out[p] = "enabled";
      return out;
    }
  }

  /// Lookup a package in the loaded app catalog (if it's there) for risk-aware
  /// prompts when disabling from the memory table — where the user picked a
  /// process by RAM, not a curated bloat entry.
  function catalogEntry(pkg: string): AppEntry | undefined {
    return apps.find((a) => a.package === pkg);
  }

  function riskLabel(entry: AppEntry | undefined): string {
    if (!entry) return "UNKNOWN";
    return entry.risk.toUpperCase();
  }

  async function forceStopFromMemory(pkg: string) {
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.forceStop(serial, pkg);
      appActionMessage = r.ok
        ? `${pkg}: stopped — refresh the report to see freed RAM.`
        : `${pkg}: ${r.message.trim()}`;
    } catch (e) {
      appActionMessage = String(e);
    } finally {
      appActionBusy = null;
    }
  }

  async function safeDisableFromMemory(pkg: string, mb: number) {
    // Make sure the catalog is loaded so we can look up risk.
    if (apps.length === 0 && device) {
      try {
        apps = await api.appListForDevice(device.device_type);
      } catch {
        // Lookup is best-effort; carry on with the generic warning.
      }
    }

    // Authoritative safety check — backend will refuse never-disable
    // packages anyway, but we surface the reason inline so the user
    // doesn't get a confusing "Refusing to disable" message after a
    // pointless confirm.
    let safety: Safety;
    try {
      safety = await api.safetyInfo(pkg);
    } catch {
      safety = { kind: "safe" };
    }
    if (safety.kind === "never_disable") {
      alert(`Cannot disable ${pkg}.\n\n${safety.reason}`);
      return;
    }

    const entry = catalogEntry(pkg);
    let prompt = `Disable ${pkg} (${mb.toFixed(0)} MB)?\n\n`;
    if (safety.kind === "caution") {
      prompt += `⚠ ${safety.reason}\n\n`;
    }
    if (entry) {
      prompt += `Risk tier: ${entry.risk.toUpperCase()}\n`;
      prompt += `${entry.optimize_description}\n\n`;
      if (entry.risk === "high" || entry.risk === "advanced") {
        prompt += "⚠ HIGH RISK — this may break system features. Re-enable via Emergency Recovery if something goes wrong.\n\n";
      }
    } else if (safety.kind === "safe") {
      prompt += "ℹ This package is not in the curated bloat catalog — disabling is allowed but unverified. Re-enable via Emergency Recovery if something goes wrong.\n\n";
    }
    prompt += "Proceed?";
    if (!confirm(prompt)) return;
    await disableApp(pkg);
  }

  async function disableApp(pkg: string) {
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.disablePackage(serial, pkg);
      appActionMessage = `${pkg}: ${r.message.trim()}`;
      if (r.ok) appStates[pkg] = "disabled";
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  async function enableApp(pkg: string) {
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.enablePackage(serial, pkg);
      appActionMessage = `${pkg}: ${r.message.trim()}`;
      if (r.ok) appStates[pkg] = "enabled";
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  async function uninstallApp(pkg: string) {
    if (!confirm(`Uninstall ${pkg}? This is semi-reversible (Play Store reinstall or pm install-existing).`)) return;
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.uninstallPackage(serial, pkg);
      appActionMessage = `${pkg}: ${r.message.trim()}`;
      if (r.ok) appStates[pkg] = "missing";
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  // Best-effort system-app re-install via `cmd package install-existing` —
  // works only for apps still present on /system. For third-party uninstalls
  // we route through the Play Store instead.
  async function reinstallApp(pkg: string) {
    appActionBusy = pkg;
    appActionMessage = "";
    try {
      const r = await api.reinstallExisting(serial, pkg);
      appActionMessage = `${pkg}: ${r.message.trim()}`;
      if (r.ok) appStates[pkg] = "enabled";
    } catch (e) {
      appActionMessage = `${pkg}: ${e}`;
    } finally {
      appActionBusy = null;
    }
  }

  type Recommendation =
    | { kind: "done"; label: string }
    | { kind: "act"; label: string; action: "disable" | "uninstall" }
    | { kind: "restore"; label: string }
    | { kind: "keep" };

  /// What v1's Optimize wizard would suggest for this row, given its current
  /// on-device state. `done` = already where the recommendation wants it.
  /// `act` = a recommended write action that's clickable. `restore` = the app
  /// is gone and v1 would have brought it back. `keep` = no recommendation.
  function recommendation(a: AppEntry, state: "enabled" | "disabled" | "missing"): Recommendation {
    if (state === "missing") {
      if (a.default_restore) return { kind: "restore", label: "Reinstall" };
      // Uninstalled and Optimize would also uninstall it — already there.
      if (a.default_optimize && a.method === "uninstall") return { kind: "done", label: "Already uninstalled" };
      return { kind: "keep" };
    }
    if (!a.default_optimize) return { kind: "keep" };
    if (a.method === "disable") {
      return state === "disabled"
        ? { kind: "done", label: "Already disabled" }
        : { kind: "act", label: "Disable", action: "disable" };
    }
    // method === "uninstall"
    return { kind: "act", label: "Uninstall", action: "uninstall" };
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
    const prevDefault = currentLauncher?.package ?? null;
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    try {
      const r = await api.enablePackage(serial, pkg);
      if (!r.ok) {
        launcherActionMessage = `${pkg}: ${r.message.trim() || "failed"}`;
        return;
      }
      await loadLauncher();
      // Android clears its preferred-HOME record when a launcher package's
      // state changes, so a freshly re-enabled launcher (especially stock)
      // can steal the active-launcher slot. Enabling ≠ switching — put the
      // user's previous default back.
      if (prevDefault && prevDefault !== pkg && currentLauncher?.package === pkg) {
        const back = await api.setDefaultLauncher(serial, prevDefault);
        await loadLauncher();
        launcherActionMessage = back.ok
          ? `Enabled ${pkg}. Android made it the active launcher, so ${prevDefault} was re-set as your default.`
          : back.stock_takeover_available
            ? `Enabled ${pkg} — it also took over HOME, and this build can't hand HOME back without disabling it again. Use "Set as default" on ${prevDefault} if you want it back.`
            : `Enabled ${pkg} — Android made it the active launcher, and re-setting ${prevDefault} failed` +
              `${back.last_error ? `: ${back.last_error}` : ""}. Use "Set as default" on your preferred launcher.`;
      } else {
        launcherActionMessage = `${pkg}: enabled`;
      }
    } catch (e) {
      launcherActionMessage = String(e);
    } finally {
      launcherActionBusy = null;
    }
  }

  async function disableLauncher(pkg: string) {
    const advice = launchers.find((l) => l.entry.package === pkg)?.other
      ? " Tip: save a snapshot first (Snapshot tab) so you have a record of today's state."
      : "";
    if (!confirm(`Disable ${pkg}? You'll lose access to it as a HOME app until you re-enable.${advice}`)) return;
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    try {
      const r = await api.disableLauncher(serial, pkg);
      launcherActionMessage = `${pkg}: ${r.message.trim() || (r.ok ? "disabled" : "failed")}`;
      if (r.ok) await loadLauncher();
    } catch (e) {
      launcherActionMessage = String(e);
    } finally {
      launcherActionBusy = null;
    }
  }

  async function setDefaultLauncher(pkg: string) {
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    try {
      let r = await api.setDefaultLauncher(serial, pkg);
      if (!r.ok && r.stock_takeover_available) {
        // The only working method on this build disables the stock launcher.
        // Never do that silently — ask, then retry with the opt-in flag.
        const proceed = confirm(
          `${r.last_error ?? "This device ignores the standard launcher-switch commands."}\n\nDisable the stock launcher and switch to ${pkg}? You can re-enable it from this list at any time.`,
        );
        if (proceed) r = await api.setDefaultLauncher(serial, pkg, true);
      }
      if (r.ok) {
        launcherActionMessage =
          r.strategy === "disable_stock_takeover"
            ? `Set ${pkg} as default — the stock launcher was disabled to hand HOME over (re-enable it from the list any time).`
            : `Set ${pkg} as default launcher (via ${r.strategy ?? "ok"}).`;
        await loadLauncher();
      } else {
        // Backend messages are full sentences (including the "device accepted
        // the change — press Home" case) — render them verbatim rather than
        // prefixing "Failed:", which once produced "Failed: Success".
        launcherActionMessage =
          r.last_error ?? "Could not set default launcher. Try disabling other launchers first.";
      }
    } catch (e) {
      launcherActionMessage = String(e);
    } finally {
      launcherActionBusy = null;
    }
  }

  async function pickAndInstallApk() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Android Packages", extensions: ["apk"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    // Remember the folder the user picked from so we can show the
    // surrounding APKs as a quick-pick list.
    const lastSep = Math.max(selected.lastIndexOf("/"), selected.lastIndexOf("\\"));
    if (lastSep > 0) {
      const folder = selected.slice(0, lastSep);
      localStorage.setItem("shieldopt.lastApkFolder", folder);
      await scanApkFolder(folder);
    }
    await installApkPath(selected);
  }

  async function pickApkFolder() {
    const picked = await openDialog({ multiple: false, directory: true });
    if (!picked || Array.isArray(picked)) return;
    localStorage.setItem("shieldopt.lastApkFolder", picked);
    await scanApkFolder(picked);
  }

  async function scanApkFolder(folder: string) {
    discoveryBusy = true;
    try {
      discoveredApks = await api.listApksInFolder(folder);
      discoveredFolder = folder;
    } catch (e) {
      sideloadResult = `Scan failed: ${e}`;
    } finally {
      discoveryBusy = false;
    }
  }

  async function installApkPath(path: string) {
    sideloadBusy = path;
    sideloadResult = `Installing ${path.split(/[\\/]/).pop()}…`;
    sideloadHint = null;
    try {
      const r = await api.installApk(serial, path, true);
      sideloadResult = r.message;
      sideloadHint = r.hint;
    } catch (e) {
      sideloadResult = String(e);
    } finally {
      sideloadBusy = null;
    }
  }

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  async function loadSnapshots() {
    snapshotsErr = null;
    try {
      snapshots = await api.listSnapshots();
    } catch (e) {
      snapshotsErr = String(e);
    }
  }

  async function saveSnapshot() {
    if (!device) return;
    saveBusy = true;
    saveResult = "";
    try {
      const result = await api.saveSnapshot(serial, device.name);
      saveResult = `Saved ${result.filename} — ${result.disabled_count} disabled packages captured.`;
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

  async function openDownloadPage(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      sideloadResult = `Open link failed: ${e}`;
    }
  }

  function remoteEnqueue(work: () => Promise<void>) {
    remoteQueue = remoteQueue.then(work).catch((e) => {
      remoteMessage = String(e);
    });
  }

  function remoteFlushBuffer() {
    if (remoteFlushTimer) {
      clearTimeout(remoteFlushTimer);
      remoteFlushTimer = null;
    }
    if (!remoteBuffer) return;
    const chunk = remoteBuffer;
    remoteBuffer = "";
    remoteEnqueue(async () => {
      const r = await api.sendText(serial, chunk);
      if (!r.ok) remoteMessage = r.message;
    });
  }

  function sendRemoteKey(key: string) {
    remoteFlushBuffer();
    remoteEnqueue(async () => {
      const r = await api.sendKey(serial, key);
      remoteMessage = r.ok ? "" : r.message;
    });
  }

  function remoteKeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "Backspace") {
      e.preventDefault();
      remoteEcho = remoteEcho.slice(0, -1);
      sendRemoteKey("delete");
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      remoteEcho = "";
      sendRemoteKey("enter");
      return;
    }
    if (e.key.length === 1 && e.key >= " " && e.key <= "~") {
      e.preventDefault();
      remoteBuffer += e.key;
      remoteEcho = (remoteEcho + e.key).slice(-60);
      // Batch rapid typing into one `input text` per pause — each adb call
      // is a ~100-300 ms round-trip, so per-character would lag behind fast
      // typists forever.
      if (remoteFlushTimer) clearTimeout(remoteFlushTimer);
      remoteFlushTimer = setTimeout(remoteFlushBuffer, 250);
    }
  }

  async function loadFiles(path: string) {
    filesLoading = true;
    filesErr = null;
    filesMessage = "";
    try {
      filesEntries = await api.listDir(serial, path);
      filesPath = path;
    } catch (e) {
      filesErr = String(e);
    } finally {
      filesLoading = false;
    }
  }

  async function startFileCopy(name: string) {
    filesMessage = "";
    try {
      const all = await api.listDevices();
      fileCopyTargets = all.filter((d) => d.status === "device" && d.serial !== serial);
    } catch (e) {
      filesMessage = String(e);
      return;
    }
    if (fileCopyTargets.length === 0) {
      filesMessage = "No other connected device to copy to — connect the target device first.";
      return;
    }
    fileCopyName = name;
  }

  async function copyFileTo(target: Device) {
    if (!fileCopyName) return;
    const name = fileCopyName;
    filesBusy = name;
    // Land it in the same path on the target so a /sdcard/Download file
    // arrives in /sdcard/Download there too.
    filesMessage = `Copying ${name} to ${target.name}…`;
    try {
      const r = await api.copyFileToDevice(serial, `${filesPath}/${name}`, target.serial, filesPath);
      filesMessage = r.message;
      if (r.ok) fileCopyName = null;
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function downloadFile(name: string) {
    const folder = await openDialog({ directory: true, title: "Choose a download folder" });
    if (!folder) return;
    filesBusy = name;
    filesMessage = "";
    try {
      const r = await api.pullFile(serial, `${filesPath}/${name}`, folder as string);
      filesMessage = r.message;
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function uploadToCurrentDir() {
    const file = await openDialog({ title: "Choose a file to upload" });
    if (!file) return;
    filesBusy = "__upload__";
    filesMessage = "";
    try {
      const r = await api.pushFile(serial, file as string, filesPath);
      filesMessage = r.message;
      if (r.ok) await loadFiles(filesPath);
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function deleteEntry(entry: FileEntry) {
    const what = entry.is_dir ? "folder AND EVERYTHING IN IT" : "file";
    if (!confirm(`Delete ${what} "${entry.name}" from the device? This cannot be undone.`)) return;
    filesBusy = entry.name;
    filesMessage = "";
    try {
      const r = await api.deletePath(serial, `${filesPath}/${entry.name}`);
      filesMessage = r.message;
      if (r.ok) await loadFiles(filesPath);
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function findAppFiles(entry: (typeof appFilesCatalog)[number]) {
    appFilesBusy = entry.package;
    filesMessage = "";
    try {
      appFilesResults[entry.package] = await api.findFiles(serial, entry.search_dirs, entry.pattern);
    } catch (e) {
      filesMessage = String(e);
    } finally {
      appFilesBusy = null;
    }
  }

  async function downloadFoundFile(path: string) {
    const folder = await openDialog({ directory: true, title: "Choose a folder for the backup" });
    if (!folder) return;
    appFilesBusy = path;
    filesMessage = "";
    try {
      const r = await api.pullFile(serial, path, folder as string);
      filesMessage = r.message;
    } catch (e) {
      filesMessage = String(e);
    } finally {
      appFilesBusy = null;
    }
  }

  function goToFolder(path: string) {
    const dir = path.slice(0, path.lastIndexOf("/")) || "/sdcard";
    loadFiles(dir);
  }

  function formatSize(bytes: number): string {
    if (bytes >= 1 << 30) return `${(bytes / (1 << 30)).toFixed(2)} GB`;
    if (bytes >= 1 << 20) return `${(bytes / (1 << 20)).toFixed(1)} MB`;
    if (bytes >= 1 << 10) return `${(bytes / (1 << 10)).toFixed(0)} KB`;
    return `${bytes} B`;
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

  async function loadTweaks() {
    tweaksLoading = true;
    tweaksErr = null;
    try {
      const [t, s] = await Promise.all([
        api.getTweaks(serial),
        api.getDisplayScaling(serial).catch(() => null),
      ]);
      tweaks = t;
      currentDisplayScaling = s;
    } catch (e) {
      tweaksErr = String(e);
    } finally {
      tweaksLoading = false;
    }
  }

  // Write a setting, then refresh the on-screen state for that key by
  // re-pulling all tweaks. Cheap (one batched shell call).
  async function writeTweak(
    namespace: SettingNamespace,
    key: string,
    value: string,
    busyId: string,
  ) {
    tweaksActionBusy = busyId;
    tweaksActionMessage = "";
    try {
      const r = await api.writeSetting(serial, namespace, key, value);
      tweaksActionMessage = `${key} → ${value || "(default)"}: ${r.message.trim()}`;
      await loadTweaks();
    } catch (e) {
      tweaksActionMessage = `${key}: ${e}`;
    } finally {
      tweaksActionBusy = null;
    }
  }

  // Animation triple is one logical control — write all three keys in one go.
  async function setAnimationScale(scale: string) {
    tweaksActionBusy = "animations";
    tweaksActionMessage = "";
    try {
      const keys = ["window_animation_scale", "transition_animation_scale", "animator_duration_scale"];
      const results = await Promise.all(
        keys.map((k) => api.writeSetting(serial, "global", k, scale)),
      );
      const failed = results.filter((r) => !r.ok);
      tweaksActionMessage =
        failed.length === 0
          ? `Animations → ${scale || "default"}`
          : `Animations partially failed (${failed.length}/3): ${failed.map((r) => r.message).join("; ")}`;
      await loadTweaks();
    } catch (e) {
      tweaksActionMessage = `Animations: ${e}`;
    } finally {
      tweaksActionBusy = null;
    }
  }

  async function loadOptimizePlan(mode: OptimizeMode) {
    optimizeMode = mode;
    optimizePlanLoading = true;
    optimizePlanErr = null;
    optimizePlan = null;
    optimizeOverrides = {};
    optimizeProgress = {};
    optimizeFailureMessages = {};
    optimizeSummary = "";
    optimizePerfApplied = false;
    try {
      optimizePlan = await api.prepareOptimize(serial, mode);
    } catch (e) {
      optimizePlanErr = String(e);
    } finally {
      optimizePlanLoading = false;
    }
  }

  /// The natural action the engine computed for an actionable row (disable /
  /// uninstall in optimize mode, enable in restore mode), or null for rows the
  /// backend marked skip (not installed / already in target state) — those
  /// aren't actionable and get no dropdown.
  function naturalAction(item: OptimizePlanItem): RowAction | null {
    return item.action.kind === "skip" ? null : item.action.kind;
  }

  /// What the dropdown defaults to. This mirrors v1's per-app defaults: only
  /// apps flagged default_optimize / default_restore are pre-selected for
  /// action; everything else defaults to Skip so the wizard never removes a
  /// streaming app (or anything not on the curated default list) unless the
  /// user explicitly chooses to. Returns null for non-actionable rows.
  function defaultAction(item: OptimizePlanItem): RowAction | null {
    const natural = naturalAction(item);
    if (natural === null) return null;
    const isDefault =
      optimizeMode === "optimize" ? item.entry.default_optimize : item.entry.default_restore;
    return isDefault ? natural : "skip";
  }

  /// The action that will actually run: the user's dropdown pick if they made
  /// one, otherwise the per-app default (or skip for non-actionable rows).
  function effectiveAction(item: OptimizePlanItem): RowAction {
    return optimizeOverrides[item.entry.package] ?? defaultAction(item) ?? "skip";
  }

  /// Dropdown choices for a row, in mode-appropriate order. Restore only ever
  /// produces enable rows, so its menu is Enable / Skip; optimize rows can be
  /// downgraded (uninstall→disable) or upgraded (disable→uninstall).
  function actionOptions(item: OptimizePlanItem): RowAction[] {
    return naturalAction(item) === "enable"
      ? ["enable", "skip"]
      : ["disable", "uninstall", "skip"];
  }

  function actionLabel(item: OptimizePlanItem, action: RowAction): string {
    const base = { disable: "Disable", uninstall: "Uninstall", enable: "Enable", skip: "Skip" }[action];
    return action === defaultAction(item) ? `${base} (recommended)` : base;
  }

  function setOptimizeAction(pkg: string, action: RowAction) {
    optimizeOverrides[pkg] = action;
  }

  async function executeOptimize() {
    if (!optimizePlan) return;
    const total = optimizePlan.items.filter((i) => effectiveAction(i) !== "skip").length;
    if (total === 0) {
      optimizeSummary = "Nothing to do — every item is in its target state.";
      return;
    }
    const label = optimizeMode === "optimize" ? "Optimize" : "Restore";
    if (!confirm(`Run ${label} on ${total} package(s)? Disabled packages can be re-enabled via Emergency Recovery.`)) return;

    optimizeRunning = true;
    optimizeAbort = false;
    optimizeProgress = {};
    optimizeFailureMessages = {};

    let done = 0, skipped = 0, failed = 0;
    for (const item of optimizePlan.items) {
      if (optimizeAbort) break;
      const pkg = item.entry.package;
      const action = effectiveAction(item);
      if (action === "skip") {
        optimizeProgress[pkg] = "skipped";
        skipped++;
        continue;
      }
      optimizeCurrent = pkg;
      optimizeProgress[pkg] = "pending";
      try {
        let r: { ok: boolean; message: string };
        if (action === "disable") r = await api.disablePackage(serial, pkg);
        else if (action === "uninstall") r = await api.uninstallPackage(serial, pkg);
        else r = await api.enablePackage(serial, pkg);
        if (r.ok) {
          optimizeProgress[pkg] = "done";
          done++;
        } else {
          optimizeProgress[pkg] = "failed";
          optimizeFailureMessages[pkg] = r.message;
          failed++;
        }
      } catch (e) {
        optimizeProgress[pkg] = "failed";
        optimizeFailureMessages[pkg] = String(e);
        failed++;
      }
    }
    optimizeCurrent = null;
    optimizeRunning = false;
    optimizeSummary = optimizeAbort
      ? `Aborted. ${done} applied, ${failed} failed, ${skipped} skipped.`
      : `${label} complete: ${done} applied, ${failed} failed, ${skipped} skipped.`;
  }

  async function applyPerformanceSettings() {
    if (!optimizePlan) return;
    const profile = optimizeMode === "optimize" ? "optimized" : "default";
    try {
      const r = await api.applyPerformanceSettings(serial, profile);
      optimizePerfApplied = r.ok;
      optimizeSummary = optimizeSummary
        ? `${optimizeSummary} Performance: ${r.message.trim()}.`
        : `Performance: ${r.message.trim()}.`;
    } catch (e) {
      optimizeSummary = optimizeSummary
        ? `${optimizeSummary} Performance failed: ${e}.`
        : `Performance failed: ${e}.`;
    }
  }

  function skipReasonLabel(item: OptimizePlanItem): string | null {
    if (item.action.kind !== "skip") return null;
    switch (item.action.reason) {
      case "not_installed": return "Not installed";
      case "already_disabled": return "Already disabled";
      case "already_enabled": return "Already enabled";
      case "user_choice": return "Skipped";
    }
  }

  async function applyDisplayScaling(preset: DisplayScalePreset) {
    const label = preset === "uhd_4k" ? "4K (3839x2160, density 640)"
      : preset === "fhd_1080p" ? "1080p (1920x1080, density 320)"
      : "device defaults";
    if (!confirm(`Apply display scaling: ${label}? The screen will reflow.`)) return;
    displayScaleBusy = preset;
    displayScaleMessage = "";
    try {
      const r = await api.setDisplayScaling(serial, preset);
      displayScaleMessage = r.message.trim() || (r.ok ? "ok" : "no output");
      // Refresh the displayed current values.
      currentDisplayScaling = await api.getDisplayScaling(serial).catch(() => currentDisplayScaling);
    } catch (e) {
      displayScaleMessage = String(e);
    } finally {
      displayScaleBusy = null;
    }
  }

  // Lazy-load each tab the first time it's opened. Sideload doesn't need
  // any prefetch — the file picker fires on user action.
  $effect(() => {
    if (activeTab === "health") {
      if (report === null && !reportLoading && !reportErr) loadHealth();
      // Preload catalog so the memory table can show risk tiers.
      if (apps.length === 0 && !appsLoading && !appsErr) loadApps();
    }
    if (activeTab === "launcher" && launchers.length === 0 && !launcherLoading && !launcherErr) loadLauncher();
    if (activeTab === "apps" && apps.length === 0 && !appsLoading && !appsErr) loadApps();
    if (activeTab === "tweaks" && tweaks === null && !tweaksLoading && !tweaksErr) loadTweaks();
    if (activeTab === "files" && filesEntries === null && !filesLoading && !filesErr) loadFiles(filesPath);
    if (activeTab === "snapshot" && snapshots.length === 0) loadSnapshots();
    if (activeTab === "sideload" && discoveredApks.length === 0 && !discoveryBusy) {
      const last = localStorage.getItem("shieldopt.lastApkFolder");
      if (last) scanApkFolder(last);
    }
  });

  /// Wipe all per-device state. Used if the route's serial changes under a
  /// live component (today the only way off this page is "← Back to devices",
  /// so this is defensive — but it guarantees no device's data or in-flight
  /// timer can leak onto another if a device→device link is ever added).
  function resetDeviceState() {
    if (liveRefreshTimer) {
      clearInterval(liveRefreshTimer);
      liveRefreshTimer = null;
    }
    liveRefresh = false;
    activeTab = "overview";
    device = null; deviceErr = null;
    report = null; reportErr = null; reportLastRefreshed = null; safetyMap = {};
    launchers = []; currentLauncher = null; channelDisabled = null;
    launcherErr = null; launcherActionMessage = "";
    apps = []; appsErr = null; appStates = {}; appActionMessage = "";
    otherPackages = []; appSearch = ""; hideNotInstalled = false; showSystemOthers = false;
    clonePkg = null; cloneTargets = [];
    snapshots = []; snapshotsErr = null; preview = null; previewPath = null; previewErr = null; saveResult = "";
    sideloadResult = ""; sideloadHint = null; discoveredApks = []; discoveredFolder = null;
    headerActionMsg = ""; recoveryResult = null; recoveryErr = null; screenshot = null;
    renaming = false; renameValue = "";
    remoteEcho = ""; remoteMessage = ""; remoteBuffer = "";
    sendTextValue = ""; sendTextMessage = ""; trimMessage = "";
    filesPath = "/sdcard"; filesEntries = null; filesErr = null; filesMessage = "";
    appFilesResults = {}; fileCopyName = null; fileCopyTargets = [];
    applyResult = null; applyErr = null;
    tweaks = null; tweaksErr = null; tweaksActionMessage = ""; currentDisplayScaling = null; displayScaleMessage = "";
    optimizePlan = null; optimizePlanErr = null; optimizeOverrides = {};
    optimizeProgress = {}; optimizeSummary = ""; optimizeFailureMessages = {}; optimizePerfApplied = false;
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

      <div class="send-text-section">
        <h3>Send text to TV</h3>
        <p class="muted small">
          Types into whatever field has focus on the TV — put the cursor in the
          Wi-Fi password or search box first, then send from a real keyboard.
        </p>
        <div class="send-text-row">
          <input
            placeholder="Text to type on the TV"
            bind:value={sendTextValue}
            onkeydown={(e) => e.key === "Enter" && sendTextToTv()}
          />
          <button
            class="primary"
            onclick={sendTextToTv}
            disabled={sendTextBusy || !sendTextValue}
            title="adb shell input text — printable ASCII only"
          >
            {sendTextBusy ? "Sending…" : "Send"}
          </button>
        </div>
        {#if sendTextMessage}
          <p class="muted small mono">{sendTextMessage}</p>
        {/if}
      </div>

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
              <tr><th>RAM</th><th>Package</th><th class="center">Risk</th><th></th></tr>
            </thead>
            <tbody>
              {#each report.top_memory as m}
                {@const entry = catalogEntry(m.package)}
                {@const safety = safetyMap[m.package] ?? { kind: "safe" }}
                {@const blocked = safety.kind === "never_disable"}
                <tr class:dim={blocked}>
                  <td
                    class="num"
                    class:warn={m.mb >= 200}
                    class:caution={m.mb >= 100 && m.mb < 200}
                  >
                    {m.mb.toFixed(1)} MB
                  </td>
                  <td class="pkg">{m.package}</td>
                  <td
                    class={`center risk ${entry
                      ? "risk-" + entry.risk
                      : blocked
                        ? "risk-blocked"
                        : safety.kind === "caution"
                          ? "risk-medium"
                          : "risk-unknown"}`}
                    title={safety.kind !== "safe" ? safety.reason : ""}
                  >
                    {#if blocked}
                      SYSTEM
                    {:else if safety.kind === "caution"}
                      CAUTION
                    {:else}
                      {riskLabel(entry)}
                    {/if}
                  </td>
                  <td class="row-actions">
                    <button
                      class="small-action subtle"
                      onclick={() => forceStopFromMemory(m.package)}
                      disabled={appActionBusy === m.package}
                      title="am force-stop {m.package} — frees its RAM now; the app restarts on next launch"
                    >
                      {appActionBusy === m.package ? "…" : "Force stop"}
                    </button>
                    {#if blocked}
                      <span class="muted small" title={safety.reason}>Protected</span>
                    {:else}
                      <button
                        class="small-action"
                        class:danger={!entry || (entry && (entry.risk === "high" || entry.risk === "advanced")) || safety.kind === "caution"}
                        onclick={() => safeDisableFromMemory(m.package, m.mb)}
                        disabled={appActionBusy === m.package}
                        title="pm disable-user --user 0 {m.package}"
                      >
                        {appActionBusy === m.package ? "…" : "Disable"}
                      </button>
                    {/if}
                  </td>
                </tr>
              {/each}
            </tbody>
          </table>
          {#if appActionMessage}
            <p class="muted small mono">{appActionMessage}</p>
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
                        disabled={launcherActionBusy !== null || !l.enabled}
                        title="Run pm enable + role API + set-home-activity strategies"
                      >
                        {busy ? "Setting…" : "Set as default"}
                      </button>
                    {/if}
                    {#if !isCurrent && l.enabled}
                      <button
                        class="small-action subtle"
                        onclick={() => disableLauncher(l.entry.package)}
                        disabled={launcherActionBusy !== null}
                        title="pm disable-user --user 0 {l.entry.package}"
                      >Disable</button>
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
      {:else if appsLoading && apps.length === 0}
        <div class="muted">Loading…</div>
      {:else}
        <p class="muted small legend">
          <strong>State</strong> is what the device reports right now.
          <strong>Recommended</strong> is what v1's Optimize wizard would pick for you —
          click to apply, or leave it. <strong>Tools</strong> has the Play Store link
          plus APK backup and copy-to-another-device.
        </p>
        {#if appActionMessage}
          <p class="muted small mono action-message">{appActionMessage}</p>
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
              <th class="center">Risk</th>
              <th>Recommended</th>
              <th class="center">Tools</th>
            </tr>
          </thead>
          <tbody>
            {#each visibleApps as a (a.package)}
              {@const state = appStates[a.package] ?? "enabled"}
              {@const rec = recommendation(a, state)}
              <tr>
                <td class="app-cell">
                  <div class="app-name-row">{a.name}</div>
                  {#if a.optimize_description}
                    <div class="muted small app-desc">{a.optimize_description}</div>
                  {/if}
                  <div class="muted small mono pkg-id">{a.package}</div>
                </td>
                <td class="center">
                  <span class={`state-badge state-${state}`}>
                    {state === "enabled" ? "Enabled" : state === "disabled" ? "Disabled" : "Missing"}
                  </span>
                </td>
                <td class={`risk center risk-${a.risk}`}>{a.risk.toUpperCase()}</td>
                <td class="rec-cell">
                  {#if rec.kind === "act"}
                    <button
                      class="small-action recommended"
                      class:danger={rec.action === "uninstall"}
                      onclick={() => applyRecommendation(a.package, rec.action)}
                      disabled={appActionBusy === a.package}
                      title={a.optimize_description}
                    >
                      {appActionBusy === a.package ? "…" : rec.label}
                    </button>
                  {:else if rec.kind === "restore"}
                    <button
                      class="small-action recommended"
                      onclick={() => reinstallApp(a.package)}
                      disabled={appActionBusy === a.package}
                      title="cmd package install-existing — works for system apps still on /system"
                    >
                      {appActionBusy === a.package ? "…" : rec.label}
                    </button>
                  {:else if rec.kind === "done"}
                    <span class="muted small done">✓ {rec.label}</span>
                  {:else}
                    <span class="muted small">Keep</span>
                  {/if}

                  {#if state === "enabled" && rec.kind !== "act"}
                    <button
                      class="small-action subtle"
                      onclick={() => disableApp(a.package)}
                      disabled={appActionBusy === a.package}
                      title="pm disable-user --user 0"
                    >Disable</button>
                  {/if}
                  {#if state === "disabled"}
                    <button
                      class="small-action subtle"
                      onclick={() => enableApp(a.package)}
                      disabled={appActionBusy === a.package}
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
              </tr>
            {/each}
            {#if visibleApps.length === 0}
              <tr><td colspan="5" class="muted">No curated apps match your filters.</td></tr>
            {/if}
          </tbody>
        </table>

        <div class="other-apps">
          <h3>Everything else {othersLoading ? "" : `(${visibleOthers.length})`}</h3>
          <p class="muted small">
            Installed apps that aren't in the curated list — sideloaded apps (SmartTube etc.)
            get the same <strong>Backup</strong> and <strong>Copy to…</strong> tools.
            {showSystemOthers ? "Showing system packages too — disable these only if you know what they are." : "System packages are hidden; tick \"Show system packages\" to include them."}
          </p>
          {#if othersLoading}
            <div class="muted">Loading installed packages…</div>
          {:else if visibleOthers.length === 0}
            <p class="muted">{otherPackages.length === 0 ? "No non-catalog packages found." : "Nothing matches your filters."}</p>
          {:else}
            <table class="app-table">
              <thead>
                <tr><th>Package</th><th class="center">Type</th><th class="center">State</th><th>Actions</th><th class="center">Tools</th></tr>
              </thead>
              <tbody>
                {#each visibleOthers as o (o.package)}
                  <tr>
                    <td class="app-cell"><div class="mono small">{o.package}</div></td>
                    <td class="center type-cell">
                      <span class={`tag ${o.system ? "missing" : "installed"}`}>{o.system ? "SYSTEM" : "3RD-PARTY"}</span>
                    </td>
                    <td class="center">
                      <span class={`state-badge state-${o.enabled ? "enabled" : "disabled"}`}>
                        {o.enabled ? "Enabled" : "Disabled"}
                      </span>
                    </td>
                    <td class="rec-cell">
                      {#if o.enabled}
                        <button class="small-action subtle" onclick={() => disableOther(o.package)} disabled={appActionBusy === o.package} title="pm disable-user --user 0">Disable</button>
                        <button class="small-action subtle danger" onclick={() => uninstallOther(o.package)} disabled={appActionBusy === o.package} title="pm uninstall --user 0">Uninstall</button>
                      {:else}
                        <button class="small-action subtle" onclick={() => enableOther(o.package)} disabled={appActionBusy === o.package} title="pm enable">Enable</button>
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
  {:else if activeTab === "optimize"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-optimize" aria-labelledby="tab-optimize">
      <div class="card-header">
        <h2>Optimize / Restore Wizard</h2>
        <div class="header-actions">
          <button
            class:primary={optimizeMode === "optimize"}
            onclick={() => loadOptimizePlan("optimize")}
            disabled={optimizePlanLoading || optimizeRunning}
          >Optimize</button>
          <button
            class:primary={optimizeMode === "restore"}
            onclick={() => loadOptimizePlan("restore")}
            disabled={optimizePlanLoading || optimizeRunning}
          >Restore</button>
        </div>
      </div>
      <p class="muted small">
        {optimizeMode === "optimize"
          ? "Disable or uninstall bloat per the device's app catalog. Each row defaults to the recommended action — change it (Disable / Uninstall / Skip) per row, then Run."
          : "Re-enable everything that's currently disabled per the device's app catalog. Set any row to Skip to leave it, then Run. Restore is reversible by running Optimize again."}
      </p>

      {#if optimizePlanErr}
        <div class="error">{optimizePlanErr}</div>
      {/if}

      {#if optimizePlanLoading}
        <p class="muted">Querying device…</p>
      {:else if !optimizePlan}
        <p class="muted">Pick Optimize or Restore to load the plan.</p>
      {:else}
        {@const actionable = optimizePlan.items.filter((i) => effectiveAction(i) !== "skip").length}
        {@const totalRunning = optimizePlan.items.reduce((acc, i) => acc + (i.memory_mb ?? 0), 0)}
        <div class="plan-summary">
          <strong>{actionable}</strong> of {optimizePlan.items.length} items will be acted on.
          {#if totalRunning > 0}
            <span class="muted">≈ {totalRunning.toFixed(0)} MB of RAM in play.</span>
          {/if}
        </div>
        <div class="apply-row">
          <button
            class="primary"
            onclick={executeOptimize}
            disabled={optimizeRunning || actionable === 0}
          >
            {optimizeRunning ? `Running… (${optimizeCurrent ?? ""})` : `Run ${optimizeMode === "optimize" ? "Optimize" : "Restore"}`}
          </button>
          {#if optimizeRunning}
            <button onclick={() => (optimizeAbort = true)}>Abort</button>
          {/if}
          {#if optimizeSummary && !optimizeRunning}
            <button
              onclick={applyPerformanceSettings}
              disabled={optimizePerfApplied}
              title={optimizeMode === "optimize" ? "Set animation scales to 0.5×" : "Reset animation scales to 1×"}
            >
              {optimizePerfApplied ? "Performance applied" : (optimizeMode === "optimize" ? "Apply 0.5× animations" : "Reset animations to 1×")}
            </button>
          {/if}
        </div>
        {#if optimizeSummary}
          <p class="muted small mono action-message">{optimizeSummary}</p>
        {/if}

        <table class="optimize-table">
          <thead>
            <tr>
              <th>App</th>
              <th>RAM</th>
              <th>Risk</th>
              <th>Action</th>
              <th>Result</th>
            </tr>
          </thead>
          <tbody>
            {#each optimizePlan.items as item (item.entry.package)}
              {@const skip = skipReasonLabel(item)}
              {@const progress = optimizeProgress[item.entry.package]}
              {@const eff = effectiveAction(item)}
              <tr class:dim={eff === "skip"} class:acting={!skip && eff !== "skip"}>
                <td>
                  <div class="app-name">
                    {item.entry.name}
                    {#if item.entry.default_optimize}
                      <span class="tag installed">DEFAULT</span>
                    {/if}
                  </div>
                  {#if item.entry.optimize_description}
                    <div class="muted small app-desc">{item.entry.optimize_description}</div>
                  {/if}
                  <div class="muted small mono">{item.entry.package}</div>
                </td>
                <td class="num">
                  {#if item.memory_mb}
                    <span
                      class:warn={item.memory_mb >= 200}
                      class:caution={item.memory_mb >= 100 && item.memory_mb < 200}
                    >{item.memory_mb.toFixed(1)} MB</span>
                  {:else}
                    <span class="muted">—</span>
                  {/if}
                </td>
                <td class={`risk risk-${item.entry.risk}`}>{item.entry.risk.toUpperCase()}</td>
                <td>
                  {#if skip}
                    <span class="terminal-reason">{skip}</span>
                  {:else}
                    <select
                      class="action-select"
                      class:will-skip={eff === "skip"}
                      class:will-remove={eff === "uninstall"}
                      class:will-act={eff === "disable" || eff === "enable"}
                      value={eff}
                      onchange={(e) =>
                        setOptimizeAction(
                          item.entry.package,
                          (e.currentTarget as HTMLSelectElement).value as RowAction,
                        )}
                      disabled={optimizeRunning}
                    >
                      {#each actionOptions(item) as opt (opt)}
                        <option value={opt}>{actionLabel(item, opt)}</option>
                      {/each}
                    </select>
                  {/if}
                </td>
                <td>
                  {#if progress === "done"}
                    <span class="tag installed">✓ DONE</span>
                  {:else if progress === "pending"}
                    <span class="muted small">…</span>
                  {:else if progress === "skipped"}
                    <span class="muted small">skipped</span>
                  {:else if progress === "failed"}
                    <span class="tag" style="background:var(--danger-surface); color:var(--danger-text)" title={optimizeFailureMessages[item.entry.package] ?? ""}>FAILED</span>
                  {/if}
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  {:else if activeTab === "tweaks"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-tweaks" aria-labelledby="tab-tweaks">
      <div class="card-header">
        <h2>System Tweaks</h2>
        <button onclick={loadTweaks} disabled={tweaksLoading}>
          {tweaksLoading ? "Loading…" : "Refresh"}
        </button>
      </div>
      <p class="muted small">
        Flip the same settings v1's Display/Input Tuning menu wrote. Each click runs
        <code>settings put</code>. Empty value resets to device default.
      </p>
      {#if tweaksErr}
        <div class="error">{tweaksErr}</div>
      {:else if !tweaks}
        <div class="muted">{tweaksLoading ? "Querying…" : "—"}</div>
      {:else}
        {#if tweaksActionMessage}
          <p class="muted small mono action-message">{tweaksActionMessage}</p>
        {/if}

        <h3>HDMI-CEC</h3>
        <p class="muted small">
          Master switch plus three sub-toggles. Disabling the master typically also
          turns off the sub-controls.
        </p>
        <div class="tweak-grid">
          {#each [
            { key: "hdmi_control_enabled", label: "Master (control on/off)", value: tweaks.hdmi_control_enabled },
            { key: "hdmi_control_auto_wakeup_enabled", label: "Auto wake on TV power", value: tweaks.hdmi_control_auto_wakeup_enabled },
            { key: "hdmi_control_auto_device_off_enabled", label: "Auto sleep when TV off", value: tweaks.hdmi_control_auto_device_off_enabled },
            { key: "hdmi_system_audio_control_enabled", label: "System audio control", value: tweaks.hdmi_system_audio_control_enabled },
          ] as row (row.key)}
            <div class="tweak-row">
              <div>
                <div>{row.label}</div>
                <div class="muted small mono">global.{row.key} = {row.value ?? "(unset)"}</div>
              </div>
              <div class="row-actions">
                <button
                  class="small-action"
                  class:active={row.value === "1"}
                  disabled={tweaksActionBusy === row.key}
                  onclick={() => writeTweak("global", row.key, "1", row.key)}
                >On</button>
                <button
                  class="small-action"
                  class:active={row.value === "0"}
                  disabled={tweaksActionBusy === row.key}
                  onclick={() => writeTweak("global", row.key, "0", row.key)}
                >Off</button>
                <button
                  class="small-action"
                  disabled={tweaksActionBusy === row.key}
                  onclick={() => writeTweak("global", row.key, "", row.key)}
                >Reset</button>
              </div>
            </div>
          {/each}
        </div>

        <h3>Match Content Frame Rate</h3>
        <p class="muted small">
          Lets apps switch refresh rate to match video content (24/25/30/60 Hz). Seamless
          only avoids visible black flashes during the switch.
        </p>
        <div class="tweak-row">
          <div>
            <div class="muted small mono">secure.match_content_frame_rate = {tweaks.match_content_frame_rate ?? "(unset)"}</div>
          </div>
          <div class="row-actions">
            {#each [
              { v: "0", label: "Never" },
              { v: "1", label: "Seamless only" },
              { v: "2", label: "Always" },
            ] as opt (opt.v)}
              <button
                class="small-action"
                class:active={tweaks.match_content_frame_rate === opt.v}
                disabled={tweaksActionBusy === "match_content_frame_rate"}
                onclick={() => writeTweak("secure", "match_content_frame_rate", opt.v, "match_content_frame_rate")}
              >{opt.label}</button>
            {/each}
            <button
              class="small-action"
              disabled={tweaksActionBusy === "match_content_frame_rate"}
              onclick={() => writeTweak("secure", "match_content_frame_rate", "", "match_content_frame_rate")}
            >Reset</button>
          </div>
        </div>

        <h3>Long Press Timeout</h3>
        <p class="muted small">
          How long the remote OK button has to be held to register a long-press. Default
          is 400ms; 300ms feels snappier.
        </p>
        <div class="tweak-row">
          <div>
            <div class="muted small mono">secure.long_press_timeout = {tweaks.long_press_timeout ?? "(unset)"}</div>
          </div>
          <div class="row-actions">
            {#each ["300", "400", "500"] as v (v)}
              <button
                class="small-action"
                class:active={tweaks.long_press_timeout === v}
                disabled={tweaksActionBusy === "long_press_timeout"}
                onclick={() => writeTweak("secure", "long_press_timeout", v, "long_press_timeout")}
              >{v} ms</button>
            {/each}
            <button
              class="small-action"
              disabled={tweaksActionBusy === "long_press_timeout"}
              onclick={() => writeTweak("secure", "long_press_timeout", "", "long_press_timeout")}
            >Reset</button>
          </div>
        </div>

        <h3>UI Animations</h3>
        <p class="muted small">
          Sets all three animation scales (window / transition / animator) at once.
          0.5× is a noticeable speedup; 0× disables them entirely.
        </p>
        <div class="tweak-row">
          <div>
            <div class="muted small mono">
              window = {tweaks.window_animation_scale ?? "(unset)"} /
              transition = {tweaks.transition_animation_scale ?? "(unset)"} /
              animator = {tweaks.animator_duration_scale ?? "(unset)"}
            </div>
          </div>
          <div class="row-actions">
            {#each [
              { v: "0", label: "Off" },
              { v: "0.5", label: "Fast (0.5×)" },
              { v: "1", label: "Default (1×)" },
            ] as opt (opt.v)}
              <button
                class="small-action"
                class:active={tweaks.window_animation_scale === opt.v && tweaks.transition_animation_scale === opt.v && tweaks.animator_duration_scale === opt.v}
                disabled={tweaksActionBusy === "animations"}
                onclick={() => setAnimationScale(opt.v)}
              >{opt.label}</button>
            {/each}
            <button
              class="small-action"
              disabled={tweaksActionBusy === "animations"}
              onclick={() => setAnimationScale("")}
            >Reset</button>
          </div>
        </div>

        <h3>Display Scaling</h3>
        <p class="muted small">
          Forces a specific resolution + density via <code>wm size</code> + <code>wm density</code>.
          Mostly for Shield TV — useful for testing 1080p mode on a 4K device.
        </p>
        {#if currentDisplayScaling}
          <div class="current-scaling muted small mono">
            {currentDisplayScaling.size || "Size: unknown"}
            <br />
            {currentDisplayScaling.density || "Density: unknown"}
          </div>
        {/if}
        <div class="scale-options">
          <button
            class="scale-option"
            disabled={displayScaleBusy !== null}
            onclick={() => applyDisplayScaling("uhd_4k")}
          >
            <span class="scale-title">{displayScaleBusy === "uhd_4k" ? "Applying…" : "Shield 4K"}</span>
            <span class="muted small">3839×2160, density 640</span>
          </button>
          <button
            class="scale-option"
            disabled={displayScaleBusy !== null}
            onclick={() => applyDisplayScaling("fhd_1080p")}
          >
            <span class="scale-title">{displayScaleBusy === "fhd_1080p" ? "Applying…" : "Shield 1080p"}</span>
            <span class="muted small">1920×1080, density 320</span>
          </button>
          <button
            class="scale-option"
            disabled={displayScaleBusy !== null}
            onclick={() => applyDisplayScaling("reset")}
          >
            <span class="scale-title">{displayScaleBusy === "reset" ? "Resetting…" : "Reset"}</span>
            <span class="muted small">Restore device defaults</span>
          </button>
        </div>
        {#if displayScaleMessage}
          <p class="muted small mono action-message">{displayScaleMessage}</p>
        {/if}
      {/if}
    </div>
  {:else if activeTab === "files"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-files" aria-labelledby="tab-files">
      <div class="card-header">
        <h2>Files</h2>
        <div class="header-actions">
          <button onclick={uploadToCurrentDir} disabled={filesBusy !== null} title="Upload a file from this computer into the current folder">
            {filesBusy === "__upload__" ? "Uploading…" : "Upload here"}
          </button>
          <button onclick={() => loadFiles(filesPath)} disabled={filesLoading}>
            {filesLoading ? "Loading…" : "Refresh"}
          </button>
        </div>
      </div>
      <p class="muted small">
        Browsing the device's user storage (<code>/sdcard</code>). System paths are off-limits by design.
      </p>

      <details class="app-backups">
        <summary>App file backups — find &amp; save exports (Projectivy theme, SmartTube settings, …)</summary>
        <p class="muted small">
          App settings live in protected storage, but most apps can export a backup to
          <code>/sdcard</code>. Export in the app first, then find the file here and save it to
          this computer. To restore later: browse to the folder below and use <strong>Upload here</strong>,
          then import it in the app.
        </p>
        {#each appFilesCatalog as entry (entry.package)}
          <div class="app-backup-row">
            <div>
              <div class="apk-name">{entry.name}</div>
              <div class="muted small">{entry.hint}</div>
            </div>
            <button
              class="small-action"
              onclick={() => findAppFiles(entry)}
              disabled={appFilesBusy !== null}
            >
              {appFilesBusy === entry.package ? "Searching…" : "Find backup files"}
            </button>
          </div>
          {#if appFilesResults[entry.package]}
            {@const found = appFilesResults[entry.package] ?? []}
            {#if found.length === 0}
              <p class="muted small found-list">No matches — export from the app first, then search again.</p>
            {:else}
              <ul class="found-list">
                {#each found as path (path)}
                  <li>
                    <span class="mono small">{path}</span>
                    <span>
                      <button class="small-action" onclick={() => downloadFoundFile(path)} disabled={appFilesBusy !== null}>
                        Save to computer
                      </button>
                      <button class="small-action subtle" onclick={() => goToFolder(path)} disabled={appFilesBusy !== null}>
                        Go to folder
                      </button>
                    </span>
                  </li>
                {/each}
              </ul>
            {/if}
          {/if}
        {/each}
      </details>

      <nav class="crumbs" aria-label="Path">
        <button
          class="small-action subtle"
          onclick={() => goToFolder(filesPath)}
          disabled={filesPath === "/sdcard" || filesLoading}
          title="Up one level"
        >
          ↑ Up
        </button>
        {#each crumbs as c, i (c.path)}
          {#if i > 0}<span class="muted">/</span>{/if}
          {#if i === crumbs.length - 1}
            <span class="crumb-current">{c.label}</span>
          {:else}
            <button class="crumb" onclick={() => loadFiles(c.path)}>{c.label}</button>
          {/if}
        {/each}
      </nav>
      {#if filesMessage}
        <p class="muted small mono action-message">{filesMessage}</p>
      {/if}
      {#if fileCopyName}
        <div class="clone-panel">
          <span>Copy <code>{fileCopyName}</code> to:</span>
          {#each fileCopyTargets as t (t.serial)}
            <button class="small-action" onclick={() => copyFileTo(t)} disabled={filesBusy !== null}>
              {filesBusy !== null ? "Copying…" : `${t.name} (${t.serial})`}
            </button>
          {/each}
          <button class="small-action subtle" onclick={() => (fileCopyName = null)} disabled={filesBusy !== null}>
            Cancel
          </button>
        </div>
      {/if}
      {#if filesErr}
        <div class="error">{filesErr}</div>
      {:else if filesEntries === null}
        <div class="muted">{filesLoading ? "Loading…" : "—"}</div>
      {:else if filesEntries.length === 0}
        <p class="muted">Empty folder.</p>
      {:else}
        <table class="files-table">
          <thead>
            <tr><th>Name</th><th class="num">Size</th><th>Modified</th><th></th></tr>
          </thead>
          <tbody>
            {#each filesEntries as f (f.name)}
              <tr>
                <td class="file-name">
                  {#if f.is_dir}
                    <button class="dir-link" onclick={() => loadFiles(`${filesPath}/${f.name}`)}>
                      📁 {f.name}
                    </button>
                  {:else}
                    <span>{f.is_symlink ? "🔗" : "📄"} {f.name}</span>
                  {/if}
                </td>
                <td class="num muted">{f.is_dir ? "—" : formatSize(f.size_bytes)}</td>
                <td class="muted small">{f.modified}</td>
                <td class="row-actions">
                  {#if !f.is_dir && !f.is_symlink}
                    <button
                      class="small-action"
                      onclick={() => downloadFile(f.name)}
                      disabled={filesBusy !== null}
                      title="Save this file to a folder on this computer"
                    >
                      {filesBusy === f.name ? "…" : "Download"}
                    </button>
                    <button
                      class="small-action subtle"
                      onclick={() => startFileCopy(f.name)}
                      disabled={filesBusy !== null}
                      title="Copy this file to another connected device"
                    >
                      Copy to…
                    </button>
                  {/if}
                  <button
                    class="small-action subtle danger"
                    onclick={() => deleteEntry(f)}
                    disabled={filesBusy !== null}
                    title="Delete from the device{f.is_dir ? ' (recursive!)' : ''}"
                  >
                    Delete
                  </button>
                </td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </div>
  {:else if activeTab === "sideload"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-sideload" aria-labelledby="tab-sideload">
      <div class="card-header">
        <h2>Install APK</h2>
        <div class="header-actions">
          <button onclick={pickApkFolder} disabled={sideloadBusy !== null || discoveryBusy}>
            {discoveryBusy ? "Scanning…" : "Choose folder…"}
          </button>
          <button class="primary" onclick={pickAndInstallApk} disabled={sideloadBusy !== null}>
            {sideloadBusy !== null ? "Installing…" : "Pick file…"}
          </button>
        </div>
      </div>
      <p class="muted small">
        Pick a file directly, or point at a folder and we'll list every APK inside.
        Either way, install runs <code>adb install -r &lt;file&gt;</code>.
      </p>

      {#if discoveredFolder && discoveredApks.length > 0}
        <div class="apk-folder muted small mono">
          {discoveredFolder} — {discoveredApks.length} APK{discoveredApks.length === 1 ? "" : "s"} found
        </div>
        <ul class="apk-list">
          {#each discoveredApks as apk (apk.path)}
            <li>
              <div class="apk-meta">
                <div class="apk-name">{apk.name}</div>
                <div class="muted small">{formatBytes(apk.size_bytes)}</div>
              </div>
              <button
                class="small-action primary"
                onclick={() => installApkPath(apk.path)}
                disabled={sideloadBusy !== null}
              >
                {sideloadBusy === apk.path ? "Installing…" : "Install"}
              </button>
            </li>
          {/each}
        </ul>
      {:else if discoveredFolder}
        <p class="muted small">No <code>.apk</code> files in {discoveredFolder}.</p>
      {/if}

      {#if sideloadResult}
        <pre class="install-output">{sideloadResult}</pre>
        {#if sideloadHint}
          <div class="warning">{sideloadHint}</div>
        {/if}
      {/if}

      <div class="sideload-catalog">
        <h3>Popular sideloads</h3>
        <p class="muted small">
          Apps people commonly install that aren't on the Play Store. Links go to the
          official source only — download the APK there, then install it with the
          buttons above. You're sideloading third-party software; check it's the
          official release.
        </p>
        <ul class="catalog-list">
          {#each sideloadCatalog as entry (entry.package)}
            <li>
              <div>
                <div class="apk-name">{entry.name}</div>
                <div class="muted small">{entry.description}</div>
                <div class="muted small mono">{entry.package}</div>
              </div>
              <button
                class="small-action"
                onclick={() => openDownloadPage(entry.url)}
                title={entry.url}
              >
                Open download page
              </button>
            </li>
          {/each}
        </ul>
      </div>
    </div>
  {:else if activeTab === "remote"}
    <div class="card" role="tabpanel" tabindex={0} id="tabpanel-remote" aria-labelledby="tab-remote">
      <h2>Remote</h2>
      <div class="remote-layout">
        <div class="remote-typing">
          <h3>Live typing</h3>
          <p class="muted small">
            Click below and type — keystrokes go straight to whatever field has
            focus on the TV, including Backspace and Enter. Each press is an ADB
            round-trip, so it feels like typing over SSH.
          </p>
          <div
            class="type-capture"
            class:focused={remoteCaptureFocused}
            tabindex="0"
            role="textbox"
            aria-label="Live typing capture — keystrokes are sent to the TV"
            onkeydown={remoteKeydown}
            onfocus={() => (remoteCaptureFocused = true)}
            onblur={() => (remoteCaptureFocused = false)}
          >
            {#if remoteEcho}
              <span class="mono">{remoteEcho}</span><span class="caret">▏</span>
            {:else if remoteCaptureFocused}
              <span class="muted">Type now — sending to the TV…</span><span class="caret">▏</span>
            {:else}
              <span class="muted">Click here, then type</span>
            {/if}
          </div>
          {#if remoteMessage}
            <p class="warn-text small mono">{remoteMessage}</p>
          {/if}
        </div>
        <div class="remote-pad">
          <h3>Buttons</h3>
          <div class="dpad">
            <span></span>
            <button onclick={() => sendRemoteKey("up")} title="D-pad up">▲</button>
            <span></span>
            <button onclick={() => sendRemoteKey("left")} title="D-pad left">◀</button>
            <button class="ok" onclick={() => sendRemoteKey("select")} title="Select / OK">OK</button>
            <button onclick={() => sendRemoteKey("right")} title="D-pad right">▶</button>
            <span></span>
            <button onclick={() => sendRemoteKey("down")} title="D-pad down">▼</button>
            <span></span>
          </div>
          <div class="remote-row">
            <button onclick={() => sendRemoteKey("back")} title="Back">Back</button>
            <button onclick={() => sendRemoteKey("home")} title="Home">Home</button>
          </div>
          <div class="remote-row">
            <button onclick={() => sendRemoteKey("rewind")} title="Rewind">◀◀</button>
            <button onclick={() => sendRemoteKey("play_pause")} title="Play / Pause">▶❙❙</button>
            <button onclick={() => sendRemoteKey("fast_forward")} title="Fast forward">▶▶</button>
          </div>
          <div class="remote-row">
            <button onclick={() => sendRemoteKey("volume_down")} title="Volume down">Vol −</button>
            <button onclick={() => sendRemoteKey("mute")} title="Mute">Mute</button>
            <button onclick={() => sendRemoteKey("volume_up")} title="Volume up">Vol +</button>
          </div>
        </div>
      </div>
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
        <table class="snap-table">
          <thead><tr><th>Saved</th><th>Device</th><th>Disabled</th><th></th></tr></thead>
          <tbody>
            {#each snapshots as s}
              <tr>
                <td class="mono small">{s.saved_at}</td>
                <td>{s.device_name}</td>
                <td>{s.disabled_count}</td>
                <td><button onclick={() => previewSnapshot(s.path)}>Preview apply</button></td>
              </tr>
            {/each}
          </tbody>
        </table>
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
            <li><strong>{Object.keys(preview.settings_to_write).length}</strong> settings will be written</li>
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
  td.pkg, td.mono {
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  td.risk {
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
    letter-spacing: 0.04em;
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
  .tweak-grid {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin: 0.4rem 0 0.8rem;
  }
  .tweak-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--bg-button);
  }
  .small-action.active {
    background: var(--accent-strong);
    color: #fff;
    border-color: var(--accent);
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
  .current-scaling {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5rem 0.7rem;
    margin: 0.4rem 0 0.6rem;
    line-height: 1.4;
  }
  .scale-options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 0.5rem;
    margin: 0.4rem 0;
  }
  .scale-option {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
    padding: 0.6rem 0.8rem;
    gap: 0.2rem;
    background: var(--bg-button);
    border: 1px solid var(--border);
    border-radius: 6px;
    cursor: pointer;
  }
  .scale-option:hover:not(:disabled) {
    background: var(--border);
  }
  .scale-option .scale-title {
    font-weight: 500;
    font-size: 0.92rem;
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
  .send-text-section {
    margin-top: 1.5rem;
    padding-top: 1.2rem;
    border-top: 1px solid var(--border);
  }
  .send-text-row {
    display: flex;
    gap: 0.5rem;
    max-width: 480px;
  }
  .send-text-row input {
    flex: 1;
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
  .sideload-catalog {
    margin-top: 1.5rem;
    padding-top: 1.2rem;
    border-top: 1px solid var(--border);
  }
  .catalog-list {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0 0;
  }
  .catalog-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 0;
    border-bottom: 1px solid var(--bg-button);
  }
  .remote-layout {
    display: flex;
    gap: 2.5rem;
    flex-wrap: wrap;
    align-items: flex-start;
  }
  .remote-typing { flex: 1; min-width: 280px; max-width: 480px; }
  .type-capture {
    min-height: 3.2rem;
    padding: 0.8rem;
    border: 1px dashed var(--border);
    border-radius: 6px;
    cursor: text;
    background: var(--bg-inset);
  }
  .type-capture.focused {
    border-style: solid;
    border-color: var(--accent);
  }
  .type-capture .caret {
    color: var(--accent);
    animation: caret-blink 1s steps(1) infinite;
  }
  @keyframes caret-blink { 50% { opacity: 0; } }
  .remote-pad { display: flex; flex-direction: column; gap: 0.6rem; }
  .dpad {
    display: grid;
    grid-template-columns: repeat(3, 3.2rem);
    grid-auto-rows: 3.2rem;
    gap: 0.4rem;
    justify-items: stretch;
  }
  .dpad button { font-size: 1rem; }
  .dpad .ok { font-weight: 700; }
  .remote-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.4rem;
    max-width: 10.4rem;
  }
  .remote-row button { padding: 0.45rem 0.3rem; white-space: nowrap; }
  .app-backups {
    margin: 0.6rem 0 1rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .app-backups summary {
    cursor: pointer;
    font-weight: 600;
  }
  .app-backup-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.5rem 0;
    border-top: 1px solid var(--bg-button);
    margin-top: 0.5rem;
  }
  .found-list {
    list-style: none;
    padding: 0 0 0 0.8rem;
    margin: 0.2rem 0 0.6rem;
  }
  .found-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8rem;
    padding: 0.25rem 0;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin: 0.4rem 0 0.8rem;
    flex-wrap: wrap;
  }
  .crumb {
    background: none;
    border: none;
    padding: 0.1rem 0.2rem;
    color: var(--accent);
    cursor: pointer;
    font-size: 0.95rem;
  }
  .crumb:hover { text-decoration: underline; }
  .crumb-current { font-weight: 600; }
  .files-table {
    width: 100%;
    border-collapse: collapse;
  }
  .files-table th, .files-table td {
    text-align: left;
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid var(--bg-button);
  }
  .files-table .num { text-align: right; white-space: nowrap; }
  .files-table .row-actions { text-align: right; white-space: nowrap; }
  .dir-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--fg);
    cursor: pointer;
    font-size: 0.95rem;
  }
  .dir-link:hover { color: var(--accent); }
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
  .plan-summary {
    margin: 0.4rem 0;
    padding: 0.5rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.9rem;
  }
  /* Skipped rows recede; rows that WILL be acted on stand out with a left
     accent bar and a faint tint so the consequential rows are obvious at a
     glance (the dim-everything approach was too subtle to read). */
  .optimize-table tr.dim {
    opacity: 0.45;
  }
  .optimize-table tr.acting td {
    background: color-mix(in srgb, var(--accent-strong) 8%, transparent);
  }
  .optimize-table tr.acting td:first-child {
    box-shadow: inset 3px 0 0 var(--accent-strong);
  }
  .checkbox-row {
    display: flex;
    align-items: center;
    gap: 0.4rem;
    font-size: 0.85rem;
    color: var(--fg-secondary);
    cursor: pointer;
  }
  .action-select {
    font-size: 0.85rem;
    padding: 0.25rem 0.5rem;
    min-width: 9.5rem;
  }
  /* Color the dropdown by what it will do, so each row's intent is legible at
     a glance: muted italic for Skip, accent for disable/enable, danger for the
     destructive uninstall. */
  .action-select.will-skip {
    color: var(--fg-muted);
    font-style: italic;
  }
  .action-select.will-act {
    color: var(--accent);
    font-weight: 500;
  }
  .action-select.will-remove {
    color: var(--danger-strong);
    font-weight: 500;
  }
  /* Terminal rows (not installed / already in target state) can't be acted on —
     a neutral pill, distinct from the italic "Skip (recommended)" dropdown so
     "nothing to do here" doesn't read like "you chose to skip this". */
  .terminal-reason {
    display: inline-block;
    font-size: 0.74rem;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    background: var(--bg-muted);
    color: var(--fg-faint);
    letter-spacing: 0.02em;
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
  .apk-folder {
    margin: 0.4rem 0;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    word-break: break-all;
  }
  .apk-list {
    list-style: none;
    padding: 0;
    margin: 0.4rem 0 0.8rem;
  }
  .apk-list li {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.8rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--bg-button);
  }
  .apk-list li:last-child {
    border-bottom: none;
  }
  .apk-name {
    font-family: ui-monospace, monospace;
    font-size: 0.88rem;
    word-break: break-all;
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
