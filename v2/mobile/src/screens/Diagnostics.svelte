<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import { reasonOf, tierOf } from "../lib/safety";
  import type { Safety } from "../lib/types";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";

  let { navigate, back }: {
    navigate: (screen: Screen) => void;
    back: () => void;
  } = $props();

  // Safety tags come from the backend `safety_info` command — the one audited
  // classifier. No inline package→risk table (that violated the "one detection
  // function" invariant and could disagree with the engine).
  let safetyMap = $state<Record<string, Safety>>({});
  let safetyRequest = 0;
  // Real catalog names (app_list_for_device). Missing package = no name, and we
  // then show the package itself rather than inventing one from its last dot
  // segment ("…youtube.tv" is not an app called "Tv").
  let catalogNames = $state<Record<string, string>>({});

  let liveMode = $state(false);
  let liveTimer: ReturnType<typeof setInterval> | undefined;

  let busyPkg = $state("");
  let disableTarget = $state<{ pkg: string; label: string; reason: string; caution: boolean } | null>(null);
  let showPaywall = $state(false);
  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  function showToast(message: string, type: "success" | "error" | "info" = "info") {
    toast = message;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3200);
  }

  const health = $derived(session.health);
  const topPackages = $derived(
    (health?.top_memory ?? []).slice(0, 8).map((m) => m.package),
  );
  // Stable dependency for the safety effect: refetch when the *set* of top
  // packages changes, not on every 3-second refresh of the same set.
  const topKey = $derived(topPackages.join(","));

  onMount(() => {
    void session.loadHealth();
  });

  // Driven by data, not by the awaited loadHealth() call — `loadHealth`
  // resolves immediately when another load is already in flight, which is why
  // the old `await loadHealth(); loadSafety()` never found any packages.
  $effect(() => {
    const key = topKey;
    const pkgs = key ? key.split(",") : [];
    const request = ++safetyRequest;
    const serial = session.serial;
    if (pkgs.length === 0) {
      safetyMap = {};
      return;
    }
    Promise.all(pkgs.map(async (p) => [p, await api.safetyInfo(p)] as const))
      .then((pairs) => {
        if (request !== safetyRequest || serial !== session.serial) return;
        const map: Record<string, Safety> = {};
        for (const [p, s] of pairs) map[p] = s;
        safetyMap = map;
      })
      .catch(() => {
        // Leave tags unresolved rather than inventing them.
      });
  });

  $effect(() => {
    const device = session.connectedDevice;
    if (!device) {
      catalogNames = {};
      return;
    }
    const serial = device.serial;
    api
      .appListForDevice(device.device_type)
      .then((entries) => {
        if (serial !== session.serial) return;
        const map: Record<string, string> = {};
        for (const e of entries) map[e.package] = e.name;
        catalogNames = map;
      })
      .catch(() => {
        // No names is fine — rows fall back to the package.
      });
  });

  function startLive() {
    stopLive();
    liveTimer = setInterval(() => void session.loadHealth(true), 3000);
  }

  function stopLive() {
    clearInterval(liveTimer);
    liveTimer = undefined;
  }

  function toggleLive() {
    liveMode = !liveMode;
    if (liveMode) {
      startLive();
      void session.loadHealth(true);
    } else {
      stopLive();
    }
  }

  onDestroy(() => {
    stopLive();
    clearTimeout(toastTimer);
  });

  async function refresh() {
    await session.loadHealth(true);
  }

  // --- Memory (real values or "—"; no fabricated 2966/1512 fallbacks) ---
  const totalRam = $derived(health?.ram?.total_mb ?? null);
  const freeRam = $derived(health?.ram?.free_mb ?? null);
  const usedRam = $derived(
    health?.ram?.used_mb ??
      (totalRam != null && freeRam != null ? Math.max(0, totalRam - freeRam) : null),
  );
  const ramText = $derived(
    usedRam != null && totalRam != null
      ? `${Math.round(usedRam)} / ${Math.round(totalRam)} MB used`
      : "—",
  );
  const ramUsedPercent = $derived(
    usedRam != null && totalRam
      ? Math.min(100, Math.max(0, Math.round((usedRam / totalRam) * 100)))
      : 0,
  );

  // --- Storage ---
  const storageText = $derived(
    health?.storage?.used != null && health?.storage?.total != null
      ? `${health.storage.used} / ${health.storage.total} used`
      : "—",
  );
  const storageUsedPercent = $derived(health?.storage?.used_percent ?? 0);

  // --- Temperature ---
  // The sensor is real; the "warm / throttle" cut-offs we used to print were
  // invented. Show the reading on a neutral, explicitly-approximate scale.
  const tempC = $derived(health?.temperature_c ?? null);
  const tempPercent = $derived(
    tempC == null ? 0 : Math.min(100, Math.max(0, ((tempC - 20) / 80) * 100)),
  );

  // --- Display & audio ---
  const displayRes = $derived(health?.display?.resolution ?? "—");
  const displayHz = $derived(
    health?.display?.refresh_hz != null ? `${health.display.refresh_hz} Hz` : "",
  );
  const hdrTypes = $derived(health?.display?.hdr_types ?? []);
  const hdrText = $derived(hdrTypes.length ? hdrTypes.join(" · ") : "");
  const audioOutput = $derived(health?.audio_device ?? "Unavailable");

  function appLabel(pkg: string): string {
    return catalogNames[pkg] || pkg;
  }

  async function handleForceStop(pkg: string) {
    if (busyPkg || !session.serial) return;
    busyPkg = pkg;
    try {
      const r = await api.forceStop(session.serial, pkg);
      showToast(
        r.ok ? `Stopped ${appLabel(pkg)}.` : r.message || "Couldn't stop the app.",
        r.ok ? "success" : "error",
      );
      if (r.ok) await session.loadHealth(true);
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyPkg = "";
    }
  }

  // Fail closed: no `safety_info` verdict yet means no disable. never_disable is
  // a hard block; caution gets the loud confirm carrying core's reason.
  function requestDisable(pkg: string) {
    const s = safetyMap[pkg];
    if (!s) {
      showToast("Still checking this package's safety — try again in a moment.", "info");
      return;
    }
    if (s.kind === "never_disable") {
      showToast(`Protected: ${s.reason}`, "error");
      return;
    }
    disableTarget = {
      pkg,
      label: appLabel(pkg),
      reason: reasonOf(s),
      caution: s.kind === "caution",
    };
  }

  async function confirmDisable() {
    const target = disableTarget;
    disableTarget = null;
    if (!target || !session.serial) return;
    busyPkg = target.pkg;
    try {
      const r = await api.disablePackage(session.serial, target.pkg);
      if (r.ok) {
        showToast(`Disabled ${target.label}.`, "success");
        session.invalidateAll();
        await session.loadHealth(true);
      } else {
        showToast(r.message || "Disable failed.", "error");
      }
    } catch (e) {
      if (String(e).includes("LOCKED:")) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      busyPkg = "";
    }
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Diagnostics</h3>
    {#if session.connectedDevice}
      <button
        class="live-refresh-btn"
        class:on={liveMode}
        onclick={toggleLive}
        aria-pressed={liveMode}
      >
        <span class="msr" class:blink={liveMode && session.healthLoading}>refresh</span>live
      </button>
    {:else}
      <span style="width:44px"></span>
    {/if}
  </div>

  {#if !session.connectedDevice}
    <div class="empty-state">
      <span class="msr empty-icon">tv_gen</span>
      <h4>No TV connected</h4>
      <p class="lede">
        Diagnostics reads memory, storage, temperature and display straight off the TV, so it needs
        a live connection.
      </p>
      <button class="primary" onclick={() => navigate("devices")}>
        <span class="msr">devices_other</span>Connect a TV
      </button>
    </div>
    <div class="spacer"></div>
  {:else if session.healthLoading && !health}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Gathering metrics…</span>
    </div>
  {:else if session.healthError && !health}
    <p class="error">{session.healthError}</p>
    <button class="primary" onclick={refresh}>Retry</button>
  {:else if health}
    {#if session.healthError}
      <div class="stale-warning" role="alert">
        <span class="msr">cloud_off</span>
        <span>Refresh failed. Showing the last successful metrics for this TV.</span>
      </div>
    {/if}
    <div class="diagnostics-content">
      <!-- Memory -->
      <div class="diagnostic-card">
        <div class="card-header">
          <span class="card-title">Memory</span>
          <span class="mono card-value">{ramText}</span>
        </div>
        <div class="progress-track">
          <div class="progress-fill ram-gradient" style="width: {ramUsedPercent}%"></div>
        </div>
      </div>

      <!-- Storage -->
      <div class="diagnostic-card">
        <div class="card-header">
          <span class="card-title">Storage</span>
          <span class="mono card-value">{storageText}</span>
        </div>
        <div class="progress-track">
          <div class="progress-fill storage-gradient" style="width: {storageUsedPercent}%"></div>
        </div>
      </div>

      <!-- Temperature -->
      <div class="diagnostic-card">
        <div class="card-header">
          <span class="card-title inline-title">
            <span class="msr temp-icon">thermostat</span>Temperature
          </span>
          <span class="mono card-value">approximate</span>
        </div>
        {#if tempC != null}
          <div class="temp-readout">
            <span class="mono temp-value">{Math.round(tempC)}<span class="temp-unit"> °C</span></span>
          </div>
          <div class="temp-bar-wrap">
            <div class="temp-neutral-bar"></div>
            <span class="temp-indicator-dot" style="left: {tempPercent}%"></span>
          </div>
          <div class="temp-labels">
            <span>20°</span>
            <span>60°</span>
            <span>100°</span>
          </div>
          <p class="temp-note">
            One sensor reading from the TV. Throttling points differ per device, so this app doesn't
            grade it as normal or hot.
          </p>
        {:else}
          <span class="mono card-value">Unavailable</span>
        {/if}
      </div>

      <!-- Display & Audio -->
      <div class="grid2">
        <div class="diagnostic-card">
          <span class="card-sub-label">Display</span>
          <span class="grid-card-value">{displayRes}{displayHz ? ` · ${displayHz}` : ""}</span>
          {#if hdrText}
            <span class="hdr-tag"><span class="msr">hdr_on</span>{hdrText}</span>
          {/if}
        </div>
        <div class="diagnostic-card">
          <span class="card-sub-label">Audio</span>
          <span class="grid-card-value">{audioOutput}</span>
        </div>
      </div>

      <!-- Top Memory Consumers -->
      <div class="top-memory-section">
        <span class="section-label">Top memory consumers</span>
        {#if (health.top_memory?.length ?? 0) === 0}
          <p class="lede empty">No process memory data available.</p>
        {:else}
          <div class="consumers-list">
            {#each health.top_memory.slice(0, 8) as consumer (consumer.package)}
              {@const tier = tierOf(safetyMap[consumer.package])}
              <div class="consumer-row">
                <div class="consumer-details">
                  <span class="consumer-name">{appLabel(consumer.package)}</span>
                  <span class="mono consumer-pkg">{consumer.package}</span>
                </div>
                {#if tier}
                  <span class="risk-badge {tier.cls}">{tier.label}</span>
                {/if}
                <span class="mono consumer-mb">{Math.round(consumer.mb)} MB</span>
                <div class="row-actions">
                  <button
                    class="rowbtn"
                    disabled={busyPkg !== ""}
                    onclick={() => handleForceStop(consumer.package)}
                    aria-label="Force stop {appLabel(consumer.package)}"
                  >
                    <span class="msr">stop_circle</span>
                  </button>
                  <button
                    class="rowbtn"
                    disabled={busyPkg !== "" || tier == null || tier.cls === "blocked"}
                    onclick={() => requestDisable(consumer.package)}
                    aria-label="Disable {appLabel(consumer.package)}"
                  >
                    <span class="msr">block</span>
                  </button>
                </div>
              </div>
            {/each}
          </div>
          <p class="consumers-note">
            Force stop frees the process now; Android may restart it. Disable is reversible from
            Apps. Blocked packages can't be disabled at all.
          </p>
        {/if}
      </div>
    </div>
  {/if}

  <ConfirmDialog
    open={disableTarget !== null}
    danger
    icon="block"
    title={`Disable ${disableTarget?.label ?? "app"}?`}
    warning={disableTarget?.caution ? disableTarget.reason : ""}
    message="Disable is reversible — re-enable it from the Apps tab. The safety engine still has the final say on the TV."
    confirmLabel="Disable"
    onConfirm={confirmDisable}
    onCancel={() => (disableTarget = null)}
  />

  <PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />
  <Toast message={toast} type={toastType} />
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
  .live-refresh-btn {
    background: transparent;
    border: none;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--muted);
    display: inline-flex;
    align-items: center;
    gap: 5px;
    cursor: pointer;
    margin-left: auto;
    padding: 6px;
  }
  .live-refresh-btn.on {
    color: var(--accent);
  }
  .live-refresh-btn .msr {
    font-size: 15px;
  }

  .empty-state {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 10px;
    padding: 34px 20px;
    border-radius: 20px;
    background: var(--surface);
    border: 1px solid var(--line);
    margin-top: 20px;
  }
  .empty-state .empty-icon {
    font-size: 40px;
    color: var(--dim);
  }
  .empty-state h4 {
    margin: 0;
    font-size: 17px;
    font-weight: 700;
  }
  .empty-state .primary {
    max-width: 240px;
    margin-top: 4px;
  }

  .diagnostics-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .stale-warning {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 14px;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--amber) 35%, transparent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--amber) 9%, transparent);
    color: var(--amber);
    font-size: 12px;
    line-height: 1.4;
  }
  .stale-warning .msr {
    font-size: 18px;
  }

  .diagnostic-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card-header {
    display: flex;
    justify-content: space-between;
    align-items: baseline;
  }
  .card-title {
    font-size: 13px;
    font-weight: 600;
    color: var(--text-soft);
  }
  .card-title.inline-title {
    display: inline-flex;
    align-items: center;
    gap: 7px;
  }
  .card-title.inline-title .temp-icon {
    font-size: 18px;
    color: var(--teal);
  }
  .card-value {
    font-size: 12px;
    color: var(--muted);
  }

  .progress-track {
    height: 9px;
    border-radius: 5px;
    background: var(--canvas);
    overflow: hidden;
    display: flex;
  }
  .progress-fill {
    height: 100%;
  }
  .ram-gradient {
    background: linear-gradient(90deg, var(--teal), #8ce6c6);
  }
  .storage-gradient {
    background: linear-gradient(90deg, var(--amber), #f7cb7a);
  }

  .temp-readout {
    display: flex;
    align-items: flex-end;
    gap: 14px;
  }
  .temp-value {
    font-size: 32px;
    font-weight: 600;
    line-height: 0.9;
  }
  .temp-unit {
    font-size: 15px;
    color: var(--muted);
  }
  .temp-bar-wrap {
    position: relative;
    height: 8px;
    margin-top: 4px;
  }
  .temp-neutral-bar {
    position: absolute;
    inset: 0;
    height: 100%;
    border-radius: 5px;
    background: color-mix(in srgb, var(--text) 12%, transparent);
  }
  .temp-indicator-dot {
    position: absolute;
    top: 50%;
    transform: translate(-50%, -50%);
    width: 16px;
    height: 16px;
    border-radius: 50%;
    background: var(--text);
    border: 3px solid var(--canvas);
    box-shadow: 0 1px 6px rgba(0, 0, 0, 0.5);
    transition: left 0.3s ease;
  }
  .temp-labels {
    display: flex;
    justify-content: space-between;
    font-size: 10px;
    color: var(--dim);
    font-family: var(--mono);
    margin-top: -4px;
  }
  .temp-note {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }

  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 16px;
  }
  .card-sub-label {
    font-size: 11px;
    color: var(--muted);
  }
  .grid-card-value {
    font-size: 14px;
    font-weight: 600;
  }
  .hdr-tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--teal);
  }
  .hdr-tag .msr {
    font-size: 14px;
  }

  .top-memory-section {
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-top: 2px;
  }
  .consumers-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
  }
  .consumer-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 12px 14px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .consumer-details {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .consumer-name {
    font-size: 13px;
    font-weight: 500;
    color: var(--text);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .consumer-pkg {
    font-size: 10px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .risk-badge {
    font-size: 11px;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 6px;
    flex: none;
  }
  .risk-badge.caution {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .risk-badge.blocked {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .risk-badge.safe {
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
  }
  .consumer-mb {
    font-size: 12px;
    color: var(--text-soft);
    flex: none;
  }
  .row-actions {
    display: flex;
    gap: 4px;
    flex: none;
  }
  .rowbtn {
    width: 34px;
    height: 34px;
    border-radius: 10px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    color: var(--text-soft);
    display: grid;
    place-items: center;
    cursor: pointer;
    padding: 0;
  }
  .rowbtn .msr {
    font-size: 18px;
  }
  .rowbtn:disabled {
    opacity: 0.4;
    cursor: not-allowed;
  }
  .consumers-note {
    margin: 2px 0 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }
</style>
