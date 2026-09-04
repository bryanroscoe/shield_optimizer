<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import {
    autoConnectEnabled,
    lastUsedLabel,
    listSavedDevices,
  } from "../lib/savedDevices";
  import { deviceTypeLabel } from "../lib/types";
  import type { Discovery, SavedDevice } from "../lib/types";
  import BrandMark from "../components/BrandMark.svelte";

  let {
    intent = "launch",
    onConnected,
    onCancel,
  }: {
    /// "launch": app start — may auto-dial the single saved TV. "add": pushed
    /// from Devices to add another TV — never dials anything unasked.
    intent?: "launch" | "add";
    onConnected: () => void;
    onCancel?: () => void;
  } = $props();

  type Step = "reconnect" | "scan" | "pair" | "connecting" | "connected";
  let step = $state<Step>("scan");

  // Saved TVs. On launch the user picks one; with exactly one saved TV (and no
  // deliberate disconnect last time) we dial it for them, but always name it
  // on screen and offer Cancel. With several we never guess.
  let savedDevices = $state<SavedDevice[]>([]);
  let reconnectError = $state("");
  let connectingHost = $state("");
  let connectingName = $state("");
  const primarySaved = $derived(savedDevices[0] ?? null);

  onMount(() => {
    savedDevices = listSavedDevices();
    if (intent !== "launch" || savedDevices.length === 0) return;
    step = "reconnect";
    if (savedDevices.length === 1 && autoConnectEnabled()) {
      attemptReconnect(savedDevices[0]);
    }
  });

  async function attemptReconnect(d: SavedDevice) {
    if (connectingHost) return;
    reconnectError = "";
    connectingHost = d.host;
    connectingName = d.name;
    try {
      const r = await session.connect(d.host, d.connectPort);
      // Cancelled attempts resolve later; ignore them.
      if (connectingHost !== d.host) return;
      if (r.ok) {
        savedDevices = listSavedDevices();
        step = "connected";
      } else {
        reconnectError = r.message || "Couldn't reach that TV.";
      }
    } catch (e) {
      if (connectingHost !== d.host) return;
      reconnectError = String(e);
    } finally {
      if (connectingHost === d.host) connectingHost = "";
    }
  }

  function cancelReconnect() {
    session.cancelConnect();
    connectingHost = "";
    reconnectError = "";
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
    name: string;
    pairingPort?: number;
    connectPort?: number;
    legacy?: boolean;
  };
  const found = $derived.by(() => {
    const byHost = new Map<string, Found>();
    for (const d of discoveries) {
      const entry = byHost.get(d.host) ?? { host: d.host, name: "" };
      const name = d.name?.trim();
      if (name && !name.startsWith("adb-")) entry.name = name;
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
      <span class="statuspill" class:live={connectingHost !== ""}>
        <span class="pdot" class:blink={connectingHost !== ""}></span>{connectingHost ? "Connecting" : "Ready"}
      </span>
    </div>

    <p class="eyebrow">Welcome back</p>
    <h1>
      {#if reconnectError}
        Couldn't reconnect
      {:else if connectingHost}
        Connecting to {connectingName}…
      {:else}
        Which TV?
      {/if}
    </h1>
    <p class="lede">
      {#if reconnectError}
        <span class="soft">{connectingName || "That TV"}</span> isn't reachable right now.
      {:else if connectingHost}
        Your only saved TV — no code needed. Cancel to pick a different one.
      {:else}
        Pick a saved TV to connect. Nothing connects until you choose.
      {/if}
    </p>

    <div class="devices">
      {#each savedDevices as d, i (d.host)}
        <button
          class="device"
          class:dialing={connectingHost === d.host}
          onclick={() => attemptReconnect(d)}
          disabled={connectingHost !== ""}
        >
          <span class="device-icon"><span class="msr">{d.deviceType === "shield" ? "cast" : "tv"}</span></span>
          <span class="device-body">
            <span class="device-name">{d.name}</span>
            <span class="mono device-addr">{d.host} · {deviceTypeLabel(d.deviceType)} · {lastUsedLabel(d.lastUsed)}</span>
            {#if connectingHost === d.host}
              <span class="device-tag">Handshaking…</span>
            {:else if i === 0 && savedDevices.length > 1}
              <span class="device-tag">Last used</span>
            {/if}
          </span>
          {#if connectingHost !== d.host}<span class="msr device-go">arrow_forward</span>{/if}
        </button>
      {/each}
    </div>

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

    <div class="spacer"></div>

    {#if connectingHost}
      <button class="ghost" onclick={cancelReconnect}>Cancel</button>
    {/if}
    <button class="ghost-link" onclick={goScan}>Scan for a different TV</button>
  {:else if step === "scan"}
    <div class="topline">
      <div class="brand">
        {#if intent === "add" && onCancel}
          <button class="iconbtn" aria-label="Back" onclick={onCancel}>
            <span class="msr">arrow_back</span>
          </button>
        {:else}
          <BrandMark size={30} />
        {/if}
        <span class="wordmark">{intent === "add" ? "Add a TV" : "ATV\u00a0Optimizer"}</span>
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
              <span class="device-name">{f.name || "Android TV"}</span>
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

    {#if intent === "launch" && savedDevices.length > 0}
      <button class="ghost-link" onclick={() => { error = ""; step = "reconnect"; }}>
        Back to saved TVs
      </button>
    {/if}
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
    <div class="callout accent" role="status">
      <span class="msr">info</span>
      <span>
        Code pairing isn't supported in this version yet. On the TV, turn on
        <span class="soft">Network debugging</span> (Developer options) and use
        <span class="soft">Connect (no code)</span> instead. Shield TVs never need a code.
      </span>
    </div>

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
            Reaching <span class="soft">{host || session.host}</span>
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
        <span class="infopill"><span class="msr">lan</span><span class="mono">{session.host}</span></span>
      </div>
    </div>

    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="spacer"></div>

    <button class="primary" onclick={onConnected}>
      Open dashboard<span class="msr">arrow_forward</span>
    </button>
  {/if}
</div>
