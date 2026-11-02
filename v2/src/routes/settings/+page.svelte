<script lang="ts">
  import { onMount } from "svelte";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import Icon from "$lib/components/Icon.svelte";
  import ErrorNotice from "$lib/components/ErrorNotice.svelte";
  import ThemeToggle from "$lib/components/ThemeToggle.svelte";
  import { api } from "$lib/api";
  import { useAppShell } from "$lib/app-shell";
  import { setBreadcrumbsEnabled } from "$lib/breadcrumbs";
  import { loadReports, reports, setReportsEnabled } from "$lib/reports.svelte";
  import { chooseAutoUpdate, loadUiPrefs, uiPrefs } from "$lib/ui-prefs.svelte";
  import {
    QUIT_ADB_TIP,
    refreshStopOnQuit,
    stopOnQuit,
    toggleStopOnQuit,
  } from "$lib/stop-adb-on-quit.svelte";
  import type { AdbStatus } from "$lib/types";
  import {
    formatSupportId,
    NOTICE_TEXT,
    TOGGLE_LABEL,
    type DiagnosticsUploadResult,
  } from "../../../shared/telemetry";

  const shell = useAppShell();
  const RELEASES_PAGE = "https://github.com/bryanroscoe/shield_optimizer/releases";

  const telemetry = $derived(reports.settings);
  const supportId = $derived(formatSupportId(telemetry?.support_id));
  let reportsError = $state<string | null>(null);

  async function toggleReports(e: Event) {
    const box = e.currentTarget as HTMLInputElement;
    reportsError = null;
    try {
      await setReportsEnabled(box.checked);
    } catch (err) {
      reportsError = String(err);
    }
    box.checked = reports.settings?.enabled ?? false;
  }

  /// Which ID the Copy button last copied, so only that one says "Copied".
  let copied = $state("");
  let copyError = $state("");
  async function copyId(id: string) {
    copyError = "";
    try {
      await navigator.clipboard.writeText(id);
      copied = id;
      setTimeout(() => {
        if (copied === id) copied = "";
      }, 2000);
    } catch (e) {
      copyError = `Couldn't reach the clipboard (${e}). Your support ID is ${id}.`;
    }
  }

  let sendStage = $state<"idle" | "confirm" | "sending">("idle");
  let sent = $state<(DiagnosticsUploadResult & { shortId: string }) | null>(null);
  let sendError = $state<unknown>(null);

  function askToSend() {
    sendError = null;
    sendStage = "confirm";
  }

  async function sendLogs() {
    if (sendStage === "sending") return;
    sendStage = "sending";
    sendError = null;
    sent = null;
    try {
      const result = await api.uploadDiagnostics();
      sent = { ...result, shortId: formatSupportId(result.support_id) };
      void loadReports();
    } catch (e) {
      sendError = e;
    } finally {
      sendStage = "idle";
    }
  }

  function kilobytes(bytes: number): string {
    return bytes < 1024 ? `${bytes} bytes` : `${(bytes / 1024).toFixed(1)} KB`;
  }

  let adb = $state<AdbStatus | null>(null);
  const nativeAdb = $derived(adb?.backend === "direct");
  let adbError = $state<string | null>(null);
  let installBusy = $state(false);
  let installMessage = $state("");
  let restartBusy = $state(false);
  let restartMessage = $state("");

  async function loadAdb() {
    adbError = null;
    try {
      adb = await api.adbStatus();
    } catch (e) {
      adb = null;
      adbError = String(e);
    }
  }

  async function downloadAdb() {
    if (nativeAdb) return;
    installBusy = true;
    installMessage = "Downloading platform-tools from Google… (~12 MB)";
    try {
      const r = await api.installAdb();
      installMessage = r.message;
      if (r.ok) await loadAdb();
    } catch (e) {
      installMessage = String(e);
    } finally {
      installBusy = false;
    }
  }

  async function restartAdb() {
    if (nativeAdb) return;
    if (
      !confirm(
        "Restart the shared ADB server? This disconnects other tools using ADB. Current network connections will be retried.",
      )
    )
      return;
    restartBusy = true;
    restartMessage = "";
    try {
      const r = await api.restartAdb();
      restartMessage = r.message;
      await loadAdb();
    } catch (e) {
      restartMessage = String(e);
    } finally {
      restartBusy = false;
    }
  }

  function clickStopOnQuit(e: MouseEvent) {
    // As on Devices: the box moves only when the reply says the setting did.
    (e.currentTarget as HTMLInputElement).checked = stopOnQuit.setting?.enabled ?? false;
    void toggleStopOnQuit();
  }

  let debugLogging = $state(false);
  let logPath = $state("");
  let logsError = $state("");

  async function loadLogs() {
    const [debug, dir] = await Promise.allSettled([api.getDebugLogging(), api.logDirPath()]);
    if (debug.status === "fulfilled") debugLogging = debug.value;
    if (dir.status === "fulfilled") logPath = dir.value;
  }

  async function toggleDebugLogging(e: Event) {
    const box = e.currentTarget as HTMLInputElement;
    logsError = "";
    try {
      // Believe the backend, not the click.
      debugLogging = await api.setDebugLogging(box.checked);
    } catch (err) {
      logsError = String(err);
      debugLogging = await api.getDebugLogging().catch(() => debugLogging);
    }
    box.checked = debugLogging;
    setBreadcrumbsEnabled(debugLogging);
  }

  async function openLogsFolder() {
    logsError = "";
    try {
      await api.openLogDir();
    } catch (e) {
      logsError = String(e);
    }
  }

  onMount(() => {
    loadUiPrefs();
    void loadReports();
    void loadAdb();
    void loadLogs();
    void refreshStopOnQuit();
  });
</script>

<section class="header-row">
  <div class="header-title">
    <h1>Settings</h1>
    <p class="muted small mono header-sub">this computer · applies to every TV</p>
  </div>
</section>

<div class="settings">
  <section class="card" id="privacy" aria-labelledby="privacy-title">
    <h2 id="privacy-title"><Icon name="shield" size={18} /> Privacy &amp; diagnostics</h2>

    <div class="row">
      <div class="row-text">
        <label class="row-label" for="reports-toggle">{TOGGLE_LABEL}</label>
        <p class="row-help" id="reports-help">
          {#if telemetry && !telemetry.available}
            This build doesn't send reports.
          {:else if !telemetry && !reports.read}
            Checking…
          {:else if !telemetry}
            Couldn't read this setting.
            <button class="inline-link" onclick={loadReports}>Check again</button>
          {:else}
            {NOTICE_TEXT} Turning this off stops sending at once and discards anything not yet sent.
          {/if}
        </p>
      </div>
      <div class="row-control">
        <input
          id="reports-toggle"
          type="checkbox"
          aria-describedby="reports-help"
          checked={telemetry?.enabled ?? false}
          disabled={!telemetry?.available}
          onchange={toggleReports}
        />
      </div>
    </div>
    {#if reportsError}
      <ErrorNotice error={reportsError} action="Change report setting" />
    {/if}

    <div class="row">
      <div class="row-text">
        <span class="row-label" id="support-id-label">Support ID</span>
        <p class="row-help">
          Quote this ID when you report a problem so the developer can find your reports.
        </p>
      </div>
      <div class="row-control" aria-labelledby="support-id-label" role="group">
        {#if supportId}
          <code class="support-id" data-support-id>{supportId}</code>
          <button onclick={() => copyId(supportId)}>
            <Icon name="content_copy" size={14} />
            {copied === supportId ? "Copied" : "Copy"}
          </button>
        {:else}
          <span class="support-none">
            {#if !telemetry}
              {reports.read ? "Unknown" : "Checking…"}
            {:else if !telemetry.available}
              None in this build
            {:else if !telemetry.enabled}
              None while reports are off
            {:else}
              Not available yet
            {/if}
          </span>
        {/if}
      </div>
    </div>

    <div class="row">
      <div class="row-text">
        <span class="row-label">Send logs to developer</span>
        <p class="row-help">
          For a problem you're reporting: sends this app's logs once, now, so the developer can
          look into it. Works with reports on or off.
        </p>
      </div>
      <div class="row-control">
        <button onclick={askToSend} disabled={sendStage !== "idle"}>
          <Icon name="upload" size={14} /> Send logs…
        </button>
      </div>
    </div>
    {#if sendStage !== "idle"}
      <div class="send-confirm" role="group" aria-labelledby="send-title">
        <p id="send-title" class="send-title">Send logs to the developer?</p>
        <p class="row-help">
          This uploads a diagnostics bundle and this app's recent logs, with addresses, serial
          numbers, names and file paths removed, labeled with your support ID. Nothing is sent
          until you press Send logs.
        </p>
        <div class="send-actions">
          <button onclick={() => (sendStage = "idle")} disabled={sendStage === "sending"}>
            Cancel
          </button>
          <button class="primary" onclick={sendLogs} disabled={sendStage === "sending"}>
            {sendStage === "sending" ? "Sending…" : "Send logs"}
          </button>
        </div>
      </div>
    {/if}
    {#if sent}
      <div class="callout callout-ok sent" role="status">
        <Icon name="check_circle" size={16} />
        <span>
          Sent {kilobytes(sent.bytes)}.
          {#if sent.shortId}
            Quote support ID <code class="support-id small-id">{sent.shortId}</code> when you report
            the problem.
          {/if}
        </span>
        {#if sent.shortId}
          <button class="callout-link" onclick={() => copyId(sent!.shortId)}>
            {copied === sent.shortId ? "Copied" : "Copy ID"}
          </button>
        {/if}
      </div>
    {/if}
    {#if sendError}
      <ErrorNotice error={sendError} action="Send logs" />
    {/if}
    {#if copyError}
      <p class="row-note">{copyError}</p>
    {/if}
  </section>

  <section class="card" id="appearance" aria-labelledby="appearance-title">
    <h2 id="appearance-title"><Icon name="tune" size={18} /> Appearance</h2>
    <div class="row">
      <div class="row-text">
        <span class="row-label">Theme</span>
        <p class="row-help">Auto follows your computer's light or dark setting.</p>
      </div>
      <div class="row-control">
        <ThemeToggle />
      </div>
    </div>
  </section>

  <section class="card" id="updates" aria-labelledby="updates-title">
    <h2 id="updates-title"><Icon name="download" size={18} /> Updates</h2>
    <div class="row">
      <div class="row-text">
        <label class="row-label" for="auto-update">Auto-update</label>
        <p class="row-help" id="auto-update-help">
          Automatically download and install updates on launch. Off, the header offers each update
          with its notes first.
        </p>
      </div>
      <div class="row-control">
        <input
          id="auto-update"
          type="checkbox"
          aria-describedby="auto-update-help"
          checked={uiPrefs.autoUpdate}
          onchange={(e) => chooseAutoUpdate((e.currentTarget as HTMLInputElement).checked)}
        />
      </div>
    </div>
    <div class="row">
      <div class="row-text">
        <span class="row-label">
          Version <span class="mono">{shell.currentVersion ? `v${shell.currentVersion}` : "unknown"}</span>
        </span>
        <p class="row-help" role="status">
          {#if shell.updateInstalled}
            {shell.restartFailed
              ? `Quit and reopen the app to finish updating to v${shell.pendingVersion}.`
              : `v${shell.pendingVersion} is installed. Restart to start using it.`}
          {:else if shell.updateBusy}
            {shell.updateProgress}
          {:else if shell.pendingVersion}
            {shell.updateProgress ? `${shell.updateProgress} ` : ""}v{shell.pendingVersion} is available.
          {:else if shell.rollingOutVersion}
            v{shell.rollingOutVersion} is rolling out. The in-app updater will offer it within a few
            minutes.
          {:else if shell.checkMessage}
            {shell.checkMessage}
          {:else}
            The app checks for updates each time it opens.
          {/if}
        </p>
      </div>
      <div class="row-control">
        {#if shell.updateInstalled && !shell.restartFailed}
          <button class="primary" onclick={() => shell.restartApp()}>
            <Icon name="restart_alt" size={14} /> Restart now
          </button>
        {:else if shell.pendingVersion && !shell.updateBusy && !shell.updateInstalled}
          <button class="primary" onclick={() => shell.openNotes()}>
            Update to v{shell.pendingVersion}…
          </button>
        {:else}
          <button
            onclick={() => shell.checkForUpdates()}
            disabled={shell.checking || shell.updateBusy || shell.updateInstalled}
          >
            <Icon name="refresh" size={14} /> {shell.checking ? "Checking…" : "Check for updates"}
          </button>
        {/if}
        <button onclick={() => shell.openHistory()}>
          <Icon name="history" size={14} /> What's new
        </button>
      </div>
    </div>
  </section>

  <section class="card" id="adb" aria-labelledby="adb-title">
    <h2 id="adb-title"><Icon name="terminal" size={18} /> ADB</h2>
    <div class="row">
      <div class="row-text">
        <span class="row-label">adb</span>
        <p class="row-help">
          {#if adbError}
            Couldn't check adb: {adbError}
          {:else if !adb}
            Checking…
          {:else if nativeAdb}
            Native Rust ADB · direct connection{adb.available ? "" : " unavailable"}
            {#if !adb.available && adb.last_probe}<span>{adb.last_probe}</span>{/if}
          {:else if adb.available}
            Using <span class="mono path">{adb.path ?? "adb from PATH"}</span>
          {:else if adb.failure !== "missing_binary"}
            ADB could not respond: {adb.last_probe ?? "The availability check failed."}
          {:else}
            Not found{adb.path ? ` at ${adb.path}` : ""}. The app can download Google's official
            platform-tools into its own folder, with no system-wide changes.
          {/if}
        </p>
        {#if !nativeAdb}
          <p class="row-help">
            To use a different adb, set <code>SHIELD_OPTIMIZER_ADB</code> to its full path and relaunch.
          </p>
        {/if}
        {#if installMessage}
          <p class="row-note">{installMessage}</p>
        {/if}
      </div>
      <div class="row-control">
        {#if adb && !nativeAdb && adb.failure === "missing_binary"}
          <button class="primary" onclick={downloadAdb} disabled={installBusy}>
            <Icon name="download" size={14} />
            {installBusy ? "Installing…" : "Download platform-tools"}
          </button>
        {/if}
      </div>
    </div>
    <div class="row">
      <div class="row-text">
        <span class="row-label">Restart ADB</span>
        <p class="row-help">
          {#if nativeAdb}
            Native ADB has no shared server. Use the TV's Forget or Try again action on Devices.
          {:else}
            Stops and starts the shared adb server, then retries network connections. Other tools
            using adb are disconnected too.
          {/if}
        </p>
        {#if restartMessage}
          <p class="row-note">{restartMessage}</p>
        {/if}
      </div>
      <div class="row-control">
        <button onclick={restartAdb} disabled={nativeAdb || restartBusy || !adb || adb.failure === "missing_binary"}>
          <Icon name="restart_alt" size={14} />
          {restartBusy ? "Restarting…" : "Restart ADB"}
        </button>
      </div>
    </div>
    <div class="row">
      <div class="row-text">
        <label class="row-label" for="stop-adb-on-quit">
          Stop ADB when quitting
          {#if stopOnQuit.setting === null && stopOnQuit.pending > 0}
            <span class="checking">checking…</span>
          {/if}
        </label>
        <p class="row-help" id="stop-adb-help">{nativeAdb ? "Native connections close when the app quits. This preference applies when using platform-tools." : QUIT_ADB_TIP}</p>
        {#if stopOnQuit.error}
          <p class="row-note quit-error">
            {stopOnQuit.error}
            {#if stopOnQuit.setting === null && stopOnQuit.pending === 0}
              <button class="inline-link" onclick={refreshStopOnQuit}>Check again</button>
            {/if}
          </p>
        {/if}
      </div>
      <div class="row-control">
        <input
          id="stop-adb-on-quit"
          type="checkbox"
          aria-describedby="stop-adb-help"
          checked={stopOnQuit.setting?.enabled ?? false}
          indeterminate={stopOnQuit.setting === null}
          disabled={nativeAdb || stopOnQuit.setting === null}
          onclick={clickStopOnQuit}
        />
      </div>
    </div>
  </section>

  <section class="card" id="logs" aria-labelledby="logs-title">
    <h2 id="logs-title"><Icon name="description" size={18} /> Logs</h2>
    <div class="row">
      <div class="row-text">
        <label class="row-label" for="debug-logging">Debug logging</label>
        <p class="row-help" id="debug-logging-help">
          While it's on, this session is also recorded (adb calls and the buttons you press) to
          <span class="mono">logs/sessions</span>. Turn it on before reproducing a problem.
        </p>
      </div>
      <div class="row-control">
        <input
          id="debug-logging"
          type="checkbox"
          aria-describedby="debug-logging-help"
          checked={debugLogging}
          onchange={toggleDebugLogging}
        />
      </div>
    </div>
    <div class="row">
      <div class="row-text">
        <span class="row-label">Logs folder</span>
        <p class="row-help mono path">{logPath || "(no log folder)"}</p>
      </div>
      <div class="row-control">
        <button onclick={openLogsFolder}>
          <Icon name="folder_open" size={14} /> Open logs folder
        </button>
      </div>
    </div>
    {#if logsError}
      <p class="row-note">{logsError}</p>
    {/if}
  </section>

  <section class="card" id="about" aria-labelledby="about-title">
    <h2 id="about-title"><Icon name="info" size={18} /> About</h2>
    <div class="row">
      <div class="row-text">
        <span class="row-label">
          ATV Optimizer <span class="mono">{shell.currentVersion ? `v${shell.currentVersion}` : ""}</span>
        </span>
        <p class="row-help">Release notes for every version are on GitHub.</p>
      </div>
      <div class="row-control">
        <button onclick={() => openUrl(RELEASES_PAGE)}>
          All releases <Icon name="open_in_new" size={14} />
        </button>
      </div>
    </div>
    <div class="row">
      <div class="row-text">
        <span class="row-label">Report a problem</span>
        <p class="row-help">
          Opens the bug report with this computer's diagnostics for you to review before anything
          is shared.
        </p>
      </div>
      <div class="row-control">
        <button onclick={() => shell.openBugReport()}>
          <Icon name="bug_report" size={14} /> Open bug report…
        </button>
      </div>
    </div>
  </section>
</div>

<style>
  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 1rem;
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
  .small {
    font-size: 0.82rem;
  }
  .settings {
    display: flex;
    flex-direction: column;
    gap: 1rem;
    max-width: 52rem;
  }
  .card > h2:first-child {
    margin-bottom: 0.2rem;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 0.6rem 1.5rem;
    padding: 0.85rem 0;
    border-top: 1px solid var(--border);
  }
  /* The title already draws a rule under itself. */
  h2 + .row {
    border-top: none;
    padding-top: 0.4rem;
  }
  .row:last-child {
    padding-bottom: 0;
  }
  .row-text {
    flex: 1 1 22rem;
    min-width: 0;
  }
  .row-label {
    display: block;
    font-weight: 550;
    font-size: 0.92rem;
  }
  label.row-label {
    cursor: pointer;
  }
  .row-help {
    margin: 0.25rem 0 0;
    font-size: 0.8rem;
    line-height: 1.5;
    color: var(--fg-muted);
    max-width: 62ch;
  }
  .row-note {
    margin: 0.4rem 0 0;
    font-size: 0.8rem;
    color: var(--fg-secondary);
  }
  .row-control {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: flex-end;
    gap: 0.5rem;
  }
  .row-control input[type="checkbox"] {
    width: 1.1rem;
    height: 1.1rem;
    accent-color: var(--accent-strong);
    cursor: pointer;
  }
  .row-control input[type="checkbox"]:disabled {
    cursor: not-allowed;
  }
  .path {
    word-break: break-all;
  }
  .support-id {
    font-family: var(--mono);
    font-size: 1rem;
    font-weight: 600;
    letter-spacing: 0.08em;
    padding: 0.2rem 0.55rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-sm);
    background: var(--bg-inset);
    user-select: all;
  }
  .support-id.small-id {
    font-size: 0.82rem;
    padding: 0.05rem 0.35rem;
  }
  .support-none {
    font-size: 0.85rem;
    color: var(--fg-muted);
  }
  .send-confirm {
    margin: 0 0 0.6rem;
    padding: 0.8rem 0.9rem;
    border: 1px solid var(--border-strong);
    border-radius: var(--radius-md);
    background: var(--bg-surface-2);
  }
  .send-title {
    margin: 0;
    font-weight: 600;
    font-size: 0.9rem;
  }
  .send-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.5rem;
    margin-top: 0.7rem;
  }
  .sent {
    align-items: center;
    margin-bottom: 0.6rem;
  }
  .checking {
    margin-left: 0.4rem;
    font-weight: 400;
    font-size: 0.78rem;
    color: var(--fg-muted);
  }
  .inline-link {
    padding: 0;
    border: none;
    background: none;
    color: var(--accent);
    font: inherit;
    cursor: pointer;
  }
  .inline-link:hover {
    text-decoration: underline;
  }
</style>
