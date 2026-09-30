<script lang="ts">
  import { goto } from "$app/navigation";
  import { onMount, tick } from "svelte";
  import { api } from "$lib/api";
  import type { Device, DeviceReport, PairedConnectProbe } from "$lib/types";
  import { explainAdbFailure, hostOf, subnetWarning } from "$lib/adb-errors";
  import { instanceSerial } from "$lib/mdns";
  import { deviceTypeLabel } from "$lib/types";
  import Icon from "$lib/components/Icon.svelte";
  import { getOpenNonTvIds, setOpenNonTv, idKey } from "$lib/prefs";

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
  /// adb's own words behind a failure `connectMessage` explains.
  let connectDetail = $state("");

  let scanBusy = $state(false);
  let scanMessage = $state("");

  let pairOpen = $state(false);
  let pairAddress = $state("");
  let pairPin = $state("");
  let pairBusy = $state(false);
  let pairMessage = $state("");
  let pairDetail = $state("");
  /// "typo?" when the pairing host is not on this computer's network.
  let pairWarning = $state<string | null>(null);
  let pairWarnToken = 0;

  /// After a successful pair: "waiting" polls mDNS for the connect port,
  /// "manual" is the fallback where the user types it (#88).
  let pairStage = $state<"idle" | "waiting" | "manual">("idle");
  let pairElapsed = $state(0);
  let pairWaitHost = "";
  let pairWaitToken = 0;
  const PAIR_CONNECT_WAIT_S = 45;
  const PAIR_PROBE_INTERVAL_MS = 1500;

  /// The row that connected last, so it can carry a call to action.
  let justConnected = $state<string | null>(null);
  let flashSerial = $state<string | null>(null);

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
        if (!devices.some((d) => d.status === "device")) {
          await scan();
        }
      }
    } catch (e) {
      installMessage = String(e);
    } finally {
      installBusy = false;
    }
  }

  function showConnectFailure(address: string, raw: string) {
    const explained = explainAdbFailure("connect", address, raw);
    connectMessage = explained.summary ?? explained.raw;
    connectDetail = explained.summary ? explained.raw : "";
  }

  /// The adb transport key `adb connect <address>` registers: a bare host
  /// gets the backend's `:5555` default.
  function connectKey(address: string): string {
    const a = address.trim();
    return /:\d+$/.test(a) ? a : `${a}:5555`;
  }

  async function connect() {
    const address = connectAddress.trim();
    if (!address) return;
    connectBusy = true;
    connectMessage = "";
    connectDetail = "";
    try {
      const r = await api.connectDevice(address);
      if (r.ok) {
        connectMessage = r.message.trim();
        connectAddress = "";
        pairNextStep = false;
        await refresh();
        await announceConnected(connectKey(address));
      } else {
        showConnectFailure(address, r.message);
      }
    } catch (e) {
      showConnectFailure(address, String(e));
    } finally {
      connectBusy = false;
    }
  }

  /// The row that just connected: panel closed, row scrolled to, outlined for
  /// a moment, and given its own call to action. Found by adb transport key
  /// only. If the key is not a listed row, nothing is highlighted rather than
  /// picking a row that merely shares its IP.
  ///
  /// `hardwareId` is the verified `ro.serialno` when the caller knows it.
  /// The list collapses one device's aliases into a single row and may keep
  /// a different key than the one just connected, so that row is found by
  /// its reported hardware id instead — still never by address.
  async function announceConnected(serial: string, hardwareId: string | null = null): Promise<Device | null> {
    const row =
      devices.find((d) => d.serial === serial) ??
      (hardwareId ? devices.find((d) => idKey(d.properties?.serial_number) === hardwareId) : undefined);
    cancelPairWait(false);
    pairOpen = false;
    pairNextStep = false;
    pairMessage = "";
    pairDetail = "";
    if (!row) return null;
    justConnected = row.serial;
    flashSerial = row.serial;
    await tick();
    const el = document.querySelector(`[data-serial="${CSS.escape(row.serial)}"]`);
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    el?.scrollIntoView({ block: "nearest", behavior: reduce ? "auto" : "smooth" });
    setTimeout(() => {
      if (flashSerial === row.serial) flashSerial = null;
    }, 2400);
    return row;
  }

  let connectInput = $state<HTMLInputElement | null>(null);
  /// Set after a successful pair: the connect box holds the paired host and
  /// is waiting for the separate connect port (#88).
  let pairNextStep = $state(false);

  /// The host part of a pairing address, without its port. The connect port
  /// is a different one, so only the host carries over.
  function pairedHost(address: string): string {
    const a = address.trim();
    if (a.startsWith("[")) {
      const end = a.indexOf("]");
      return end > 0 ? a.slice(0, end + 1) : a;
    }
    const m = /^([^:]+):\d+$/.exec(a);
    return m ? m[1] : a;
  }

  async function scan() {
    scanBusy = true;
    scanMessage = "Scanning local network…";
    try {
      const r = await api.scanNetwork();
      scanMessage = r.message;
      // A device advertising only a pairing service cannot be connected to
      // until the user enters the code from its screen. Open the pairing form
      // on its real advertised port rather than making them read two
      // different IP:port pairs off the TV (#88).
      const waiting = r.needs_pairing?.[0];
      if (waiting && !pairAddress.trim()) {
        pairAddress = waiting;
        pairOpen = true;
      }
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

  /// Do we positively know this is not an Android TV?
  ///
  /// Only when the device said so: a `phone`/`tablet`/`watch` characteristic,
  /// or a flat "no" to the leanback feature. This used to be inferred from
  /// `device_type === "unknown"`, which answers a different question — "no
  /// catalog match" — and so locked people out of perfectly ordinary TV boxes
  /// that report something unusual (#120).
  function isNotATv(d: Device): boolean {
    return d.tv_evidence === "not_tv";
  }

  /// Readable, and it never said either way. The tools open; the row is honest
  /// about not knowing. A device we could not read at all claims nothing.
  function isUnconfirmedTv(d: Device): boolean {
    return d.tv_evidence === "unknown" && d.properties !== null;
  }

  function toolsHref(d: Device): string {
    return `/devices/${encodeURIComponent(d.serial)}`;
  }

  /// Hardware ids of not-a-TV devices the user chose to open anyway.
  let openNonTv = $state<Set<string>>(getOpenNonTvIds());

  function openedAnyway(d: Device): boolean {
    const id = idKey(d.properties?.serial_number);
    return isNotATv(d) && !!id && openNonTv.has(id);
  }

  function deviceHref(d: Device): string | null {
    if (d.status !== "device") return null;
    // The device tools are all Android TV operations, so a device that said it
    // is not one does not open by default — unless its owner has already said
    // "open anyway" for this hardware (#120).
    if (isNotATv(d) && !openedAnyway(d)) return null;
    return toolsHref(d);
  }

  /// The row whose "Open anyway" confirm is showing.
  let confirmOpenSerial = $state<string | null>(null);

  function confirmOpenAnyway(d: Device) {
    const id = idKey(d.properties?.serial_number);
    // No hardware id means nothing to file the choice under, so it opens this
    // once and asks again next time rather than remembering it by address.
    if (id) openNonTv = setOpenNonTv(id, true);
    confirmOpenSerial = null;
    goto(toolsHref(d));
  }

  /// A control inside the row link. The row is the link, so its own actions
  /// have to say so explicitly or every click navigates instead.
  function rowAction(e: MouseEvent, run: () => void) {
    e.preventDefault();
    e.stopPropagation();
    run();
  }

  let diagnosticsBusy = $state<string | null>(null);
  let diagnosticsCopied = $state<string | null>(null);

  /// Copy the bug-report bundle for one device. Offered exactly where it is
  /// needed: a row the app cannot open, or one it is not sure about. Nothing
  /// is sent anywhere — it goes to the clipboard and no further.
  async function copyDiagnostics(d: Device) {
    diagnosticsBusy = d.serial;
    diagnosticsCopied = null;
    connectMessage = "";
    try {
      const report = await api.collectDiagnostics(d.serial);
      await navigator.clipboard.writeText(report);
      diagnosticsCopied = d.serial;
    } catch (e) {
      // The clipboard can be refused; say so rather than silently doing
      // nothing.
      connectMessage = `Couldn't copy diagnostics for ${d.serial}: ${e}`;
    } finally {
      diagnosticsBusy = null;
    }
  }

  /// What the authorization prompt is called on this device.
  ///
  /// Over USB it is reliably "Allow USB debugging?". Over the network the
  /// title varies by Android version and OEM — plenty of TVs still show the
  /// USB wording for a Wi-Fi connection — so don't swear to one.
  /// Held under an Android 11+ Wireless debugging key, so it appears in that
  /// screen's Paired devices list. Decided from the adb transport key alone:
  /// a `:5555` address is legacy network debugging and never does.
  function isWirelessDebuggingTransport(d: Device): boolean {
    return d.serial.includes("._adb-tls-connect.");
  }

  function authPromptLabel(d: Device): string {
    return d.connection === "usb" ? '"Allow USB debugging?"' : '"Allow debugging?"';
  }

  function showPairFailure(address: string, raw: string) {
    const explained = explainAdbFailure("pair", address, raw);
    pairMessage = explained.summary ?? explained.raw;
    pairDetail = explained.summary ? explained.raw : "";
  }

  $effect(() => {
    const host = hostOf(pairAddress);
    const token = ++pairWarnToken;
    if (!/^\d{1,3}(\.\d{1,3}){3}$/.test(host)) {
      pairWarning = null;
      return;
    }
    api
      .localAddressFor(host)
      .then((local) => {
        if (token === pairWarnToken) pairWarning = subnetWarning(host, local);
      })
      .catch(() => {
        if (token === pairWarnToken) pairWarning = null;
      });
  });

  async function pair() {
    const address = pairAddress.trim();
    if (!address || pairPin.length !== 6) return;
    cancelPairWait(false);
    pairBusy = true;
    pairMessage = "";
    pairDetail = "";
    pairNextStep = false;
    // Closing the panel bumps pairWaitToken. Capture it before the request so
    // a pair that finishes after the user cancelled never starts a wait that
    // would connect the device in the background.
    const requestToken = pairWaitToken;
    try {
      const r = await api.pairDevice(address, pairPin.trim());
      const cancelled = requestToken !== pairWaitToken || !pairOpen;
      if (r.ok && cancelled) {
        await refresh();
        return;
      }
      if (r.ok) {
        const host = pairedHost(address);
        pairAddress = "";
        // The manual fallback is ready from the start; the connect port is a
        // different one from the pairing port, so only the host carries over.
        if (host) connectAddress = `${host}:`;
        await refresh();
        void waitForConnect(address, host, r.instance);
      } else {
        showPairFailure(address, r.message);
      }
    } catch (e) {
      showPairFailure(address, String(e));
    } finally {
      pairPin = "";
      pairBusy = false;
    }
  }

  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  /// Watch mDNS for the connect endpoint the just-paired device advertises and
  /// connect to it. Android often holds that back while the pairing dialog is
  /// still open, which is why the owner only saw his phone after cancelling
  /// it. Only an advertised port is ever dialled: never a guess, never the
  /// pairing port.
  async function waitForConnect(pairTarget: string, host: string, instance: string | null) {
    const token = ++pairWaitToken;
    pairWaitHost = host;
    pairStage = "waiting";
    pairElapsed = 0;
    const started = Date.now();
    const hardwareId = instanceSerial(instance);
    /// The endpoint was advertised but refused us: that, not discovery, is
    /// what the user has to fix if the wait runs out.
    let lastFailure: { address: string; message: string } | null = null;
    while (token === pairWaitToken) {
      let probe: PairedConnectProbe;
      try {
        probe = await api.probePairedConnect(pairTarget, instance);
      } catch {
        probe = { state: "waiting" };
      }
      if (token !== pairWaitToken) return;
      if (probe.state === "unidentified") {
        fallBackToManual(
          "Paired. Close the pairing dialog on your phone/TV. The app couldn't tell which advertised device this is, so it won't connect by itself.",
        );
        return;
      }
      if (probe.state === "attached") {
        await refresh();
        if (token !== pairWaitToken) return;
        await pairConnected(probe.serial, hardwareId);
        return;
      }
      if (probe.state === "endpoint" && instance) {
        const r = await api
          .connectPaired(probe.address, instance)
          .catch((e) => ({ ok: false, not_the_paired_device: false, message: String(e) }));
        if (token !== pairWaitToken) return;
        if (r.ok) {
          await refresh();
          if (token !== pairWaitToken) return;
          await pairConnected(probe.address, hardwareId);
          return;
        }
        if (r.not_the_paired_device) {
          fallBackToManual(r.message);
          return;
        }
        lastFailure = { address: probe.address, message: r.message };
      }
      if (probe.state === "not_the_paired_device") {
        fallBackToManual(probe.message);
        return;
      }
      if (probe.state === "ambiguous") {
        fallBackToManual(
          `It advertises more than one connect port (${probe.addresses.join(", ")}). Use the one on its main Wireless debugging screen.`,
        );
        return;
      }
      pairElapsed = Math.min(PAIR_CONNECT_WAIT_S, Math.floor((Date.now() - started) / 1000));
      if (Date.now() - started >= PAIR_CONNECT_WAIT_S * 1000) {
        if (lastFailure) {
          const explained = explainAdbFailure("connect", lastFailure.address, lastFailure.message);
          fallBackToManual(
            `It advertised ${lastFailure.address}, but connecting kept failing. ${explained.summary ?? ""}`.trim(),
            explained.raw,
          );
        } else {
          fallBackToManual(`No connect port appeared within ${PAIR_CONNECT_WAIT_S} seconds.`);
        }
        return;
      }
      await sleep(PAIR_PROBE_INTERVAL_MS);
    }
  }

  async function pairConnected(serial: string, hardwareId: string | null) {
    const row = await announceConnected(serial, hardwareId);
    connectAddress = "";
    connectMessage = row ? `Connected ${row.name}.` : `Connected ${serial}.`;
    connectDetail = "";
  }

  /// Stop watching. `toManual` hands over to the typed fallback, which is
  /// what Cancel does; a new pair or a closed panel just stops.
  function cancelPairWait(toManual: boolean) {
    const wasWaiting = pairStage === "waiting";
    pairWaitToken++;
    if (wasWaiting && toManual) {
      fallBackToManual("Stopped waiting.");
    } else if (wasWaiting) {
      pairStage = "idle";
    }
  }

  async function fallBackToManual(why: string, detail = "") {
    pairWaitToken++;
    pairStage = "manual";
    pairMessage = why;
    pairDetail = detail;
    if (pairWaitHost) connectAddress = `${pairWaitHost}:`;
    pairNextStep = true;
    await tick();
    connectInput?.focus();
    const end = connectAddress.length;
    connectInput?.setSelectionRange(end, end);
  }

  function togglePair() {
    if (pairOpen) cancelPairWait(false);
    pairOpen = !pairOpen;
  }

  async function openPairPanel() {
    pairOpen = true;
    await tick();
    const input = document.querySelector<HTMLInputElement>(".pair-form input");
    input?.scrollIntoView({ block: "nearest" });
    input?.focus();
  }

  function focusConnectBox() {
    connectInput?.scrollIntoView({ block: "nearest" });
    connectInput?.focus();
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
  let forgetBusy = $state<string | null>(null);

  /// Drop a network transport so the row stops appearing. Only offered for
  /// network devices: `adb disconnect` is the only thing "forget" can mean
  /// here, and it has nothing to drop for a USB one. Not a delete — the TV is
  /// untouched and reconnects the moment you add it again.
  ///
  /// One device can be held under several adb keys (its mDNS name and an
  /// `ip:port`), and the row shows only one, so the backend drops every key
  /// with the same verified hardware id. A paired device that is still
  /// advertising Wireless debugging is re-attached by adb within seconds; the
  /// backend says so, and the list is re-read so it never shows a device as
  /// gone while it is back.
  async function forgetDevice(d: Device) {
    if (d.connection !== "network") return;
    forgetBusy = d.serial;
    connectMessage = "";
    connectDetail = "";
    if (justConnected === d.serial) justConnected = null;
    try {
      const r = await api.forgetDevice(d.serial);
      await refresh();
      if (!r.ok || r.still_advertised) connectMessage = r.message;
      if (r.ok && r.still_advertised) void watchReattach(d);
    } catch (e) {
      connectMessage = `Could not disconnect ${d.serial}: ${e}`;
    } finally {
      forgetBusy = null;
    }
  }

  let reattachToken = 0;
  const REATTACH_WATCH_MS = 15000;

  async function watchReattach(d: Device) {
    const token = ++reattachToken;
    const id = idKey(d.properties?.serial_number);
    const started = Date.now();
    while (token === reattachToken && Date.now() - started < REATTACH_WATCH_MS) {
      await sleep(3000);
      if (token !== reattachToken) return;
      let list: Device[];
      try {
        list = await api.listDevices();
      } catch {
        return;
      }
      if (token !== reattachToken) return;
      devices = list;
      if (id && list.some((x) => idKey(x.properties?.serial_number) === id)) {
        connectMessage =
          `${d.name} reconnected by itself: it is still advertising Wireless debugging, and adb ` +
          "re-attaches paired devices while it does. To keep it off this list, turn off Wireless " +
          "debugging on the device, or remove this computer under Wireless debugging → Paired devices there.";
        return;
      }
    }
  }

  async function bootDiscovery() {
    await refresh();
    if (adbMissing) return;
    if (devices.some((d) => d.status === "device")) return;
    await scan();
  }

  onMount(() => {
    void bootDiscovery();
    return () => {
      pairWaitToken++;
      reattachToken++;
    };
  });
</script>

<section class="header-row">
  <div class="header-title">
    <h1>Devices</h1>
    <p class="muted small mono header-sub">
      {#if adbMissing}
        adb not found
      {:else if devices.length === 0}
        no TV connected
      {:else}
        {devices.length} connected · {devices.filter((d) => d.status === "device").length} ready
      {/if}
    </p>
  </div>
  <div class="header-actions">
    <button onclick={scan} disabled={scanBusy || adbMissing} title="Scan the local /24 subnet for ADB-listening devices">
      <Icon name="wifi_tethering" size={16} /> {scanBusy ? "Scanning…" : "Scan LAN"}
    </button>
    <button onclick={refresh} disabled={loading} title="Re-read the list of connected devices">
      {loading ? "Refreshing…" : "Refresh"}
    </button>
  </div>
</section>

<!-- The board leads with Scan LAN and Add by IP. The other four are still
     here, one row down, because every one of them is the only way to do the
     thing it does. -->
<section class="connect-form">
  <input
    placeholder="IP[:port] — e.g. 192.168.42.71"
    bind:value={connectAddress}
    bind:this={connectInput}
    onkeydown={(e) => e.key === "Enter" && connect()}
  />
  <button class="primary" onclick={connect} disabled={connectBusy || !connectAddress.trim()}>
    <Icon name="add" size={16} /> {connectBusy ? "Connecting…" : "Add by IP"}
  </button>
  <button onclick={togglePair} disabled={adbMissing} title="PIN pairing, for a device that only offers Wireless debugging">
    {pairOpen ? "Cancel Pair" : "Pair PIN"}
  </button>
  <button onclick={restartAdb} disabled={restartBusy || adbMissing} title="adb kill-server then start-server">
    {restartBusy ? "Restarting…" : "Restart ADB"}
  </button>
  <button onclick={reportAll} disabled={reportBusy || adbMissing} title="Run a health report against every connected device">
    {reportBusy ? "Reporting…" : "Report All"}
  </button>
  {#if pairNextStep}
    <p class="connect-message pair-next">
      Enter the port shown on the TV or phone's main Wireless debugging screen, then Add by IP.
    </p>
  {/if}
  {#if connectMessage}
    <p class="connect-message muted" class:plain={!!connectDetail}>{connectMessage}</p>
  {/if}
  {#if connectDetail}
    <details class="adb-details connect-details">
      <summary>Details</summary>
      <pre>{connectDetail}</pre>
    </details>
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
      On the TV or phone, open Developer options (Settings → System on Google TV and phones;
      Settings → Device Preferences on older Android TV and Shield), then Wireless debugging → Pair
      device with pairing code. Enter the IP:port and 6-digit PIN from that dialog.
    </p>
    <p class="pair-note small">
      After it pairs, close the pairing dialog; the app connects automatically. If it doesn't
      appear, enter the IP:port from the main Wireless debugging screen and click
      <strong>Add by IP</strong>. Do not reuse the pairing port.
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
        disabled={pairBusy || pairStage === "waiting" || !pairAddress.trim() || pairPin.length !== 6}
      >
        {pairBusy ? "Pairing…" : "Pair"}
      </button>
    </div>
    {#if pairWarning}
      <p class="pair-warning small" role="status">{pairWarning}</p>
    {/if}
    {#if pairStage === "waiting"}
      <div class="pair-waiting" role="status">
        <p class="pair-paired"><strong>Paired.</strong> Close the pairing dialog on your phone/TV.</p>
        <div class="pair-progress-row">
          <span class="spinner" aria-hidden="true"></span>
          <span class="muted small">
            Waiting for the device to advertise its connect port… {pairElapsed}s of {PAIR_CONNECT_WAIT_S}s
          </span>
          <button class="row-action" onclick={() => cancelPairWait(true)}>Cancel</button>
        </div>
      </div>
    {/if}
    {#if pairMessage}
      <p class="small" class:muted={!pairDetail} class:pair-error={!!pairDetail}>{pairMessage}</p>
    {/if}
    {#if pairDetail}
      <details class="adb-details">
        <summary>Details</summary>
        <pre>{pairDetail}</pre>
      </details>
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
      ATV Optimizer needs Android's <code>adb</code> binary to talk to your TV.
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
      Use Scan LAN or Add by IP above. A device that only offers Wireless debugging may need Pair
      PIN first.
    </p>
  </div>
{:else}
  <ul class="device-list">
    {#each sortedDevices as d (d.serial)}
      {@const href = deviceHref(d)}
      <li>
        {#if href}
          <a
            class="device-row clickable"
            class:flash={flashSerial === d.serial}
            href={href}
            data-serial={d.serial}
          >
            <span class="device-icon" aria-hidden="true">
              <Icon name={d.connection === "network" ? "cast_connected" : "tv"} size={20} />
            </span>
            <div class="device-main">
              <div class="device-name">
                <span>{d.name}</span>
                <!-- Whether it is reachable belongs with the name, not stranded
                     at the far edge: it qualifies the device, and reading it
                     meant crossing the address line to get there. -->
                <span class="device-status online">
                  <span class="status-dot" aria-hidden="true"></span> Online
                </span>
                {#if openedAnyway(d)}
                  <span
                    class="status-tag not-a-tv"
                    data-tip="Reported it is not an Android TV; you chose to open it anyway"
                  >NOT A TV</span>
                {:else if isUnconfirmedTv(d)}
                  <span
                    class="status-tag unconfirmed"
                    data-tip="Didn't report itself as an Android TV; tools may not apply"
                  >UNCONFIRMED TV</span>
                {/if}
              </div>
              <div class="device-meta muted mono">
                {d.serial} · {deviceTypeLabel(d.device_type)}
                {#if d.model}· {d.model}{/if}
                · {d.connection === "network" ? "network" : "usb"}
              </div>
            </div>
            <!-- Row actions. They live inside the link, so each one has to
                 stop the click reaching it — otherwise Forget would navigate
                 to the device it just disconnected. -->
            <span class="row-actions">
              {#if justConnected === d.serial}
                <button class="row-action primary" onclick={(e) => rowAction(e, () => goto(toolsHref(d)))}>
                  Open
                </button>
              {/if}
              {#if isUnconfirmedTv(d)}
                <button
                  class="row-action"
                  onclick={(e) => rowAction(e, () => copyDiagnostics(d))}
                  disabled={diagnosticsBusy === d.serial}
                  data-tip="Copies what this device reported — paste it into a bug report"
                >
                  {diagnosticsBusy === d.serial
                    ? "Copying…"
                    : diagnosticsCopied === d.serial
                      ? "Copied"
                      : "Copy diagnostics"}
                </button>
              {/if}
              {#if d.connection === "network"}
                <button
                  class="row-action forget-btn"
                  onclick={(e) => rowAction(e, () => forgetDevice(d))}
                  disabled={forgetBusy === d.serial}
                  data-tip="Disconnects every connection to this device · the device is untouched"
                  data-tip-align="end"
                >
                  {forgetBusy === d.serial ? "Forgetting…" : "Forget"}
                </button>
              {/if}
            </span>
            <!-- A chevron says "this opens" without pretending to be
                 separately clickable. -->
            <span class="device-go" aria-hidden="true"><Icon name="chevron_right" size={28} /></span>
          </a>
        {:else}
          <div
            class="device-row not-clickable"
            class:unauthorized={d.status === "unauthorized"}
            class:flash={flashSerial === d.serial}
            data-serial={d.serial}
          >
            <span class="device-icon" aria-hidden="true">
              <Icon name={d.status === "offline" ? "tv_off" : d.connection === "network" ? "cast" : "tv"} size={20} />
            </span>
            <div class="device-main">
              <div class="device-name">
                <span>{d.name}</span>
                {#if d.status === "unauthorized"}
                  <span class="status-tag unauthorized">UNAUTHORIZED</span>
                {:else if d.status === "offline"}
                  <span class="status-tag offline">OFFLINE</span>
                {/if}
                {#if isNotATv(d)}
                  <span class="status-tag not-a-tv">NOT AN ANDROID TV</span>
                {:else if isUnconfirmedTv(d)}
                  <span
                    class="status-tag unconfirmed"
                    data-tip="Didn't report itself as an Android TV; tools may not apply"
                  >UNCONFIRMED TV</span>
                {/if}
              </div>
              <div class="device-meta muted mono">
                {deviceTypeLabel(d.device_type)}
                {#if d.model}· {d.model}{/if}
                · {d.serial}
              </div>
              {#if justConnected === d.serial && isNotATv(d) && d.status === "device"}
                <p class="small just-connected-note">
                  Connected. It reported it isn't an Android TV, so it doesn't open by default; use
                  Open anyway to try the tools.
                </p>
              {/if}
              {#if d.status === "unauthorized"}
                <div class="unauthorized-help">
                  <strong>This device needs to be authorized:</strong>
                  <ol>
                    <li>
                      Look at the TV — there should be an <em>{authPromptLabel(d)}</em> dialog.
                      {#if d.connection === "network"}
                        <span class="muted">(some TVs still say "USB" even over Wi-Fi)</span>
                      {/if}
                    </li>
                    <li>Check <em>"Always allow from this computer"</em>.</li>
                    <li>Click <em>Allow</em>.</li>
                    <li>Click Refresh above.</li>
                  </ol>
                  <p class="muted small">If you don't see the dialog:</p>
                  <ol class="small unauthorized-fallback">
                    <li>Wake the TV; the prompt can be hidden behind the screensaver.</li>
                    {#if d.connection === "network"}
                      <li>Click <strong>Forget</strong> here, then Add by IP again; the prompt reappears.</li>
                    {:else}
                      <li>Unplug the cable and plug it back in; the prompt reappears.</li>
                    {/if}
                    <li>
                      Still nothing? On the TV, open Developer options and choose Revoke USB debugging
                      authorizations{#if isWirelessDebuggingTransport(d)} (or Wireless debugging → tap this
                        computer → Forget){/if}. That un-trusts every computer, not just this one. Then
                      reconnect.
                    </li>
                  </ol>
                </div>
              {/if}
              {#if confirmOpenSerial === d.serial}
                <div class="open-anyway-confirm" role="group" aria-label="Open anyway">
                  <p class="small">
                    This device reported that it is not an Android TV. The tools are built for
                    Android TV, so some of them may not apply here.
                  </p>
                  <div class="open-anyway-actions">
                    <button class="primary" onclick={() => confirmOpenAnyway(d)}>Open tools</button>
                    <button onclick={() => (confirmOpenSerial = null)}>Cancel</button>
                  </div>
                </div>
              {/if}
            </div>
            <!-- A row the app will not open is exactly the row someone needs
                 to report, so the bundle is one click away from it. "Forget"
                 is `adb disconnect`: only for network transports, because
                 there is nothing to disconnect on USB. It removes the row,
                 not the device — adding the address back brings it straight
                 home. -->
            <span class="row-actions">
              {#if isNotATv(d) && d.status === "device"}
                <button
                  class="row-action"
                  class:primary={justConnected === d.serial}
                  onclick={() => (confirmOpenSerial = d.serial)}
                  disabled={confirmOpenSerial === d.serial}
                  data-tip="Open the tools on this device even though it is not an Android TV"
                >
                  Open anyway
                </button>
              {/if}
              <button
                class="row-action"
                onclick={() => copyDiagnostics(d)}
                disabled={diagnosticsBusy === d.serial}
                data-tip="Copies what this device reported — paste it into a bug report"
              >
                {diagnosticsBusy === d.serial
                  ? "Copying…"
                  : diagnosticsCopied === d.serial
                    ? "Copied"
                    : "Copy diagnostics"}
              </button>
              {#if d.connection === "network"}
                <button
                  class="row-action forget-btn"
                  onclick={() => forgetDevice(d)}
                  disabled={forgetBusy === d.serial}
                  data-tip="Disconnects every connection to this device · the device is untouched"
                  data-tip-align="end"
                >
                  {forgetBusy === d.serial ? "Forgetting…" : "Forget"}
                </button>
              {/if}
            </span>
          </div>
        {/if}
      </li>
    {/each}
  </ul>
{/if}

{#if !adbMissing}
  <div class="callout devices-note">
    <Icon name="info" size={16} />
    <span>
      <strong>Don't see your device?</strong> Some devices, mostly newer ones that only offer
      Wireless debugging, need a one-time pairing: use
      <button class="inline-link" onclick={openPairPanel}>Pair PIN</button>. Devices with Network
      debugging connect directly with
      <button class="inline-link" onclick={focusConnectBox}>Add by IP</button>.
    </span>
  </div>
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
    font-family: var(--mono);
  }
  .device-list {
    list-style: none;
    padding: 0;
    margin: 0;
  }
  /* Board 11.12's row: a glyph for what it is, the name, the address in mono,
     whether it is reachable, and the one thing to do about it. The [NET] tag
     folded into the meta line — a bracketed word beside the name read as part
     of the name. */
  .device-row {
    display: flex;
    align-items: center;
    gap: 1rem;
    padding: 0.9rem 1rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    margin-bottom: 0.6rem;
    transition: background 0.1s;
    text-decoration: none;
    color: inherit;
  }
  .device-row.flash {
    animation: row-flash 2.4s ease-out;
  }
  @keyframes row-flash {
    0%,
    45% {
      box-shadow: 0 0 0 2px var(--accent);
    }
    100% {
      box-shadow: 0 0 0 2px transparent;
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .device-row.flash {
      animation: none;
      box-shadow: 0 0 0 2px var(--accent);
    }
  }
  .just-connected-note {
    margin: 0.35rem 0 0;
  }
  .device-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 2.8rem;
    height: 2.8rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-inset);
    color: var(--fg-muted);
  }
  a.device-row .device-icon {
    color: var(--fg-secondary);
  }
  .device-status {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    flex: none;
    font-size: 0.78rem;
    font-weight: 500;
    color: var(--fg-muted);
  }
  .status-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: var(--fg-muted);
  }
  .device-status.online {
    color: var(--ok);
  }
  .device-status.online .status-dot {
    background: var(--ok);
  }
  /* The row's own controls, pushed to the far end and kept clear of the
     chevron. Secondary by design: the row itself is the primary action. */
  /* As tall as the device icon, so in a row that top-aligns for its help text
     (unauthorized) the buttons still sit on the icon's centre line. */
  .row-actions {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
    flex: none;
    margin-left: auto;
    min-height: 2.8rem;
  }
  .row-action {
    flex: none;
    padding: 0.3rem 0.8rem;
    font-size: 0.82rem;
  }
  .forget-btn {
    flex: none;
    padding: 0.3rem 0.8rem;
    font-size: 0.82rem;
  }
  .device-go {
    display: inline-flex;
    align-items: center;
    flex: none;
    margin-left: 0.6rem;
    color: var(--accent);
  }
  a.device-row:hover .device-go {
    transform: translateX(2px);
  }
  .devices-note {
    margin-top: 1rem;
  }
  .inline-link {
    display: inline;
    padding: 0;
    border: none;
    background: none;
    font: inherit;
    color: var(--accent);
    text-decoration: underline;
    text-underline-offset: 2px;
    cursor: pointer;
    vertical-align: baseline;
  }
  .inline-link:hover {
    background: none;
    text-decoration-thickness: 2px;
  }
  .inline-link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 2px;
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
  .header-title {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .header-title h1 {
    margin: 0;
  }
  .header-sub {
    margin: 0;
  }
  .header-actions {
    display: flex;
    gap: 0.6rem;
    align-items: center;
  }
  .status-tag {
    font-size: 0.72rem;
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-sm);
  }
  .status-tag.unauthorized {
    background: var(--danger-surface);
    color: var(--danger-text);
  }
  .status-tag.offline {
    background: var(--bg-muted);
    color: var(--fg-faint);
  }
  /* Informational, not a problem — this device is simply not what the app is
     for. Deliberately quieter than the warning tags above it. */
  .status-tag.not-a-tv {
    background: var(--bg-muted);
    color: var(--fg-faint);
  }
  /* Amber: a caveat, not a refusal. The row still opens — this says only that
     the device never confirmed what it is, so a tool may not land. */
  .status-tag.unconfirmed {
    background: var(--warn-surface-2);
    color: var(--warn);
    white-space: nowrap;
  }
  .device-meta {
    font-size: 0.82rem;
    margin-top: 0.2rem;
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
    border-radius: var(--radius-lg);
    padding: 1rem 1.2rem;
    margin-bottom: 1rem;
  }
  .pair-form h3 {
    margin: 0 0 0.4rem;
    font-size: 1rem;
  }
  .pair-note {
    padding: 0.65rem 0.75rem;
    border-radius: var(--radius-md);
    background: var(--bg-inset);
    color: var(--fg-secondary);
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
  .pair-warning {
    margin: 0.5rem 0 0;
    padding: 0.4rem 0.65rem;
    border: 1px solid var(--warn-border);
    border-radius: var(--radius-md);
    background: var(--warn-surface);
    color: var(--warn);
  }
  .pair-waiting {
    margin-top: 0.75rem;
    padding: 0.65rem 0.75rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-inset);
  }
  .pair-paired {
    margin: 0 0 0.4rem;
  }
  .pair-progress-row {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
  }
  .pair-progress-row .row-action {
    margin-left: auto;
  }
  .spinner {
    flex: none;
    width: 0.9rem;
    height: 0.9rem;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.9s linear infinite;
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .spinner {
      animation: none;
    }
  }
  .pair-error {
    margin: 0.6rem 0 0;
    color: var(--fg-primary);
  }
  .adb-details {
    margin-top: 0.3rem;
    font-size: 0.8rem;
    color: var(--fg-muted);
  }
  .adb-details pre {
    margin: 0.3rem 0 0;
    white-space: pre-wrap;
    word-break: break-word;
    font-family: var(--mono);
  }
  .connect-details {
    flex-basis: 100%;
  }
  .connect-message.plain {
    font-family: inherit;
  }
  .report-all {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
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
    border-bottom: 1px solid var(--border);
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
  .open-anyway-confirm {
    margin-top: 0.6rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .open-anyway-confirm p {
    margin: 0 0 0.5rem;
  }
  .open-anyway-actions {
    display: flex;
    gap: 0.5rem;
  }
  .unauthorized-help {
    margin-top: 0.6rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--danger-surface);
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-lg);
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
    font-family: var(--mono);
  }
  .small {
    font-size: 0.82rem;
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-family: var(--mono);
    font-size: 0.85rem;
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 0.85em;
  }
</style>
