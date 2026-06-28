<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type ConnectResult = { ok: boolean; message: string };
  type DiscoveryResult = {
    devices: Array<{ name: string; host: string; port: number; service: string }>;
    message: string;
  };
  type Device = {
    serial: string;
    name: string;
    model: string;
    status: string;
    device_type: string;
  };
  type HealthReport = {
    ram: { total_mb: number | null; free_mb: number | null; used_mb: number | null };
    storage: { total: string | null; used: string | null; available: string | null; used_percent: number | null };
    top_memory: Array<{ package: string; mb: number }>;
  };

  let host = $state("");
  let pairPort = $state(37099);
  let connectPort = $state(5555);
  let code = $state("");
  let status = $state("Open Wireless debugging on your TV, then scan or enter the shown IP/port.");
  let busy = $state(false);
  let discoveries = $state<DiscoveryResult["devices"]>([]);
  let devices = $state<Device[]>([]);
  let selectedSerial = $state("");
  let health = $state<HealthReport | null>(null);

  const selectedDevice = $derived(devices.find((d) => d.serial === selectedSerial) ?? null);

  async function run<T>(label: string, fn: () => Promise<T>): Promise<T | null> {
    busy = true;
    status = `${label}…`;
    try {
      const result = await fn();
      status = `${label} complete.`;
      return result;
    } catch (error) {
      status = String(error);
      return null;
    } finally {
      busy = false;
    }
  }

  async function discover() {
    const result = await run("Scanning", () => invoke<DiscoveryResult>("wireless_discover"));
    if (!result) return;
    discoveries = result.devices;
    status = result.message;
    const pairing = result.devices.find((d) => d.service.includes("pairing"));
    const connect = result.devices.find((d) => d.service.includes("connect"));
    const first = pairing ?? connect ?? result.devices[0];
    if (first) host = first.host;
    if (pairing) pairPort = pairing.port;
    if (connect) connectPort = connect.port;
  }

  async function pair() {
    const result = await run("Pairing", () =>
      invoke<ConnectResult>("wireless_pair", { host, port: Number(pairPort), code }),
    );
    if (result) status = result.message;
  }

  async function connect() {
    const result = await run("Connecting", () =>
      invoke<ConnectResult>("wireless_connect", { host, port: Number(connectPort) }),
    );
    if (result) status = result.message;
    await refreshDevices();
  }

  async function disconnect() {
    const result = await run("Disconnecting", () => invoke<ConnectResult>("wireless_disconnect"));
    if (result) status = result.message;
    devices = [];
    selectedSerial = "";
    health = null;
  }

  async function refreshDevices() {
    const result = await run("Refreshing devices", () => invoke<Device[]>("list_devices"));
    if (!result) return;
    devices = result;
    selectedSerial = result.find((d) => d.status === "device")?.serial ?? result[0]?.serial ?? "";
  }

  async function loadHealth() {
    if (!selectedSerial) return;
    health = await run("Loading health", () => invoke<HealthReport>("health_report", { serial: selectedSerial }));
  }
</script>

<main>
  <section class="hero card">
    <p class="eyebrow">No-PC Android TV care</p>
    <h1>ATV Optimizer</h1>
    <p>Pair over wireless debugging, inspect your TV, and run safe quick wins from your phone.</p>
  </section>

  <section class="card stack">
    <div class="row between">
      <div>
        <h2>1. Find the TV</h2>
        <p class="muted">Use Android TV Settings → Developer options → Wireless debugging.</p>
      </div>
      <button disabled={busy} onclick={discover}>Scan</button>
    </div>

    {#if discoveries.length}
      <div class="chips">
        {#each discoveries as d}
          <button
            class="chip"
            onclick={() => {
              host = d.host;
              if (d.service.includes("pairing")) pairPort = d.port;
              if (d.service.includes("connect")) connectPort = d.port;
            }}
          >
            {d.name}<br /><span>{d.host}:{d.port}</span>
          </button>
        {/each}
      </div>
    {/if}

    <label>
      TV IP address
      <input bind:value={host} placeholder="192.168.1.42" inputmode="decimal" />
    </label>
    <div class="grid2">
      <label>
        Pairing port
        <input bind:value={pairPort} type="number" min="1" max="65535" />
      </label>
      <label>
        Connect port
        <input bind:value={connectPort} type="number" min="1" max="65535" />
      </label>
    </div>
  </section>

  <section class="card stack">
    <h2>2. Pair and connect</h2>
    <label>
      6-digit pairing code
      <input bind:value={code} inputmode="numeric" maxlength="6" placeholder="123456" />
    </label>
    <div class="row">
      <button disabled={busy || !host || code.length !== 6} onclick={pair}>Pair</button>
      <button disabled={busy || !host} onclick={connect}>Connect</button>
      <button class="secondary" disabled={busy} onclick={disconnect}>Disconnect</button>
    </div>
  </section>

  <section class="card stack">
    <div class="row between">
      <h2>3. Free diagnostic</h2>
      <button class="secondary" disabled={busy} onclick={refreshDevices}>Refresh</button>
    </div>
    {#if devices.length}
      <div class="device-list">
        {#each devices as device}
          <button class:selected={device.serial === selectedSerial} onclick={() => (selectedSerial = device.serial)}>
            <strong>{device.name || "Android TV"}</strong>
            <span>{device.model || device.serial} · {device.status}</span>
          </button>
        {/each}
      </div>
      <button disabled={busy || !selectedSerial} onclick={loadHealth}>Load health report</button>
    {:else}
      <p class="muted">No connected TV yet.</p>
    {/if}

    {#if selectedDevice}
      <div class="summary">
        <strong>{selectedDevice.name}</strong>
        <span>{selectedDevice.device_type}</span>
      </div>
    {/if}

    {#if health}
      <div class="metrics">
        <div><span>RAM free</span><strong>{health.ram.free_mb ?? "—"} MB</strong></div>
        <div><span>Storage used</span><strong>{health.storage.used_percent ?? "—"}%</strong></div>
      </div>
      <h3>Top memory users</h3>
      <ul>
        {#each health.top_memory.slice(0, 5) as item}
          <li><span>{item.package}</span><strong>{item.mb.toFixed(0)} MB</strong></li>
        {/each}
      </ul>
    {/if}
  </section>

  <p class="status" aria-live="polite">{busy ? "Working… " : ""}{status}</p>
</main>

<style>
  :global(body) {
    margin: 0;
    font-family: Inter, ui-sans-serif, system-ui, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif;
    background: #07111f;
    color: #eef5ff;
  }

  main {
    min-height: 100vh;
    padding: 18px;
    box-sizing: border-box;
    display: grid;
    gap: 16px;
  }

  .card {
    border: 1px solid rgba(148, 163, 184, 0.24);
    border-radius: 28px;
    padding: 20px;
    background: rgba(15, 23, 42, 0.86);
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.25);
  }

  .hero {
    background: linear-gradient(145deg, rgba(37, 99, 235, 0.32), rgba(15, 23, 42, 0.9));
  }

  .stack {
    display: grid;
    gap: 14px;
  }

  .row {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    align-items: center;
  }

  .between {
    justify-content: space-between;
  }

  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .eyebrow {
    color: #93c5fd;
    font-size: 0.78rem;
    font-weight: 800;
    letter-spacing: 0.14em;
    text-transform: uppercase;
  }

  h1, h2, h3, p {
    margin: 0;
  }

  h1 {
    margin-top: 8px;
    font-size: clamp(2.4rem, 15vw, 4rem);
    line-height: 0.94;
  }

  h2 {
    font-size: 1.1rem;
  }

  h3 {
    font-size: 0.95rem;
    color: #bfdbfe;
  }

  .muted, label, .status {
    color: #cbd5e1;
  }

  label {
    display: grid;
    gap: 7px;
    font-size: 0.85rem;
    font-weight: 700;
  }

  input {
    min-height: 46px;
    border-radius: 16px;
    border: 1px solid rgba(148, 163, 184, 0.34);
    padding: 0 14px;
    background: rgba(2, 6, 23, 0.74);
    color: #eef5ff;
    font: inherit;
  }

  button {
    min-height: 48px;
    border: 0;
    border-radius: 999px;
    padding: 0 18px;
    color: #07111f;
    background: #93c5fd;
    font-weight: 800;
  }

  button:disabled {
    opacity: 0.5;
  }

  .secondary {
    color: #dbeafe;
    background: rgba(148, 163, 184, 0.22);
  }

  .chips, .device-list {
    display: grid;
    gap: 8px;
  }

  .chip, .device-list button {
    width: 100%;
    height: auto;
    min-height: 54px;
    padding: 12px 14px;
    text-align: left;
    border-radius: 18px;
    color: #dbeafe;
    background: rgba(30, 41, 59, 0.86);
  }

  .chip span, .device-list span, .summary span, .metrics span {
    display: block;
    color: #94a3b8;
    font-size: 0.78rem;
    margin-top: 3px;
  }

  .selected {
    outline: 2px solid #93c5fd;
  }

  .summary, .metrics {
    display: grid;
    gap: 10px;
    grid-template-columns: 1fr 1fr;
  }

  .summary > *, .metrics > * {
    border-radius: 18px;
    background: rgba(30, 41, 59, 0.74);
    padding: 12px;
  }

  ul {
    display: grid;
    gap: 8px;
    padding: 0;
    margin: 0;
    list-style: none;
  }

  li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    color: #cbd5e1;
    font-size: 0.85rem;
  }

  li span {
    overflow-wrap: anywhere;
  }

  .status {
    padding: 0 4px 20px;
    font-size: 0.9rem;
  }
</style>
