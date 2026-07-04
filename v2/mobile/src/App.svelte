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

  type Step = "scan" | "pair" | "connecting" | "connected";
  let step = $state<Step>("scan");

  // Connection target + inputs (shapes match the Rust commands exactly).
  let host = $state("");
  let pairPort = $state(37099);
  let connectPort = $state(5555);
  let code = $state("");

  let busy = $state(false);
  let error = $state("");
  let discoveries = $state<Discovery[]>([]);
  let devices = $state<Device[]>([]);
  let scanned = $state(false);
  let manual = $state(false);

  // Remember which path we took into 'connecting' so retry replays the right one.
  let needsPairing = $state(false);

  const connectedDevice = $derived(
    devices.find((d) => d.status === "device") ?? devices[0] ?? null,
  );

  // One card per TV: fold a host's mDNS services together. A service whose string
  // does NOT contain "tls" is LEGACY (network debugging, :5555) — no pairing code.
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

  const foundLabel = $derived(
    found.length === 1 ? "1 device found" : `${found.length} devices found`,
  );
  const codeReady = $derived(/^\d{6}$/.test(code));

  async function scan() {
    error = "";
    busy = true;
    try {
      const result = await invoke<DiscoveryResult>("wireless_discover");
      discoveries = result.devices;
      scanned = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function selectFound(f: Found) {
    host = f.host;
    if (f.pairingPort) pairPort = f.pairingPort;
    if (f.connectPort) connectPort = f.connectPort;

    if (f.pairingPort && !f.legacy) {
      // TLS device — needs a 6-digit code first.
      code = "";
      needsPairing = true;
      step = "pair";
    } else {
      // Legacy / network debugging — no code, just connect and Allow on the TV.
      needsPairing = false;
      startConnect();
    }
  }

  // Manual entry escape hatches (from the "Enter IP address manually" panel).
  function manualPair() {
    if (!host) return;
    code = "";
    needsPairing = true;
    step = "pair";
  }
  function manualConnect() {
    if (!host) return;
    needsPairing = false;
    startConnect();
  }

  function onCodeInput(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    code = el.value.replace(/\D/g, "").slice(0, 6);
  }

  // Pair (if needed) then connect. Drives the 'connecting' → 'connected' transition.
  async function startConnect() {
    error = "";
    step = "connecting";
    busy = true;
    try {
      if (needsPairing) {
        const paired = await invoke<ConnectResult>("wireless_pair", {
          host,
          port: Number(pairPort),
          code,
        });
        if (!paired.ok) {
          error = paired.message;
          return;
        }
      }
      const result = await invoke<ConnectResult>("wireless_connect", {
        host,
        port: Number(connectPort),
      });
      if (!result.ok) {
        error = result.message; // surface it — do not clobber.
        return;
      }
      await refreshDevices();
      step = "connected";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  async function refreshDevices() {
    try {
      devices = await invoke<Device[]>("list_devices");
    } catch (e) {
      error = String(e);
    }
  }

  async function disconnect() {
    busy = true;
    try {
      await invoke<ConnectResult>("wireless_disconnect");
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
    devices = [];
    code = "";
    discoveries = [];
    scanned = false;
    step = "scan";
  }

  function backToScan() {
    error = "";
    code = "";
    step = "scan";
  }

  // Connected-device display helpers.
  const connName = $derived(
    connectedDevice?.name || connectedDevice?.model || "your Android TV",
  );
</script>

<div class="screen">
  {#if step === "scan"}
    <!-- 1.1 — Scan & discover -->
    <div class="topline">
      <div class="brand">
        <span class="logo"><span class="msr">tune</span></span>
        <span class="wordmark">ATV&nbsp;Optimizer</span>
      </div>
      <span class="statuspill" class:live={busy}>
        <span class="pdot" class:blink={busy}></span>{busy ? "Scanning" : "Ready"}
      </span>
    </div>

    <p class="eyebrow">Step 1 of 2</p>
    <h1>Find your TV</h1>
    <p class="lede">
      On the TV, open <span class="soft">Settings › System › Developer options › Wireless
        debugging</span>.
    </p>

    <div class="radar" aria-hidden="true">
      <span class="ring"></span>
      <span class="ring ring2"></span>
      <span class="radar-core"><span class="msr">tv_gen</span></span>
    </div>

    {#if scanned && found.length}
      <p class="section-label">{foundLabel}</p>
      <div class="devices">
        {#each found as f (f.host)}
          <button class="device" onclick={() => selectFound(f)}>
            <span class="device-icon"><span class="msr">cast</span></span>
            <span class="device-body">
              <span class="device-name">Android&nbsp;TV</span>
              <span class="mono device-addr">{f.host}</span>
              {#if f.legacy && !f.pairingPort}
                <span class="device-tag">No code needed</span>
              {/if}
            </span>
            <span class="msr device-go">arrow_forward</span>
          </button>
        {/each}
      </div>
    {:else if scanned}
      <p class="section-label">No devices found</p>
      <p class="lede empty">Confirm Wireless debugging is on, then scan again.</p>
    {/if}

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="spacer"></div>

    <button class="primary" disabled={busy} onclick={scan}>
      <span class="msr">wifi_tethering</span>{scanned ? "Scan again" : "Scan network"}
    </button>
    <button class="ghost-link" onclick={() => (manual = !manual)}>
      {manual ? "Hide manual entry" : "Enter IP address manually"}
    </button>

    {#if manual}
      <div class="manual">
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
        <div class="manual-actions">
          <button class="primary small" disabled={!host} onclick={manualPair}>Pair with code</button>
          <button class="ghost small" disabled={!host} onclick={manualConnect}>Connect (no code)</button>
        </div>
      </div>
    {/if}
  {:else if step === "pair"}
    <!-- 1.2 — Pair with code -->
    <div class="pairhead">
      <button class="iconbtn" aria-label="Back" onclick={backToScan}>
        <span class="msr">arrow_back</span>
      </button>
      <span class="mono target">{host}</span>
    </div>

    <p class="eyebrow">Step 2 of 2</p>
    <h1>Enter pairing code</h1>
    <p class="lede">
      Newer Google&nbsp;TV devices show a <span class="soft">6-digit code</span> the first time.
      Older ones (like Shield) connect directly — no code.
    </p>

    <div class="code-row">
      <input
        class="code-input"
        value={code}
        oninput={onCodeInput}
        inputmode="numeric"
        maxlength="6"
        autocomplete="one-time-code"
        aria-label="6-digit pairing code"
      />
      {#each Array(6) as _, i (i)}
        <div class="digit" class:active={i === code.length && code.length < 6}>
          {#if code[i]}
            <span class="mono">{code[i]}</span>
          {:else if i === code.length}
            <span class="caret"></span>
          {/if}
        </div>
      {/each}
    </div>

    <div class="callout teal">
      <span class="msr">lock</span>
      <span>The pairing key stays on your phone. Nothing is sent to a server.</span>
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="spacer"></div>

    <button class="primary" disabled={!codeReady || busy} onclick={startConnect}>
      Pair &amp; connect
    </button>
  {:else if step === "connecting"}
    <!-- Connecting / handshaking (covers pair-then-connect and no-code connect) -->
    <div class="center">
      <div class="radar" aria-hidden="true">
        <span class="ring"></span>
        <span class="ring ring2"></span>
        <span class="radar-core"><span class="msr">cast</span></span>
      </div>

      <div class="connect-copy">
        <h1>{error ? "Couldn't connect" : "Connecting…"}</h1>
        <p class="lede">
          {#if error}
            The connection didn't complete.
          {:else}
            Reaching <span class="soft">{host}</span>
          {/if}
        </p>
        {#if !error}
          <span class="handshake">
            <span class="hdot blink"></span>Handshaking…
          </span>
        {/if}
      </div>

      {#if error}
        <p class="error" role="alert">{error}</p>
      {:else}
        <div class="callout accent">
          <span class="msr">tv_gen</span>
          <span>
            Look at your TV — if an <span class="soft">"Allow debugging?"</span> prompt appears,
            select <span class="soft">Allow</span> to finish connecting.
          </span>
        </div>
      {/if}
    </div>

    <div class="spacer"></div>

    {#if error}
      <button class="primary" disabled={busy} onclick={startConnect}>Try again</button>
      <button class="ghost-link" onclick={backToScan}>Back to scan</button>
    {/if}
  {:else if step === "connected"}
    <!-- 1.3 — Connected -->
    <div class="center connected">
      <div class="check-wrap" aria-hidden="true">
        <span class="check-halo"></span>
        <span class="check-circle"><span class="msr">check</span></span>
      </div>
      <div class="connect-copy">
        <h1>Connected</h1>
        <p class="lede">You're now controlling<br /><span class="soft strong">{connName}</span></p>
      </div>
      <div class="pills">
        {#if connectedDevice?.device_type}
          <span class="infopill"><span class="msr">memory</span>{connectedDevice.device_type}</span>
        {/if}
        <span class="infopill"><span class="msr">lan</span><span class="mono">{host}</span></span>
      </div>
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="spacer"></div>

    <button class="primary" onclick={() => (step = "connected")}>
      Open dashboard<span class="msr">arrow_forward</span>
    </button>
    <button class="ghost-link" onclick={disconnect}>Disconnect</button>
  {/if}
</div>

<style>
  .screen {
    box-sizing: border-box;
    min-height: 100dvh;
    display: flex;
    flex-direction: column;
    padding: calc(env(safe-area-inset-top) + 14px) max(24px, env(safe-area-inset-right))
      calc(env(safe-area-inset-bottom) + 28px) max(24px, env(safe-area-inset-left));
    background: radial-gradient(
      1200px 800px at 20% -5%,
      color-mix(in srgb, var(--accent) 5%, transparent),
      transparent 60%
    );
  }

  .spacer {
    flex: 1;
    min-height: 20px;
  }

  /* Header */
  .topline {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 32px;
  }
  .brand {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .logo {
    width: 30px;
    height: 30px;
    border-radius: 9px;
    background: var(--accent);
    display: grid;
    place-items: center;
  }
  .logo .msr {
    font-size: 19px;
    color: var(--accent-ink);
  }
  .wordmark {
    font-weight: 700;
    font-size: 15px;
    letter-spacing: -0.01em;
  }
  .statuspill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
    background: var(--surface-2);
    padding: 6px 11px;
    border-radius: 999px;
  }
  .statuspill.live {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .pdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--muted);
  }
  .statuspill.live .pdot {
    background: var(--accent);
  }

  /* Type */
  .eyebrow {
    margin: 0 0 10px;
    font-family: var(--mono);
    font-size: 12px;
    font-weight: 600;
    letter-spacing: 0.16em;
    color: var(--accent);
    text-transform: uppercase;
  }
  h1 {
    margin: 0 0 10px;
    font-size: 32px;
    font-weight: 700;
    letter-spacing: -0.02em;
    line-height: 1.05;
  }
  .lede {
    margin: 0 0 26px;
    font-size: 14px;
    line-height: 1.5;
    color: var(--muted);
  }
  .lede.empty {
    margin-bottom: 0;
  }
  .soft {
    color: var(--text-soft);
  }
  .soft.strong {
    color: var(--text);
    font-weight: 600;
  }
  .section-label {
    margin: 22px 0 12px;
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
  }

  /* Radar */
  .radar {
    position: relative;
    height: 184px;
    display: grid;
    place-items: center;
    margin-bottom: 6px;
  }
  .ring {
    position: absolute;
    width: 150px;
    height: 150px;
    border-radius: 50%;
    border: 1.5px solid color-mix(in srgb, var(--accent) 50%, transparent);
    animation: radar 2.4s ease-out infinite;
  }
  .ring2 {
    animation-delay: 1.2s;
  }
  .radar-core {
    width: 92px;
    height: 92px;
    border-radius: 26px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: grid;
    place-items: center;
    box-shadow: 0 0 40px color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .radar-core .msr {
    font-size: 44px;
    color: var(--accent);
  }
  @keyframes radar {
    0% {
      transform: scale(0.55);
      opacity: 0.9;
    }
    100% {
      transform: scale(1.9);
      opacity: 0;
    }
  }
  @keyframes blink {
    0%,
    100% {
      opacity: 1;
    }
    50% {
      opacity: 0.3;
    }
  }
  .blink {
    animation: blink 1s infinite;
  }

  /* Device cards */
  .devices {
    display: grid;
    gap: 10px;
  }
  .device {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    box-shadow: 0 0 0 1px color-mix(in srgb, var(--accent) 6%, transparent);
    text-align: left;
    cursor: pointer;
    color: inherit;
    font: inherit;
    min-height: 44px;
  }
  .device:active {
    transform: translateY(1px);
  }
  .device-icon {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    flex: none;
  }
  .device-icon .msr {
    font-size: 24px;
    color: var(--text);
  }
  .device-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .device-name {
    font-weight: 600;
    font-size: 15px;
  }
  .device-addr {
    font-size: 12px;
    color: var(--muted);
  }
  .device-tag {
    margin-top: 4px;
    align-self: flex-start;
    font-size: 11px;
    font-weight: 600;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
    padding: 2px 8px;
    border-radius: 6px;
  }
  .device-go {
    font-size: 22px;
    color: var(--accent);
    flex: none;
  }

  /* Buttons */
  .primary {
    width: 100%;
    min-height: 54px;
    border: 0;
    border-radius: 16px;
    background: var(--accent);
    color: var(--accent-ink);
    font-family: var(--sans);
    font-size: 16px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 8px;
  }
  .primary .msr {
    font-size: 20px;
  }
  .primary:active {
    transform: translateY(1px);
  }
  .primary:disabled {
    background: var(--surface-2);
    color: var(--muted);
    cursor: default;
  }
  .primary.small,
  .ghost.small {
    min-height: 48px;
    font-size: 14px;
    flex: 1;
  }
  .ghost {
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
    color: var(--text);
    font-family: var(--sans);
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .ghost:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .ghost-link {
    width: 100%;
    min-height: 48px;
    margin-top: 10px;
    border: 0;
    background: transparent;
    color: var(--muted);
    font-family: var(--sans);
    font-size: 14px;
    font-weight: 500;
    cursor: pointer;
  }
  .iconbtn {
    width: 44px;
    height: 44px;
    border-radius: 11px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: grid;
    place-items: center;
    cursor: pointer;
    flex: none;
  }
  .iconbtn .msr {
    font-size: 22px;
    color: var(--text);
  }

  /* Manual entry */
  .manual {
    margin-top: 14px;
    display: grid;
    gap: 12px;
  }
  label {
    display: grid;
    gap: 7px;
    font-size: 12px;
    font-weight: 600;
    color: var(--muted);
  }
  input {
    min-height: 48px;
    border-radius: 12px;
    border: 1px solid var(--line);
    padding: 0 14px;
    background: var(--canvas);
    color: var(--text);
    font: inherit;
  }
  input:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 1px;
    border-color: transparent;
  }
  .grid2 {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .manual-actions {
    display: flex;
    gap: 10px;
  }

  /* Pair header */
  .pairhead {
    display: flex;
    align-items: center;
    gap: 12px;
    margin-bottom: 34px;
  }
  .target {
    font-size: 15px;
    font-weight: 600;
    color: var(--muted);
  }

  /* Code boxes */
  .code-row {
    position: relative;
    display: flex;
    gap: 9px;
    margin-bottom: 28px;
  }
  .code-input {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    opacity: 0;
    border: 0;
    padding: 0;
    margin: 0;
    cursor: pointer;
    color: transparent;
    z-index: 2;
  }
  .code-input:focus-visible {
    outline: none;
  }
  .digit {
    flex: 1;
    height: 60px;
    border-radius: 14px;
    background: var(--surface);
    border: 1.5px solid var(--line);
    display: grid;
    place-items: center;
    font-size: 26px;
    font-weight: 600;
  }
  .digit .mono {
    color: var(--text);
  }
  .digit.active {
    background: var(--canvas);
    border-color: var(--accent);
    box-shadow: 0 0 0 4px color-mix(in srgb, var(--accent) 12%, transparent);
  }
  .caret {
    width: 2px;
    height: 26px;
    background: var(--accent);
    animation: blink 1s infinite;
  }

  /* Callouts */
  .callout {
    display: flex;
    align-items: flex-start;
    gap: 12px;
    padding: 14px 16px;
    border-radius: 16px;
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-soft);
  }
  .callout .msr {
    font-size: 22px;
    flex: none;
  }
  .callout.teal {
    background: color-mix(in srgb, var(--teal) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--teal) 25%, transparent);
  }
  .callout.teal .msr {
    color: var(--teal);
  }
  .callout.accent {
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .callout.accent .msr {
    color: var(--accent);
  }

  /* Connecting */
  .center {
    flex: 1;
    display: flex;
    flex-direction: column;
    align-items: center;
    justify-content: center;
    text-align: center;
    gap: 26px;
  }
  .center .lede {
    margin: 0;
  }
  .connect-copy {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .handshake {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--teal);
  }
  .hdot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: var(--teal);
  }

  /* Connected */
  .check-wrap {
    position: relative;
    display: grid;
    place-items: center;
  }
  .check-halo {
    position: absolute;
    width: 150px;
    height: 150px;
    border-radius: 50%;
    background: color-mix(in srgb, var(--teal) 12%, transparent);
  }
  .check-circle {
    width: 104px;
    height: 104px;
    border-radius: 50%;
    background: var(--teal);
    display: grid;
    place-items: center;
    box-shadow: 0 0 46px color-mix(in srgb, var(--teal) 45%, transparent);
  }
  .check-circle .msr {
    font-size: 60px;
    color: #06231a;
    font-variation-settings: "FILL" 0, "wght" 500, "GRAD" 0, "opsz" 24;
  }
  .pills {
    display: flex;
    gap: 10px;
    flex-wrap: wrap;
    justify-content: center;
  }
  .infopill {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-soft);
    background: var(--surface);
    border: 1px solid var(--line);
    padding: 8px 12px;
    border-radius: 999px;
  }
  .infopill .msr {
    font-size: 15px;
    color: var(--muted);
  }

  .error {
    margin: 0 0 14px;
    padding: 12px 14px;
    border-radius: 12px;
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger) 30%, transparent);
    color: var(--danger);
    font-size: 13px;
    line-height: 1.4;
  }

  @media (prefers-reduced-motion: reduce) {
    .ring,
    .blink,
    .caret {
      animation: none;
    }
    .ring {
      opacity: 0.4;
      transform: scale(1.4);
    }
  }
</style>
