<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { Safety } from "../lib/types";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";

  let { navigate, back }: {
    navigate: (screen: Screen) => void;
    back: () => void;
  } = $props();

  // Safety tags come from the backend `safety_info` command — the one audited
  // classifier. No inline package→risk table (that violated the "one detection
  // function" invariant and could disagree with the engine).
  let safetyMap = $state<Record<string, Safety>>({});
  let safetyRequest = 0;

  async function loadSafety() {
    const request = ++safetyRequest;
    const serial = session.serial;
    const pkgs = (session.health?.top_memory ?? []).slice(0, 8).map((m) => m.package);
    safetyMap = {};
    if (pkgs.length === 0) {
      return;
    }
    try {
      const pairs = await Promise.all(
        pkgs.map(async (p) => [p, await api.safetyInfo(p)] as const),
      );
      if (request !== safetyRequest || serial !== session.serial) return;
      const map: Record<string, Safety> = {};
      for (const [p, s] of pairs) map[p] = s;
      safetyMap = map;
    } catch {
      // Leave tags unresolved rather than inventing them.
    }
  }

  onMount(async () => {
    await session.loadHealth();
    loadSafety();
  });

  async function refresh() {
    await session.loadHealth(true);
    loadSafety();
  }

  const health = $derived(session.health);

  // --- Memory (real values or "—"; no fabricated 2966/1512 fallbacks) ---
  const totalRam = $derived(health?.ram?.total_mb ?? null);
  const freeRam = $derived(health?.ram?.free_mb ?? null);
  const ramText = $derived(
    freeRam != null && totalRam != null ? `${freeRam} / ${totalRam} MB free` : "—",
  );
  const ramFreePercent = $derived(
    freeRam != null && totalRam ? Math.round((freeRam / totalRam) * 100) : 0,
  );

  // --- Storage ---
  const storageText = $derived(
    health?.storage?.used != null && health?.storage?.total != null
      ? `${health.storage.used} / ${health.storage.total} used`
      : "—",
  );
  const storageUsedPercent = $derived(health?.storage?.used_percent ?? 0);

  // --- Temperature ---
  const tempC = $derived(health?.temperature_c ?? null);
  const tempStatus = $derived(
    tempC == null ? "—" : tempC >= 80 ? "THROTTLE" : tempC >= 65 ? "WARM" : "NORMAL",
  );
  const tempPercent = $derived(
    tempC == null ? 0 : Math.min(100, Math.max(0, ((tempC - 30) / 60) * 100)),
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
    return pkg.split(".").pop() || pkg;
  }

  function safetyTag(pkg: string): { label: string; cls: string } | null {
    const s = safetyMap[pkg];
    if (!s) return null;
    if (s.kind === "never_disable") return { label: "keep", cls: "keep" };
    if (s.kind === "caution") return { label: "caution", cls: "caution" };
    return { label: "safe", cls: "safe" };
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
    <button class="live-refresh-btn" onclick={refresh} disabled={session.healthLoading}>
      <span class="msr" class:blink={session.healthLoading}>refresh</span>live
    </button>
  </div>

  {#if session.healthLoading && !health}
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
          <div class="progress-fill ram-gradient" style="width: {ramFreePercent}%"></div>
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
          {#if tempC != null}
            <span class="status-badge" class:warm={tempStatus === "WARM"} class:throttle={tempStatus === "THROTTLE"}>
              {tempStatus}
            </span>
          {/if}
        </div>
        {#if tempC != null}
          <div class="temp-readout">
            <span class="mono temp-value">{Math.round(tempC)}<span class="temp-unit"> °C</span></span>
          </div>
          <div class="temp-bar-wrap">
            <div class="temp-gradient-bar"></div>
            <span class="temp-indicator-dot" style="left: {tempPercent}%"></span>
          </div>
          <div class="temp-labels">
            <span>COOL</span>
            <span>WARM</span>
            <span>THROTTLE 85°</span>
          </div>
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
              {@const tag = safetyTag(consumer.package)}
              <div class="consumer-row">
                <div class="consumer-details">
                  <span class="consumer-name">{appLabel(consumer.package)}</span>
                  <span class="mono consumer-pkg">{consumer.package}</span>
                </div>
                {#if tag}
                  <span class="risk-badge {tag.cls}">{tag.label}</span>
                {/if}
                <span class="mono consumer-mb">{Math.round(consumer.mb)} MB</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}
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
  .live-refresh-btn .msr {
    font-size: 15px;
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

  .status-badge {
    font-size: 11px;
    font-weight: 700;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
    padding: 4px 9px;
    border-radius: 7px;
  }
  .status-badge.warm {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .status-badge.throttle {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
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
  .temp-gradient-bar {
    position: absolute;
    inset: 0;
    height: 100%;
    border-radius: 5px;
    background: linear-gradient(90deg, var(--teal), var(--amber) 62%, var(--danger));
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
    gap: 12px;
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
    text-transform: capitalize;
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
    text-transform: capitalize;
    flex: none;
  }
  .risk-badge.caution {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .risk-badge.keep {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
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
</style>
