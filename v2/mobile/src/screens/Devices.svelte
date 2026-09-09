<script lang="ts">
  import { onMount } from "svelte";
  import { session } from "../lib/session.svelte";
  import { api } from "../lib/api";
  import {
    forgetSavedDevice,
    lastUsedLabel,
    listSavedDevices,
    savedDeviceKey,
    savedDeviceMatchesConnection,
    savedHostHasMultipleIdentities,
  } from "../lib/savedDevices";
  import { deviceTypeLabel } from "../lib/types";
  import type { Screen } from "../lib/router.svelte";
  import type { SavedDevice } from "../lib/types";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import Toast from "../components/Toast.svelte";

  let { navigate, back, onDisconnect }: {
    navigate: (screen: Screen) => void;
    back: () => void;
    onDisconnect: () => void;
  } = $props();

  let saved = $state<SavedDevice[]>([]);
  let connectingToken = $state("");
  let connectErrorKey = $state("");
  let disconnectConfirm = $state(false);
  let renaming = $state(false);
  let renameValue = $state("");
  let forgetTarget = $state<SavedDevice | null>(null);

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3800);
  }

  function refreshSaved() {
    saved = listSavedDevices();
  }

  onMount(() => {
    refreshSaved();
    session.loadHealth();
    session.checkLiveness();
  });

  // The shared health cache is the one source; a second per-device sweep here
  // used to profile the connected TV twice before the card painted.
  const health = $derived(session.health);

  const ramFree = $derived(
    health?.ram?.free_mb != null ? `${(health.ram.free_mb / 1024).toFixed(1)} GB free` : "—",
  );
  const temp = $derived(
    health?.temperature_c != null ? `${Math.round(health.temperature_c)} °C` : "—",
  );
  const storageUsed = $derived(
    health?.storage?.used_percent != null ? `${health.storage.used_percent}% used` : "—",
  );
  const androidRelease = $derived(
    session.connectedDevice?.properties?.android_release || "",
  );

  const currentHardwareId = $derived(
    session.connectedDevice?.properties?.serial_number?.trim() || undefined,
  );
  const otherTvs = $derived(
    saved.filter(
      (device) =>
        !session.connectedDevice ||
        !savedDeviceMatchesConnection(
          device,
          session.host,
          session.connectPort,
          currentHardwareId,
        ),
    ),
  );

  async function reconnect(d: SavedDevice) {
    if (connectingToken) return;
    const token = savedDeviceKey(d);
    connectingToken = token;
    connectErrorKey = "";
    try {
      const r = await session.connect(d.host, d.connectPort);
      if (r.ok) {
        showToast(`Connected to ${session.deviceLabel}.`, "success");
        session.loadHealth(true);
        session.loadBloat(true);
        refreshSaved();
        navigate("dashboard");
      } else {
        connectErrorKey = token;
        showToast(r.message || "Couldn't connect.", "error");
      }
    } catch (e) {
      connectErrorKey = token;
      showToast(String(e), "error");
    } finally {
      connectingToken = "";
    }
  }

  /// Redial the current TV after the connection was lost.
  async function reconnectCurrent() {
    if (connectingToken) return;
    connectingToken = "current";
    try {
      const r = await session.reconnect();
      showToast(r.ok ? "Reconnected." : r.message || "Couldn't reconnect.", r.ok ? "success" : "error");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      connectingToken = "";
    }
  }

  function doForget(d: SavedDevice) {
    forgetTarget = null;
    forgetSavedDevice(d);
    refreshSaved();
    showToast(`Removed ${d.name}.`, "info");
  }

  function startRename() {
    renameValue = session.deviceLabel;
    renaming = true;
  }

  async function saveRename() {
    const name = renameValue.trim();
    renaming = false;
    if (!name || !session.serial) return;
    try {
      const r = await api.renameDevice(session.serial, name);
      showToast(r.message || (r.ok ? "Renamed." : "Rename failed."), r.ok ? "success" : "error");
      if (r.ok) {
        await session.refreshDevices();
        session.rememberCurrentDevice();
        refreshSaved();
      }
    } catch (e) {
      showToast(String(e), "error");
    }
  }

  function iconFor(t: string): string {
    return t === "shield" ? "cast" : "tv";
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
    <h3 class="header-title">Devices</h3>
    <button class="iconbtn" onclick={() => navigate("addtv")} aria-label="Add a TV">
      <span class="msr">add</span>
    </button>
  </div>

  <div class="devices-content">
    <!-- Connected device -->
    {#if session.connectedDevice}
      <div class="active-card">
        <div class="active-top">
          <div class="active-icon"><span class="msr">{iconFor(session.connectedDevice.device_type)}</span></div>
          <div class="active-body">
            {#if renaming}
              <input class="rename-input" bind:value={renameValue} onkeydown={(e) => e.key === "Enter" && saveRename()} />
            {:else}
              <span class="active-name">{session.deviceLabel}</span>
            {/if}
            <span class="active-sub mono">
              {session.host}:{session.connectPort}{androidRelease ? ` · Android ${androidRelease}` : ""}
            </span>
          </div>
          <span class="active-tag" class:lost={session.liveness === "lost"}>
            {session.liveness === "lost"
              ? "Offline"
              : session.liveness === "reconnecting"
                ? "Reconnecting"
                : "Active"}
          </span>
        </div>

        <div class="active-stats">
          <span class="stat"><span class="msr">memory</span>{ramFree}</span>
          <span class="stat"><span class="msr">database</span>{storageUsed}</span>
          <span class="stat"><span class="msr">thermostat</span>{temp}</span>
        </div>

        <div class="active-actions">
          {#if renaming}
            <button class="mini-btn accent" onclick={saveRename}>Save name</button>
            <button class="mini-btn" onclick={() => (renaming = false)}>Cancel</button>
          {:else}
            {#if session.liveness === "lost"}
              <button class="mini-btn accent" disabled={connectingToken !== ""} onclick={reconnectCurrent}>
                {#if connectingToken === "current"}<span class="pdot blink"></span>{:else}<span class="msr">refresh</span>Reconnect{/if}
              </button>
            {/if}
            <button class="mini-btn" onclick={startRename}><span class="msr">edit</span>Rename</button>
            <button class="mini-btn danger" onclick={() => (disconnectConfirm = true)}><span class="msr">power_settings_new</span>Disconnect</button>
          {/if}
        </div>
      </div>
    {:else}
      <div class="callout accent">
        <span class="msr">cast</span>
        <span class="callout-text">No TV connected. Pair one to get started.</span>
      </div>
    {/if}

    <!-- Other saved TVs -->
    {#if otherTvs.length > 0}
      <span class="section-label">Other TVs</span>
      <div class="other-list">
        {#each otherTvs as d (savedDeviceKey(d))}
          <div class="other-row">
            <div class="o-icon"><span class="msr">{iconFor(d.deviceType)}</span></div>
            <div class="o-body">
              <span class="o-name">{d.name}</span>
              <span class="o-sub mono">{d.host}:{d.connectPort} · {deviceTypeLabel(d.deviceType)} · {lastUsedLabel(d.lastUsed)}</span>
              {#if connectErrorKey === savedDeviceKey(d)}
                <span class="o-note error">Couldn't connect to this saved entry</span>
              {:else if savedHostHasMultipleIdentities(saved, d.host)}
                <span class="o-note">Shared saved address · identities kept separate</span>
              {/if}
            </div>
            <button class="o-forget" onclick={() => (forgetTarget = d)} aria-label="Forget {d.name}">
              <span class="msr">close</span>
            </button>
            <button class="o-connect" disabled={connectingToken !== ""} onclick={() => reconnect(d)}>
              {#if connectingToken === savedDeviceKey(d)}<span class="pdot blink"></span>{:else}Connect{/if}
            </button>
          </div>
        {/each}
      </div>
    {/if}

    {#if session.connectedDevice}
      <div class="quick-actions">
        <button class="quick-btn" onclick={() => navigate("files")}>
          <span class="msr">sync_alt</span>
          <span class="quick-label">Transfer files</span>
        </button>
        <button class="quick-btn" onclick={() => navigate("backups")}>
          <span class="msr">cloud_sync</span>
          <span class="quick-label">Backups</span>
        </button>
      </div>
    {/if}

    <div class="callout teal">
      <span class="msr">verified_user</span>
      <span class="callout-text">Your phone remembers each paired TV, so reconnecting is one tap — no code needed.</span>
    </div>
  </div>

  <div class="spacer"></div>

  <ConfirmDialog
    open={forgetTarget !== null}
    icon="close"
    title={`Forget ${forgetTarget?.name ?? "this TV"}?`}
    message="It's removed from this list. You can pair it again any time — the TV isn't affected."
    confirmLabel="Forget"
    onConfirm={() => forgetTarget && doForget(forgetTarget)}
    onCancel={() => (forgetTarget = null)}
  />

  <ConfirmDialog
    open={disconnectConfirm}
    icon="power_settings_new"
    title="Disconnect from {session.deviceLabel}?"
    message="The app stops talking to this TV until you connect again. Nothing on the TV changes."
    confirmLabel="Disconnect"
    onConfirm={() => { disconnectConfirm = false; onDisconnect(); }}
    onCancel={() => (disconnectConfirm = false)}
  />

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

  .devices-content {
    display: flex;
    flex-direction: column;
    gap: 14px;
  }

  .active-card {
    padding: 18px;
    border-radius: 20px;
    background: linear-gradient(155deg, color-mix(in srgb, var(--accent) 12%, #141519), #141519);
    border: 1px solid color-mix(in srgb, var(--accent) 20%, transparent);
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .active-top {
    display: flex;
    align-items: center;
    gap: 13px;
  }
  .active-icon {
    width: 46px;
    height: 46px;
    border-radius: 13px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    flex: none;
  }
  .active-icon .msr {
    font-size: 26px;
    color: var(--accent);
  }
  .active-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .active-name {
    font-size: 16px;
    font-weight: 700;
  }
  .rename-input {
    min-height: 34px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 0 10px;
    color: var(--text);
    font-size: 15px;
    font-weight: 600;
  }
  .rename-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .active-sub {
    font-size: 11px;
    color: var(--muted);
  }
  .active-tag {
    font-size: 10px;
    font-weight: 700;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
    padding: 4px 9px;
    border-radius: 7px;
    flex: none;
  }
  .active-tag.lost {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .active-stats {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .stat {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-soft);
    background: rgba(0, 0, 0, 0.25);
    padding: 6px 10px;
    border-radius: 9px;
  }
  .stat .msr {
    font-size: 15px;
    color: var(--muted);
  }
  .active-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .mini-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    height: 38px;
    padding: 0 13px;
    border: 1px solid var(--line);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .mini-btn:active {
    background: var(--surface);
  }
  .mini-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .mini-btn .msr {
    font-size: 16px;
  }
  .mini-btn.accent {
    color: var(--accent-ink);
    background: var(--accent);
    border-color: transparent;
  }
  .mini-btn.danger {
    color: var(--danger);
    border-color: rgba(251, 107, 95, 0.25);
    background: rgba(251, 107, 95, 0.08);
  }

  .other-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .other-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 13px 14px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .o-icon {
    width: 42px;
    height: 42px;
    border-radius: 12px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    flex: none;
  }
  .o-icon .msr {
    font-size: 22px;
    color: var(--text-soft);
  }
  .o-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .o-name {
    font-size: 14px;
    font-weight: 600;
  }
  .o-sub {
    font-size: 10px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .o-note {
    font-size: 10px;
    color: var(--muted);
  }
  .o-note.error {
    color: var(--danger);
  }
  .o-forget {
    background: transparent;
    border: none;
    color: var(--dim);
    cursor: pointer;
    padding: 2px;
    flex: none;
  }
  .o-forget .msr {
    font-size: 18px;
  }
  .o-connect {
    height: 38px;
    padding: 0 16px;
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
    border-radius: 11px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    flex: none;
    min-width: 84px;
    display: flex;
    align-items: center;
    justify-content: center;
  }
  .o-connect:disabled {
    opacity: 0.6;
    cursor: default;
  }

  .callout-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.45;
  }

  .quick-actions {
    display: flex;
    gap: 11px;
  }
  .quick-btn {
    flex: 1;
    min-height: 74px;
    border: 1px solid var(--line);
    border-radius: 16px;
    background: var(--surface);
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 6px;
    padding: 0 15px;
    cursor: pointer;
    color: var(--text);
    font-family: var(--sans);
  }
  .quick-btn:active {
    background: var(--surface-2);
  }
  .quick-btn .msr {
    font-size: 24px;
    color: var(--accent);
  }
  .quick-label {
    font-size: 13px;
    font-weight: 600;
  }
</style>
