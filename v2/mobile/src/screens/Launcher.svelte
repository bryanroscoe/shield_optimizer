<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { CurrentLauncher, LauncherStatus, OtherPackage } from "../lib/types";
  import {
    isInstalled,
    setupHelperView,
    sourceSiteHost,
    stockDisableGate,
    stockDisableMessage,
    takeoverTurnsOffSetupHelper,
  } from "../lib/launcherView";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";
  import SetupWraithCard from "../components/SetupWraithCard.svelte";
  import LauncherDiagnostics from "../components/LauncherDiagnostics.svelte";
  import HomeAppPicker from "../components/HomeAppPicker.svelte";
  import StockTakeoverDialog from "../components/StockTakeoverDialog.svelte";

  let {
    navigate,
    back,
  }: { navigate: (screen: Screen) => void; back: () => void } = $props();

  let loading = $state(true);
  let error = $state("");
  let launchers = $state<LauncherStatus[]>([]);
  let current = $state<CurrentLauncher | null>(null);
  // null until the check answers; the warning only renders on a real `true`.
  let channelDisabled = $state<boolean | null>(null);
  let currentReadFailed = $state(false);
  let busyPkg = $state("");
  let progress = $state("");
  let showPaywall = $state(false);
  let loadGeneration = 0;
  let shownSerial = session.serial;

  // Stock-takeover confirm: set_default_launcher reports when the only way to
  // switch is disabling the active stock launcher — we ask before retrying.
  let takeover = $state<{ pkg: string; name: string } | null>(null);
  // Disable confirm for a row (stock launcher or Setup Wraith).
  let disableConfirm = $state<LauncherStatus | null>(null);

  // The last not-ok row action, kept on screen (a toast is gone before a long
  // backend sentence can be read) with its diagnostics to copy.
  let actionNote = $state("");
  let actionDiagnostics = $state<string[]>([]);

  // "Open Play Store on TV" worked; the list re-reads until the app shows up.
  let storeOpened = $state<{ serial: string; pkg: string; name: string; checking: boolean } | null>(
    null,
  );
  let storeTimer: ReturnType<typeof setInterval> | undefined;
  let storePollBusy = false;
  // Clipboard refused for a source-site link: show it to select by hand.
  let linkShown = $state<{ name: string; url: string } | null>(null);

  // Advanced: set another app as Home. Every installed app, not only the ones
  // that declare a Home screen. Taking over from stock is its own step.
  let advancedOpen = $state(false);
  let pickerOpen = $state(false);
  let pickerPackages = $state<OtherPackage[]>([]);
  let pickerLoading = $state(false);
  let pickerError = $state("");
  let pickerSerial = "";
  let pickerGeneration = 0;
  let choice = $state<OtherPackage | null>(null);
  let activity = $state("");
  let homeBusy = $state<"set" | "stock" | null>(null);
  let homeResult = $state<{ ok: boolean; text: string } | null>(null);
  let homeDiagnostics = $state<string[]>([]);
  /// The last set_home_any said the stock launcher is what's in the way, for this app.
  let stockHoldsHomeFor = $state<string | null>(null);
  let stockConfirmOpen = $state(false);

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3800);
  }

  function isLocked(e: unknown): boolean {
    return String(e).includes("LOCKED:");
  }

  function noteFailure(text: string, diagnostics: string[] = []) {
    actionNote = text;
    actionDiagnostics = diagnostics;
  }

  function clearNote() {
    actionNote = "";
    actionDiagnostics = [];
  }

  /// Re-read the TV's launcher state. Resolves true when the launcher list
  /// itself was read for the TV that is still connected.
  async function load(): Promise<boolean> {
    const serial = session.serial;
    const generation = ++loadGeneration;
    if (!serial) {
      error = "No TV connected.";
      loading = false;
      return false;
    }
    loading = launchers.length === 0;
    error = "";
    const [rows, cur, chan] = await Promise.allSettled([
      api.listLaunchers(serial),
      api.currentLauncher(serial),
      api.channelProviderDisabled(serial),
    ]);
    if (generation !== loadGeneration || serial !== session.serial) return false;
    if (rows.status === "fulfilled") {
      launchers = rows.value;
      error = "";
    } else {
      error = String(rows.reason);
    }
    if (cur.status === "fulfilled") {
      current = cur.value;
      currentReadFailed = false;
    } else {
      current = null;
      currentReadFailed = true;
    }
    channelDisabled = chan.status === "fulfilled" ? chan.value : null;
    loading = false;
    return rows.status === "fulfilled";
  }

  /// Every launcher action ends here, ok or not: a not-ok result can still
  /// have landed (stock disabled with its helper left on, a switch that took
  /// a beat longer than the backend's poll), so the screen shows what the TV
  /// reports now rather than what the action hoped for (#157/#158).
  async function reread(serial: string): Promise<boolean> {
    if (serial !== session.serial) return false;
    progress = "Refreshing the launcher list";
    session.invalidateAll();
    return load();
  }

  onMount(load);

  onDestroy(() => {
    stopStorePoll();
    clearTimeout(toastTimer);
  });

  // A different TV: nothing read from the previous one applies any more.
  $effect(() => {
    const serial = session.serial;
    if (serial === shownSerial) return;
    shownSerial = serial;
    stopStorePoll();
    storeOpened = null;
    linkShown = null;
    clearNote();
    takeover = null;
    disableConfirm = null;
    stockConfirmOpen = false;
    pickerGeneration += 1;
    pickerOpen = false;
    pickerPackages = [];
    pickerSerial = "";
    pickerLoading = false;
    pickerError = "";
    choice = null;
    activity = "";
    homeResult = null;
    homeDiagnostics = [];
    stockHoldsHomeFor = null;
    launchers = [];
    current = null;
    channelDisabled = null;
    void load();
  });

  const anyBusy = $derived(busyPkg !== "" || homeBusy !== null);
  const currentPkg = $derived(current?.package ?? null);
  const activeRow = $derived(
    currentPkg ? launchers.find((l) => l.entry.package === currentPkg) : undefined,
  );
  const activeLabel = $derived(activeRow?.entry.name ?? currentPkg ?? "");
  const wraith = $derived(setupHelperView(launchers));
  const takeoverTurnsOffWraith = $derived(takeoverTurnsOffSetupHelper(launchers));
  const homePackages = $derived(new Set(launchers.map((l) => l.entry.package)));
  const gate = $derived(
    stockDisableGate({
      launchers,
      choice: choice?.package ?? "",
      currentPkg,
      stockHoldsHomeFor,
    }),
  );

  function nameOf(pkg: string): string {
    return launchers.find((l) => l.entry.package === pkg)?.entry.name ?? pkg;
  }

  function iconFor(l: LauncherStatus): string {
    if (l.stock) return "tv";
    if (l.other) return "widgets";
    return "grid_view";
  }

  function subtitle(l: LauncherStatus): string {
    if (l.stock) return l.enabled ? "Stock launcher" : "Stock launcher · disabled";
    if (l.setup_helper) {
      return l.enabled
        ? "Google TV setup helper, not a launcher"
        : "Google TV setup helper · off";
    }
    if (l.other) return l.enabled ? "Home-capable app" : "Home-capable app · disabled";
    if (!l.installed) return "Not installed";
    return l.enabled ? "Installed" : "Installed · disabled";
  }

  async function setDefault(l: LauncherStatus, allowStockDisable = false) {
    const serial = session.serial;
    if (anyBusy || !serial) return;
    const name = l.entry.name;
    busyPkg = l.entry.package;
    progress = "";
    clearNote();
    let locked = false;
    try {
      const res = await api.setDefaultLauncher(
        serial,
        l.entry.package,
        allowStockDisable,
        (msg) => (progress = msg),
      );
      if (serial !== session.serial) return;
      if (res.ok) {
        let text =
          res.strategy === "disable_stock_takeover"
            ? `${name} is now the default launcher. The stock launcher was disabled to hand it over; re-enable it here any time.`
            : `${name} is now the default launcher.`;
        // A successful switch only carries `last_error` as a follow-up note.
        if (res.last_error) text += ` ${res.last_error}`;
        showToast(text, res.last_error ? "info" : "success");
      } else if (res.stock_takeover_available && !allowStockDisable) {
        takeover = { pkg: l.entry.package, name };
      } else {
        const text = res.last_error || "Couldn't switch launcher.";
        showToast(text, "error");
        noteFailure(text, res.diagnostics ?? []);
      }
    } catch (e) {
      if (isLocked(e)) {
        locked = true;
        showPaywall = true;
      } else {
        showToast(String(e), "error");
        noteFailure(String(e));
      }
    } finally {
      // A locked call never reached the TV, so there is nothing to re-read.
      if (!locked) await reread(serial);
      busyPkg = "";
      progress = "";
    }
  }

  async function confirmTakeover() {
    const t = takeover;
    takeover = null;
    if (!t) return;
    const row = launchers.find((l) => l.entry.package === t.pkg);
    if (row) await setDefault(row, true);
  }

  async function doDisable(l: LauncherStatus) {
    disableConfirm = null;
    const serial = session.serial;
    if (anyBusy || !serial) return;
    busyPkg = l.entry.package;
    progress = l.setup_helper ? "Turning off Google TV's setup helper" : "Disabling";
    clearNote();
    let locked = false;
    try {
      const res = await api.disableLauncher(serial, l.entry.package);
      if (serial !== session.serial) return;
      const text = res.ok
        ? res.message || `${l.entry.name} disabled.`
        : `Couldn't fully disable ${l.entry.name}: ${res.message.trim() || "failed"}`;
      showToast(text, res.ok ? "success" : "error");
      if (!res.ok) noteFailure(text);
    } catch (e) {
      if (isLocked(e)) {
        locked = true;
        showPaywall = true;
      } else {
        showToast(String(e), "error");
        noteFailure(String(e));
      }
    } finally {
      if (!locked) await reread(serial);
      busyPkg = "";
      progress = "";
    }
  }

  async function turnOffWraith() {
    const serial = session.serial;
    const row = wraith?.row;
    if (anyBusy || !serial || !row) return;
    busyPkg = row.entry.package;
    progress = "Turning off Google TV's setup helper";
    clearNote();
    let locked = false;
    try {
      const r = await api.disableSetupHelper(serial, row.entry.package);
      if (serial !== session.serial) return;
      if (r.ok) {
        showToast("Setup Wraith is off. Press Home on the TV to check it lands on your launcher.", "success");
      } else {
        const text = `Couldn't turn off Setup Wraith: ${r.message.trim() || "failed"}`;
        showToast(text, "error");
        noteFailure(text);
      }
    } catch (e) {
      if (isLocked(e)) {
        locked = true;
        showPaywall = true;
      } else {
        showToast(String(e), "error");
        noteFailure(String(e));
      }
    } finally {
      if (!locked) await reread(serial);
      busyPkg = "";
      progress = "";
    }
  }

  async function reenableWraith() {
    const serial = session.serial;
    const row = wraith?.row;
    if (anyBusy || !serial || !row) return;
    const pkg = row.entry.package;
    const prev = currentPkg;
    const prevName = prev ? nameOf(prev) : "";
    busyPkg = pkg;
    progress = "Re-enabling Google TV's setup helper";
    clearNote();
    try {
      const r = await api.enablePackage(serial, pkg);
      if (serial !== session.serial) return;
      const read = await reread(serial);
      if (!r.ok) {
        const text = `Couldn't re-enable Setup Wraith: ${r.message.trim() || "failed"}`;
        showToast(text, "error");
        noteFailure(text);
        return;
      }
      // Android can hand Home to a package whose state just changed.
      // Re-enabling is not switching, so put the previous default back.
      if (read && prev && prev !== pkg && currentPkg === pkg) {
        if (!session.isPro) {
          const text = `Setup Wraith is back on, and Android gave it the Home button. Set ${prevName} as default again to get it back.`;
          showToast(text, "info");
          noteFailure(text);
          return;
        }
        progress = `Restoring ${prevName} as default`;
        const backRes = await api.setDefaultLauncher(serial, prev);
        if (serial !== session.serial) return;
        await reread(serial);
        if (backRes.ok) {
          showToast(
            `Setup Wraith is back on. Android gave it the Home button, so ${prevName} was set as default again.`,
            "success",
          );
        } else {
          const text =
            `Setup Wraith is back on, and Android gave it the Home button. Setting ${prevName} back failed` +
            `${backRes.last_error ? `: ${backRes.last_error}` : "."} Use Set default on your launcher.`;
          showToast(text, "error");
          noteFailure(text, backRes.diagnostics ?? []);
        }
      } else {
        showToast("Setup Wraith is back on.", "success");
      }
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else {
        showToast(String(e), "error");
        noteFailure(String(e));
      }
      await reread(serial);
    } finally {
      busyPkg = "";
      progress = "";
    }
  }

  function stopStorePoll() {
    if (storeTimer) clearInterval(storeTimer);
    storeTimer = undefined;
  }

  /// The install happens on the TV, out of our sight, so look again every 5 s
  /// for a minute and stop as soon as the app shows up.
  function startStorePoll(serial: string, pkg: string, name: string) {
    stopStorePoll();
    storeOpened = { serial, pkg, name, checking: true };
    const deadline = Date.now() + 60_000;
    storeTimer = setInterval(async () => {
      if (serial !== session.serial) {
        stopStorePoll();
        storeOpened = null;
        return;
      }
      if (Date.now() > deadline) {
        stopStorePoll();
        if (storeOpened?.pkg === pkg) storeOpened = { ...storeOpened, checking: false };
        return;
      }
      if (loading || anyBusy || storePollBusy) return;
      storePollBusy = true;
      try {
        await load();
      } finally {
        storePollBusy = false;
      }
      if (serial === session.serial && isInstalled(launchers, pkg)) {
        stopStorePoll();
        storeOpened = null;
        showToast(`${name} is installed.`, "success");
      }
    }, 5_000);
  }

  async function install(l: LauncherStatus) {
    const serial = session.serial;
    if (anyBusy || !serial) return;
    busyPkg = l.entry.package;
    progress = "Opening the Play Store";
    stopStorePoll();
    storeOpened = null;
    try {
      const res = await api.openPlayStore(serial, l.entry.package);
      if (serial !== session.serial) return;
      if (res.ok) startStorePoll(serial, l.entry.package, l.entry.name);
      else showToast(res.message.trim() || "Couldn't open the Play Store on the TV.", "error");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyPkg = "";
      progress = "";
    }
  }

  /// A launcher's official page, for one the TV's Play Store doesn't carry.
  /// Nothing is downloaded or installed. This app has no way to hand a URL to
  /// the phone's browser yet (a plain link would load the site inside the app
  /// itself), so it uses `api.openUrl` once that exists and copies the link
  /// until then.
  async function openSourceSite(l: LauncherStatus) {
    const url = l.entry.source_url;
    const host = sourceSiteHost(url);
    if (!url || !host) return;
    linkShown = null;
    if ("openUrl" in api && typeof api.openUrl === "function") {
      try {
        await api.openUrl(url);
        return;
      } catch {
        // Fall through to copying.
      }
    }
    try {
      await navigator.clipboard.writeText(url);
      showToast(`Copied the ${host} link for ${l.entry.name}. Paste it into your phone's browser.`, "info");
    } catch {
      linkShown = { name: l.entry.name, url };
    }
  }

  async function loadPicker(force = false) {
    const serial = session.serial;
    if (!serial) {
      pickerError = "No TV connected.";
      return;
    }
    if (!force && pickerSerial === serial && pickerPackages.length > 0) return;
    const generation = ++pickerGeneration;
    pickerLoading = true;
    pickerError = "";
    try {
      const rows = await api.listInstalledPackages(serial);
      if (generation !== pickerGeneration || serial !== session.serial) return;
      pickerPackages = rows;
      pickerSerial = serial;
    } catch (e) {
      if (generation !== pickerGeneration || serial !== session.serial) return;
      pickerError = String(e);
    } finally {
      if (generation === pickerGeneration) pickerLoading = false;
    }
  }

  function openPicker() {
    pickerOpen = true;
    void loadPicker();
  }

  function pick(p: OtherPackage) {
    choice = p;
    pickerOpen = false;
    homeResult = null;
    homeDiagnostics = [];
    stockHoldsHomeFor = null;
    stockConfirmOpen = false;
  }

  async function setHomeFromPicker() {
    const serial = session.serial;
    const pkg = choice?.package;
    if (anyBusy || !serial || !pkg) return;
    homeBusy = "set";
    homeResult = null;
    homeDiagnostics = [];
    stockConfirmOpen = false;
    let locked = false;
    try {
      const r = await api.setHomeAny(serial, pkg, activity.trim() || null);
      if (serial !== session.serial) return;
      // The backend's sentence says exactly what Android did; `ok` is only
      // true once the TV confirmed the app holds Home.
      homeResult = { ok: r.ok, text: r.message };
      stockHoldsHomeFor = r.stock_holds_home ? pkg : null;
      homeDiagnostics = r.ok ? [] : (r.diagnostics ?? []);
    } catch (e) {
      if (isLocked(e)) {
        locked = true;
        showPaywall = true;
      } else {
        homeResult = { ok: false, text: String(e) };
      }
    } finally {
      if (!locked) await reread(serial);
      homeBusy = null;
      progress = "";
    }
  }

  async function disableStock(saveFirst: boolean) {
    stockConfirmOpen = false;
    const serial = session.serial;
    const pkg = choice?.package;
    if (anyBusy || !serial || !pkg) return;
    homeBusy = "stock";
    homeResult = null;
    homeDiagnostics = [];
    if (saveFirst) {
      try {
        await api.saveSnapshot(serial, session.deviceLabel, "Before disabling the stock launcher");
      } catch (e) {
        if (isLocked(e)) showPaywall = true;
        if (serial === session.serial) {
          homeResult = {
            ok: false,
            text: `The snapshot wasn't saved, so the stock launcher was left alone.${isLocked(e) ? "" : ` ${String(e)}`}`,
          };
        }
        homeBusy = null;
        return;
      }
      if (serial !== session.serial) {
        homeBusy = null;
        return;
      }
    }
    const expectWraithOff = takeoverTurnsOffWraith;
    try {
      const r = await api.disableStockLauncher(serial, pkg);
      if (serial !== session.serial) return;
      stockHoldsHomeFor = null;
      const read = await reread(serial);
      const helperAfter = read ? (setupHelperView(launchers)?.state ?? null) : null;
      homeResult = { ok: r.ok, text: stockDisableMessage(r, pkg, expectWraithOff, helperAfter) };
      homeDiagnostics = r.ok ? [] : (r.diagnostics ?? []);
    } catch (e) {
      if (isLocked(e)) {
        showPaywall = true;
      } else {
        homeResult = { ok: false, text: String(e) };
        await reread(serial);
      }
    } finally {
      homeBusy = null;
      progress = "";
    }
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Launcher</h3>
    <button class="iconbtn" onclick={() => load()} disabled={anyBusy || loading} aria-label="Refresh">
      <span class="msr">refresh</span>
    </button>
  </div>

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Reading launchers…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={() => load()}>Retry</button>
    <div class="spacer"></div>
  {:else}
    <div class="launcher-content">
      <!-- Active home screen -->
      <div class="active-card" class:unknown={activeLabel === ""}>
        <div class="active-icon"><span class="msr">{activeLabel === "" ? "error" : "home"}</span></div>
        <div class="active-body">
          <span class="active-eyebrow">Active home screen</span>
          {#if activeLabel === ""}
            <span class="active-name">Couldn't read the current launcher</span>
            <span class="active-hint">
              {currentReadFailed
                ? "The TV didn't answer the resolver query."
                : "The TV reported no HOME activity."} Switching still works.
            </span>
          {:else}
            <span class="active-name">{activeLabel}</span>
            {#if current?.note}
              <span class="active-hint">{current.note}</span>
            {/if}
          {/if}
        </div>
        {#if activeLabel !== ""}
          <span class="default-badge">DEFAULT</span>
        {:else}
          <button class="l-btn" disabled={anyBusy} onclick={() => load()}>Retry</button>
        {/if}
      </div>

      {#if channelDisabled}
        <div class="callout amber">
          <span class="msr">warning</span>
          <span class="callout-text">
            <span class="mono">com.android.providers.tv</span> is disabled on this TV. Watch Next /
            Continue Watching rows from Netflix, Disney+, Apple TV etc. stay empty until you
            re-enable it from the Apps list.
          </span>
        </div>
      {/if}

      <span class="section-label nomargin">Available launchers</span>

      {#if launchers.length === 0}
        <p class="lede empty">No launchers detected on this TV.</p>
      {:else}
        <div class="launcher-list">
          {#each launchers as l (l.entry.package)}
            {@const isActive = l.entry.package === currentPkg}
            {@const busy = busyPkg === l.entry.package}
            {@const host = !l.installed && !l.stock ? sourceSiteHost(l.entry.source_url) : null}
            <div class="launcher-row" class:active={isActive}>
              <div class="l-icon"><span class="msr">{iconFor(l)}</span></div>
              <div class="l-body">
                <span class="l-name">{l.entry.name}</span>
                <span class="l-sub">{subtitle(l)}</span>
                {#if host}
                  <button
                    class="src-link"
                    onclick={() => openSourceSite(l)}
                    aria-label={`Source site for ${l.entry.name} (${host})`}
                  >
                    <span class="msr">content_copy</span>Source site · {host}
                  </button>
                {/if}
              </div>

              {#if busy}
                <span class="l-progress"><span class="pdot blink"></span>{progress || "Working…"}</span>
              {:else if isActive}
                <span class="l-active-tag">Active</span>
              {:else if !l.installed && !l.stock}
                <button class="l-btn install" disabled={anyBusy} onclick={() => install(l)}>
                  <span class="msr">download</span>Install
                </button>
              {:else if l.setup_helper}
                <!-- Google TV's setup wizard declares Home but is never a default.
                     Its on/off fix lives in the Setup Wraith card below. -->
                {#if l.enabled}
                  <button class="l-btn" disabled={anyBusy} onclick={() => (disableConfirm = l)}>
                    Disable
                  </button>
                {/if}
              {:else if l.stock && l.enabled}
                <button class="l-btn" disabled={anyBusy} onclick={() => (disableConfirm = l)}>
                  Disable
                </button>
              {:else}
                <button class="l-btn set" disabled={anyBusy} onclick={() => setDefault(l)}>
                  Set default
                  {#if !session.isPro}<span class="pro-badge">PRO</span>{/if}
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      {#if wraith}
        <SetupWraithCard
          state={wraith.state}
          pkg={wraith.row.entry.package}
          disabled={anyBusy}
          progress={busyPkg === wraith.row.entry.package ? progress || "Working" : ""}
          onTurnOff={turnOffWraith}
          onReenable={reenableWraith}
        />
      {/if}

      {#if storeOpened}
        <div class="callout teal store-callout" role="status">
          <span class="msr">check_circle</span>
          <span class="callout-text">
            Opened the Play Store on the TV. Confirm the install there.
            <span class="muted-line">
              {storeOpened.checking
                ? `Checking for ${storeOpened.name} every few seconds.`
                : `Stopped checking for ${storeOpened.name}. Tap Refresh once the install finishes.`}
            </span>
          </span>
          <button
            class="callout-dismiss"
            aria-label="Dismiss"
            onclick={() => {
              stopStorePoll();
              storeOpened = null;
            }}
          >
            <span class="msr">close</span>
          </button>
        </div>
      {/if}

      {#if linkShown}
        <div class="callout accent" role="status">
          <span class="msr">info</span>
          <span class="callout-text">
            Couldn't copy the link for {linkShown.name}. Select it and open it in your phone's
            browser:
            <span class="mono link-text">{linkShown.url}</span>
          </span>
          <button class="callout-dismiss" aria-label="Dismiss" onclick={() => (linkShown = null)}>
            <span class="msr">close</span>
          </button>
        </div>
      {/if}

      {#if actionNote}
        <div class="action-note" role="status">
          <p class="action-text">{actionNote}</p>
          <LauncherDiagnostics lines={actionDiagnostics} />
        </div>
      {/if}

      <div class="callout amber launcher-note">
        <span class="msr">info</span>
        <span class="callout-text">
          Changing the launcher is reversible — set stock back as default any time. Disabling the
          current home app without setting a replacement first leaves the TV with no home screen,
          so save a snapshot before you change this. A snapshot records which launcher was active;
          it never re-enables or reinstalls anything on its own.
        </span>
      </div>

      <button
        class="adv-toggle"
        aria-expanded={advancedOpen}
        onclick={() => (advancedOpen = !advancedOpen)}
      >
        <span class="adv-title">Advanced: set another app as Home</span>
        <span class="msr">{advancedOpen ? "expand_less" : "expand_more"}</span>
      </button>

      {#if advancedOpen}
        <div class="adv">
          <p class="adv-note">
            Any installed app can be tried. Android only accepts an app that declares a Home
            screen, and this says so when it doesn't. Setting an app never disables anything;
            taking over from the stock launcher is the separate step below.
          </p>

          {#if pickerOpen}
            <HomeAppPicker
              packages={pickerPackages}
              loading={pickerLoading}
              error={pickerError}
              {homePackages}
              onPick={pick}
              onRetry={() => loadPicker(true)}
              onCancel={() => (pickerOpen = false)}
            />
          {:else}
            <div class="adv-choice">
              <span class="msr adv-choice-icon">{choice ? "android" : "apps"}</span>
              <span class="adv-choice-body">
                {#if choice}
                  <span class="adv-choice-name">{choice.name ?? choice.package}</span>
                  <span class="adv-choice-pkg mono">{choice.package}</span>
                {:else}
                  <span class="adv-choice-empty">No app chosen</span>
                {/if}
              </span>
              <button class="l-btn" disabled={anyBusy} onclick={openPicker}>
                {choice ? "Change" : "Choose an app"}
              </button>
            </div>

            <label class="adv-field">
              <span class="adv-label">Activity (optional)</span>
              <input
                class="adv-input mono"
                placeholder=".MainActivity"
                bind:value={activity}
                disabled={homeBusy !== null}
                autocapitalize="off"
                autocomplete="off"
                spellcheck="false"
                aria-label="Activity to register as Home (optional)"
              />
            </label>

            <button
              class="primary small adv-action"
              disabled={!choice || anyBusy}
              onclick={setHomeFromPicker}
            >
              {#if homeBusy === "set"}
                <span class="pdot blink"></span>{progress || "Setting"}…
              {:else}
                <span class="msr">home</span>Set as Home
                {#if !session.isPro}<span class="pro-badge">PRO</span>{/if}
              {/if}
            </button>

            <button
              class="ghost small adv-action danger-ghost"
              disabled={!gate.allowed || anyBusy}
              onclick={() => (stockConfirmOpen = true)}
            >
              {#if homeBusy === "stock"}
                <span class="pdot blink"></span>{progress || "Disabling"}…
              {:else}
                Disable stock launcher
                {#if !session.isPro}<span class="pro-badge">PRO</span>{/if}
              {/if}
            </button>
            {#if !gate.allowed}
              <p class="adv-reason">{gate.reason}</p>
            {/if}

            {#if homeResult}
              <p class="adv-result" class:ok={homeResult.ok} role="status">{homeResult.text}</p>
            {/if}
            <LauncherDiagnostics lines={homeDiagnostics} />
          {/if}
        </div>
      {/if}
    </div>
    <div class="spacer"></div>
  {/if}

  <ConfirmDialog
    open={takeover !== null}
    icon="home"
    title="Disable the stock launcher?"
    message={`On this TV the only way to hand Home to ${takeover?.name ?? "this launcher"} is to disable the stock launcher. Your other launchers are left alone, and stock can be re-enabled here any time.`}
    warning={takeoverTurnsOffWraith
      ? "Also turns off Google TV's setup helper (Setup Wraith), or it takes the Home button back. You may need to turn it back on briefly to sign in to Google again or pair a remote."
      : ""}
    confirmLabel="Disable stock & switch"
    onConfirm={confirmTakeover}
    onCancel={() => (takeover = null)}
  />

  <ConfirmDialog
    open={disableConfirm !== null}
    danger
    icon="block"
    title={disableConfirm?.setup_helper
      ? "Disable Setup Wraith?"
      : `Disable ${disableConfirm?.entry.name ?? "launcher"}?`}
    message={disableConfirm?.setup_helper
      ? "This is Google TV's setup wizard, not a launcher. You don't need to disable it: the app already shows your real launcher as current. Disabling it is only useful if the Home button keeps landing on a setup screen. It stays disabled until you re-enable it here, and Android TV needs it to run first-time setup. Re-enable it before a factory reset."
      : "The TV will fall back to another enabled launcher for its home screen. You can re-enable this one here or from the Apps list."}
    confirmLabel="Disable"
    onConfirm={() => disableConfirm && doDisable(disableConfirm)}
    onCancel={() => (disableConfirm = null)}
  />

  <StockTakeoverDialog
    open={stockConfirmOpen}
    target={choice?.package ?? ""}
    turnsOffSetupHelper={takeoverTurnsOffWraith}
    onSaveFirst={() => disableStock(true)}
    onConfirm={() => disableStock(false)}
    onCancel={() => (stockConfirmOpen = false)}
  />

  <PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />
  <Toast message={toast} type={toastType} />
</div>

<style>
  .header-left {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .launcher-content {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .active-card.unknown {
    background: var(--surface);
    border-color: var(--line);
  }
  .active-card.unknown .active-icon {
    background: var(--surface-2);
  }
  .active-card.unknown .active-icon .msr {
    color: var(--muted);
  }
  .active-hint {
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }

  .active-card {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 16px;
    border-radius: 18px;
    background: linear-gradient(155deg, color-mix(in srgb, var(--accent) 13%, #141519), #141519);
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .active-icon {
    width: 46px;
    height: 46px;
    border-radius: 13px;
    background: var(--accent);
    display: grid;
    place-items: center;
    flex: none;
  }
  .active-icon .msr {
    font-size: 26px;
    color: var(--accent-ink);
  }
  .active-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .active-eyebrow {
    font-size: 11px;
    color: var(--muted);
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .active-name {
    font-size: 16px;
    font-weight: 700;
  }
  .default-badge {
    font-size: 10px;
    font-weight: 700;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
    padding: 4px 9px;
    border-radius: 7px;
    flex: none;
  }

  .section-label.nomargin {
    margin: 4px 0 0;
  }

  .launcher-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .launcher-row {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 14px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .launcher-row.active {
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .l-icon {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    flex: none;
  }
  .l-icon .msr {
    font-size: 24px;
    color: var(--text-soft);
  }
  .l-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .l-name {
    font-size: 14px;
    font-weight: 600;
  }
  .l-sub {
    font-size: 11px;
    color: var(--muted);
  }
  .l-btn {
    height: 38px;
    padding: 0 14px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    gap: 6px;
    flex: none;
  }
  .l-btn:active {
    background: var(--surface);
  }
  .l-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .l-btn .msr {
    font-size: 16px;
  }
  .l-btn.install {
    color: var(--accent);
  }
  .pro-badge {
    font-size: 9px;
    font-weight: 700;
    background: var(--accent);
    color: var(--accent-ink);
    padding: 2px 5px;
    border-radius: 5px;
  }
  .l-active-tag {
    font-size: 11px;
    font-weight: 700;
    color: var(--accent);
    flex: none;
  }
  .l-progress {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-size: 11px;
    color: var(--muted);
    flex: none;
    max-width: 130px;
    text-align: right;
  }

  .launcher-note {
    margin-top: 4px;
  }
  .callout-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.45;
  }
  .callout-text .mono {
    font-family: var(--mono);
    font-size: 11px;
  }
  .callout.amber {
    background: color-mix(in srgb, var(--amber) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--amber) 22%, transparent);
  }
  .callout.amber .msr {
    color: var(--amber);
  }
  .iconbtn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .src-link {
    align-self: flex-start;
    margin-top: 4px;
    padding: 4px 0;
    min-height: 28px;
    border: 0;
    background: transparent;
    color: var(--accent);
    font-family: var(--sans);
    font-size: 11px;
    font-weight: 600;
    display: inline-flex;
    align-items: center;
    gap: 5px;
    cursor: pointer;
  }
  .src-link .msr {
    font-size: 14px;
  }
  .muted-line {
    display: block;
    margin-top: 3px;
    color: var(--muted);
  }
  .link-text {
    display: block;
    margin-top: 6px;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .callout-dismiss {
    width: 32px;
    height: 32px;
    flex: none;
    border: 0;
    background: transparent;
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .callout-dismiss .msr {
    font-size: 18px;
    color: var(--muted);
  }
  .action-note {
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px 14px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--danger) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger) 24%, transparent);
  }
  .action-text {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
  }

  .adv-toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 48px;
    padding: 0 14px;
    border-radius: 14px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    font-family: var(--sans);
    cursor: pointer;
    text-align: left;
  }
  .adv-title {
    flex: 1;
    font-size: 13px;
    font-weight: 600;
  }
  .adv-toggle .msr {
    font-size: 22px;
    color: var(--muted);
  }
  .adv {
    display: flex;
    flex-direction: column;
    gap: 12px;
    padding: 14px;
    border-radius: 16px;
    border: 1px solid var(--line);
    background: color-mix(in srgb, var(--surface) 60%, transparent);
  }
  .adv-note {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
  }
  .adv-choice {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: 13px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .adv-choice-icon {
    font-size: 22px;
    color: var(--text-soft);
    flex: none;
  }
  .adv-choice-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .adv-choice-name {
    font-size: 14px;
    font-weight: 600;
  }
  .adv-choice-pkg {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .adv-choice-empty {
    font-size: 13px;
    color: var(--muted);
  }
  .adv-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .adv-label {
    font-family: var(--mono);
    font-size: 10px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--dim);
  }
  .adv-input {
    min-height: 44px;
    padding: 0 12px;
    border-radius: 11px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    font-family: var(--mono);
    font-size: 13px;
  }
  .adv-action {
    flex: none;
    width: 100%;
    gap: 8px;
  }
  .danger-ghost {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 35%, transparent);
  }
  .adv-reason {
    margin: -4px 0 0;
    font-size: 11px;
    line-height: 1.45;
    color: var(--muted);
  }
  .adv-result {
    margin: 0;
    padding: 10px 12px;
    border-radius: 12px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
    background: color-mix(in srgb, var(--amber) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--amber) 24%, transparent);
  }
  .adv-result.ok {
    background: color-mix(in srgb, var(--teal) 8%, transparent);
    border-color: color-mix(in srgb, var(--teal) 25%, transparent);
  }
</style>
