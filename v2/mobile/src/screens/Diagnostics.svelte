<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import { tierOf } from "../lib/safety";
  import type { ResourceSample, Safety } from "../lib/types";
  import { formatSupport, matchContentLabel, type MediaCapabilities } from "../../../shared/media";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";

  let { navigate, back }: {
    navigate: (screen: Screen) => void;
    back: () => void;
  } = $props();

  // Safety tags come from the backend `safety_info` command — the one audited
  // classifier. No inline package→risk table (that violated the "one detection
  // function" invariant and could disagree with the engine).
  let safetyMap = $state<Record<string, Safety>>({});
  let safetyLoading = $state(false);
  let safetyFailed = $state(false);
  let safetyRetry = $state(0);
  let safetyRequest = 0;
  // Real catalog names (app_list_for_device). Missing package = no name, and we
  // then show the package itself rather than inventing one from its last dot
  // segment ("…youtube.tv" is not an app called "Tv").
  let catalogNames = $state<Record<string, string>>({});
  let catalogRequest = 0;
  let destroyed = false;
  let playback = $state<MediaCapabilities | null>(null);
  let playbackError = $state("");
  let playbackBusy = $state(false);
  let playbackRequest = 0;
  let resources = $state<ResourceSample | null>(null);
  let resourceError = $state("");
  let resourceBusy = $state(false);
  let resourceRequest = 0;

  $effect(() => {
    void session.serial;
    void session.generation;
    void session.liveness;
    ++playbackRequest;
    ++resourceRequest;
    playback = null;
    resources = null;
    playbackError = "";
    resourceError = "";
    playbackBusy = false;
    resourceBusy = false;
  });

  async function readPlayback() {
    if (playbackBusy || resourceBusy || !session.isConnected) return;
    const serial = session.serial;
    const generation = session.generation;
    const request = ++playbackRequest;
    const current = () => !destroyed && request === playbackRequest && serial === session.serial && generation === session.generation && session.isConnected;
    playbackBusy = true;
    playbackError = "";
    playback = null;
    try {
      const report = await api.mediaReport(serial);
      if (current()) playback = report;
    } catch (error) {
      if (current()) playbackError = String(error);
    } finally {
      if (current()) playbackBusy = false;
    }
  }

  async function sampleResources() {
    if (resourceBusy || playbackBusy || !session.isConnected) return;
    const serial = session.serial;
    const generation = session.generation;
    const request = ++resourceRequest;
    const current = () => !destroyed && request === resourceRequest && serial === session.serial && generation === session.generation && session.isConnected;
    resourceBusy = true;
    resourceError = "";
    resources = null;
    try {
      const sample = await api.resourceSample(serial);
      if (current()) resources = sample;
    } catch (error) {
      if (current()) resourceError = String(error);
    } finally {
      if (current()) resourceBusy = false;
    }
  }

  const rate = (value: number | null) => value == null ? "Unavailable" : `${(value / 1024).toFixed(1)} KiB/s`;

  let liveMode = $state(false);
  let liveTimer: ReturnType<typeof setInterval> | undefined;

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
    const device = session.connectedDevice;
    const liveness = session.liveness;
    const generation = session.generation;
    void safetyRetry;
    const request = ++safetyRequest;
    const serial = device?.serial ?? "";
    safetyMap = {};
    safetyFailed = false;
    safetyLoading = pkgs.length > 0 && liveness === "live";
    if (pkgs.length === 0 || !device || liveness !== "live") return;
    const current = () =>
      !destroyed &&
      request === safetyRequest &&
      serial === session.serial &&
      generation === session.generation &&
      session.isConnected;
    Promise.all(pkgs.map(async (p) => [p, await api.safetyInfo(p)] as const))
      .then((pairs) => {
        if (!current()) return;
        const map: Record<string, Safety> = {};
        for (const [p, s] of pairs) map[p] = s;
        safetyMap = map;
      })
      .catch(() => {
        if (!current()) return;
        safetyMap = {};
        safetyFailed = true;
      })
      .finally(() => {
        if (current()) safetyLoading = false;
      });
  });

  $effect(() => {
    const device = session.connectedDevice;
    const liveness = session.liveness;
    const generation = session.generation;
    const request = ++catalogRequest;
    catalogNames = {};
    if (!device || liveness !== "live") return;
    const serial = device.serial;
    api
      .appListForDevice(device.device_type)
      .then((entries) => {
        if (
          destroyed ||
          request !== catalogRequest ||
          serial !== session.serial ||
          generation !== session.generation ||
          !session.isConnected
        ) return;
        const map: Record<string, string> = {};
        for (const e of entries) map[e.package] = e.name;
        catalogNames = map;
      })
      .catch(() => {
        if (request === catalogRequest) catalogNames = {};
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
    destroyed = true;
    ++safetyRequest;
    ++catalogRequest;
    ++playbackRequest;
    ++resourceRequest;
    stopLive();
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

  // --- About this TV ---
  // Every value here comes from the shared core's one device-profile read.
  // A property the TV didn't answer arrives as an empty string; show an em
  // dash rather than guessing or hiding the row, so the reading is always
  // traceable to something the device actually reported.
  const deviceProps = $derived(session.connectedDevice?.properties ?? null);
  const shown = (value: string | null | undefined): string => {
    const trimmed = value?.trim() ?? "";
    return trimmed === "" || trimmed === "unknown" ? "—" : trimmed;
  };
  const androidText = $derived.by(() => {
    const release = shown(deviceProps?.android_release);
    const sdk = shown(deviceProps?.sdk_level);
    if (release === "—") return sdk === "—" ? "—" : `API ${sdk}`;
    return sdk === "—" ? release : `${release} (API ${sdk})`;
  });
  const totalRamText = $derived(
    totalRam != null ? `${Math.round(totalRam)} MB` : "—",
  );
  const totalStorageText = $derived(shown(health?.storage?.total));
  const aboutRows = $derived([
    { label: "Android version", value: androidText },
    { label: "Manufacturer", value: shown(deviceProps?.manufacturer || deviceProps?.brand) },
    { label: "Model", value: shown(deviceProps?.model) },
    { label: "Codename", value: shown(deviceProps?.device_codename) },
    { label: "Chipset", value: shown(deviceProps?.board_platform) },
    { label: "Build ID", value: shown(deviceProps?.build_id) },
    { label: "Total RAM", value: totalRamText },
    { label: "Total storage", value: totalStorageText },
    // The same id saved-TV matching trusts. Shown so a user can tell two
    // identical TVs apart, and so a missing id is visible rather than silent.
    { label: "Hardware ID", value: shown(deviceProps?.serial_number) },
  ]);

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

      <!-- About this TV -->
      <div class="diagnostic-card">
        <div class="card-header">
          <span class="card-title">About this TV</span>
        </div>
        {#if deviceProps}
          <dl class="about-list">
            {#each aboutRows as row (row.label)}
              <dt>{row.label}</dt>
              <dd class="mono">{row.value}</dd>
            {/each}
          </dl>
        {:else}
          <p class="lede empty">
            The TV hasn't reported its details. Reconnect, and approve the
            "Allow debugging?" prompt on the TV if it appears.
          </p>
        {/if}
      </div>

      <!-- Top Memory Consumers -->
      <div class="top-memory-section">
        <span class="section-label">Top memory consumers</span>
        <p class="consumers-note">
          These are process names, so the app behind each one isn't confirmed. Inspection only.
        </p>
        {#if safetyFailed}
          <div class="stale-warning" role="alert">
            <span class="msr">warning</span>
            <span>Couldn't check these names against the safety list.</span>
            <button class="retry-link" onclick={() => ++safetyRetry}>Retry</button>
          </div>
        {/if}
        {#if (health.top_memory?.length ?? 0) === 0}
          <p class="lede empty">No process memory data available.</p>
        {:else}
          <div class="consumers-list">
            {#each health.top_memory.slice(0, 8) as consumer (consumer.package)}
              {@const tier = tierOf(safetyMap[consumer.package])}
              <div class="consumer-row">
                <div class="consumer-details">
                  <span class="mono consumer-name">{consumer.package}</span>
                  {#if catalogNames[consumer.package]}
                    <span class="consumer-pkg">Looks like {catalogNames[consumer.package]}</span>
                  {/if}
                </div>
                {#if tier}
                  <span class="risk-badge {tier.cls}" title="Rule lookup for the reported name only">
                    {tier.label}
                  </span>
                {:else if safetyLoading}
                  <span class="risk-badge pending">Checking safety…</span>
                {:else}
                  <span class="risk-badge pending">Safety unavailable</span>
                {/if}
                <span class="mono consumer-mb">{Math.round(consumer.mb)} MB</span>
              </div>
            {/each}
          </div>
        {/if}
      </div>
    </div>
  {/if}

  {#if session.connectedDevice}
    <div class="diagnostics-content reports">
      <section class="diagnostic-card" aria-label="Playback report">
        <div class="card-header">
          <span class="card-title">Playback report</span>
          <button class="retry-link" disabled={playbackBusy || resourceBusy || !session.isConnected} onclick={readPlayback}>{playbackBusy ? "Reading…" : "Read playback report"}</button>
        </div>
        <p class="temp-note">Advertised configuration, not a playback test. Actual decoding and passthrough depend on the app, content and connected display or receiver.</p>
        {#if playbackError}<p class="error" role="alert">{playbackError}</p>{/if}
        {#if playback}
          <dl class="about-list">
            {#each playback.video as format (format.mime)}
              <dt>{format.label}</dt><dd>{formatSupport(format).label}</dd>
            {/each}
            <dt>Advertised HDR</dt><dd>{playback.hdr_types.length ? playback.hdr_types.join(", ") : "Unknown / not reported"}</dd>
            <dt>Display modes</dt><dd>{playback.modes.length ? playback.modes.map((mode) => `${mode.width}×${mode.height} @ ${mode.fps} Hz${mode.active ? " (active)" : ""}`).join(", ") : "Unknown / not reported"}</dd>
            <dt>Surround policy</dt><dd>{playback.audio.mode === "unset" ? "Default / unset" : playback.audio.mode}</dd>
            <dt>Manual formats</dt><dd>{playback.audio.enabled_formats.join(", ") || "None / unset"}</dd>
            <dt>Frame-rate policy</dt><dd>{playback.match_content_frame_rate == null ? "Default / unset" : (matchContentLabel[playback.match_content_frame_rate] ?? `Unknown (${playback.match_content_frame_rate})`)}</dd>
          </dl>
          {#each playback.verdicts as verdict}
            <p class="temp-note"><strong>{verdict.title}</strong> {verdict.detail}</p>
          {/each}
        {/if}
      </section>
      <section class="diagnostic-card" aria-label="Resource sample">
        <div class="card-header">
          <span class="card-title">CPU &amp; network</span>
          <button class="retry-link" disabled={resourceBusy || playbackBusy || !session.isConnected} onclick={sampleResources}>{resourceBusy ? "Sampling…" : "Sample resources"}</button>
        </div>
        <p class="temp-note">One device-side sample per tap. Interfaces are shown separately because VPN traffic can also appear on the physical interface.</p>
        {#if resourceError}<p class="error" role="alert">{resourceError}</p>{/if}
        {#if resources}
          <dl class="about-list">
            <dt>CPU use</dt><dd>{resources.cpu_percent == null ? "Unavailable" : `${resources.cpu_percent.toFixed(1)}%`}</dd>
            <dt>Sample duration</dt><dd>{resources.interval_ms == null ? "Unavailable" : `${(resources.interval_ms / 1000).toFixed(2)} s`}</dd>
            {#each resources.interfaces as network (network.name)}
              <dt>{network.name}</dt><dd>Receive {rate(network.rx_bytes_per_s)} · Send {rate(network.tx_bytes_per_s)}</dd>
            {/each}
          </dl>
          {#if resources.interfaces.length === 0}<p class="temp-note">No network counters available.</p>{/if}
        {/if}
      </section>
    </div>
  {/if}
</div>

<style>
  .reports {
    margin-top: 16px;
  }
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
  .about-list {
    display: grid;
    grid-template-columns: auto 1fr;
    gap: 8px 16px;
    margin: 0;
    align-items: baseline;
  }
  .about-list dt {
    font-size: 12px;
    color: var(--muted);
  }
  .about-list dd {
    margin: 0;
    font-size: 12px;
    text-align: right;
    overflow-wrap: anywhere;
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
  .risk-badge.safe {
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
  }
  .risk-badge.caution {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .risk-badge.blocked {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .risk-badge.unknown,
  .risk-badge.pending {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }
  .consumer-mb {
    font-size: 12px;
    color: var(--text-soft);
    flex: none;
  }
  .consumers-note {
    margin: 2px 0 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }
  .retry-link {
    margin-left: auto;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-weight: 700;
    cursor: pointer;
  }
</style>
