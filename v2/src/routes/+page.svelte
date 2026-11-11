<script lang="ts">
  import HelpDetails from "$lib/components/HelpDetails.svelte";
  import ApprovalDialog, { type ApprovalPhase } from "$lib/components/ApprovalDialog.svelte";
  import { goto } from "$app/navigation";
  import { onMount, tick } from "svelte";
  import { tip } from "$lib/tip";
  import { onSupportTiersUpdated } from "$lib/support-updates";
  import { api } from "$lib/api";
  import { confirmAction } from "$lib/confirm";
  import type { Device, DeviceReport, PairedConnectProbe, ScanConnectionProgress, NativeAdvertisement, NativeDiscoveryReport } from "$lib/types";
  import {
    explainAdbFailure,
    hostOf,
    isNoRoute,
    LOCAL_NETWORK_POSSIBLE,
    subnetWarning,
    type LocalNetworkHint,
  } from "$lib/adb-errors";
  import {
    freshLocalNetworkStatus,
    localNetwork,
    openLocalNetworkSettings,
  } from "$lib/local-network.svelte";
  import LocalNetworkNotice from "$lib/components/LocalNetworkNotice.svelte";
  import { instanceSerial } from "$lib/mdns";
  import { deviceTypeLabel } from "$lib/types";
  import { nativeScanRows, type DeviceRow } from "$lib/native-scan-rows";
  import { buildNativeDiscoveryRows, discoveryAddressUsable } from "$lib/native-discovery-rows";
  import NativeConnectionChoices from "$lib/components/NativeConnectionChoices.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import { getOpenNonTvIds, setOpenNonTv, idKey } from "$lib/prefs";
  import {
    QUIT_ADB_TIP,
    stopOnQuit,
    refreshStopOnQuit,
    toggleStopOnQuit,
  } from "$lib/stop-adb-on-quit.svelte";
  import { openContextMenu, type MenuItem } from "$lib/contextmenu";
  import ErrorNotice from "$lib/components/ErrorNotice.svelte";
  import SupportBadge from "$lib/components/SupportBadge.svelte";
  import SupportBanner from "$lib/components/SupportBanner.svelte";

  let devices = $state<Device[]>([]);
  let nativeConnections = $state<ScanConnectionProgress>({ pending: [], failed: [] });
  let nativeDiscovery = $state<NativeDiscoveryReport | null>(null);
  let discoveryReceivedAt = $state(0);
  let scanDetailsOpen = $state(false);
  let adbBackend = $state<"direct" | "subprocess" | null>(null);

  /// Sort priority: authorized first (status === "device"), then unauthorized
  /// (the user can act on them via the inline guidance), then offline. Tiebreak
  /// by display name so the order doesn't shuffle on refresh.
  const STATUS_ORDER: Record<string, number> = { device: 0, unauthorized: 1, offline: 2 };
  let sortedDevices = $derived(
    (adbBackend === "direct" ? nativeScanRows(devices, nativeConnections) : [...devices] as DeviceRow[]).sort((a, b) => {
      const sa = STATUS_ORDER[a.status] ?? 9;
      const sb = STATUS_ORDER[b.status] ?? 9;
      if (sa !== sb) return sa - sb;
      return a.name.localeCompare(b.name);
    }),
  );
  const isScanAttempt = (d: DeviceRow) => !d.properties && (d.discoveryState === "connecting" || d.discoveryState === "failed" || d.discoveryState === "offline");
  const scanAttempts = $derived(sortedDevices.filter(isScanAttempt));
  const visibleDevices = $derived(sortedDevices.filter(d => !isScanAttempt(d)));
  /// A device that hasn't allowed this computer, under either engine. The
  /// built-in engine shows nothing on it until Request approval asks; under
  /// platform-tools the adb server already offered its key, so it is probably
  /// asking now.
  const isUnapproved = (d: DeviceRow) => adbBackend !== null && d.status === "unauthorized" && !d.properties;
  const isAwaitingApproval = (d: DeviceRow) =>
    isUnapproved(d) &&
    (adbBackend === "direct" ? d.discoveryState === "approval" : approvals[d.serial]?.phase === "waiting");
  const canRetryConnection = (d: DeviceRow) => d.connection === "network" && !isUnapproved(d) && (d.discoveryState === "failed" || d.discoveryState === "offline");
  /// Endpoints the main list already shows, so Scan details doesn't offer them again.
  const listedEndpoints = $derived(visibleDevices.filter(d => d.status === "device" || d.status === "unauthorized").map(d => d.serial));
  let loading = $state(true);
  let error = $state<string | null>(null);

  let connectAddress = $state("");
  let connectBusy = $state(false);
  let failedConnectAddress = $state("");
  let connectMessage = $state("");
  /// adb's own words behind a failure `connectMessage` explains.
  let connectDetail = $state("");
  /// A failed Add by IP: adb's words plus the explanation adb-errors.ts gives.
  let connectError = $state<{ raw: string; summary: string | null; localNetwork: LocalNetworkHint } | null>(null);

  let scanBusy = $state(false);
  let scanMessage = $state("");
  let scanDetails = $state<string[]>([]);
  let nativeScanSummary = "";
  let scanBaseDetails: string[] = [];
  let lastNativeProgress = "";
  let disposed = false;
  let refreshInFlight: Promise<void> | null = null;
  let nativeMonitorTimer: ReturnType<typeof setTimeout> | null = null;
  let nativeMonitorToken = 0;
  let nativeMonitoring = false;

  function monitorNativeConnections() {
    if (disposed || adbBackend !== "direct") return;
    if (nativeMonitorTimer) clearTimeout(nativeMonitorTimer);
    const token = ++nativeMonitorToken;
    const deadline = Date.now() + 90_000;
    nativeMonitoring = true;
    const schedule = () => {
      nativeMonitorTimer = setTimeout(async () => {
        nativeMonitorTimer = null;
        if (disposed || token !== nativeMonitorToken) return;
        if (!refreshInFlight) await startRefresh(true);
        if (disposed || token !== nativeMonitorToken) return;
        // A request waits for the person (the backend bounds it); a scan's quiet
        // attempts are watched for 90 seconds. An ended request still on
        // screen is watched while it is shown, so a later listing that the
        // TV allowed this computer corrects it.
        const connecting = (scanBusy || nativeConnections.pending.length > 0) && Date.now() < deadline;
        if (!backendUnavailable && (connecting || waitingForApproval() || endedRequestShown())) schedule();
        else nativeMonitoring = false;
      }, 1000);
    };
    schedule();
  }

  let scanError = $state<string | null>(null);

  const endedRequestShown = () =>
    dialogState?.engine === "direct" && (dialogState.phase === "not_approved" || dialogState.phase === "error");
  const waitingForApproval = () =>
    (nativeConnections.awaiting_authorization?.length ?? 0) > 0 || (nativeConnections.requesting?.length ?? 0) > 0;

  let pairOpen = $state(false);
  let pairAddress = $state("");
  let pairPin = $state("");
  let pairBusy = $state(false);
  let pairMessage = $state("");
  let pairDetail = $state("");
  let pairLocalNetwork = $state<LocalNetworkHint>(null);
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
  /// macOS: a restart whose own reconnects still found no route, while the
  /// probe can't tell whether the app is blocked.
  const restartLocalNetwork = $derived(isNoRoute(restartMessage) && localNetwork.status === "unknown");

  let reportBusy = $state(false);
  let reportData = $state<DeviceReport[] | null>(null);
  let reportError = $state<string | null>(null);

  // Triggers the install-platform-tools button rather than a generic error pane.
  let adbMissing = $state(false);
  let backendUnavailable = $state(false);
  const directUnavailable = $derived(adbBackend === "direct" && backendUnavailable);
  const hasConnectionChoices = $derived(!!nativeDiscovery && buildNativeDiscoveryRows(nativeDiscovery.advertisements
    .filter(row => row.service_kind !== "tls_pairing" && !(row.connect_address && listedEndpoints.includes(row.connect_address)))).length > 0);
  const hasScanDetails = $derived(!backendUnavailable && !!(scanDetails.length || scanAttempts.length || hasConnectionChoices));
  $effect(() => {
    if (!hasScanDetails) scanDetailsOpen = false;
  });
  let installBusy = $state(false);
  let installMessage = $state("");

  async function refresh() {
    while (refreshInFlight) await refreshInFlight;
    if (!disposed) await startRefresh(false);
  }

  function startRefresh(background: boolean): Promise<void> {
    const work = performRefresh(background).finally(() => {
      if (refreshInFlight === work) refreshInFlight = null;
    });
    refreshInFlight = work;
    return work;
  }

  async function performRefresh(background: boolean) {
    if (!background) {
      loading = true;
      error = null;
    }
    try {
      const status = await api.adbStatus();
      if (disposed) return;
      adbBackend = status.backend ?? null;
      backendUnavailable = !status.available;
      if (!status.available) {
        nativeDiscovery = null;
        adbMissing = adbBackend === "subprocess" && status.failure === "missing_binary";
        devices = [];
        scanMessage = "";
        scanDetails = [];
        if (!adbMissing) error = status.last_probe ?? "The selected ADB backend is unavailable.";
        return;
      }
      adbMissing = false;
      const progress = status.connections ?? { pending: [], failed: [] };
      const progressKey = JSON.stringify([ [...progress.pending].sort(), [...(progress.awaiting_authorization ?? [])].sort(), [...(progress.requesting ?? [])].sort(), [...(progress.not_allowed ?? [])].sort(), [...(progress.ready ?? [])].sort(), progress.failed.map(f => [f.address, f.message]).sort() ]);
      const changed = progressKey !== lastNativeProgress;
      nativeConnections = progress;
      if (adbBackend === "direct" && nativeScanSummary && !scanBusy) {
        scanMessage = nativeScanSummary;
        scanDetails = [...new Set([...scanBaseDetails, ...progress.failed.map(f => `${f.address}: ${f.message}`)])];
        if (progress.pending.length === 0 && progress.failed.length > 0) {
          scanMessage += " Other connection attempts are in Scan details.";
        }
      }
      // Pending status reads are cheap; unchanged progress must not repeatedly
      // re-query properties from every connected TV.
      if (!background || changed) {
        const list = await api.listDevices();
        if (disposed) return;
        devices = list;
        noteUnapproved(list);
        lastNativeProgress = progressKey;
      }
      reconcileApprovals();
      if (!background && (progress.pending.length > 0 || waitingForApproval()) && !nativeMonitoring) monitorNativeConnections();
    } catch (e) {
      if (!disposed && !background) error = String(e);
    } finally {
      if (!background && !disposed) loading = false;
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

  async function showConnectFailure(address: string, raw: string) {
    const status = await freshLocalNetworkStatus(raw, address);
    const explained = explainAdbFailure("connect", address, raw, status, adbBackend !== "direct");
    connectMessage = "";
    connectDetail = "";
    connectError = { raw: explained.raw, summary: explained.summary, localNetwork: explained.localNetwork };
  }

  /// The adb transport key `adb connect <address>` registers: a bare host
  /// gets the backend's `:5555` default.
  function connectKey(address: string): string {
    const a = address.trim();
    return /:\d+$/.test(a) ? a : `${a}:5555`;
  }

  async function connect() {
    const address = connectAddress.trim();
    if (directUnavailable || connectBusy || scanBusy || restartBusy || !address || nativePairNeedsPort) return;
    connectBusy = true;
    connectMessage = adbBackend === "direct" ? "Connecting…" : "";
    connectDetail = "";
    connectError = null;
    try {
      const r = await api.connectDevice(address);
      if (r.ok) {
        connectMessage = r.message.trim();
        connectAddress = "";
        failedConnectAddress = "";
        pairNextStep = false;
        await refresh();
        await announceConnected(connectKey(address));
      } else {
        failedConnectAddress = address;
        await showConnectFailure(address, r.message);
      }
    } catch (e) {
      failedConnectAddress = address;
      await showConnectFailure(address, String(e));
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
    await highlightRow(row.serial);
    return row;
  }

  /// Scroll to a row, outline it for a moment and give it its call to
  /// action. Leaves the Pair panel and Add by IP alone.
  async function highlightRow(serial: string) {
    justConnected = serial;
    flashSerial = serial;
    await tick();
    const el = document.querySelector(`[data-serial="${CSS.escape(serial)}"]`);
    const reduce = window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;
    el?.scrollIntoView({ block: "nearest", behavior: reduce ? "auto" : "smooth" });
    setTimeout(() => {
      if (flashSerial === serial) flashSerial = null;
    }, 2400);
  }

  let connectInput = $state<HTMLInputElement | null>(null);
  /// Set after a successful pair: the connect box holds the paired host and
  /// is waiting for the separate connect port (#88).
  let pairNextStep = $state(false);
  const nativePairNeedsPort = $derived(adbBackend !== "subprocess" && pairNextStep && !explicitConnectPort(connectAddress.trim()));

  function explicitConnectPort(address: string): boolean {
    const port = address.startsWith("[")
      ? /^\[[^\]]+\]:(\d+)$/.exec(address)?.[1]
      : /^[^:\s]+:(\d+)$/.exec(address)?.[1];
    return port !== undefined && Number(port) > 0 && Number(port) <= 65535;
  }

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
    if (disposed || directUnavailable) return;
    if (adbBackend !== "subprocess" && scanBusy) return;
    scanBusy = true;
    scanError = null;
    scanDetails = [];
    scanBaseDetails = [];
    nativeScanSummary = "";
    nativeDiscovery = null;
    if (adbBackend === "direct") monitorNativeConnections();
    scanMessage = "Scanning local network…";
    try {
      const r = await api.scanNetwork();
      if (disposed) return;
      scanMessage = r.message;
      scanDetails = r.details ?? [];
      scanBaseDetails = r.details ?? [];
      nativeDiscovery = adbBackend === "direct" ? r.native_discovery ?? null : null;
      discoveryReceivedAt = performance.now();
      if (adbBackend === "direct" && (r.pending?.length ?? 0) > 0) {
        nativeScanSummary = "Scan finished.";
        scanMessage = nativeScanSummary;
        nativeConnections = { ...nativeConnections, pending: r.pending ?? [] };
      }
      scanBusy = false;
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
      if (!disposed) {
        scanMessage = "";
        scanError = String(e);
      }
    } finally {
      scanBusy = false;
    }
  }

  function connectUsingAdvertisement(address: NativeAdvertisement) {
    if (adbBackend !== "direct" || !nativeDiscovery || !address.connect_address || scanBusy || connectBusy || pairBusy) return;
    if (!discoveryAddressUsable(address, performance.now() - discoveryReceivedAt, nativeDiscovery.observation_max_age_ms)) return;
    connectAddress = address.connect_address;
    void connect();
  }

  /// Do we positively know this is not an Android TV?
  ///
  /// Only when the device said so: a `phone`/`tablet`/`watch` characteristic,
  /// or a flat "no" to the leanback feature. `device_type === "unknown"` is not
  /// evidence: it answers a different question — "no catalog match" — and
  /// ordinary TV boxes that report something unusual land there (#120).
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

  /// The IP address a network row is reached at, read from its adb key. A
  /// Wireless debugging key is an mDNS service name, not an address, and a USB
  /// key is a serial number, so neither offers one.
  function ipOf(d: Device): string | null {
    if (d.connection !== "network") return null;
    const host = hostOf(d.serial).replace(/^\[|\]$/g, "");
    if (/^\d{1,3}(\.\d{1,3}){3}$/.test(host) || (host.includes(":") && /^[0-9a-f:.]+(%[\w.-]+)?$/i.test(host))) {
      return host;
    }
    return null;
  }

  /// The device's own serial number (`ro.serialno`), which is what identifies
  /// it. Over USB the adb key is that serial; over the network it is not, so
  /// an unreadable network device has none to offer.
  function hardwareSerialOf(d: Device): string | null {
    return d.properties?.serial_number || (d.connection === "usb" ? d.serial : null);
  }

  async function copyText(text: string, what: string): Promise<string> {
    await navigator.clipboard.writeText(text);
    return `Copied ${what}`;
  }

  function deviceMenu(d: DeviceRow): MenuItem[] {
    const href = deviceHref(d);
    const ip = ipOf(d);
    const serial = hardwareSerialOf(d);
    const items: MenuItem[] = [];
    if (href) {
      items.push({ label: "Open", run: () => void goto(href) });
    } else if (isNotATv(d) && d.status === "device") {
      items.push({ label: "Open anyway…", run: () => void (confirmOpenSerial = d.serial) });
    } else {
      items.push({ label: "Open", run: () => {}, disabled: true });
    }
    items.push(
      { label: "Copy IP", run: () => copyText(ip ?? "", "IP address"), disabled: !ip },
      { label: "Copy serial", run: () => copyText(serial ?? "", "serial"), disabled: !serial },
      {
        label: "Copy diagnostics",
        disabled: diagnosticsBusy === d.serial,
        run: async () => {
          await copyDiagnostics(d);
          return diagnosticsCopied === d.serial ? "Copied diagnostics" : null;
        },
      },
    );
    // Forget goes through the same backend call as the row's button: it drops
    // every adb key with this device's verified hardware id, never a match on
    // address. A USB row has nothing to forget.
    if (isUnapproved(d)) {
      if (requestOpen(d)) {
        items.push(
          { label: "Show approval request", run: () => showApproval(d) },
          { label: "Cancel approval request", run: () => cancelApprovalRequest(d.serial) },
        );
      } else {
        items.push({ label: "Request approval", run: () => startApproval(d) });
      }
    } else if (canRetryConnection(d)) {
      items.push({
        label: "Retry connection",
        disabled: repromptBusy === d.serial,
        run: () => repromptDevice(d),
      });
    }
    if (d.connection === "network") {
      items.push({
        label: "Forget",
        danger: true,
        disabled: forgetBusy === d.serial,
        run: () => forgetDevice(d),
      });
    }
    return items;
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

  async function showPairFailure(address: string, raw: string) {
    const status = await freshLocalNetworkStatus(raw, address);
    const explained = explainAdbFailure("pair", address, raw, status, adbBackend !== "direct");
    pairMessage = explained.summary ?? explained.raw;
    pairDetail = explained.summary ? explained.raw : "";
    pairLocalNetwork = explained.localNetwork;
  }

  $effect(() => {
    const host = hostOf(pairAddress);
    const token = ++pairWarnToken;
    if (adbBackend !== "subprocess") {
      pairWarning = null;
      return;
    }
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
    if (directUnavailable) return;
    const address = pairAddress.trim();
    if (!address || pairPin.length !== 6) return;
    cancelPairWait(false);
    pairBusy = true;
    pairMessage = "";
    pairDetail = "";
    pairLocalNetwork = null;
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
        // The built-in engine can only watch for the device it paired with
        // when the device named itself while pairing.
        const autoConnect = adbBackend === "subprocess" || (adbBackend === "direct" && !!r.instance);
        if (!autoConnect) {
          pairStage = "manual";
          pairMessage = r.message;
          pairNextStep = true;
          await tick();
          focusConnectBox();
          return;
        }
        await refresh();
        // The refresh is another await the user can cancel during.
        if (requestToken !== pairWaitToken || !pairOpen) return;
        void waitForConnect(address, host, r.instance);
      } else {
        await showPairFailure(address, r.message);
      }
    } catch (e) {
      await showPairFailure(address, String(e));
    } finally {
      pairPin = "";
      pairBusy = false;
    }
  }

  const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

  /// Watch mDNS for the connect endpoint the just-paired device advertises and
  /// connect to it. Android often holds that back while the pairing dialog is
  /// still open, so the device may only appear once that dialog is dismissed.
  /// Only an advertised port is ever dialled: never a guess, never the
  /// pairing port.
  async function waitForConnect(pairTarget: string, host: string, instance: string | null) {
    const token = ++pairWaitToken;
    pairWaitHost = host;
    pairStage = "waiting";
    pairElapsed = 0;
    const started = Date.now();
    // A built-in engine probe is itself a multi-second mDNS browse, so the
    // count ticks on its own instead of once per probe.
    const ticker = setInterval(() => {
      if (token === pairWaitToken) pairElapsed = Math.min(PAIR_CONNECT_WAIT_S, Math.floor((Date.now() - started) / 1000));
    }, 1000);
    try {
      await pollPairedConnect(token, started, pairTarget, instance);
    } finally {
      clearInterval(ticker);
    }
  }

  async function pollPairedConnect(token: number, started: number, pairTarget: string, instance: string | null) {
    const hardwareId = instanceSerial(instance);
    /// The endpoint was advertised but refused us: that, not discovery, is
    /// what the user has to fix if the wait runs out.
    let lastFailure: { address: string; message: string } | null = null;
    while (token === pairWaitToken) {
      const probeStarted = Date.now();
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
        if (token !== pairWaitToken) {
          // Cancelled while the dial was already in flight. The UI has said it
          // stopped, so undo a connect that landed anyway rather than leave
          // the device attached in the background.
          if (r.ok) await api.disconnectDevice(probe.address).catch(() => {});
          await refresh();
          return;
        }
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
          const status = await freshLocalNetworkStatus(lastFailure.message, lastFailure.address);
          if (token !== pairWaitToken) return;
          const explained = explainAdbFailure("connect", lastFailure.address, lastFailure.message, status, adbBackend !== "direct");
          pairLocalNetwork = explained.localNetwork;
          fallBackToManual(
            `It advertised ${lastFailure.address}, but connecting kept failing. ${explained.summary ?? ""}`.trim(),
            explained.raw,
          );
        } else {
          fallBackToManual(`No connect port appeared within ${PAIR_CONNECT_WAIT_S} seconds.`);
        }
        return;
      }
      // At most one probe per interval; a probe that browsed that long
      // already waited.
      await sleep(Math.max(0, PAIR_PROBE_INTERVAL_MS - (Date.now() - probeStarted)));
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
    if (!detail) pairLocalNetwork = null;
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

  function clickStopOnQuit(e: MouseEvent) {
    // Undo the click's own tick at once; the box moves only when the reply
    // says the setting did. (preventDefault would restore it only after the
    // event, which can land after a fast reply and leave the box wrong.)
    (e.currentTarget as HTMLInputElement).checked = stopOnQuit.setting?.enabled ?? false;
    void toggleStopOnQuit();
  }

  async function restartAdb() {
    const retryAddress = failedConnectAddress && failedConnectAddress === connectAddress.trim()
      ? failedConnectAddress : "";
    const retryPrompt = retryAddress ? ` Then retry Add by IP for ${retryAddress} once.` : "";
    if (restartBusy) return;
    if (!(await confirmAction(`Restart the shared ADB server? This disconnects other tools using ADB. Current network connections will be retried.${retryPrompt}`)) || restartBusy) return;
    restartBusy = true;
    restartMessage = "";
    try {
      const r = await api.restartAdb();
      restartMessage = r.message;
      await freshLocalNetworkStatus(r.message);
      await refresh();
      restartBusy = false;
      if (r.server_ready && retryAddress && connectAddress.trim() === retryAddress) await connect();
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

  let forgetBusy = $state<string | null>(null);
  let repromptBusy = $state<string | null>(null);
  /// How long the app waits for Allow, under either engine. The built-in
  /// engine enforces it itself (`APPROVAL_WAIT`); under platform-tools the
  /// app watches the transport this long.
  const APPROVAL_WAIT_MS = 10 * 60_000;
  const APPROVED_CLOSE_MS = 1600;
  const APPROVAL_POLL_MS = 2000;
  /// Platform-tools: the adb server offers its key when a TV connects, so for
  /// this long after a TV first appears unapproved it is probably asking.
  const PROMPT_FRESH_MS = 2 * 60_000;

  type ApprovalState = {
    serial: string;
    name: string;
    address: string | null;
    usb: boolean;
    wireless: boolean;
    engine: "direct" | "subprocess";
    phase: ApprovalPhase;
    error: string | null;
    /// When this window's wait ends; null when it doesn't know (a request
    /// the built-in engine was already running when this window opened).
    deadline: number | null;
    /// Which Request approval this is. Results for an older one are dropped.
    token: number;
    /// It ended while the dialog was closed, so the row says how.
    unseen: boolean;
  };
  /// Approval requests this window knows about, by transport key. The
  /// built-in engine's wait lives in the backend; this is what the dialog
  /// and the row show for it.
  let approvals = $state<Record<string, ApprovalState>>({});
  let dialogSerial = $state<string | null>(null);
  const dialogState = $derived(dialogSerial ? approvals[dialogSerial] ?? null : null);
  let approvalToken = 0;
  let approvalPoll: ReturnType<typeof setInterval> | null = null;
  let approvalPollBusy = false;
  /// Requests whose outcome is being resolved, so it is resolved once.
  const settling = new Set<string>();
  /// Platform-tools: unapproved keys already listed, and when each newly
  /// listed one appeared (the adb server offered its key then).
  let seenUnapproved: Set<string> | null = null;
  let freshPrompt = $state<Record<string, number>>({});

  /// A request is being made or waits on the TV: the dialog is the way back
  /// to it, and Request approval is not offered again meanwhile.
  const requestOpen = (d: DeviceRow) => isAwaitingApproval(d) || approvals[d.serial]?.phase === "asking";
  /// How a request ended while its dialog was closed.
  const unseenResult = (d: DeviceRow): string | null => {
    const state = approvals[d.serial];
    if (!state?.unseen) return null;
    if (state.phase === "error") return state.error ?? "Couldn't ask it.";
    if (state.phase === "not_approved") return "The request ended without Allow.";
    return null;
  };

  function approvalFor(d: DeviceRow, phase: ApprovalPhase, token: number, deadline: number | null): ApprovalState {
    return {
      serial: d.serial,
      name: d.name,
      address: d.name !== d.serial ? d.serial : null,
      usb: d.connection === "usb",
      wireless: d.serial.includes("._adb-tls-connect."),
      engine: adbBackend === "direct" ? "direct" : "subprocess",
      phase,
      error: null,
      deadline,
      token,
      unseen: false,
    };
  }

  /// End a request in `phase`. Shown in the dialog when it is open, else on
  /// the row.
  function finish(serial: string, phase: "not_approved" | "error", error: string | null = null) {
    const current = approvals[serial];
    if (!current) return;
    approvals[serial] = { ...current, phase, error, unseen: dialogSerial !== serial };
    delete freshPrompt[serial];
  }

  /// Request approval: opens the dialog and asks. Built-in: the backend
  /// offers the key once and waits. Platform-tools: the adb server asks by
  /// itself; a network transport is redialled so the TV asks again, and the
  /// app watches the transport. A USB device asks again when replugged.
  async function startApproval(d: DeviceRow) {
    const serial = d.serial;
    if (approvals[serial]?.phase === "asking") {
      dialogSerial = serial;
      return;
    }
    const token = ++approvalToken;
    approvals[serial] = approvalFor(d, "asking", token, null);
    dialogSerial = serial;
    const current = () => approvals[serial]?.token === token;
    const waiting = () => {
      approvals[serial] = { ...approvals[serial], phase: "waiting", deadline: Date.now() + APPROVAL_WAIT_MS };
    };
    if (adbBackend === "direct") {
      let status: string;
      try {
        status = (await api.requestDeviceApproval(serial)).status;
      } catch (e) {
        if (current()) finish(serial, "error", String(e));
        return;
      }
      if (!current()) return;
      if (status === "canceled") {
        delete approvals[serial];
        if (dialogSerial === serial) closeApprovalDialog();
        return;
      }
      if (status === "waiting") {
        waiting();
        monitorNativeConnections();
      }
      await refresh();
      if (!current()) return;
      if (status === "device") void settle(serial, token, "ended");
      else if (status === "not_allowed") finish(serial, "not_approved");
      return;
    }
    if (!approvals[serial].usb) {
      try {
        const r = await api.repromptDevice(serial);
        if (!current()) return;
        if (!r.ok) {
          finish(serial, "error", r.message);
          return;
        }
      } catch (e) {
        if (current()) finish(serial, "error", String(e));
        return;
      }
    }
    if (!current()) return;
    waiting();
    pollApprovals();
    void pollApprovalsOnce();
  }

  function showApproval(d: DeviceRow) {
    // The built-in engine's wait outlives this window's state (a reload).
    if (!approvals[d.serial]) approvals[d.serial] = approvalFor(d, "waiting", ++approvalToken, null);
    dialogSerial = d.serial;
  }

  /// The TV allowed this computer and is listed as `row`: show it, then close.
  function approved(serial: string, row: string) {
    const state = approvals[serial];
    if (!state || state.phase === "approved") return;
    approvals[serial] = { ...state, phase: "approved", error: null, unseen: false };
    delete freshPrompt[serial];
    const token = state.token;
    setTimeout(() => {
      if (approvals[serial]?.token !== token || approvals[serial]?.phase !== "approved") return;
      delete approvals[serial];
      if (dialogSerial === serial) closeApprovalDialog(row);
      void highlightRow(row);
    }, APPROVED_CLOSE_MS);
  }

  /// The row a TV that allowed this computer is listed under: its own key,
  /// or the row its transport was folded into, found by that transport's own
  /// verified hardware id (never its address). Null while it isn't listed as
  /// a device.
  async function approvedRow(serial: string): Promise<string | null> {
    if (devices.some(d => d.serial === serial && d.status === "device")) return serial;
    try {
      const row = await api.deviceProfile(serial);
      return row.status === "device" ? row.serial : null;
    } catch {
      return null;
    }
  }

  /// Resolve how a request ended. "refused": the backend recorded a refusal.
  /// "ended": it's over with no refusal, so the TV either allowed this
  /// computer (perhaps folded into another row) or the connection ended.
  /// "late": an ended request, and a later listing may show the TV allowed.
  async function settle(serial: string, token: number, how: "refused" | "ended" | "late") {
    if (settling.has(serial)) return;
    settling.add(serial);
    try {
      const row = how === "refused" ? null : await approvedRow(serial);
      const state = approvals[serial];
      if (!state || state.token !== token || state.phase === "approved") return;
      if (row) approved(serial, row);
      else if (how === "refused") finish(serial, "not_approved");
      else if (how === "ended" && (state.phase === "waiting" || state.phase === "asking")) {
        finish(serial, "error", `The connection to ${state.name} ended before it allowed this computer. Try again.`);
      }
    } finally {
      settling.delete(serial);
    }
  }

  /// Settle each request against what the backend now reports.
  function reconcileApprovals() {
    const open = new Set([...(nativeConnections.awaiting_authorization ?? []), ...(nativeConnections.requesting ?? [])]);
    const refused = new Set(nativeConnections.not_allowed ?? []);
    const ready = new Set(nativeConnections.ready ?? []);
    for (const state of Object.values(approvals)) {
      if (state.phase === "asking" || state.phase === "approved") continue;
      if (devices.some(d => d.serial === state.serial && d.status === "device")) {
        approved(state.serial, state.serial);
      } else if (state.phase !== "waiting") {
        // Ended, but a scan reached it since: the TV may have allowed it.
        if (state.engine === "direct" && ready.has(state.serial)) void settle(state.serial, state.token, "late");
      } else if (state.engine === "direct") {
        if (!open.has(state.serial)) void settle(state.serial, state.token, refused.has(state.serial) ? "refused" : "ended");
      } else if (state.deadline !== null && Date.now() >= state.deadline) {
        finish(state.serial, "not_approved");
      }
    }
  }

  /// Platform-tools: which unapproved TVs were listed just now, so the row
  /// says one is probably asking only while that is likely.
  function noteUnapproved(list: Device[]) {
    if (adbBackend !== "subprocess") {
      seenUnapproved = null;
      if (Object.keys(freshPrompt).length > 0) freshPrompt = {};
      return;
    }
    const listed = new Set(list.filter(d => d.status === "unauthorized").map(d => d.serial));
    if (seenUnapproved) {
      for (const serial of listed) {
        if (seenUnapproved.has(serial)) continue;
        const stamp = Date.now();
        freshPrompt[serial] = stamp;
        setTimeout(() => {
          if (freshPrompt[serial] === stamp) delete freshPrompt[serial];
        }, PROMPT_FRESH_MS);
      }
    }
    for (const serial of Object.keys(freshPrompt)) if (!listed.has(serial)) delete freshPrompt[serial];
    seenUnapproved = listed;
  }

  const subprocessWaiting = () =>
    Object.values(approvals).filter(a => a.engine === "subprocess" && a.phase === "waiting");

  /// The wait limit holds whether or not adb answers.
  function expireApprovals() {
    const now = Date.now();
    for (const state of subprocessWaiting()) {
      if (state.deadline !== null && now >= state.deadline) finish(state.serial, "not_approved");
    }
  }

  /// Platform-tools: read each waiting TV's transport state (cheap: `adb
  /// devices` alone), and re-read the full list only once one changed.
  async function pollApprovalsOnce() {
    if (approvalPollBusy || disposed) return;
    approvalPollBusy = true;
    try {
      const allowed: ApprovalState[] = [];
      let changed = false;
      for (const state of subprocessWaiting()) {
        let transport: string | null;
        try {
          transport = await api.transportState(state.serial);
        } catch {
          continue;
        }
        if (disposed || approvals[state.serial]?.token !== state.token) continue;
        if (transport !== "unauthorized") changed = true;
        if (transport === "device") allowed.push(state);
      }
      if (!changed || disposed) return;
      await refresh();
      for (const state of allowed) {
        if (approvals[state.serial]?.token === state.token && approvals[state.serial]?.phase === "waiting") {
          void settle(state.serial, state.token, "late");
        }
      }
    } finally {
      approvalPollBusy = false;
    }
  }

  function pollApprovals() {
    if (approvalPoll || disposed) return;
    approvalPoll = setInterval(() => {
      expireApprovals();
      if (disposed || subprocessWaiting().length === 0) {
        if (approvalPoll) clearInterval(approvalPoll);
        approvalPoll = null;
        return;
      }
      void pollApprovalsOnce();
    }, APPROVAL_POLL_MS);
  }

  /// Stop this window's part of a request: the built-in engine's wait ends
  /// in the backend, platform-tools polling stops with the state.
  async function endApproval(serial: string): Promise<string | null> {
    const engine = approvals[serial]?.engine ?? (adbBackend === "direct" ? "direct" : "subprocess");
    delete approvals[serial];
    delete freshPrompt[serial];
    if (dialogSerial === serial) dialogSerial = null;
    if (engine !== "direct") return null;
    try {
      await api.cancelDeviceApproval(serial);
      return null;
    } catch (e) {
      return `Couldn't cancel the request: ${e}`;
    }
  }

  /// Cancel request: the only way to stop a wait. Closing the dialog keeps it.
  async function cancelApprovalRequest(serial: string) {
    const failed = await endApproval(serial);
    if (failed) connectMessage = failed;
    await refresh();
    await focusRow(serial);
  }

  function closeApprovalDialog(focus: string | null = null) {
    const serial = dialogSerial;
    dialogSerial = null;
    // A finished request has nothing left to show once it was seen.
    const state = serial ? approvals[serial] : undefined;
    if (serial && state && state.phase !== "waiting" && state.phase !== "asking") delete approvals[serial];
    if (serial) void focusRow(focus ?? serial);
  }

  /// Return focus to the row the dialog belongs to: its approval button, or
  /// the row itself once it opens like any other TV.
  async function focusRow(serial: string) {
    await tick();
    const row = document.querySelector<HTMLElement>(`[data-serial="${CSS.escape(serial)}"]`);
    const target = row?.querySelector<HTMLElement>("[data-approval-action]") ?? row;
    target?.focus();
  }

  // Retry connection for a failed or offline network address.
  async function repromptDevice(d: Device) {
    repromptBusy = d.serial;
    connectMessage = "";
    connectDetail = "";
    try {
      const r = await api.repromptDevice(d.serial);
      connectMessage = r.message;
      await refresh();
    } catch (e) {
      connectMessage = `Could not reconnect to ${d.name}: ${e}`;
    } finally {
      repromptBusy = null;
    }
  }

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
      // Forget ends a request for this TV too.
      if (approvals[d.serial]) await endApproval(d.serial);
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

  // Best-effort discovery on boot: if no ready device shows up after the
  // initial refresh and adb is available, kick off a scan so users with
  // already-paired devices don't have to click anything.
  async function bootDiscovery() {
    await refresh();
    if (disposed || adbMissing || directUnavailable) return;
    if (devices.some((d) => d.status === "device")) return;
    await scan();
  }

  onMount(() => {
    void bootDiscovery();
    void refreshStopOnQuit();
    // Fetched support tiers landed after the list was read: re-list so the
    // badges aren't stale.
    const stopTierWatch = onSupportTiersUpdated(() => {
      if (!loading) void refresh();
    });
    return () => {
      disposed = true;
      nativeMonitorToken++;
      if (nativeMonitorTimer) clearTimeout(nativeMonitorTimer);
      stopTierWatch();
      pairWaitToken++;
      reattachToken++;
      if (approvalPoll) clearInterval(approvalPoll);
    };
  });
</script>

<section class="header-row">
  <div class="header-title">
    <h1>Devices</h1>
    <p class="muted small mono header-sub">
      {#if adbMissing}
        adb not found
      {:else if error && adbBackend !== "subprocess"}
        connection status unavailable
      {:else if adbBackend === "direct"}
        {devices.filter((d) => d.status === "device").length} connected
        {#if visibleDevices.some(d => d.status === "unauthorized")}
          · {visibleDevices.filter(d => d.status === "unauthorized").length} not allowed yet
        {/if}
      {:else if sortedDevices.length === 0}
        no TV connected
      {:else}
        {sortedDevices.length} connected · {devices.filter((d) => d.status === "device").length} ready
      {/if}
    </p>
    <p class="muted small" data-testid="adb-backend">{adbBackend === "direct" ? "Native Rust ADB · direct connection" : adbBackend === "subprocess" ? "Platform-tools ADB · host server" : "Checking ADB driver…"}</p>
  </div>
  <div class="header-actions">
    <button onclick={scan} disabled={scanBusy || adbMissing || directUnavailable || (adbBackend !== "subprocess" && (connectBusy || pairBusy))} title="Scan the local network for ADB-listening devices">
      <Icon name="wifi_tethering" size={16} /> {scanBusy ? "Scanning…" : "Scan LAN"}
    </button>
    <button onclick={refresh} disabled={loading} title="Re-read the list of connected devices">
      {loading ? "Refreshing…" : "Refresh"}
    </button>
  </div>
</section>

<LocalNetworkNotice hostServer={adbBackend !== "direct"} />

<!-- What a macOS "No route to host" offers, by what the probe saw. -->
{#snippet remedies(hint: LocalNetworkHint)}
  <span class="remedies">
    {#if hint === "restart" || hint === "restart_or_settings"}
      <button
        onclick={restartAdb}
        disabled={restartBusy || connectBusy || adbMissing}
      >
        <Icon name="restart_alt" size={14} /> {restartBusy ? "Restarting ADB…" : "Restart ADB"}
      </button>
    {/if}
    {#if hint === "restart_or_settings" || hint === "settings"}
      <button onclick={openLocalNetworkSettings}>
        <Icon name="open_in_new" size={14} /> Open Privacy &amp; Security
      </button>
    {/if}
  </span>
{/snippet}

<!-- The board leads with Scan LAN and Add by IP. The other four stay, one
     row down, because every one of them is the only way to do the
     thing it does. -->
<section class="connect-form">
  <input
    placeholder="IP[:port] — e.g. 192.168.42.71"
    bind:value={connectAddress}
    disabled={restartBusy}
    bind:this={connectInput}
    onkeydown={(e) => e.key === "Enter" && connect()}
  />
  <button class="primary" onclick={connect} disabled={directUnavailable || connectBusy || scanBusy || restartBusy || !connectAddress.trim() || nativePairNeedsPort}>
    <Icon name="add" size={16} /> {connectBusy ? "Connecting…" : "Add by IP"}
  </button>
  <button onclick={togglePair} disabled={adbMissing || directUnavailable} title="PIN pairing, for a device that only offers Wireless debugging">
    {pairOpen ? "Cancel Pair" : "Pair PIN"}
  </button>
  {#if adbBackend === "subprocess"}
  <button onclick={restartAdb} disabled={restartBusy || connectBusy || adbMissing} title="adb kill-server then start-server">
    {restartBusy ? "Restarting…" : "Restart ADB"}
  </button>
  <label class="quit-toggle" data-tip={QUIT_ADB_TIP} data-tip-wrap>
    <input
      type="checkbox"
      aria-describedby="quit-adb-tip"
      checked={stopOnQuit.setting?.enabled ?? false}
      indeterminate={stopOnQuit.setting === null}
      disabled={stopOnQuit.setting === null}
      onclick={clickStopOnQuit}
    />
    Stop ADB when quitting
    <span id="quit-adb-tip" hidden>{QUIT_ADB_TIP}</span>
    {#if stopOnQuit.setting === null && stopOnQuit.pending > 0}
      <span class="quit-checking">checking…</span>
    {/if}
  </label>
  {/if}
  <button onclick={reportAll} disabled={reportBusy || adbMissing || directUnavailable} title="Run a health report against every connected device">
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
  {#if connectError}
    <ErrorNotice
      class="connect-message connect-details"
      error={connectError.raw}
      summary={connectError.summary}
    />
    {#if connectError.localNetwork === "restart" || connectError.localNetwork === "restart_or_settings" || connectError.localNetwork === "settings"}
      <div class="connect-message plain">{@render remedies(connectError.localNetwork)}</div>
    {/if}
  {/if}
  {#if scanMessage}
    <p class="connect-message muted">{scanMessage}</p>
  {/if}
  {#if adbBackend === "direct" && !scanBusy && nativeConnections.pending.length > 0}
    <p class="connect-message muted" role="status">Still connecting to {nativeConnections.pending.length} network addresses; ready TVs appear automatically.</p>
  {/if}
  {#if hasScanDetails}
    <details class="adb-details connect-details" bind:open={scanDetailsOpen}>
      <summary>Scan details{#if scanAttempts.length} — {scanAttempts.length} other connection {scanAttempts.length === 1 ? "attempt" : "attempts"}{/if}</summary>
      {#if scanDetailsOpen && nativeDiscovery}
        <NativeConnectionChoices report={nativeDiscovery} receivedAt={discoveryReceivedAt} listed={listedEndpoints} busy={scanBusy || connectBusy || pairBusy || restartBusy || backendUnavailable} onChoose={connectUsingAdvertisement} />
      {/if}
      {#if scanAttempts.length}
        <p class="muted small">These addresses aren't connected. They may include multiple routes to one device.</p>
        <ul class="scan-attempts">
          {#each scanAttempts as d (d.serial)}
            <li class="scan-attempt" data-serial={d.serial}>
              <div class="scan-attempt-info">
                <span class="mono">{d.serial}</span>
                <span class="muted small">{d.discoveryState === "connecting" ? "Connecting…" : d.discoveryState === "offline" ? "Disconnected" : "Didn't connect"}</span>
              </div>
              <div class="scan-attempt-actions">
                {#if canRetryConnection(d)}
                  <button class="row-action" onclick={() => repromptDevice(d)} disabled={repromptBusy === d.serial}>{repromptBusy === d.serial ? "Connecting…" : d.discoveryState === "offline" ? "Retry connection" : "Try again"}</button>
                {/if}
                <button class="row-action" onclick={() => copyDiagnostics(d)} disabled={diagnosticsBusy === d.serial}>
                  {diagnosticsBusy === d.serial ? "Copying…" : diagnosticsCopied === d.serial ? "Copied" : "Copy diagnostics"}
                </button>
                <button class="row-action" onclick={() => forgetDevice(d)} disabled={forgetBusy === d.serial}>
                  {forgetBusy === d.serial ? "Forgetting…" : "Forget"}
                </button>
              </div>
            </li>
          {/each}
        </ul>
      {/if}
      {#if scanDetails.length}<pre>{scanDetails.join("\n\n")}</pre>{/if}
    </details>
  {/if}
  {#if scanError}
    <ErrorNotice class="connect-message" error={scanError} action="Scan LAN" />
  {/if}
  {#if restartMessage}
    <p class="connect-message muted">{restartMessage}</p>
    {#if restartLocalNetwork}
      <p class="connect-message muted plain">
        {LOCAL_NETWORK_POSSIBLE}
        <button class="inline-link" onclick={openLocalNetworkSettings}>Open Privacy &amp; Security</button>
      </p>
    {/if}
  {/if}
  {#if stopOnQuit.error}
    <p class="connect-message muted quit-error">
      {stopOnQuit.error}
      {#if stopOnQuit.setting === null && stopOnQuit.pending === 0}
        <button class="inline-link" onclick={refreshStopOnQuit}>Check again</button>
      {/if}
    </p>
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
      {#if adbBackend === null}
        After pairing, enter the connect port from the main Wireless debugging screen, then use <strong>Add by IP</strong>. Do not reuse the pairing port.
      {:else}
        After it pairs, close the pairing dialog; the app connects automatically. If it doesn't
        appear, enter the IP:port from the main Wireless debugging screen and click
        <strong>Add by IP</strong>. Do not reuse the pairing port.
      {/if}
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
    {#if pairLocalNetwork === "restart" || pairLocalNetwork === "restart_or_settings" || pairLocalNetwork === "settings"}
      <div class="local-network-action">{@render remedies(pairLocalNetwork)}</div>
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
      Already have <code>adb</code> installed? Set <code>ATV_OPTIMIZER_ADB</code> to its full path and relaunch.
    </p>
    <button class="primary" onclick={downloadAdb} disabled={installBusy}>
      {installBusy ? "Installing…" : "Download platform-tools"}
    </button>
    {#if installMessage}
      <p class="install-message muted small">{installMessage}</p>
    {/if}
  </div>
{:else if error}
  <div class="error" role="alert">{directUnavailable ? "Rust ADB unavailable" : "Failed to list devices"}: {error}</div>
{:else if (loading || scanBusy || (adbBackend === "direct" && nativeConnections.pending.length > 0)) && visibleDevices.length === 0}
  <div class="muted">{scanBusy ? "Scanning for devices…" : loading ? "Looking for devices…" : "Connecting to discovered addresses…"}</div>
{:else if visibleDevices.length === 0}
  <div class="empty">
    <h2>No devices connected.</h2>
  </div>
{:else}
  <ul class="device-list">
    {#each visibleDevices as d (d.serial)}
      {@const href = deviceHref(d)}
      {@const tvNote = openedAnyway(d)
        ? "Reported it is not an Android TV; you chose to open it anyway"
        : isUnconfirmedTv(d)
          ? "Didn't report itself as an Android TV; tools may not apply"
          : null}
      {@const tvNoteId = `tv-note-${d.serial}`}
      <li>
        {#if href}
          <a
            class="device-row clickable"
            class:flash={flashSerial === d.serial}
            href={href}
            data-serial={d.serial}
            aria-describedby={tvNote ? tvNoteId : undefined}
            oncontextmenu={(e) => openContextMenu(e, deviceMenu(d))}
          >
            {#if tvNote}<span id={tvNoteId} hidden>{tvNote}</span>{/if}
            <span class="device-icon" aria-hidden="true">
              <Icon name={d.connection === "network" ? "cast_connected" : "tv"} size={20} />
            </span>
            <div class="device-main">
              <div class="device-name">
                <span>{d.name}</span>
                <!-- Whether it is reachable belongs with the name, not at the
                     far edge: it qualifies the device. -->
                <span class="device-status online">
                  <span class="status-dot" aria-hidden="true"></span> Online
                </span>
                <!-- Inside the row's link, so not a control of its own: the
                     explanation opens with the link's keyboard focus and is
                     the link's accessible description. -->
                {#if openedAnyway(d)}
                  <span class="status-tag not-a-tv" data-tip={tvNote}>NOT A TV</span>
                {:else if isUnconfirmedTv(d)}
                  <span class="status-tag unconfirmed" data-tip={tvNote}>UNCONFIRMED TV</span>
                {/if}
                {#if d.properties && d.support?.tier !== "fully_supported"}
                  <SupportBadge support={d.support} />
                {/if}
              </div>
              <div class="device-meta muted mono">
                {d.serial} · {deviceTypeLabel(d.device_type)}
                {#if d.model}· {d.model}{/if}
                · {d.connection === "network" ? "network" : "usb"}
              </div>
              {#if d.properties && d.support?.tier !== "fully_supported"}
                {#if d.support?.tier === "not_supported"}
                  <SupportBanner support={d.support} />
                {:else}
                  <SupportBadge support={d.support} mode="line" />
                {/if}
              {/if}
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
          <!-- The right-click menu only repeats this row's own buttons, which
               are the keyboard route to the same actions. -->
          <!-- svelte-ignore a11y_no_static_element_interactions -->
          <div
            class="device-row not-clickable"
            class:discovered={!!d.discoveryState}
            class:unapproved={isUnapproved(d)}
            class:flash={flashSerial === d.serial}
            data-serial={d.serial}
            oncontextmenu={(e) => openContextMenu(e, deviceMenu(d))}
          >
            <span class="device-icon" aria-hidden="true">
              <Icon name={d.status === "offline" && !d.discoveryState ? "tv_off" : d.connection === "network" ? "cast" : "tv"} size={20} />
            </span>
            <div class="device-main">
              <div class="device-name">
                <span>{d.name}</span>
                {#if isUnapproved(d)}
                  <span class="status-tag unauthorized">{isAwaitingApproval(d) ? "WAITING FOR ALLOW" : "NOT ALLOWED YET"}</span>
                {:else if d.discoveryState === "connecting"}
                  <span class="status-tag">CONNECTING…</span>
                {:else if d.discoveryState === "failed"}
                  <span class="status-tag offline">CONNECTION FAILED</span>
                {:else if d.status === "offline"}
                  <span class="status-tag offline">OFFLINE</span>
                {/if}
                {#if isNotATv(d)}
                  <span class="status-tag not-a-tv">NOT AN ANDROID TV</span>
                {:else if isUnconfirmedTv(d)}
                  <span
                    class="status-tag unconfirmed"
                    use:tip={"Didn't report itself as an Android TV; tools may not apply"}
                  >UNCONFIRMED TV</span>
                {/if}
              </div>
              {#if !isUnapproved(d)}
              <div class="device-meta muted mono">
                {deviceTypeLabel(d.device_type)}
                {#if d.model}· {d.model}{/if}
                · {d.serial}
              </div>
              {:else if d.name !== d.serial}
              <div class="device-meta muted mono">{d.serial}</div>
              {/if}
              {#if isUnapproved(d)}
                <span class="sr-only" role="status">
                  {isAwaitingApproval(d) ? `${d.name}: waiting for Allow.` : unseenResult(d) ? `${d.name}: ${unseenResult(d)}` : ""}
                </span>
                <p class="approval-line" class:approval-result={!!unseenResult(d) && approvals[d.serial]?.phase === "error"} data-approval-line>
                  {#if isAwaitingApproval(d)}
                    Waiting for you to choose Allow on it.
                  {:else if unseenResult(d)}
                    {unseenResult(d)}
                  {:else if adbBackend !== "direct" && freshPrompt[d.serial]}
                    Hasn't allowed this computer yet; it's probably asking now.
                  {:else}
                    Hasn't allowed this computer yet.
                  {/if}
                </p>
              {/if}
              {#if d.discoveryState === "connecting"}
                <p class="muted small">Found on the network. Connecting…</p>
              {:else if d.discoveryState === "failed"}
                <p class="small">{d.discoveryError}</p>
              {/if}
              {#if justConnected === d.serial && isNotATv(d) && d.status === "device"}
                <p class="small just-connected-note">
                  Connected. It reported it isn't an Android TV, so it doesn't open by default; use
                  Open anyway to try the tools.
                </p>
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
              {#if requestOpen(d)}
                <button
                  class="row-action"
                  onclick={() => showApproval(d)}
                  aria-label={`Show the approval request for ${d.name}`}
                  data-approval-action
                >
                  Show request
                </button>
              {:else if isUnapproved(d)}
                <button
                  class="row-action primary"
                  onclick={() => startApproval(d)}
                  aria-label={`Request approval from ${d.name}`}
                  data-approval-action
                >
                  Request approval
                </button>
              {/if}
              {#if canRetryConnection(d)}
                <button
                  class="row-action primary"
                  onclick={() => repromptDevice(d)}
                  disabled={repromptBusy === d.serial}
                  data-tip="Reconnects only this address and retries authorization. Check the device for a debugging prompt."
                >
                  {repromptBusy === d.serial ? "Connecting…" : "Try again"}
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

{#if dialogState}
  <ApprovalDialog
    name={dialogState.name}
    address={dialogState.address}
    usb={dialogState.usb}
    wirelessDebugging={dialogState.wireless}
    phase={dialogState.phase}
    error={dialogState.error}
    deadline={dialogState.deadline}
    onClose={closeApprovalDialog}
    onCancel={() => cancelApprovalRequest(dialogState.serial)}
    onRetry={() => {
      const row = sortedDevices.find(d => d.serial === dialogState?.serial);
      if (row) void startApproval(row);
      else closeApprovalDialog();
    }}
  />
{/if}

{#if !adbMissing && !directUnavailable}
  <HelpDetails
    label="Connection help"
    open={!loading && !scanBusy && !error && visibleDevices.length === 0 && nativeConnections.pending.length === 0}
  >
    <div class="callout devices-note">
      <Icon name="info" size={16} />
      <span>
        <strong>Don't see your device?</strong>
        {#if adbBackend === "subprocess"}
        Try
        <button
          class="inline-link"
          aria-label="Restart ADB from connection help"
          onclick={restartAdb}
          disabled={restartBusy || connectBusy || adbMissing}
        >{restartBusy ? "Restarting ADB…" : "Restart ADB"}</button> first.
        {:else}
          Check that Network debugging is enabled on the TV.
        {/if}
        Wireless debugging may need pairing with
        <button class="inline-link" onclick={openPairPanel}>Pair PIN</button>; Network debugging uses
        <button class="inline-link" onclick={focusConnectBox}>Add by IP</button>.
        {#if adbBackend === "direct"}
          Connected by USB? Built-in ADB works over the network only; choose Google platform-tools in
          <a class="inline-link" href="/settings#adb">Settings</a>.
        {/if}
      </span>
    </div>
    {#if localNetwork.status && localNetwork.status !== "not_applicable"}
      <p class="local-network-help">
        {#if adbBackend === "direct"}
          On a Mac, check that ATV Optimizer is on in
          <button class="inline-link" onclick={openLocalNetworkSettings}>Privacy &amp; Security</button>
          → Local Network; if it already is, turn it off and on again (all of them, if it's listed
          more than once), then scan again.
        {:else}
          On a Mac, after an update, macOS can cut off an ADB server the earlier version left
          running, and Restart ADB replaces it. If that doesn't help, check that ATV Optimizer is on
          in <button class="inline-link" onclick={openLocalNetworkSettings}>Privacy &amp; Security</button>
          → Local Network; if it already is, turn it off and on again (all of them, if it's listed
          more than once), then Restart ADB.
        {/if}
      </p>
    {/if}
  </HelpDetails>
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
  .quit-toggle {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.85rem;
    color: var(--fg-muted);
    cursor: pointer;
    white-space: nowrap;
  }
  .quit-checking {
    font-style: italic;
  }
  .connect-form .quit-toggle input {
    flex: none;
    min-width: 0;
    accent-color: var(--accent-strong);
    cursor: pointer;
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
  .device-row.unapproved {
    flex-wrap: wrap;
  }
  .unapproved .device-main {
    flex: 1 1 18rem;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .unapproved .device-name,
  .unapproved .row-actions {
    flex-wrap: wrap;
  }
  .approval-line {
    margin: 0.3rem 0 0;
    font-size: 0.85rem;
    color: var(--fg-secondary);
  }
  .approval-line.approval-result {
    color: var(--danger-text);
  }
  .device-row:focus-visible .status-tag[data-tip]::after {
    opacity: 1;
    visibility: visible;
  }
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .scan-attempts {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0;
  }
  .scan-attempt {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 0.75rem;
    padding: 0.6rem 0;
    border-bottom: 1px solid var(--border);
  }
  .scan-attempt-info {
    display: grid;
    gap: 0.25rem;
    flex: 1 1 18rem;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .scan-attempt-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 0.5rem;
  }
  /* Board 11.12's row: a glyph for what it is, the name, the address in mono,
     whether it is reachable, and the one thing to do about it. The connection
     type sits in the meta line, not beside the name, where a bracketed word
     reads as part of the name. */
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
  .device-row.discovered {
    flex-wrap: wrap;
  }
  .discovered .device-main {
    flex: 1 1 18rem;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .discovered .device-name {
    flex-wrap: wrap;
  }
  .discovered .row-actions {
    flex-wrap: wrap;
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
  .local-network-action {
    margin-top: 0.4rem;
  }
  .remedies {
    display: inline-flex;
    flex-wrap: wrap;
    gap: 0.5rem;
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
  .inline-link:disabled {
    color: var(--fg-muted);
    cursor: default;
    text-decoration: none;
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
  .device-main {
    flex: 1 1 auto;
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .device-name {
    display: flex;
    flex-wrap: wrap;
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
