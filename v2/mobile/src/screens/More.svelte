<script lang="ts">
  import { onMount } from "svelte";
  import { session } from "../lib/session.svelte";
  import { frontendLogText } from "../lib/log";
  import { api } from "../lib/api";
  import type { Screen } from "../lib/router.svelte";
  import type { RebootMode, RecoveryResult } from "../lib/types";
  import BottomTabs from "../components/BottomTabs.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import Toast from "../components/Toast.svelte";

  let { onDisconnect, navigate }: {
    onDisconnect: () => void;
    navigate: (screen: Screen) => void;
  } = $props();

  let licenseKey = $state("");
  let activating = $state(false);

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3500);
  }

  onMount(() => session.loadEntitlement());

  // Real Pro activation — calls the backend activate_license and updates the
  // shared entitlement. No more cosmetic `=== "pro"` string match.
  async function activate() {
    const key = licenseKey.trim();
    if (!key || activating) return;
    activating = true;
    try {
      const ent = await session.activatePro(key);
      if (ent === "pro") {
        showToast("Pro unlocked — thank you!", "success");
        licenseKey = "";
      } else {
        showToast("That key wasn't accepted. Check and try again.", "error");
      }
    } catch (e) {
      // Backend rejects invalid keys with an error string.
      showToast(String(e).replace(/^.*Error:\s*/, ""), "error");
    } finally {
      activating = false;
    }
  }

  // Detail screens reachable from here, grouped by what they touch. Pro tools
  // show a lock for free users; the screen itself still handles the LOCKED
  // paywall on any write.
  type Tool = { screen: Screen; icon: string; title: string; desc: string; pro: boolean };
  const toolGroups: { label: string; tools: Tool[] }[] = [
    {
      label: "Your TV",
      tools: [
        { screen: "devices", icon: "devices_other", title: "Devices", desc: "Switch, rename, or add a TV", pro: false },
        { screen: "files", icon: "sync_alt", title: "Files", desc: "Browse the TV and copy files to this phone", pro: false },
        { screen: "backups", icon: "cloud_sync", title: "Backups", desc: "Back up and restore an app's APK", pro: false },
      ],
    },
    {
      label: "Tuning",
      tools: [
        { screen: "launcher", icon: "home", title: "Launcher", desc: "Set a custom home screen", pro: true },
        { screen: "tweaks", icon: "tune", title: "Tweaks", desc: "CEC, frame rate, DNS, animations", pro: true },
        { screen: "snapshots", icon: "photo_camera_back", title: "Snapshots", desc: "Record a setup and re-apply it later", pro: true },
      ],
    },
    {
      label: "Help",
      tools: [
        { screen: "riskguide", icon: "help", title: "Safety tiers & actions", desc: "What each tier and action means", pro: false },
      ],
    },
  ];

  // Emergency recovery: `pm enable` every disabled package. This is the undo
  // for a debloat that went too far; it never uninstalls or installs anything.
  let recoveryConfirm = $state(false);
  let recovering = $state(false);
  let recovery = $state<RecoveryResult | null>(null);

  async function runRecovery() {
    recoveryConfirm = false;
    if (recovering || !session.serial) return;
    recovering = true;
    recovery = null;
    try {
      recovery = await api.panicRecovery(session.serial);
      session.invalidateAll();
      showToast(recovery.message || `Re-enabled ${recovery.restored.length} apps.`, recovery.failed.length ? "info" : "success");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      recovering = false;
    }
  }

  // Advanced reboot targets. Recovery and bootloader are for troubleshooting;
  // both drop the ADB connection.
  let rebootTarget = $state<RebootMode | null>(null);
  let rebooting = $state(false);
  const rebootLabels: Record<RebootMode, string> = {
    normal: "Restart",
    recovery: "Recovery mode",
    bootloader: "Bootloader",
  };

  async function runReboot() {
    const mode = rebootTarget;
    rebootTarget = null;
    if (!mode || rebooting || !session.serial) return;
    rebooting = true;
    try {
      const r = await api.rebootDevice(session.serial, mode);
      showToast(r.ok ? `${rebootLabels[mode]} command sent.` : r.message || "Reboot failed.", r.ok ? "success" : "error");
      if (r.ok) setTimeout(() => onDisconnect(), 1500);
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      rebooting = false;
    }
  }

  let disconnectConfirm = $state(false);

  // Debug log — the native tail plus the frontend call ring buffer, both
  // copyable for support.
  let showLog = $state(false);
  let nativeLog = $state("");
  let logLoading = $state(false);
  let logError = $state("");

  async function loadLog() {
    showLog = true;
    logLoading = true;
    logError = "";
    try {
      nativeLog = await api.readDebugLog();
    } catch (e) {
      logError = String(e);
    } finally {
      logLoading = false;
    }
  }

  const combinedLog = $derived(
    `# Frontend calls\n${frontendLogText()}\n\n# Native log\n${nativeLog}`,
  );

  async function copyLog() {
    try {
      await navigator.clipboard.writeText(combinedLog);
      showToast("Debug log copied.", "success");
    } catch {
      showToast("Couldn't copy to clipboard.", "error");
    }
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={() => navigate("dashboard")} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Settings</h3>
  </div>

  <div class="more-content">
    <!-- Licensing -->
    <div class="more-card">
      <span class="card-label">Licensing</span>
      <div class="license-status-row">
        <span class="license-title">Current plan:</span>
        <span class="license-badge" class:pro={session.isPro}>{session.isPro ? "Pro" : "Free"}</span>
      </div>

      {#if !session.isPro}
        <p class="license-desc">Activate Pro to unlock Curated Debloat, Snapshots, and Launcher takeover.</p>
        <div class="license-input-row">
          <input
            type="text"
            bind:value={licenseKey}
            placeholder="Enter license key…"
            class="license-input mono"
            autocapitalize="none"
            autocorrect="off"
            spellcheck="false"
            onkeydown={(e) => e.key === "Enter" && activate()}
          />
          <button class="primary small-btn" disabled={!licenseKey || activating} onclick={activate}>
            {activating ? "…" : "Activate"}
          </button>
        </div>
      {:else}
        <p class="license-desc success-color">Thank you for supporting ATV Optimizer Pro!</p>
      {/if}
    </div>

    {#each toolGroups as group (group.label)}
      <div class="more-card">
        <span class="card-label">{group.label}</span>
        {#each group.tools as t (t.screen)}
          <button class="setting-row" onclick={() => navigate(t.screen)}>
            <span class="msr tool-icon">{t.icon}</span>
            <div class="setting-info">
              <span class="setting-title">{t.title}</span>
              <span class="setting-desc">{t.desc}</span>
            </div>
            {#if t.pro && !session.isPro}
              <span class="msr lock-icon">lock</span>
            {:else}
              <span class="msr chevron">chevron_right</span>
            {/if}
          </button>
        {/each}
      </div>
    {/each}

    <!-- Recovery -->
    <div class="more-card">
      <span class="card-label">Recovery</span>
      <p class="card-desc">
        Re-enables every app that is currently disabled on the TV, including ones this app
        didn't touch. Use it if something stopped working after a debloat. It never uninstalls
        or installs anything.
      </p>
      <button class="ghost-btn" disabled={recovering || !session.connectedDevice} onclick={() => (recoveryConfirm = true)}>
        {#if recovering}
          <span class="pdot blink"></span>Re-enabling apps…
        {:else}
          <span class="msr">medical_services</span>Emergency recovery
        {/if}
      </button>
      {#if recovery}
        <div class="recovery-result">
          <span class="mono">{recovery.restored.length} re-enabled · {recovery.failed.length} failed</span>
          {#if recovery.failed.length}
            <ul class="recovery-failed mono">
              {#each recovery.failed as f (f.package)}
                <li>{f.package}: {f.error}</li>
              {/each}
            </ul>
          {/if}
        </div>
      {/if}
    </div>

    <!-- Reboot -->
    <div class="more-card">
      <span class="card-label">Reboot</span>
      <p class="card-desc">Every option restarts the TV and drops this connection.</p>
      <div class="reboot-row">
        {#each ["normal", "recovery", "bootloader"] as mode (mode)}
          <button class="ghost-btn compact" disabled={rebooting || !session.connectedDevice} onclick={() => (rebootTarget = mode as RebootMode)}>
            <span class="msr">{mode === "normal" ? "restart_alt" : mode === "recovery" ? "build" : "developer_board"}</span>{rebootLabels[mode as RebootMode]}
          </button>
        {/each}
      </div>
    </div>

    <!-- Debug log -->
    <div class="more-card">
      <span class="card-label">Debug log</span>
      <p class="card-desc">Recent app activity and the device transport log — useful for support.</p>
      {#if !showLog}
        <button class="ghost-btn" onclick={loadLog}>
          <span class="msr">description</span>Show debug log
        </button>
      {:else}
        <div class="log-toolbar">
          <button class="ghost-btn compact" onclick={loadLog} disabled={logLoading}>
            <span class="msr" class:blink={logLoading}>refresh</span>Refresh
          </button>
          <button class="ghost-btn compact" onclick={copyLog}>
            <span class="msr">content_copy</span>Copy
          </button>
        </div>
        {#if logError}
          <p class="log-error mono">{logError}</p>
        {/if}
        <pre class="log-view mono">{combinedLog}</pre>
      {/if}
    </div>

    <!-- Disconnect -->
    <button class="ghost danger-btn" onclick={() => (disconnectConfirm = true)}>
      <span class="msr">power_settings_new</span>Disconnect from TV
    </button>
  </div>

  <ConfirmDialog
    open={recoveryConfirm}
    icon="medical_services"
    title="Re-enable every disabled app?"
    message="This turns back on all apps currently disabled on {session.deviceLabel}, including bloat you disabled on purpose. You can run Optimize again afterwards."
    confirmLabel="Re-enable all"
    onConfirm={runRecovery}
    onCancel={() => (recoveryConfirm = false)}
  />

  <ConfirmDialog
    open={rebootTarget !== null}
    icon="restart_alt"
    danger
    title={rebootTarget ? `${rebootLabels[rebootTarget]}?` : ""}
    message={rebootTarget === "normal"
      ? "The TV restarts normally. Reconnect once it's back."
      : rebootTarget === "recovery"
        ? "The TV boots into its recovery menu. You'll need the TV remote to leave it."
        : "The TV boots into the bootloader. Only do this if you know how to get back out."}
    confirmLabel="Reboot"
    onConfirm={runReboot}
    onCancel={() => (rebootTarget = null)}
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

  <div class="spacer"></div>
  <BottomTabs active="more" {navigate} />
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

  .more-content {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .more-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .card-label {
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
  }
  .card-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    margin: 0;
  }

  .recovery-result {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
  }
  .recovery-failed {
    margin: 0;
    padding-left: 16px;
    font-size: 11px;
    color: var(--danger);
    word-break: break-all;
  }
  .reboot-row {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .license-status-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  .license-title {
    font-size: 14px;
    font-weight: 600;
  }
  .license-badge {
    font-size: 11px;
    font-weight: 700;
    color: var(--muted);
    background: rgba(255, 255, 255, 0.06);
    padding: 4px 9px;
    border-radius: 7px;
    text-transform: uppercase;
  }
  .license-badge.pro {
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
  }
  .license-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
    margin: 0;
  }
  .success-color {
    color: var(--teal);
  }
  .license-input-row {
    display: flex;
    gap: 8px;
  }
  .license-input {
    flex: 1;
    min-height: 40px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 9px;
    padding: 0 10px;
    color: var(--text);
    font-size: 13px;
  }
  .license-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .small-btn {
    min-height: 40px;
    padding: 0 16px;
    width: auto;
    font-size: 13px;
    border-radius: 9px;
  }

  .ghost-btn {
    flex: 1;
    min-height: 44px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .ghost-btn:active {
    background: var(--surface);
  }
  .ghost-btn:disabled {
    opacity: 0.6;
  }
  .ghost-btn .msr {
    font-size: 16px;
  }
  .ghost-btn.compact {
    flex: none;
    padding: 0 14px;
  }

  .setting-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    width: 100%;
    background: transparent;
    border: none;
    padding: 4px 0;
    cursor: pointer;
    text-align: left;
    color: var(--text);
    font-family: var(--sans);
  }
  .setting-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .setting-title {
    font-size: 14px;
    font-weight: 600;
  }
  .setting-desc {
    font-size: 12px;
    color: var(--muted);
  }
  .tool-icon {
    font-size: 22px;
    color: var(--text-soft);
    flex: none;
  }
  .lock-icon {
    font-size: 18px;
    color: var(--dim);
    flex: none;
  }
  .chevron {
    font-size: 20px;
    color: var(--dim);
    flex: none;
  }

  .log-toolbar {
    display: flex;
    gap: 8px;
  }
  .log-view {
    max-height: 260px;
    overflow: auto;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px;
    font-size: 10px;
    line-height: 1.5;
    color: var(--text-soft);
    white-space: pre-wrap;
    word-break: break-word;
    margin: 0;
  }
  .log-error {
    font-size: 11px;
    color: var(--danger);
    margin: 0;
  }

  .danger-btn {
    min-height: 50px;
    gap: 8px;
    border-color: rgba(251, 107, 95, 0.25);
    background: rgba(251, 107, 95, 0.08);
    color: var(--danger);
  }
  .danger-btn:active {
    background: rgba(251, 107, 95, 0.05);
  }
  .danger-btn .msr {
    font-size: 20px;
  }
</style>
