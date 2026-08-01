<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import {
    lastSavedDevice,
    lastUsedLabel,
    listSavedDevices,
  } from "../lib/savedDevices";
  import { deviceTypeLabel } from "../lib/types";
  import type { Discovery, SavedDevice } from "../lib/types";
  import BrandMark from "../components/BrandMark.svelte";

  let { onConnected }: { onConnected: () => void } = $props();

  type Step = "reconnect" | "scan" | "pair" | "connecting" | "connected";
  let step = $state<Step>("scan");

  // §1.0 Reconnect: if this phone has paired a TV before, offer a one-tap
  // reconnect on launch (the RSA key is persisted Kotlin-side, so it's silent)
  // and fall back to scanning otherwise.
  let savedDevices = $state<SavedDevice[]>([]);
  let reconnectError = $state("");
  let reconnectingSaved = $state(false);
  const primarySaved = $derived(savedDevices[0] ?? null);

  onMount(() => {
    savedDevices = listSavedDevices();
    const saved = lastSavedDevice();
    if (saved && session.consumeAutoReconnectPermission()) {
      step = "reconnect";
      attemptReconnect(saved);
    }
  });

  async function attemptReconnect(d: SavedDevice) {
    if (reconnectingSaved) return;
    reconnectError = "";
    reconnectingSaved = true;
    try {
      const r = await session.connect(d.host, d.connectPort);
      if (r.ok) {
        step = "connected";
      } else {
        reconnectError = r.message || "Couldn't reach that TV.";
      }
    } catch (e) {
      reconnectError = String(e);
    } finally {
      reconnectingSaved = false;
    }
  }

  function goScan() {
    reconnectError = "";
    step = "scan";
  }

  // Connection target + inputs
  let host = $state("");
  let pairPort = $state(37099);
  let connectPort = $state(5555);
  let code = $state("");

  let busy = $state(false);
  let error = $state("");
  let discoveries = $state<Discovery[]>([]);
  let scanned = $state(false);
  let manual = $state(false);
  let needsPairing = $state(false);

  const codeReady = $derived(code.length === 6);

  type Found = {
    host: string;
    pairingPort?: number;
    connectPort?: number;
    legacy?: boolean;
  };
  const found = $derived.by(() => {
    const byHost = new Map<string, Found>();
    for (const d of discoveries) {
      const entry = byHost.get(d.host) ?? { host: d.host };
      if (d.service.includes("pairing")) {
        entry.pairingPort = d.port;
      } else {
        entry.connectPort = d.port;
        if (!d.service.includes("tls")) entry.legacy = true;
      }
      byHost.set(d.host, entry);
    }
    return [...byHost.values()];
  });

  const foundLabel = $derived(
    found.length === 1 ? "1 TV found" : `${found.length} TVs found`,
  );

  async function scan() {
    error = "";
    busy = true;
    try {
      const result = await api.wirelessDiscover();
      discoveries = result.devices;
      scanned = true;
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function selectFound(f: Found) {
    error = "";
    host = f.host;
    if (f.pairingPort) pairPort = f.pairingPort;
    if (f.connectPort) connectPort = f.connectPort;

    if (f.pairingPort && !f.legacy) {
      code = "";
      needsPairing = true;
      step = "pair";
    } else {
      needsPairing = false;
      startConnect();
    }
  }

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

  async function startConnect() {
    error = "";
    step = "connecting";
    busy = true;
    try {
      if (needsPairing) {
        const paired = await session.pair(host, Number(pairPort), code);
        if (!paired.ok) {
          error = paired.message;
          return;
        }
      }
      const result = await session.connect(host, Number(connectPort));
      if (!result.ok) {
        error = result.message;
        return;
      }
      step = "connected";
    } catch (e) {
      error = String(e);
    } finally {
      busy = false;
    }
  }

  function backToScan() {
    error = "";
    code = "";
    step = "scan";
  }

  const connName = $derived(session.deviceLabel);
</script>

<div class="screen">
  {#if step === "reconnect"}
    <div class="topline">
      <div class="brand">
        <BrandMark size={30} />
        <span class="wordmark">ATV&nbsp;Optimizer</span>
      </div>
      <span class="statuspill" class:live={reconnectingSaved}>
        <span class="pdot" class:blink={reconnectingSaved}></span>{reconnectingSaved ? "Reconnecting" : "Ready"}
      </span>
    </div>

    <p class="eyebrow">Welcome back</p>
    <h1>{reconnectError ? "Couldn't reconnect" : "Reconnecting…"}</h1>
    <p class="lede">
      {#if reconnectError}
        Your last TV isn't reachable right now.
      {:else}
        Your last TV reconnects automatically — no code needed.
      {/if}
    </p>

    {#if primarySaved}
      <button class="device" onclick={() => attemptReconnect(primarySaved)} disabled={reconnectingSaved}>
        <span class="device-icon"><span class="msr">cast</span></span>
        <span class="device-body">
          <span class="device-name">{primarySaved.name}</span>
          <span class="mono device-addr">{primarySaved.host} · last used {lastUsedLabel(primarySaved.lastUsed)}</span>
          {#if reconnectingSaved}
            <span class="device-tag">Handshaking…</span>
          {/if}
        </span>
        {#if !reconnectingSaved}<span class="msr device-go">refresh</span>{/if}
      </button>
    {/if}

    {#if reconnectError}
      <div class="callout accent" role="status">
        <span class="msr">tv_gen</span>
        <span>
          If the TV shows <span class="soft">"Allow debugging?"</span>, choose
          <span class="soft">Always allow</span>, select <span class="soft">Allow</span>, and retry.
        </span>
      </div>
      <p class="error" role="alert">{reconnectError}</p>
    {/if}

    {#if savedDevices.length > 1}
      <p class="section-label">Saved TVs</p>
      <div class="devices">
        {#each savedDevices.slice(1) as d (d.host + ":" + d.connectPort)}
          <button class="device" onclick={() => attemptReconnect(d)} disabled={reconnectingSaved}>
            <span class="device-icon"><span class="msr">tv</span></span>
            <span class="device-body">
              <span class="device-name">{d.name}</span>
              <span class="mono device-addr">{d.host} · {deviceTypeLabel(d.deviceType)}</span>
            </span>
            <span class="msr device-go">arrow_forward</span>
          </button>
        {/each}
      </div>
    {/if}

    <div class="spacer"></div>

    <button class="ghost-link" onclick={goScan}>Scan for a different TV</button>
  {:else if step === "scan"}
    <div class="topline">
      <div class="brand">
        <BrandMark size={30} />
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
        <div class="callout accent" role="status">
          <span class="msr">tv_gen</span>
          <span>
            Try again, then look at the TV for <span class="soft">"Allow debugging?"</span>.
            Choose <span class="soft">Always allow</span> when offered and select
            <span class="soft">Allow</span> within 30 seconds.
          </span>
        </div>
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
        {#if session.connectedDevice?.device_type}
          <span class="infopill"><span class="msr">memory</span>{session.connectedDevice.device_type}</span>
        {/if}
        <span class="infopill"><span class="msr">lan</span><span class="mono">{host}</span></span>
      </div>
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="spacer"></div>

    <button class="primary" onclick={onConnected}>
      Open dashboard<span class="msr">arrow_forward</span>
    </button>
  {/if}
</div>
