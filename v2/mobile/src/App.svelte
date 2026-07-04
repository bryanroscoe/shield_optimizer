<script lang="ts">
  import { invoke } from "@tauri-apps/api/core";

  type ConnectResult = { ok: boolean; message: string };
  type Discovery = { name: string; host: string; port: number; service: string };
  type DiscoveryResult = { devices: Discovery[]; message: string };
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
  let status = $state("Turn on Wireless debugging on your TV, then scan.");
  let busy = $state(false);
  let discoveries = $state<Discovery[]>([]);
  let devices = $state<Device[]>([]);
  let selectedSerial = $state("");
  let health = $state<HealthReport | null>(null);

  const selectedDevice = $derived(devices.find((d) => d.serial === selectedSerial) ?? null);
  const connectedDevice = $derived(devices.find((d) => d.status === "device") ?? null);

  // One card per TV: fold a host's mDNS services together. `legacy` marks a TV on
  // "Network debugging" (_adb._tcp :5555) — no pairing code, connect straight away.
  type Found = { host: string; pairingPort?: number; connectPort?: number; legacy?: boolean };
  const found = $derived.by(() => {
    const byHost = new Map<string, Found>();
    for (const d of discoveries) {
      const entry = byHost.get(d.host) ?? { host: d.host };
      if (d.service.includes("pairing")) {
        entry.pairingPort = d.port;
      } else {
        entry.connectPort = d.port; // _adb-tls-connect or legacy _adb._tcp
        if (!d.service.includes("tls")) entry.legacy = true;
      }
      byHost.set(d.host, entry);
    }
    return [...byHost.values()];
  });

  type Phase = "idle" | "working" | "connected";
  const phase = $derived<Phase>(busy ? "working" : connectedDevice ? "connected" : "idle");
  const phaseLabel = $derived(
    busy ? "Working" : connectedDevice ? "Connected" : "Not connected",
  );

  async function run<T>(working: string, done: string, fn: () => Promise<T>): Promise<T | null> {
    busy = true;
    status = working;
    try {
      const result = await fn();
      status = done;
      return result;
    } catch (error) {
      status = String(error);
      return null;
    } finally {
      busy = false;
    }
  }

  async function scan() {
    const result = await run("Scanning your network…", "", () =>
      invoke<DiscoveryResult>("wireless_discover"),
    );
    if (!result) return;
    discoveries = result.devices;
    if (found.length === 0) {
      status = "No TVs found. Confirm Wireless debugging is on, then scan again.";
      return;
    }
    status = found.length === 1 ? "Found 1 TV." : `Found ${found.length} TVs.`;
    pick(found[0]);
  }

  function pick(f: Found) {
    host = f.host;
    if (f.pairingPort) pairPort = f.pairingPort;
    if (f.connectPort) connectPort = f.connectPort;
  }

  async function pair() {
    const result = await run("Pairing with the TV…", "", () =>
      invoke<ConnectResult>("wireless_pair", { host, port: Number(pairPort), code }),
    );
    if (result) status = result.message;
  }

  async function connect() {
    const result = await run("Connecting…", "", () =>
      invoke<ConnectResult>("wireless_connect", { host, port: Number(connectPort) }),
    );
    if (result) status = result.message;
    await refreshDevices();
  }

  async function disconnect() {
    const result = await run("Disconnecting…", "Disconnected.", () =>
      invoke<ConnectResult>("wireless_disconnect"),
    );
    if (result) status = result.message;
    devices = [];
    selectedSerial = "";
    health = null;
  }

  async function refreshDevices() {
    const result = await run("Refreshing…", "", () => invoke<Device[]>("list_devices"));
    if (!result) return;
    devices = result;
    selectedSerial = connectedDevice?.serial ?? result[0]?.serial ?? "";
    if (result.length) status = "Connected. Load a diagnostic below.";
  }

  async function loadHealth() {
    if (!selectedSerial) return;
    const report = await run("Reading the TV…", "Diagnostic ready.", () =>
      invoke<HealthReport>("health_report", { serial: selectedSerial }),
    );
    if (report) health = report;
  }

  const canPair = $derived(!busy && !!host && code.length === 6);
  const canConnect = $derived(!busy && !!host);
</script>

<header class="appbar">
  <div class="brand">
    <span class="mark" aria-hidden="true"></span>
    <span class="wordmark">ATV&nbsp;Optimizer</span>
  </div>
  <span class="statuspill" data-phase={phase}>
    <span class="dot"></span>{phaseLabel}
  </span>
</header>

<main>
  <section class="intro">
    <p class="eyebrow">No computer required</p>
    <h1>Tune your Android&nbsp;TV from your phone.</h1>
  </section>

  <section class="card">
    <div class="step">
      <span class="num">1</span>
      <div class="step-head">
        <h2>Find your TV</h2>
        <p class="muted">On the TV: Settings › System › Developer options › Wireless debugging.</p>
      </div>
    </div>

    <button class="primary" disabled={busy} onclick={scan}>
      {busy ? "Scanning…" : "Scan network"}
    </button>

    {#if found.length}
      <div class="found">
        {#each found as f (f.host)}
          <button class="device" class:active={host === f.host} onclick={() => pick(f)}>
            <span class="device-name">Android TV</span>
            <span class="mono device-addr">{f.host}</span>
            <span class="tags">
              {#if f.pairingPort}<span class="tag pair">pair · {f.pairingPort}</span>{/if}
              {#if f.connectPort}<span class="tag conn">{f.legacy ? "network" : "connect"} · {f.connectPort}</span>{/if}
            </span>
            {#if f.legacy && !f.pairingPort}
              <span class="hint">No code needed — tap Connect, then allow it on the TV.</span>
            {/if}
          </button>
        {/each}
      </div>
    {/if}

    <details class="manual">
      <summary>Enter address manually</summary>
      <label>
        TV IP address
        <input class="mono" bind:value={host} placeholder="192.168.1.42" inputmode="decimal" />
      </label>
      <div class="grid2">
        <label>
          Pairing port
          <input class="mono" bind:value={pairPort} type="number" min="1" max="65535" />
        </label>
        <label>
          Connect port
          <input class="mono" bind:value={connectPort} type="number" min="1" max="65535" />
        </label>
      </div>
    </details>
  </section>

  <section class="card">
    <div class="step">
      <span class="num">2</span>
      <div class="step-head">
        <h2>Pair &amp; connect</h2>
        <p class="muted">Wireless debugging: enter the 6-digit code, then Pair. Network debugging: skip the code and just Connect, then allow it on the TV.</p>
      </div>
    </div>

    <label>
      Pairing code
      <input
        class="mono code"
        bind:value={code}
        inputmode="numeric"
        maxlength="6"
        placeholder="000000"
      />
    </label>
    <div class="actions">
      <button class="primary" disabled={!canPair} onclick={pair}>Pair</button>
      <button class="ghost" disabled={!canConnect} onclick={connect}>Connect</button>
      <button class="ghost" disabled={busy || !connectedDevice} onclick={disconnect}>Disconnect</button>
    </div>
  </section>

  <section class="card">
    <div class="step">
      <span class="num">3</span>
      <div class="step-head">
        <h2>Diagnose</h2>
        <p class="muted">A free read of what's eating memory and storage.</p>
      </div>
    </div>

    {#if devices.length}
      <div class="found">
        {#each devices as device (device.serial)}
          <button
            class="device"
            class:active={device.serial === selectedSerial}
            onclick={() => (selectedSerial = device.serial)}
          >
            <span class="device-name">{device.name || device.model || "Android TV"}</span>
            <span class="mono device-addr">{device.serial}</span>
            <span class="tags"><span class="tag conn">{device.device_type}</span></span>
          </button>
        {/each}
      </div>
      <button class="primary" disabled={busy || !selectedSerial} onclick={loadHealth}>
        Run free diagnostic
      </button>
    {:else}
      <p class="empty">Connect a TV in step 2 to run a diagnostic.</p>
    {/if}

    {#if health}
      <div class="metrics">
        <div class="metric">
          <span class="metric-label">RAM free</span>
          <strong class="mono">{health.ram.free_mb ?? "—"}<span class="unit">MB</span></strong>
        </div>
        <div class="metric">
          <span class="metric-label">Storage used</span>
          <strong class="mono">{health.storage.used_percent ?? "—"}<span class="unit">%</span></strong>
        </div>
      </div>
      <h3>Top memory users</h3>
      <ul class="hogs">
        {#each health.top_memory.slice(0, 5) as item (item.package)}
          <li>
            <span class="pkg">{item.package}</span>
            <span class="mono">{item.mb.toFixed(0)} MB</span>
          </li>
        {/each}
      </ul>
    {/if}
  </section>
</main>

<p class="toast" class:busy aria-live="polite">{status}</p>

<style>
  :root {
    --bg: #0a0c10;
    --surface: #12161d;
    --surface-2: #191f29;
    --line: rgba(255, 255, 255, 0.08);
    --text: #eaeef4;
    --muted: #828d9e;
    --accent: #f5b942;
    --accent-ink: #1a1206;
    --ok: #57d9a3;
    --danger: #ff7a7a;
    --mono: ui-monospace, "Roboto Mono", "SF Mono", Menlo, monospace;
    --sans: system-ui, -apple-system, "Roboto", "Segoe UI", sans-serif;
  }

  :global(html) {
    background: var(--bg);
  }

  :global(body) {
    margin: 0;
    font-family: var(--sans);
    background: var(--bg);
    color: var(--text);
    -webkit-font-smoothing: antialiased;
  }

  .mono {
    font-family: var(--mono);
    font-feature-settings: "tnum" 1;
  }

  /* Top bar — pinned, clears the status bar / notch. */
  .appbar {
    position: sticky;
    top: 0;
    z-index: 10;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    padding: calc(env(safe-area-inset-top) + 12px) max(16px, env(safe-area-inset-left)) 12px
      max(16px, env(safe-area-inset-right));
    background: color-mix(in srgb, var(--bg) 82%, transparent);
    backdrop-filter: blur(12px);
    border-bottom: 1px solid var(--line);
  }

  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  .mark {
    width: 22px;
    height: 22px;
    border-radius: 7px;
    background: var(--accent);
    box-shadow: 0 0 18px rgba(245, 185, 66, 0.45);
    position: relative;
  }

  .mark::after {
    content: "";
    position: absolute;
    inset: 0;
    margin: auto;
    width: 0;
    height: 0;
    border-left: 7px solid var(--accent-ink);
    border-top: 5px solid transparent;
    border-bottom: 5px solid transparent;
    transform: translateX(1px);
  }

  .wordmark {
    font-weight: 800;
    font-size: 1.02rem;
    letter-spacing: -0.01em;
  }

  .statuspill {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 6px 11px;
    border-radius: 999px;
    font-size: 0.74rem;
    font-weight: 700;
    color: var(--muted);
    background: var(--surface-2);
    border: 1px solid var(--line);
    white-space: nowrap;
  }

  .dot {
    width: 8px;
    height: 8px;
    border-radius: 50%;
    background: var(--muted);
  }

  .statuspill[data-phase="working"] {
    color: var(--accent);
  }
  .statuspill[data-phase="working"] .dot {
    background: var(--accent);
    animation: pulse 1s ease-in-out infinite;
  }
  .statuspill[data-phase="connected"] {
    color: var(--ok);
  }
  .statuspill[data-phase="connected"] .dot {
    background: var(--ok);
    box-shadow: 0 0 10px var(--ok);
  }

  @keyframes pulse {
    50% {
      opacity: 0.35;
    }
  }

  main {
    display: grid;
    gap: 14px;
    padding: 16px max(16px, env(safe-area-inset-left))
      calc(env(safe-area-inset-bottom) + 88px) max(16px, env(safe-area-inset-right));
    box-sizing: border-box;
  }

  .intro {
    padding: 8px 4px 2px;
  }

  .eyebrow {
    margin: 0 0 8px;
    color: var(--accent);
    font-size: 0.72rem;
    font-weight: 800;
    letter-spacing: 0.16em;
    text-transform: uppercase;
  }

  h1 {
    margin: 0;
    font-size: 1.7rem;
    line-height: 1.12;
    font-weight: 800;
    letter-spacing: -0.02em;
  }

  .card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 20px;
    padding: 18px;
    display: grid;
    gap: 14px;
  }

  .step {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }

  .num {
    flex: none;
    width: 26px;
    height: 26px;
    border-radius: 8px;
    display: grid;
    place-items: center;
    font-family: var(--mono);
    font-size: 0.85rem;
    font-weight: 700;
    color: var(--accent);
    background: rgba(245, 185, 66, 0.12);
    border: 1px solid rgba(245, 185, 66, 0.28);
  }

  .step-head {
    display: grid;
    gap: 4px;
  }

  h2 {
    margin: 0;
    font-size: 1.05rem;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  h3 {
    margin: 4px 0 0;
    font-size: 0.8rem;
    font-weight: 700;
    letter-spacing: 0.02em;
    color: var(--muted);
    text-transform: uppercase;
  }

  .muted {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
    line-height: 1.4;
  }

  /* Buttons */
  button {
    font: inherit;
    color: var(--text);
    cursor: pointer;
    border: 0;
    border-radius: 13px;
    min-height: 50px;
    padding: 0 18px;
    font-weight: 700;
  }

  .primary {
    color: var(--accent-ink);
    background: var(--accent);
  }
  .primary:active {
    transform: translateY(1px);
  }

  .ghost {
    color: var(--text);
    background: var(--surface-2);
    border: 1px solid var(--line);
  }

  button:disabled {
    opacity: 0.55;
  }

  /* Disabled primary reads as a clean neutral, not a muddy dimmed amber. */
  .primary:disabled {
    background: var(--surface-2);
    color: var(--muted);
    border: 1px solid var(--line);
    opacity: 1;
  }

  .actions {
    display: flex;
    gap: 10px;
  }
  .actions .primary {
    flex: 1.2;
  }
  .actions .ghost {
    flex: 1;
    padding: 0 12px;
  }

  /* Discovered / connected device cards */
  .found {
    display: grid;
    gap: 10px;
  }

  .device {
    display: grid;
    gap: 6px;
    text-align: left;
    padding: 14px;
    min-height: 0;
    border-radius: 14px;
    background: var(--surface-2);
    border: 1px solid var(--line);
  }

  .device.active {
    border-color: var(--accent);
    background: rgba(245, 185, 66, 0.08);
  }

  .device-name {
    font-weight: 700;
    font-size: 0.95rem;
  }

  .device-addr {
    font-size: 0.85rem;
    color: var(--muted);
  }

  .tags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 2px;
  }

  .tag {
    font-family: var(--mono);
    font-size: 0.7rem;
    font-weight: 600;
    padding: 3px 8px;
    border-radius: 6px;
    background: rgba(255, 255, 255, 0.05);
    color: var(--muted);
  }
  .tag.pair {
    color: var(--accent);
    background: rgba(245, 185, 66, 0.12);
  }
  .tag.conn {
    color: var(--ok);
    background: rgba(87, 217, 163, 0.12);
  }

  .hint {
    font-size: 0.78rem;
    color: var(--muted);
    line-height: 1.35;
  }

  /* Manual entry */
  .manual {
    border-top: 1px solid var(--line);
    padding-top: 12px;
  }
  .manual summary {
    cursor: pointer;
    color: var(--muted);
    font-size: 0.85rem;
    font-weight: 600;
    list-style: none;
  }
  .manual summary::-webkit-details-marker {
    display: none;
  }
  .manual summary::after {
    content: " +";
    color: var(--accent);
  }
  .manual[open] summary::after {
    content: " –";
  }
  .manual[open] {
    display: grid;
    gap: 12px;
  }

  label {
    display: grid;
    gap: 7px;
    font-size: 0.78rem;
    font-weight: 700;
    color: var(--muted);
  }

  input {
    min-height: 48px;
    border-radius: 12px;
    border: 1px solid var(--line);
    padding: 0 14px;
    background: #0d1117;
    color: var(--text);
    font: inherit;
  }
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-color: transparent;
  }

  .code {
    font-size: 1.5rem;
    letter-spacing: 0.4em;
    text-align: center;
  }

  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }

  .empty {
    margin: 0;
    color: var(--muted);
    font-size: 0.85rem;
    padding: 6px 0;
  }

  /* Metrics */
  .metrics {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .metric {
    background: var(--surface-2);
    border: 1px solid var(--line);
    border-radius: 14px;
    padding: 14px;
    display: grid;
    gap: 6px;
  }
  .metric-label {
    font-size: 0.75rem;
    color: var(--muted);
    font-weight: 600;
  }
  .metric strong {
    font-size: 1.6rem;
    font-weight: 700;
  }
  .unit {
    font-size: 0.8rem;
    color: var(--muted);
    margin-left: 3px;
  }

  .hogs {
    list-style: none;
    margin: 0;
    padding: 0;
    display: grid;
    gap: 8px;
  }
  .hogs li {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    align-items: center;
    font-size: 0.85rem;
    color: var(--muted);
  }
  .pkg {
    overflow-wrap: anywhere;
  }
  .hogs .mono {
    flex: none;
    color: var(--text);
    font-size: 0.8rem;
  }

  /* Bottom status toast */
  .toast {
    position: fixed;
    left: 12px;
    right: 12px;
    bottom: calc(env(safe-area-inset-bottom) + 12px);
    margin: 0;
    padding: 13px 16px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--surface-2) 92%, transparent);
    backdrop-filter: blur(12px);
    border: 1px solid var(--line);
    color: var(--text);
    font-size: 0.85rem;
    box-shadow: 0 12px 40px rgba(0, 0, 0, 0.5);
  }
  .toast.busy {
    border-color: rgba(245, 185, 66, 0.4);
  }

  @media (prefers-reduced-motion: reduce) {
    .statuspill[data-phase="working"] .dot {
      animation: none;
    }
  }
</style>
