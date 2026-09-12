<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import {
    autoConnectEnabled,
    lastUsedLabel,
    listSavedDevices,
    savedDeviceKey,
    savedHostHasMultipleIdentities,
    shouldAutoDialSavedDevices,
  } from "../lib/savedDevices";
  import { buildDiscoveryRows } from "../lib/discoveryRows";
  import type { DiscoveryRow } from "../lib/discoveryRows";
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
  let reconnectErrorKey = $state("");
  let connectingKey = $state("");
  let connectingName = $state("");
  let reconnectPending = false;
  let connectionAttempt = 0;
  const primarySaved = $derived(savedDevices[0] ?? null);

  onMount(() => {
    savedDevices = listSavedDevices();
    if (intent !== "launch" || savedDevices.length === 0) return;
    step = "reconnect";
    if (shouldAutoDialSavedDevices(savedDevices, autoConnectEnabled())) {
      attemptReconnect(savedDevices[0]);
    }
  });

  async function attemptReconnect(d: SavedDevice) {
    if (connectingKey) return;
    const attempt = ++connectionAttempt;
    reconnectError = "";
    reconnectErrorKey = "";
    connectingKey = savedDeviceKey(d);
    connectingName = d.name;
    reconnectPending = true;
    try {
      const r = await session.connect(d.host, d.connectPort);
      // Cancelled attempts resolve later; ignore them.
      if (attempt !== connectionAttempt) return;
      if (r.ok) {
        savedDevices = listSavedDevices();
        // Retire the pending marker before navigation unmounts this screen so
        // onDestroy does not cancel the connection we just established.
        reconnectPending = false;
        connectingKey = "";
        onConnected();
      } else {
        reconnectError = r.message || "Couldn't reach that TV.";
        reconnectErrorKey = savedDeviceKey(d);
      }
    } catch (e) {
      if (attempt !== connectionAttempt) return;
      reconnectError = String(e);
      reconnectErrorKey = savedDeviceKey(d);
    } finally {
      if (attempt === connectionAttempt) {
        reconnectPending = false;
        connectingKey = "";
      }
    }
  }

  function cancelReconnect() {
    ++connectionAttempt;
    reconnectPending = false;
    void session.cancelConnect().catch((e) => { reconnectError = String(e); });
    connectingKey = "";
    reconnectError = "";
    reconnectErrorKey = "";
  }

  function goScan() {
    if (connectingKey) cancelReconnect();
    reconnectError = "";
    reconnectErrorKey = "";
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
  let connectAfterPair = $state<number | null>(null);
  let notice = $state("");
  let scanGeneration = 0;

  const codeReady = $derived(code.length === 6);

  const found = $derived(buildDiscoveryRows(discoveries, savedDevices, {
    connected: session.isConnected,
    host: session.host,
    connectPort: session.connectPort,
  }));

  const foundLabel = "Connection options";

  async function scan() {
    const generation = ++scanGeneration;
    error = "";
    notice = "";
    busy = true;
    try {
      const result = await api.wirelessDiscover();
      if (generation !== scanGeneration) return;
      discoveries = result.devices;
      scanned = true;
    } catch (e) {
      if (generation === scanGeneration) error = String(e);
    } finally {
      if (generation === scanGeneration) busy = false;
    }
  }

  function leaveScan() {
    ++scanGeneration;
    busy = false;
  }

  function connectFound(row: DiscoveryRow, advertisedPort: number) {
    leaveScan();
    error = "";
    notice = "";
    host = row.host;
    connectPort = advertisedPort;
    connectAfterPair = null;
    needsPairing = false;
    startConnect();
  }

  function pairFound(row: DiscoveryRow, advertisedPort: number) {
    leaveScan();
    error = "";
    notice = "";
    host = row.host;
    pairPort = advertisedPort;
    connectAfterPair = row.connectPorts.length === 1 ? row.connectPorts[0] : null;
    if (connectAfterPair !== null) connectPort = connectAfterPair;
    code = "";
    needsPairing = true;
    step = "pair";
  }

  function retrySaved(row: DiscoveryRow) {
    if (!row.savedTarget) return;
    leaveScan();
    step = "reconnect";
    attemptReconnect(row.savedTarget);
  }

  function openDashboard() {
    leaveScan();
    onConnected();
  }

  function endpointLabel(row: DiscoveryRow): string {
    const endpoints: string[] = [];
    if (row.connectPorts.length > 0) {
      endpoints.push(`connect ${row.connectPorts.map((port) => `:${port}`).join(", ")}`);
    }
    if (row.pairingPorts.length > 0) {
      endpoints.push(`pair ${row.pairingPorts.map((port) => `:${port}`).join(", ")}`);
    }
    return [row.host, ...endpoints].join(" · ");
  }

  function manualPair() {
    if (!host) return;
    leaveScan();
    error = "";
    notice = "";
    code = "";
    connectAfterPair = Number(connectPort);
    needsPairing = true;
    step = "pair";
  }

  function manualConnect() {
    if (!host) return;
    leaveScan();
    error = "";
    notice = "";
    connectAfterPair = null;
    needsPairing = false;
    startConnect();
  }

  function onCodeInput(e: Event) {
    const el = e.currentTarget as HTMLInputElement;
    code = el.value.replace(/\D/g, "").slice(0, 6);
  }

  async function startConnect() {
    const attempt = ++connectionAttempt;
    error = "";
    notice = "";
    step = "connecting";
    busy = true;
    try {
      if (needsPairing) {
        const paired = await session.pair(host, Number(pairPort), code);
        if (attempt !== connectionAttempt) return;
        if (!paired.ok) {
          error = paired.message;
          return;
        }
        if (connectAfterPair === null) {
          code = "";
          needsPairing = false;
          notice = "Pairing succeeded. Scan again to choose an advertised connect service.";
          step = "scan";
          return;
        }
        connectPort = connectAfterPair;
      }
      const result = await session.connect(host, Number(connectPort));
      if (attempt !== connectionAttempt) return;
      if (!result.ok) {
        error = result.message;
        return;
      }
      savedDevices = listSavedDevices();
      step = "connected";
    } catch (e) {
      if (attempt === connectionAttempt) error = String(e);
    } finally {
      if (attempt === connectionAttempt) busy = false;
    }
  }

  onDestroy(() => {
    ++scanGeneration;
    ++connectionAttempt;
    if (reconnectPending || (busy && step === "connecting")) {
      void session.cancelConnect().catch(() => {});
    }
  });

  function backToScan() {
    error = "";
    notice = "";
    code = "";
    connectAfterPair = null;
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
      <span class="statuspill" class:live={connectingKey !== ""}>
        <span class="pdot" class:blink={connectingKey !== ""}></span>{connectingKey ? "Connecting" : "Ready"}
      </span>
    </div>

    <p class="eyebrow">Welcome back</p>
    <h1>
      {#if reconnectError}
        Couldn't reconnect
      {:else if connectingKey}
        Connecting to {connectingName}…
      {:else}
        Which TV?
      {/if}
    </h1>
    <p class="lede">
      {#if reconnectError}
        <span class="soft">{connectingName || "That TV"}</span> isn't reachable right now.
      {:else if connectingKey}
        {savedDevices.length === 1
          ? "Your only saved TV — no code needed. Cancel to pick a different one."
          : "Connecting only to the saved entry you selected."}
      {:else}
        Pick a saved TV to connect. Nothing connects until you choose.
      {/if}
    </p>

    <div class="devices">
      {#each savedDevices as d, i (savedDeviceKey(d))}
        <button
          class="device"
          class:dialing={connectingKey === savedDeviceKey(d)}
          onclick={() => attemptReconnect(d)}
          disabled={connectingKey !== ""}
        >
          <span class="device-icon"><span class="msr">{d.deviceType === "shield" ? "cast" : "tv"}</span></span>
          <span class="device-body">
            <span class="device-name">{d.name}</span>
            <span class="mono device-addr">{d.host}:{d.connectPort} · {deviceTypeLabel(d.deviceType)} · {lastUsedLabel(d.lastUsed)}</span>
            {#if connectingKey === savedDeviceKey(d)}
              <span class="device-tag">Handshaking…</span>
            {:else if reconnectError && reconnectErrorKey === savedDeviceKey(d)}
              <span class="device-tag">Couldn't reconnect</span>
            {:else if savedHostHasMultipleIdentities(savedDevices, d.host)}
              <span class="device-tag">Shared saved address · identities kept separate</span>
            {:else if i === 0 && savedDevices.length > 1}
              <span class="device-tag">Last used</span>
            {/if}
          </span>
          {#if connectingKey !== savedDeviceKey(d)}<span class="msr device-go">arrow_forward</span>{/if}
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

    {#if connectingKey}
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
        {#each found as row (row.key)}
          <div class="device">
            <span class="device-icon"><span class="msr">cast</span></span>
            <span class="device-body">
              <span class="device-name">{row.name}</span>
              <span class="mono device-addr">{endpointLabel(row)}</span>
              {#if row.status === "connected"}
                <span class="device-tag">Connected on :{session.connectPort}</span>
              {:else if row.status === "saved-address"}
                <span class="device-tag">Saved address (unverified)</span>
              {:else if row.status === "saved-other-port"}
                <span class="device-tag">Saved · This address answered on another port</span>
              {:else if row.status === "saved-missing"}
                <span class="device-tag">Saved · Not found in this scan</span>
              {:else}
                <span class="device-tag">Found on network</span>
              {/if}
              {#if row.legacyConnectPorts.length > 0 && row.pairingPorts.length === 0}
                <span class="device-tag">No code needed</span>
              {/if}
            </span>
            <div class="manual-actions">
              {#if row.status === "connected"}
                <button class="primary small" onclick={openDashboard}>Open dashboard</button>
              {:else if row.source === "saved"}
                <button class="ghost small" onclick={() => retrySaved(row)}>Retry connection</button>
              {/if}
              {#if row.source === "discovery"}
                {#each row.connectPorts as port (`connect:${row.key}:${port}`)}
                  {#if row.status !== "connected" || port !== session.connectPort}
                    <button class="primary small" onclick={() => connectFound(row, port)}>
                      {row.status === "connected" || row.connectPorts.length > 1 ? `Connect :${port}` : "Connect"}
                    </button>
                  {/if}
                {/each}
                {#each row.pairingPorts as port (`pair:${row.key}:${port}`)}
                  <button class="ghost small" onclick={() => pairFound(row, port)}>
                    {row.status === "connected" || row.pairingPorts.length > 1 ? `Pair :${port}` : "Pair with code"}
                  </button>
                {/each}
              {/if}
            </div>
          </div>
        {/each}
      </div>
    {:else if scanned}
      <p class="section-label">No devices found</p>
      <p class="lede empty">Confirm Wireless debugging is on, then scan again.</p>
    {/if}

    {#if notice}<p class="lede empty" role="status">{notice}</p>{/if}
    {#if error}<p class="error" role="alert">{error}</p>{/if}

    <div class="spacer"></div>

    {#if intent === "launch" && savedDevices.length > 0}
      <button class="ghost-link" onclick={() => { leaveScan(); error = ""; notice = ""; step = "reconnect"; }}>
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
        <span class="soft">New: code pairing.</span> If it fails, turn on
        <span class="soft">Network debugging</span> (Developer options) on the TV and use
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
      {connectAfterPair === null ? "Pair with code" : "Pair & connect"}
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
