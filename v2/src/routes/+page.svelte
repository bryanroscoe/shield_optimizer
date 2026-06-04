<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import type { Device, DeviceReport } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";

  let devices = $state<Device[]>([]);

  /// Sort priority: authorized first (status === "device"), then unauthorized
  /// (the user can act on them via the inline guidance), then offline. Tiebreak
  /// by display name so the order doesn't shuffle on refresh.
  const STATUS_ORDER: Record<string, number> = { device: 0, unauthorized: 1, offline: 2 };
  let sortedDevices = $derived(
    [...devices].sort((a, b) => {
      const sa = STATUS_ORDER[a.status] ?? 9;
      const sb = STATUS_ORDER[b.status] ?? 9;
      if (sa !== sb) return sa - sb;
      return a.name.localeCompare(b.name);
    }),
  );
  let loading = $state(true);
  let error = $state<string | null>(null);

  let connectAddress = $state("");
  let connectBusy = $state(false);
  let connectMessage = $state("");

  let scanBusy = $state(false);
  let scanMessage = $state("");

  let pairOpen = $state(false);
  let pairAddress = $state("");
  let pairPin = $state("");
  let pairBusy = $state(false);
  let pairMessage = $state("");

  let restartBusy = $state(false);
  let restartMessage = $state("");

  let reportBusy = $state(false);
  let reportData = $state<DeviceReport[] | null>(null);
  let reportError = $state<string | null>(null);

  // Triggers the install-platform-tools button rather than a generic error pane.
  let adbMissing = $state(false);
  let installBusy = $state(false);
  let installMessage = $state("");

  async function refresh() {
    loading = true;
    error = null;
    try {
      // Structured probe first so we render the install pane on a clean
      // signal instead of substring-matching a free-form error message.
      const status = await api.adbStatus();
      if (!status.available) {
        adbMissing = true;
        devices = [];
        return;
      }
      adbMissing = false;
      devices = await api.listDevices();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  async function downloadAdb() {
    installBusy = true;
    installMessage = "Downloading platform-tools from Google… (~12 MB)";
    try {
      const r = await api.installAdb();
      installMessage = r.message;
      if (r.ok) {
        await refresh();
      }
    } catch (e) {
      installMessage = String(e);
    } finally {
      installBusy = false;
    }
  }

  async function connect() {
    if (!connectAddress.trim()) return;
    connectBusy = true;
    connectMessage = "";
    try {
      const r = await api.connectDevice(connectAddress.trim());
      connectMessage = r.message.trim();
      if (r.ok) {
        connectAddress = "";
        await refresh();
      }
    } catch (e) {
      connectMessage = String(e);
    } finally {
      connectBusy = false;
    }
  }

  async function scan() {
    scanBusy = true;
    scanMessage = "Scanning local network…";
    try {
      const r = await api.scanNetwork();
      scanMessage = r.message;
      // Always refresh: even a "failed" connect can register the device with
      // the daemon (e.g. unauthorized — waiting for on-TV approval), and the
      // list is where that state is visible.
      await refresh();
    } catch (e) {
      scanMessage = String(e);
    } finally {
      scanBusy = false;
    }
  }

  function deviceHref(d: Device): string | null {
    return d.status === "device" ? `/devices/${encodeURIComponent(d.serial)}` : null;
  }

  async function pair() {
    if (!pairAddress.trim() || pairPin.length !== 6) return;
    pairBusy = true;
    pairMessage = "";
    try {
      const r = await api.pairDevice(pairAddress.trim(), pairPin.trim());
      pairMessage = r.message;
      if (r.ok) {
        pairAddress = "";
        pairPin = "";
        await refresh();
      }
    } catch (e) {
      pairMessage = String(e);
    } finally {
      pairBusy = false;
    }
  }

  async function restartAdb() {
    if (!confirm("Restart the ADB server? All current device connections will reconnect.")) return;
    restartBusy = true;
    restartMessage = "";
    try {
      const r = await api.restartAdb();
      restartMessage = r.message;
      await refresh();
    } catch (e) {
      restartMessage = String(e);
    } finally {
      restartBusy = false;
    }
  }

  async function reportAll() {
    reportBusy = true;
    reportError = null;
    reportData = null;
    try {
      reportData = await api.reportAll();
    } catch (e) {
      reportError = String(e);
    } finally {
      reportBusy = false;
    }
  }

  // Best-effort discovery on boot: if no devices show up after the initial
  // refresh and adb is available, kick off a scan so users with already-paired
  // devices don't have to click anything. v1 behaved similarly.
  async function bootDiscovery() {
    await refresh();
    if (adbMissing) return;
    if (devices.some((d) => d.status === "device")) return;
    await scan();
  }

  onMount(bootDiscovery);
</script>

<section class="header-row">
  <h1>Devices</h1>
  <button onclick={refresh} disabled={loading}>
    {loading ? "Refreshing…" : "Refresh"}
  </button>
</section>

<section class="connect-form">
  <input
    placeholder="IP[:port] — e.g. 192.168.42.71"
    bind:value={connectAddress}
    onkeydown={(e) => e.key === "Enter" && connect()}
  />
  <button class="primary" onclick={connect} disabled={connectBusy || !connectAddress.trim()}>
    {connectBusy ? "Connecting…" : "Connect IP"}
  </button>
  <button onclick={scan} disabled={scanBusy || adbMissing} title="Scan the local /24 subnet for ADB-listening devices">
    {scanBusy ? "Scanning…" : "Scan Network"}
  </button>
  <button onclick={() => (pairOpen = !pairOpen)} disabled={adbMissing} title="Android 11+ PIN pairing flow">
    {pairOpen ? "Cancel Pair" : "Pair PIN"}
  </button>
  <button onclick={restartAdb} disabled={restartBusy || adbMissing} title="adb kill-server then start-server">
    {restartBusy ? "Restarting…" : "Restart ADB"}
  </button>
  <button onclick={reportAll} disabled={reportBusy || adbMissing} title="Run a health report against every connected device">
    {reportBusy ? "Reporting…" : "Report All"}
  </button>
  {#if connectMessage}
    <p class="connect-message muted">{connectMessage}</p>
  {/if}
  {#if scanMessage}
    <p class="connect-message muted">{scanMessage}</p>
  {/if}
  {#if restartMessage}
    <p class="connect-message muted">{restartMessage}</p>
  {/if}
</section>

{#if pairOpen}
  <section class="pair-form">
    <h3>Pair a new device</h3>
    <p class="muted small">
      On the TV: Settings → Developer options → Wireless debugging → Pair device with pairing code.
      The TV shows an IP[:port] and a 6-digit PIN.
    </p>
    <div class="pair-row">
      <input
        placeholder="IP:pair_port — e.g. 192.168.42.71:43219"
        bind:value={pairAddress}
      />
      <input
        placeholder="6-digit PIN"
        maxlength={6}
        inputmode="numeric"
        bind:value={pairPin}
      />
      <button
        class="primary"
        onclick={pair}
        disabled={pairBusy || !pairAddress.trim() || pairPin.length !== 6}
      >
        {pairBusy ? "Pairing…" : "Pair"}
      </button>
    </div>
    {#if pairMessage}
      <p class="muted small mono">{pairMessage}</p>
    {/if}
  </section>
{/if}

{#if reportData || reportError}
  <section class="report-all">
    <div class="header-row">
      <h3>Report All</h3>
      <button onclick={() => { reportData = null; reportError = null; }}>Close</button>
    </div>
    {#if reportError}
      <div class="error">{reportError}</div>
    {:else if reportData}
      {#each reportData as r}
        <div class="report-row">
          <div class="report-head">
            <strong>{r.name}</strong> <span class="muted small mono">{r.serial}</span>
          </div>
          {#if r.error}
            <p class="muted small">{r.error}</p>
          {:else if r.report}
            <ul class="report-vitals">
              <li>Temp: {r.report.temperature_c != null ? `${r.report.temperature_c.toFixed(1)}°C` : "—"}</li>
              <li>RAM: {r.report.ram.used_mb ?? "?"} / {r.report.ram.total_mb ?? "?"} MB</li>
              {#if r.report.storage.total}
                <li>Storage: {r.report.storage.used ?? "?"} / {r.report.storage.total}{#if r.report.storage.used_percent != null} ({r.report.storage.used_percent}%){/if}</li>
              {/if}
              {#if r.report.display.resolution}
                <li>Display: {r.report.display.resolution}{#if r.report.display.refresh_hz} @ {r.report.display.refresh_hz}Hz{/if}{#if r.report.display.hdr_types.length}, HDR: {r.report.display.hdr_types.join(", ")}{/if}</li>
              {/if}
              {#if r.report.audio_device}
                <li>Audio: {r.report.audio_device}</li>
              {/if}
            </ul>
          {/if}
        </div>
      {/each}
    {/if}
  </section>
{/if}

{#if adbMissing}
  <div class="install-pane">
    <h2>ADB not found on this system</h2>
    <p>
      Shield Optimizer needs Android's <code>adb</code> binary to talk to your TV.
      We can download Google's official platform-tools and install them locally —
      no system-wide changes, just a self-contained copy under your app-data folder.
    </p>
    <p class="muted small">
      Already have <code>adb</code> installed? Set <code>SHIELD_OPTIMIZER_ADB</code> to its full path and relaunch.
    </p>
    <button class="primary" onclick={downloadAdb} disabled={installBusy}>
      {installBusy ? "Installing…" : "Download platform-tools"}
    </button>
    {#if installMessage}
      <p class="install-message muted small">{installMessage}</p>
    {/if}
  </div>
{:else if error}
  <div class="error">Failed to list devices: {error}</div>
{:else if loading && devices.length === 0}
  <div class="muted">Looking for devices…</div>
{:else if devices.length === 0}
  <div class="empty">
    <h2>No devices connected.</h2>
    <p class="muted">
      Connect by IP above, or run <code>adb connect &lt;ip&gt;:5555</code> in a terminal.
    </p>
  </div>
{:else}
  <ul class="device-list">
    {#each sortedDevices as d (d.serial)}
      {@const href = deviceHref(d)}
      <li>
        {#if href}
          <a class="device-row clickable" href={href}>
            <div class="device-main">
              <div class="device-name">
                <span class="conn-tag">[{d.connection === "network" ? "NET" : "USB"}]</span>
                <span>{d.name}</span>
              </div>
              <div class="device-meta muted">
                {deviceTypeLabel(d.device_type)}
                {#if d.model}· {d.model}{/if}
                · {d.serial}
              </div>
            </div>
            <span class="chevron">›</span>
          </a>
        {:else}
          <div class="device-row" class:unauthorized={d.status === "unauthorized"}>
            <div class="device-main">
              <div class="device-name">
                <span class="conn-tag">[{d.connection === "network" ? "NET" : "USB"}]</span>
                <span>{d.name}</span>
                {#if d.status === "unauthorized"}
                  <span class="status-tag unauthorized">UNAUTHORIZED</span>
                {:else if d.status === "offline"}
                  <span class="status-tag offline">OFFLINE</span>
                {/if}
              </div>
              <div class="device-meta muted">
                {deviceTypeLabel(d.device_type)}
                {#if d.model}· {d.model}{/if}
                · {d.serial}
              </div>
              {#if d.status === "unauthorized"}
                <div class="unauthorized-help">
                  <strong>This device needs to be authorized:</strong>
                  <ol>
                    <li>Look at the TV — there should be an <em>"Allow USB debugging?"</em> dialog.</li>
                    <li>Check <em>"Always allow from this computer"</em>.</li>
                    <li>Click <em>Allow</em>.</li>
                    <li>Click Refresh above.</li>
                  </ol>
                  <p class="muted small">
                    If you don't see the dialog, run <code>adb disconnect {d.serial}</code> from a terminal,
                    then on the TV go to Developer options → Revoke USB debugging authorizations, and reconnect.
                  </p>
                </div>
              {/if}
            </div>
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

<style>
  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  .connect-form {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-bottom: 1.5rem;
    flex-wrap: wrap;
  }
  .connect-form input {
    flex: 1;
    min-width: 240px;
  }
  .connect-message {
    flex-basis: 100%;
    margin: 0.4rem 0 0;
    font-size: 0.85rem;
    font-family: ui-monospace, SFMono-Regular, monospace;
  }
  .device-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  .device-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 0.9rem 1rem;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--bg-surface);
    margin-bottom: 0.6rem;
    transition: background 0.1s;
    text-decoration: none;
    color: inherit;
  }
  a.device-row {
    color: inherit;
  }
  a.device-row:hover {
    text-decoration: none;
    background: var(--bg-surface-2);
  }
  .device-row.clickable {
    cursor: pointer;
  }
  .device-name {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-weight: 500;
  }
  .conn-tag {
    color: var(--fg-muted);
    font-size: 0.78rem;
    font-family: ui-monospace, monospace;
  }
  .status-tag {
    font-size: 0.72rem;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
  }
  .status-tag.unauthorized {
    background: var(--danger-surface);
    color: var(--danger-text);
  }
  .status-tag.offline {
    background: var(--bg-muted);
    color: var(--fg-faint);
  }
  .device-meta {
    font-size: 0.82rem;
    margin-top: 0.2rem;
  }
  .chevron {
    color: var(--fg-muted);
    font-size: 1.4rem;
  }
  .empty {
    text-align: center;
    padding: 3rem 1rem;
  }
  .empty h2 {
    margin: 0 0 0.6rem;
    font-size: 1.1rem;
    color: var(--fg-secondary);
  }
  .pair-form {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem 1.2rem;
    margin-bottom: 1rem;
  }
  .pair-form h3 {
    margin: 0 0 0.4rem;
    font-size: 1rem;
  }
  .pair-row {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .pair-row input {
    flex: 1;
    min-width: 200px;
  }
  .pair-row input[inputmode="numeric"] {
    flex: 0 0 8rem;
  }
  .report-all {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1rem 1.2rem;
    margin-bottom: 1rem;
  }
  .report-all h3 {
    margin: 0;
    font-size: 1rem;
  }
  .report-row {
    margin: 0.7rem 0;
    padding-bottom: 0.7rem;
    border-bottom: 1px solid var(--bg-button);
  }
  .report-row:last-child {
    border-bottom: none;
  }
  .report-head {
    margin-bottom: 0.3rem;
  }
  .report-vitals {
    margin: 0;
    padding-left: 1.2rem;
    font-size: 0.85rem;
  }
  .unauthorized-help {
    margin-top: 0.6rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--danger-surface);
    border-radius: 4px;
    font-size: 0.85rem;
  }
  .unauthorized-help strong {
    color: var(--danger-text);
  }
  .unauthorized-help ol {
    margin: 0.4rem 0;
    padding-left: 1.2rem;
  }
  .unauthorized-help p {
    margin: 0.3rem 0 0;
  }
  .device-row.unauthorized {
    align-items: flex-start;
  }
  .install-pane {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.5rem;
  }
  .install-pane h2 {
    margin: 0 0 0.6rem;
    font-size: 1.1rem;
  }
  .install-pane p {
    margin: 0.4rem 0;
    font-size: 0.92rem;
    line-height: 1.4;
  }
  .install-message {
    margin-top: 0.8rem;
    font-family: ui-monospace, monospace;
  }
  .small {
    font-size: 0.82rem;
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
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
