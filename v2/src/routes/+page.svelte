<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import type { Device } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";

  let devices = $state<Device[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);

  let connectAddress = $state("");
  let connectBusy = $state(false);
  let connectMessage = $state("");

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

  function deviceHref(d: Device): string | null {
    return d.status === "device" ? `/devices/${encodeURIComponent(d.serial)}` : null;
  }

  onMount(refresh);
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
  {#if connectMessage}
    <p class="connect-message muted">{connectMessage}</p>
  {/if}
</section>

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
    {#each devices as d (d.serial)}
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
          <div class="device-row">
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
    border: 1px solid #30363d;
    border-radius: 8px;
    background: #161b22;
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
    background: #1c2128;
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
    color: #7d8590;
    font-size: 0.78rem;
    font-family: ui-monospace, monospace;
  }
  .status-tag {
    font-size: 0.72rem;
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
  }
  .status-tag.unauthorized {
    background: #5d1b1b;
    color: #ff8a80;
  }
  .status-tag.offline {
    background: #3d3d3d;
    color: #aaa;
  }
  .device-meta {
    font-size: 0.82rem;
    margin-top: 0.2rem;
  }
  .chevron {
    color: #7d8590;
    font-size: 1.4rem;
  }
  .empty {
    text-align: center;
    padding: 3rem 1rem;
  }
  .empty h2 {
    margin: 0 0 0.6rem;
    font-size: 1.1rem;
    color: #c9d1d9;
  }
  .install-pane {
    background: #161b22;
    border: 1px solid #30363d;
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
    background: #5d1b1b;
    color: #ff8a80;
    padding: 0.7rem 1rem;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
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
