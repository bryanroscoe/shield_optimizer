<script lang="ts">
  import { onMount } from "svelte";
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { api } from "$lib/api";
  import type { Device, HealthReport, LauncherStatus, CurrentLauncher, AppEntry, SnapshotFile, SnapshotApplyPlan } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";

  let serial = $derived(decodeURIComponent($page.params.serial ?? ""));

  type Tab = "overview" | "health" | "launcher" | "apps" | "snapshot";
  let activeTab = $state<Tab>("overview");

  let device = $state<Device | null>(null);
  let deviceErr = $state<string | null>(null);

  let report = $state<HealthReport | null>(null);
  let reportLoading = $state(false);
  let reportErr = $state<string | null>(null);

  let launchers = $state<LauncherStatus[]>([]);
  let currentLauncher = $state<CurrentLauncher | null>(null);
  let channelDisabled = $state<boolean | null>(null);
  let launcherLoading = $state(false);
  let launcherErr = $state<string | null>(null);

  let apps = $state<AppEntry[]>([]);
  let appsLoading = $state(false);
  let appsErr = $state<string | null>(null);

  let snapshots = $state<SnapshotFile[]>([]);
  let snapshotsErr = $state<string | null>(null);
  let saveBusy = $state(false);
  let saveResult = $state<string>("");
  let previewPath = $state<string | null>(null);
  let preview = $state<SnapshotApplyPlan | null>(null);
  let previewBusy = $state(false);
  let previewErr = $state<string | null>(null);

  async function loadDevice() {
    deviceErr = null;
    try {
      device = await api.deviceProfile(serial);
    } catch (e) {
      deviceErr = String(e);
    }
  }

  async function loadHealth() {
    reportLoading = true;
    reportErr = null;
    try {
      report = await api.healthReport(serial);
    } catch (e) {
      reportErr = String(e);
    } finally {
      reportLoading = false;
    }
  }

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
      apps = await api.appListForDevice(device.device_type);
    } catch (e) {
      appsErr = String(e);
    } finally {
      appsLoading = false;
    }
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
    try {
      preview = await api.previewApply(serial, path);
    } catch (e) {
      previewErr = String(e);
    } finally {
      previewBusy = false;
    }
  }

  // Lazy-load each tab the first time it's opened.
  $effect(() => {
    if (activeTab === "health" && report === null && !reportLoading && !reportErr) loadHealth();
    if (activeTab === "launcher" && launchers.length === 0 && !launcherLoading && !launcherErr) loadLauncher();
    if (activeTab === "apps" && apps.length === 0 && !appsLoading && !appsErr) loadApps();
    if (activeTab === "snapshot" && snapshots.length === 0) loadSnapshots();
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
    <h1>{device.name}</h1>
    <div class="device-meta">
      <span>{deviceTypeLabel(device.device_type)}</span>
      {#if device.model}<span>· {device.model}</span>{/if}
      <span class="serial">· {device.serial}</span>
      {#if device.properties?.android_release}
        <span>· Android {device.properties.android_release}</span>
      {/if}
    </div>
  </header>

  <nav class="tabs">
    <button class:active={activeTab === "overview"} onclick={() => (activeTab = "overview")}>Overview</button>
    <button class:active={activeTab === "health"} onclick={() => (activeTab = "health")}>Health</button>
    <button class:active={activeTab === "launcher"} onclick={() => (activeTab = "launcher")}>Launcher</button>
    <button class:active={activeTab === "apps"} onclick={() => (activeTab = "apps")}>App List</button>
    <button class:active={activeTab === "snapshot"} onclick={() => (activeTab = "snapshot")}>Snapshot</button>
  </nav>

  {#if activeTab === "overview"}
    <section class="card">
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
    </section>
  {:else if activeTab === "health"}
    <section class="card">
      <div class="card-header">
        <h2>Health Report</h2>
        <button onclick={loadHealth} disabled={reportLoading}>
          {reportLoading ? "Loading…" : "Refresh"}
        </button>
      </div>
      {#if reportErr}
        <div class="error">{reportErr}</div>
      {:else if !report}
        <div class="muted">{reportLoading ? "Querying…" : "—"}</div>
      {:else}
        <h3>Display & Audio</h3>
        <dl class="kv">
          <dt>Resolution</dt><dd>{report.display.resolution ?? "—"}</dd>
          <dt>Refresh</dt><dd>{report.display.refresh_hz ? `${report.display.refresh_hz} Hz` : "—"}</dd>
          <dt>HDR</dt><dd>{report.display.hdr_types.length ? report.display.hdr_types.join(", ") : "SDR only"}</dd>
        </dl>

        <h3>Top Memory Users</h3>
        {#if report.top_memory.length === 0}
          <p class="muted">No process data.</p>
        {:else}
          <table class="mem-table">
            <thead>
              <tr><th>RAM</th><th>Package</th></tr>
            </thead>
            <tbody>
              {#each report.top_memory as m}
                <tr>
                  <td class="num" class:warn={m.mb >= 200} class:caution={m.mb >= 100 && m.mb < 200}>
                    {m.mb.toFixed(1)} MB
                  </td>
                  <td class="pkg">{m.package}</td>
                </tr>
              {/each}
            </tbody>
          </table>
        {/if}
      {/if}
    </section>
  {:else if activeTab === "launcher"}
    <section class="card">
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
              <li>
                <div>
                  <div class="launcher-name">{l.entry.name}</div>
                  <div class="muted small mono">{l.entry.package}</div>
                </div>
                <div class="tags">
                  {#if l.installed}
                    <span class="tag installed">INSTALLED</span>
                  {:else}
                    <span class="tag missing">MISSING</span>
                  {/if}
                  {#if l.installed && !l.enabled}
                    <span class="tag disabled">DISABLED</span>
                  {/if}
                </div>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    </section>
  {:else if activeTab === "apps"}
    <section class="card">
      <div class="card-header">
        <h2>App List for {deviceTypeLabel(device.device_type)}</h2>
        <span class="muted">{apps.length} entries</span>
      </div>
      {#if appsErr}
        <div class="error">{appsErr}</div>
      {:else if appsLoading && apps.length === 0}
        <div class="muted">Loading…</div>
      {:else}
        <table class="app-table">
          <thead>
            <tr><th>App</th><th>Method</th><th>Risk</th><th>Default</th></tr>
          </thead>
          <tbody>
            {#each apps as a}
              <tr>
                <td>
                  <div class="app-name">{a.name}</div>
                  <div class="muted small mono">{a.package}</div>
                  <div class="muted small">{a.optimize_description}</div>
                </td>
                <td class="mono">{a.method.toUpperCase()}</td>
                <td class={`risk risk-${a.risk}`}>{a.risk.toUpperCase()}</td>
                <td>{a.default_optimize ? "YES" : "no"}</td>
              </tr>
            {/each}
          </tbody>
        </table>
      {/if}
    </section>
  {:else if activeTab === "snapshot"}
    <section class="card">
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
            <li><strong>{preview.packages_to_disable.length}</strong> packages would be disabled</li>
            <li><strong>{preview.packages_already_disabled.length}</strong> already disabled (no-op)</li>
            <li><strong>{preview.packages_not_installed.length}</strong> not present on device</li>
            <li>Launcher: <code>{preview.launcher_to_set ?? "(unchanged)"}</code></li>
            <li><strong>{Object.keys(preview.settings_to_write).length}</strong> settings would be written</li>
          </ul>
          <p class="muted small">Execution of the plan is not yet wired in this build — preview only.</p>
        </div>
      {/if}
    </section>
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
    color: #7d8590;
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
    border-bottom: 1px solid #30363d;
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
    color: #58a6ff;
    border-bottom-color: #58a6ff;
  }
  .card {
    background: #161b22;
    border: 1px solid #30363d;
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
    color: #c9d1d9;
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
    color: #7d8590;
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
    border-bottom: 1px solid #21262d;
  }
  th {
    color: #7d8590;
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
  td.num.warn { color: #f85149; }
  td.num.caution { color: #d29922; }
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
    border-bottom: 1px solid #21262d;
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
  .tag.installed { background: #1b3d2c; color: #3fb950; }
  .tag.missing { background: #3d3d3d; color: #aaa; }
  .tag.disabled { background: #5d3b1b; color: #d29922; }
  .warning {
    background: #3d2f00;
    border: 1px solid #5d4a00;
    color: #d29922;
    padding: 0.7rem 1rem;
    border-radius: 6px;
    margin: 0.8rem 0;
    font-size: 0.9rem;
  }
  .warning code {
    background: #0d1117;
    padding: 0.1rem 0.3rem;
    border-radius: 3px;
  }
  .error {
    background: #5d1b1b;
    color: #ff8a80;
    padding: 0.7rem 1rem;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  .preview-box {
    margin-top: 1rem;
    padding: 1rem;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 6px;
  }
  .preview-box ul {
    margin: 0.4rem 0;
    padding-left: 1.2rem;
  }
  code {
    background: #0d1117;
    border: 1px solid #30363d;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
  }
</style>
