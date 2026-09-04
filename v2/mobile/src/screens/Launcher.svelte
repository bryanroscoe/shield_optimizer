<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { CurrentLauncher, LauncherStatus } from "../lib/types";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";

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

  // Stock-takeover confirm: set_default_launcher reports when the only way to
  // switch is disabling the active stock launcher — we ask before retrying.
  let takeover = $state<{ pkg: string; name: string } | null>(null);
  // Disable-stock confirm.
  let disableConfirm = $state<LauncherStatus | null>(null);

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

  async function load() {
    const serial = session.serial;
    const generation = ++loadGeneration;
    if (!serial) {
      error = "No TV connected.";
      loading = false;
      return;
    }
    loading = launchers.length === 0;
    error = "";
    const [rows, cur, chan] = await Promise.allSettled([
      api.listLaunchers(serial),
      api.currentLauncher(serial),
      api.channelProviderDisabled(serial),
    ]);
    if (generation !== loadGeneration || serial !== session.serial) return;
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
  }

  onMount(load);

  const currentPkg = $derived(current?.package ?? null);
  const activeRow = $derived(
    currentPkg ? launchers.find((l) => l.entry.package === currentPkg) : undefined,
  );
  const activeLabel = $derived(activeRow?.entry.name ?? currentPkg ?? "");

  function iconFor(l: LauncherStatus): string {
    if (l.stock) return "tv";
    if (l.other) return "widgets";
    return "grid_view";
  }

  function subtitle(l: LauncherStatus): string {
    if (l.stock) return l.enabled ? "Stock launcher" : "Disabled";
    if (l.other) return "Home-capable app";
    if (!l.installed) return "Not installed";
    return l.enabled ? "Installed" : "Installed · disabled";
  }

  async function setDefault(l: LauncherStatus, allowStockDisable = false) {
    if (busyPkg || !session.serial) return;
    busyPkg = l.entry.package;
    progress = "";
    try {
      const res = await api.setDefaultLauncher(
        session.serial,
        l.entry.package,
        allowStockDisable,
        (msg) => (progress = msg),
      );
      if (res.ok) {
        showToast(`${l.entry.name} is now the default launcher.`, "success");
        session.invalidateAll();
        await load();
      } else if (res.stock_takeover_available) {
        takeover = { pkg: l.entry.package, name: l.entry.name };
      } else {
        showToast(res.last_error || "Couldn't switch launcher.", "error");
      }
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
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
    if (busyPkg || !session.serial) return;
    busyPkg = l.entry.package;
    try {
      const res = await api.disableLauncher(session.serial, l.entry.package);
      showToast(res.message || (res.ok ? "Disabled." : "Couldn't disable."), res.ok ? "success" : "error");
      if (res.ok) {
        session.invalidateAll();
        await load();
      }
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      busyPkg = "";
    }
  }

  async function install(l: LauncherStatus) {
    if (busyPkg || !session.serial) return;
    busyPkg = l.entry.package;
    try {
      const res = await api.openPlayStore(session.serial, l.entry.package);
      showToast(
        res.ok ? `Opened the Play Store for ${l.entry.name} on the TV.` : res.message,
        res.ok ? "success" : "error",
      );
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyPkg = "";
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
    <span style="width:44px"></span>
  </div>

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Reading launchers…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={load}>Retry</button>
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
          {/if}
        </div>
        {#if activeLabel !== ""}
          <span class="default-badge">DEFAULT</span>
        {:else}
          <button class="l-btn" disabled={busyPkg !== ""} onclick={load}>Retry</button>
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
            <div class="launcher-row" class:active={isActive}>
              <div class="l-icon"><span class="msr">{iconFor(l)}</span></div>
              <div class="l-body">
                <span class="l-name">{l.entry.name}</span>
                <span class="l-sub">{subtitle(l)}</span>
              </div>

              {#if busy}
                <span class="l-progress"><span class="pdot blink"></span>{progress || "Working…"}</span>
              {:else if isActive}
                <span class="l-active-tag">Active</span>
              {:else if !l.installed && !l.stock}
                <button class="l-btn install" disabled={busyPkg !== ""} onclick={() => install(l)}>
                  <span class="msr">download</span>Install
                </button>
              {:else if l.stock && l.enabled}
                <button class="l-btn" disabled={busyPkg !== ""} onclick={() => (disableConfirm = l)}>
                  Disable
                </button>
              {:else}
                <button class="l-btn set" disabled={busyPkg !== ""} onclick={() => setDefault(l)}>
                  Set default{session.isPro ? "" : ""}
                  {#if !session.isPro}<span class="pro-badge">PRO</span>{/if}
                </button>
              {/if}
            </div>
          {/each}
        </div>
      {/if}

      <div class="callout amber launcher-note">
        <span class="msr">info</span>
        <span class="callout-text">
          Changing the launcher is reversible — set stock back as default any time. A snapshot
          records which launcher was active so you can re-apply it later; it never re-enables or
          reinstalls anything on its own.
        </span>
      </div>
    </div>
    <div class="spacer"></div>
  {/if}

  <ConfirmDialog
    open={takeover !== null}
    icon="home"
    title="Disable the stock launcher?"
    message={`On this TV the only way to hand Home to ${takeover?.name ?? "this launcher"} is to disable the stock launcher. Your other launchers are left alone, and stock can be re-enabled here any time.`}
    confirmLabel="Disable stock & switch"
    onConfirm={confirmTakeover}
    onCancel={() => (takeover = null)}
  />

  <ConfirmDialog
    open={disableConfirm !== null}
    danger
    icon="block"
    title={`Disable ${disableConfirm?.entry.name ?? "launcher"}?`}
    message="The TV will fall back to another enabled launcher for its home screen. You can re-enable this one from the Apps list."
    confirmLabel="Disable"
    onConfirm={() => disableConfirm && doDisable(disableConfirm)}
    onCancel={() => (disableConfirm = null)}
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
</style>
