<script lang="ts">
  // AppDetailSheet plus the actions and measurements behind it, for screens
  // that open one app at a time (Optimize, Diagnostics). It follows the Apps
  // screen's rules: a removal re-reads `safety_info` before the confirm and
  // again before the mutation, Protected is refused outright, and every result
  // is bound to the connection the sheet was opened on.
  import { onDestroy, untrack } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { verdictSummary } from "../lib/safety";
  import { uninstallNote } from "../lib/recommendation";
  import { isReportablePackage, type AppReportState } from "../lib/app-report";
  import {
    idleMeasurements,
    loadingMeasurements,
    reportDeviceOf,
    reportFigures,
    reportLiveState,
    settled,
    type ApkReportMeasurement,
    type AppMeasurements,
  } from "../lib/app-details";
  import type { AppItem } from "../lib/appsList";
  import { router, type Screen } from "../lib/router.svelte";
  import type { AppStorage, AppUsage, Safety } from "../lib/types";
  import { destructiveWarning } from "../../../shared/support";
  import packageMetadata from "../../package.json";
  import AppDetailSheet from "./AppDetailSheet.svelte";
  import AppReportSheet from "./AppReportSheet.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import PaywallSheet from "./PaywallSheet.svelte";

  let {
    app,
    navigate,
    onClose,
    onChanged,
    onToast,
  }: {
    app: AppItem | null;
    navigate: (screen: Screen) => void;
    onClose: () => void;
    /// Something on the TV changed; the caller's data is stale.
    onChanged: () => void;
    onToast: (message: string, type: "success" | "error" | "info") => void;
  } = $props();

  type Target = Readonly<{ serial: string; generation: number }>;
  type RemovalIntent = Target & { action: "disable" | "uninstall"; app: AppItem; verdict: Safety };

  let target = $state.raw<Target | null>(null);
  let shown = $state<AppItem | null>(null);
  let uninstalled = $state(false);
  let busy = $state(false);
  let intent = $state<RemovalIntent | null>(null);
  let showPaywall = $state(false);

  let measures = $state<AppMeasurements>(idleMeasurements());
  let memoryMap = $state<Record<string, number>>({});
  let usageMap = $state<Record<string, AppUsage>>({});
  let storageMap = $state<Record<string, AppStorage>>({});
  /// Also the batch id an APK-size receipt is matched against.
  let measureRequest = $state(0);
  let request = 0;
  let destroyed = false;

  // "Report this app", as the Apps screen offers it. The verdict is the one
  // the sheet resolved, so the report carries what the user was shown.
  let sheetVerdict = $state.raw<{ pkg: string; verdict: Safety | null } | null>(null);
  let reportApp = $state<AppItem | null>(null);
  let reportVerdict = $state<Safety | null>(null);
  let reportApk = $state.raw<ApkReportMeasurement | null>(null);
  let reportRequest = 0;

  let selectedApp: AppItem | null = null;

  function invalidateSelection() {
    ++request;
    ++measureRequest;
    ++reportRequest;
    reportApp = null;
    reportApk = null;
    reportVerdict = null;
    sheetVerdict = null;
    target = null;
    shown = null;
    intent = null;
    showPaywall = false;
    busy = false;
    uninstalled = false;
    measures = idleMeasurements();
    memoryMap = {};
    usageMap = {};
    storageMap = {};
  }

  function closeSelection() {
    invalidateSelection();
    onClose(); // Clear the parent's selection too: reconnect must not reopen it.
  }

  // Only a new explicit app selection captures an opening identity. Session
  // changes must invalidate it, never bind the old package to a new transport.
  $effect(() => {
    const next = app;
    untrack(() => {
      if (next === selectedApp) return;
      selectedApp = next;
      invalidateSelection();
      if (!next) return;
      const serial = session.serial;
      const generation = session.generation;
      if (!serial || !session.isConnected) {
        onClose();
        return;
      }
      const opening = Object.freeze({ serial, generation });
      target = opening;
      shown = { ...next };
      void measure(opening);
    });
  });

  $effect(() => {
    const serial = session.serial;
    const generation = session.generation;
    const live = session.isConnected;
    const opening = target;
    if (opening && (!live || serial !== opening.serial || generation !== opening.generation)) {
      untrack(closeSelection);
    }
  });

  onDestroy(() => {
    destroyed = true;
    ++request;
    ++measureRequest;
    ++reportRequest;
  });

  // The same three batch reads the Apps screen makes, each stamped with when
  // it answered; a failed read is "unavailable", never zero.
  async function measure(t: Target) {
    const mine = ++measureRequest;
    measures = loadingMeasurements();
    const [memory, usage, storage] = await Promise.allSettled([
      api.appMemoryMap(t.serial),
      api.appUsageMap(t.serial),
      api.appStorageMap(t.serial),
    ]);
    if (mine !== measureRequest || !current(t)) return;
    const at = Date.now();
    memoryMap = memory.status === "fulfilled" ? memory.value : {};
    usageMap = usage.status === "fulfilled" ? usage.value : {};
    storageMap = storage.status === "fulfilled" ? storage.value : {};
    measures = {
      memory: settled(memory, at),
      usage: settled(usage, at),
      storage: settled(storage, at),
    };
  }

  function current(t: Target | null): t is Target {
    return (
      !destroyed &&
      t !== null &&
      t === target &&
      session.serial === t.serial &&
      session.generation === t.generation &&
      session.isConnected
    );
  }

  function label(a: AppItem): string {
    return a.name || a.package;
  }

  async function run(action: (t: Target) => Promise<void>) {
    const t = target;
    if (busy || !current(t)) return;
    const mine = ++request;
    busy = true;
    try {
      await action(t);
    } catch (e) {
      if (mine === request && current(t)) {
        if (String(e).includes("LOCKED:")) showPaywall = true;
        else onToast(String(e), "error");
      }
    } finally {
      if (mine === request) busy = false;
    }
  }

  /// A change can land even when its reply is lost or arrives after a
  /// reconnect, so the shared caches are dropped as soon as the call settles,
  /// before any currency check can return early.
  async function settleChange<T>(change: Promise<T>): Promise<T> {
    try {
      return await change;
    } finally {
      session.invalidateAll();
    }
  }

  function changed(t: Target) {
    onChanged();
    void measure(t);
  }

  function handleToggle(a: AppItem) {
    if (a.state === "enabled") {
      void requestRemoval(a, "disable");
      return;
    }
    if (a.state !== "disabled") return;
    void run(async (t) => {
      const r = await settleChange(api.enablePackage(t.serial, a.package));
      if (!current(t)) return;
      onToast(r.ok ? `Enabled ${label(a)}.` : r.message || "Enable failed.", r.ok ? "success" : "error");
      changed(t);
      if (r.ok) closeSelection();
    });
  }

  async function requestRemoval(a: AppItem, action: "disable" | "uninstall") {
    await run(async (t) => {
      const verdict = await api.safetyInfo(a.package).catch(() => null);
      if (!current(t)) return;
      if (!verdict) {
        onToast("Safety unavailable. Retry before removing this app.", "error");
        return;
      }
      if (verdict.kind === "never_disable") {
        onToast(`Protected: ${verdict.reason}`, "error");
        return;
      }
      intent = { ...t, action, app: a, verdict };
    });
  }

  async function confirmRemoval() {
    const pending = intent;
    intent = null;
    if (!pending) return;
    await run(async (t) => {
      if (t.serial !== pending.serial || t.generation !== pending.generation) return;
      const fresh = await api.safetyInfo(pending.app.package).catch(() => null);
      if (!current(t)) return;
      if (!fresh) {
        onToast("Safety unavailable. Retry before removing this app.", "error");
        return;
      }
      if (fresh.kind === "never_disable") {
        onToast(`Protected: ${fresh.reason}`, "error");
        return;
      }
      if (fresh.kind !== pending.verdict.kind || fresh.reason !== pending.verdict.reason) {
        intent = { ...pending, verdict: fresh };
        onToast("Safety guidance changed. Review the updated warning before confirming.", "info");
        return;
      }
      const r = await settleChange(
        pending.action === "disable"
          ? api.disablePackage(t.serial, pending.app.package)
          : api.uninstallPackage(t.serial, pending.app.package),
      );
      if (!current(t)) return;
      const done = pending.action === "disable" ? "Disabled" : "Uninstalled";
      const verb = pending.action === "disable" ? "Disable" : "Uninstall";
      onToast(r.ok ? `${done} ${label(pending.app)}.` : r.message || `${verb} failed.`, r.ok ? "success" : "error");
      // A not-ok result can still have landed, so the caller reloads either way.
      changed(t);
      if (r.ok && pending.action === "uninstall") uninstalled = true;
      else closeSelection();
    });
  }

  function handleForceStop(a: AppItem) {
    void run(async (t) => {
      const r = await api.forceStop(t.serial, a.package);
      if (!current(t)) return;
      onToast(r.ok ? `Stopped ${label(a)}.` : r.message || "Couldn't stop the app.", r.ok ? "success" : "error");
      if (r.ok) session.invalidateHealth();
    });
  }

  function handlePlayStore(a: AppItem) {
    void run(async (t) => {
      const r = await api.openPlayStore(t.serial, a.package);
      if (!current(t)) return;
      onToast(r.ok ? `Opened the Play Store for ${label(a)} on the TV.` : r.message, r.ok ? "success" : "error");
    });
  }

  function handleReinstall(a: AppItem) {
    if (!uninstalled) return;
    void run(async (t) => {
      const r = await settleChange(api.reinstallExisting(t.serial, a.package));
      if (!current(t)) return;
      if (r.ok) {
        onToast(`Reinstalled ${label(a)}.`, "success");
        uninstalled = false;
        changed(t);
        closeSelection();
      } else {
        onToast(r.message || `Reinstall failed. ${uninstallNote(a.package, a.entry)}`, "error");
      }
    });
  }

  const note = $derived(
    intent?.action === "uninstall" ? uninstallNote(intent.app.package, intent.app.entry) : null,
  );

  const reportDevice = $derived(reportDeviceOf(session.connectedDevice, session.serial));
  const canReport = $derived(!!shown && !!reportDevice && isReportablePackage(shown.package));

  function openReport(a: AppItem, apk?: ApkReportMeasurement) {
    const t = target;
    if (!current(t) || !canReport || a.package !== shown?.package) return;
    const mine = ++reportRequest;
    reportApp = a;
    reportApk = apk ?? null;
    reportVerdict = sheetVerdict?.pkg === a.package ? sheetVerdict.verdict : null;
    if (reportVerdict) return;
    api.safetyInfo(a.package).then(
      (verdict) => {
        if (mine === reportRequest && reportApp?.package === a.package && current(t)) reportVerdict = verdict;
      },
      () => {},
    );
  }

  function closeReport() {
    ++reportRequest;
    reportApp = null;
    reportApk = null;
  }

  function research(packageId: string) {
    closeSelection();
    router.navigate("catalog", packageId);
  }

  /// What the TV last said about this row: its state when the sheet opened,
  /// or missing once this sheet uninstalled it. None while a run is changing
  /// the device underneath it.
  function reportState(a: AppItem): AppReportState {
    const rowState = uninstalled ? "missing" : a.state;
    const live = reportLiveState(!!a.entry, rowState ?? undefined, false, session.applyInProgress);
    const apk = reportApk;
    const t = target;
    const apkCurrent =
      !!apk &&
      current(t) &&
      apk.serial === t.serial &&
      apk.generation === t.generation &&
      apk.package === a.package &&
      apk.measurementId === measureRequest &&
      rowState !== "missing";
    return {
      status: live !== undefined ? live : rowState,
      ...reportFigures(measures, memoryMap[a.package], storageMap[a.package], apkCurrent ? apk : null),
    };
  }
</script>

<AppDetailSheet
  app={shown}
  memoryMb={shown ? (memoryMap[shown.package] ?? null) : null}
  usage={shown ? (usageMap[shown.package] ?? null) : null}
  storage={shown ? (storageMap[shown.package] ?? null) : null}
  {measures}
  measurementId={measureRequest}
  onRemeasure={() => target && current(target) && void measure(target)}
  {busy}
  {uninstalled}
  {canReport}
  onClose={closeSelection}
  onToggle={handleToggle}
  onForceStop={handleForceStop}
  onUninstall={(a) => void requestRemoval(a, "uninstall")}
  onPlayStore={handlePlayStore}
  onReinstall={handleReinstall}
  onReport={openReport}
  onResearch={research}
  onVerdict={(pkg, verdict) => (sheetVerdict = { pkg, verdict })}
/>

{#if reportApp && reportDevice}
  <AppReportSheet
    package={reportApp.package}
    appName={reportApp.name}
    verdict={reportVerdict}
    device={reportDevice}
    appState={reportState(reportApp)}
    appVersion={packageMetadata.version}
    onClose={closeReport}
  />
{/if}

<ConfirmDialog
  open={intent !== null}
  danger
  icon={intent?.action === "disable" ? "block" : "delete"}
  title={`${intent?.action === "disable" ? "Disable" : "Uninstall"} ${intent ? label(intent.app) : "app"}?`}
  warning={intent?.verdict.reason ?? ""}
  supportWarning={destructiveWarning(session.support)}
  message={intent?.action === "disable"
    ? `${verdictSummary(intent.verdict)} Disable is reversible with Enable.`
    : `${verdictSummary(intent?.verdict)} Uninstall removes the app for this TV's current user. Reinstall works only while its APK remains on the TV.${note ? ` ${note}` : ""}`}
  confirmLabel={intent?.action === "disable" ? "Disable" : "Uninstall"}
  onConfirm={confirmRemoval}
  onCancel={() => (intent = null)}
/>

<PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />
