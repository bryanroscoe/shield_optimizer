<script lang="ts">
  // AppDetailSheet plus the actions and measurements behind it, for screens
  // that open one app at a time (Optimize, Diagnostics). It follows the Apps
  // screen's rules: a removal re-reads `safety_info` before the confirm and
  // again before the mutation, Protected is refused outright, and every result
  // is bound to the connection the sheet was opened on.
  import { onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { verdictSummary } from "../lib/safety";
  import { uninstallNote } from "../lib/recommendation";
  import { idleMeasurements, loadingMeasurements, settled, type AppMeasurements } from "../lib/app-details";
  import type { AppItem } from "../lib/appsList";
  import type { Screen } from "../lib/router.svelte";
  import type { AppStorage, AppUsage, Safety } from "../lib/types";
  import AppDetailSheet from "./AppDetailSheet.svelte";
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

  type Target = { serial: string; generation: number };
  type RemovalIntent = Target & { action: "disable" | "uninstall"; app: AppItem; verdict: Safety };

  let target = $state<Target | null>(null);
  let shown = $state<AppItem | null>(null);
  let uninstalled = $state(false);
  let busy = $state(false);
  let intent = $state<RemovalIntent | null>(null);
  let showPaywall = $state(false);

  let measures = $state<AppMeasurements>(idleMeasurements());
  let memoryMap = $state<Record<string, number>>({});
  let usageMap = $state<Record<string, AppUsage>>({});
  let storageMap = $state<Record<string, AppStorage>>({});
  let measuredFor = "";
  let measureRequest = 0;
  let request = 0;
  let destroyed = false;

  // Bind to the connection the sheet opened on; a reconnect closes it.
  $effect(() => {
    const next = app;
    const serial = session.serial;
    const generation = session.generation;
    const live = session.isConnected;
    ++request;
    intent = null;
    busy = false;
    uninstalled = false;
    if (!next || !serial || !live) {
      shown = null;
      target = null;
      return;
    }
    shown = { ...next };
    target = { serial, generation };
    if (measuredFor !== `${serial}|${generation}`) void measure({ serial, generation });
  });

  onDestroy(() => {
    destroyed = true;
    ++request;
    ++measureRequest;
  });

  // The same three batch reads the Apps screen makes, each stamped with when
  // it answered; a failed read is "unavailable", never zero.
  async function measure(t: Target) {
    measuredFor = `${t.serial}|${t.generation}`;
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

  function changed(t: Target) {
    session.invalidateAll();
    onChanged();
    measuredFor = "";
    void measure(t);
  }

  function handleToggle(a: AppItem) {
    if (a.state === "enabled") {
      void requestRemoval(a, "disable");
      return;
    }
    if (a.state !== "disabled") return;
    void run(async (t) => {
      const r = await api.enablePackage(t.serial, a.package);
      if (!current(t)) return;
      onToast(r.ok ? `Enabled ${label(a)}.` : r.message || "Enable failed.", r.ok ? "success" : "error");
      changed(t);
      if (r.ok) onClose();
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
      const r =
        pending.action === "disable"
          ? await api.disablePackage(t.serial, pending.app.package)
          : await api.uninstallPackage(t.serial, pending.app.package);
      if (!current(t)) return;
      const done = pending.action === "disable" ? "Disabled" : "Uninstalled";
      const verb = pending.action === "disable" ? "Disable" : "Uninstall";
      onToast(r.ok ? `${done} ${label(pending.app)}.` : r.message || `${verb} failed.`, r.ok ? "success" : "error");
      // A not-ok result can still have landed, so the caller reloads either way.
      changed(t);
      if (r.ok && pending.action === "uninstall") uninstalled = true;
      else if (r.ok) onClose();
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
      const r = await api.reinstallExisting(t.serial, a.package);
      if (!current(t)) return;
      if (r.ok) {
        onToast(`Reinstalled ${label(a)}.`, "success");
        uninstalled = false;
        changed(t);
        onClose();
      } else {
        onToast(r.message || "Reinstall failed. Try the Play Store.", "error");
      }
    });
  }

  const note = $derived(
    intent?.action === "uninstall" ? uninstallNote(intent.app.package, intent.app.entry) : null,
  );
</script>

<AppDetailSheet
  app={shown}
  memoryMb={shown ? (memoryMap[shown.package] ?? null) : null}
  usage={shown ? (usageMap[shown.package] ?? null) : null}
  storage={shown ? (storageMap[shown.package] ?? null) : null}
  {measures}
  onRemeasure={() => target && current(target) && void measure(target)}
  {busy}
  {uninstalled}
  onClose={() => {
    ++request;
    intent = null;
    onClose();
  }}
  onToggle={handleToggle}
  onForceStop={handleForceStop}
  onUninstall={(a) => void requestRemoval(a, "uninstall")}
  onPlayStore={handlePlayStore}
  onReinstall={handleReinstall}
/>

<ConfirmDialog
  open={intent !== null}
  danger
  icon={intent?.action === "disable" ? "block" : "delete"}
  title={`${intent?.action === "disable" ? "Disable" : "Uninstall"} ${intent ? label(intent.app) : "app"}?`}
  warning={intent?.verdict.reason ?? ""}
  message={intent?.action === "disable"
    ? `${verdictSummary(intent.verdict)} Disable is reversible with Enable.`
    : `${verdictSummary(intent?.verdict)} Uninstall removes the app for this TV's current user. Reinstall works only while its APK remains on the TV; otherwise use the Play Store.${note ? ` ${note}` : ""}`}
  confirmLabel={intent?.action === "disable" ? "Disable" : "Uninstall"}
  onConfirm={confirmRemoval}
  onCancel={() => (intent = null)}
/>

<PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />
