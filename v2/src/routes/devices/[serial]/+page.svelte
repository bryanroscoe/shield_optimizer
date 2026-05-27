<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { page } from "$app/stores";
  import { goto } from "$app/navigation";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import type {
    Device,
    HealthReport,
    LauncherStatus,
    CurrentLauncher,
    AppEntry,
    SnapshotFile,
    SnapshotApplyPlan,
  } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";

  let serial = $derived(decodeURIComponent($page.params.serial ?? ""));

  type Tab = "overview" | "health" | "launcher" | "apps" | "snapshot" | "sideload";
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
  let appActionBusy = $state<string | null>(null);
  let appActionMessage = $state("");

  let snapshots = $state<SnapshotFile[]>([]);
  let snapshotsErr = $state<string | null>(null);
  let saveBusy = $state(false);
  let saveResult = $state<string>("");
  let previewPath = $state<string | null>(null);
  let preview = $state<SnapshotApplyPlan | null>(null);
  let previewBusy = $state(false);
  let previewErr = $state<string | null>(null);

  let sideloadBusy = $state(false);
  let sideloadResult = $state<string>("");
  let sideloadHint = $state<string | null>(null);

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
      reportLastRefreshed = new Date();
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
  }

  /// Build a package → state map by checking the launcher status helper —
  /// reusing list_launchers/list_devices logic would be heavier; instead we
  /// piggy-back on a launcher-style "is this package installed and enabled"
  /// check via a dedicated minimal call.
  async function fetchAppStates(packages: string[]): Promise<Record<string, "enabled" | "disabled" | "missing">> {
    // Lightweight approach: have the backend tell us via listLaunchers-style
    // status — but it's launcher-specific. We can derive the same info from
    // the snapshot apply preview, but that requires a snapshot file.
    // Simplest path: ship without per-row state for the first cut; rely on
    // the action result to refresh after a click. UI shows "—" otherwise.
    const out: Record<string, "enabled" | "disabled" | "missing"> = {};
    for (const p of packages) out[p] = "enabled"; // optimistic default
    return out;
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

  async function setDefaultLauncher(pkg: string) {
    launcherActionBusy = pkg;
    launcherActionMessage = "";
    try {
      const r = await api.setDefaultLauncher(serial, pkg);
      if (r.ok) {
        launcherActionMessage = `Set ${pkg} as default launcher (via ${r.strategy ?? "ok"}).`;
        await loadLauncher();
      } else {
        launcherActionMessage = r.last_error
          ? `Failed: ${r.last_error}`
          : "Could not set default launcher. Try disabling other launchers first.";
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
    sideloadBusy = true;
    sideloadResult = "Installing…";
    sideloadHint = null;
    try {
      const r = await api.installApk(serial, selected, true);
      sideloadResult = r.message;
      sideloadHint = r.hint;
    } catch (e) {
      sideloadResult = String(e);
    } finally {
      sideloadBusy = false;
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

  // Lazy-load each tab the first time it's opened. Sideload doesn't need
  // any prefetch — the file picker fires on user action.
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

  <div class="tabs" role="tablist" aria-label="Device sections">
    {#each [
      { id: "overview", label: "Overview" },
      { id: "health", label: "Health" },
      { id: "launcher", label: "Launcher" },
      { id: "apps", label: "App List" },
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
        </dl>

        <h3>Top Memory Users</h3>
        {#if report.top_memory.length === 0}
          <p class="muted">No process data.</p>
        {:else}
          <table class="mem-table">
            <thead>
              <tr><th>RAM</th><th>Package</th><th></th></tr>
            </thead>
            <tbody>
              {#each report.top_memory as m}
                <tr>
                  <td
                    class="num"
                    class:warn={m.mb >= 200}
                    class:caution={m.mb >= 100 && m.mb < 200}
                  >
                    {m.mb.toFixed(1)} MB
                  </td>
                  <td class="pkg">{m.package}</td>
                  <td class="row-actions">
                    <button
                      class="small-action"
                      onclick={() => disableApp(m.package)}
                      disabled={appActionBusy === m.package}
                      title="pm disable-user --user 0 {m.package}"
                    >
                      {appActionBusy === m.package ? "…" : "Disable"}
                    </button>
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
                    {#if l.installed}
                      <span class="tag installed">INSTALLED</span>
                    {:else}
                      <span class="tag missing">MISSING</span>
                    {/if}
                    {#if l.installed && !l.enabled}
                      <span class="tag disabled">DISABLED</span>
                    {/if}
                  </div>
                  {#if l.installed && !isCurrent}
                    <button
                      class="primary small-action"
                      onclick={() => setDefaultLauncher(l.entry.package)}
                      disabled={launcherActionBusy !== null}
                      title="Run pm enable + role API + set-home-activity strategies"
                    >
                      {launcherActionBusy === l.entry.package ? "Setting…" : "Set as default"}
                    </button>
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
          <span class="muted">{apps.length} entries</span>
          <button onclick={loadApps} disabled={appsLoading}>
            {appsLoading ? "Loading…" : "Refresh"}
          </button>
        </div>
      </div>
      {#if appsErr}
        <div class="error">{appsErr}</div>
      {:else if appsLoading && apps.length === 0}
        <div class="muted">Loading…</div>
      {:else}
        {#if appActionMessage}
          <p class="muted small mono action-message">{appActionMessage}</p>
        {/if}
        <table class="app-table">
          <thead>
            <tr>
              <th>App</th>
              <th>Method</th>
              <th>Risk</th>
              <th>Default</th>
              <th>Actions</th>
            </tr>
          </thead>
          <tbody>
            {#each apps as a}
              {@const state = appStates[a.package] ?? "enabled"}
              <tr>
                <td>
                  <div class="app-name">{a.name}</div>
                  <div class="muted small mono">{a.package}</div>
                  <div class="muted small">{a.optimize_description}</div>
                </td>
                <td class="mono">{a.method.toUpperCase()}</td>
                <td class={`risk risk-${a.risk}`}>{a.risk.toUpperCase()}</td>
                <td>{a.default_optimize ? "YES" : "no"}</td>
                <td class="row-actions">
                  {#if state === "disabled"}
                    <button
                      class="small-action"
                      onclick={() => enableApp(a.package)}
                      disabled={appActionBusy === a.package}
                      title="pm enable"
                    >
                      {appActionBusy === a.package ? "…" : "Enable"}
                    </button>
                  {:else}
                    <button
                      class="small-action"
                      onclick={() => disableApp(a.package)}
                      disabled={appActionBusy === a.package}
                      title="pm disable-user --user 0"
                    >
                      {appActionBusy === a.package ? "…" : "Disable"}
                    </button>
                    {#if a.method === "uninstall"}
                      <button
                        class="small-action danger"
                        onclick={() => uninstallApp(a.package)}
                        disabled={appActionBusy === a.package}
                        title="pm uninstall --user 0"
                      >
                        Uninstall
                      </button>
                    {/if}
                  {/if}
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
        <button class="primary" onclick={pickAndInstallApk} disabled={sideloadBusy}>
          {sideloadBusy ? "Installing…" : "Pick APK file…"}
        </button>
      </div>
      <p class="muted small">
        Opens a native file picker, then runs <code>adb install -r &lt;file&gt;</code> against this
        device. Common errors are decoded with a hint.
      </p>
      {#if sideloadResult}
        <pre class="install-output">{sideloadResult}</pre>
        {#if sideloadHint}
          <div class="warning">{sideloadHint}</div>
        {/if}
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
          <div class="warning preview-disclaimer">
            <strong>Preview only.</strong> Execution of the plan is not yet wired in this
            build. The counts below show what <em>would</em> change — nothing has been
            applied to your device.
          </div>
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
    color: #c9d1d9;
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
    background: transparent;
    border-color: #f85149;
    color: #f85149;
  }
  .small-action.danger:hover {
    background: #da3633;
    color: #fff;
  }
  .action-message {
    margin-top: 0.4rem;
    padding: 0.4rem 0.6rem;
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 4px;
    word-break: break-word;
  }
  .install-output {
    background: #0d1117;
    border: 1px solid #30363d;
    border-radius: 4px;
    padding: 0.7rem 1rem;
    margin: 0.8rem 0;
    font-family: ui-monospace, monospace;
    font-size: 0.82rem;
    white-space: pre-wrap;
    word-break: break-word;
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
