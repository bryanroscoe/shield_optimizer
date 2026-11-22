<script module lang="ts">
  // Off applies at once even when saving it fails, but then reports come back
  // on at the next launch. Say so and offer a retry rather than a plain "Off".
  // Module state, so the warning survives leaving the panel and coming back.
  let telemetryOffUnsaved = $state(false);
</script>

<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { frontendLogText, publicConnectionHistory } from "../lib/log";
  import { MOBILE_NOTICE_TEXT, TOGGLE_LABEL, type TelemetrySettings } from "../../../shared/telemetry";
  import HelpSheet from "./HelpSheet.svelte";
  import Toast from "./Toast.svelte";

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3500);
  }

  onMount(() => { void loadTelemetry(); });
  onDestroy(() => clearTimeout(toastTimer));

  // Anonymous reports. null until the backend answers; a failed read leaves
  // the switch hidden rather than showing a state we don't know.
  let telemetry = $state<TelemetrySettings | null>(null);
  let telemetryBusy = $state(false);
  const PRIVACY_POLICY_URL = "https://atv-optimizer-telemetry.bryanroscoe.workers.dev/privacy";

  async function openPrivacyPolicy() {
    try {
      await api.openUrl(PRIVACY_POLICY_URL);
    } catch {
      showToast("Couldn't open the privacy policy. Please try again.", "error");
    }
  }

  async function copySupportId() {
    const id = telemetry?.support_id;
    if (!id) return;
    try {
      await navigator.clipboard.writeText(id);
      showToast("Support ID copied.", "success");
    } catch {
      showToast("Couldn't copy. You can select the Support ID below.", "error");
    }
  }

  async function loadTelemetry() {
    try {
      telemetry = await api.telemetrySettings();
    } catch {
      telemetry = null;
    }
  }

  async function setTelemetry(enabled: boolean) {
    telemetryBusy = true;
    try {
      telemetry = await api.setTelemetryEnabled(enabled);
      telemetryOffUnsaved = false;
    } catch {
      await loadTelemetry();
      telemetryOffUnsaved = !enabled && telemetry?.enabled === false;
      if (!telemetryOffUnsaved) showToast("Couldn't change the report setting.", "error");
    } finally {
      telemetryBusy = false;
    }
  }

  function toggleTelemetry(event: Event) {
    void setTelemetry((event.currentTarget as HTMLInputElement).checked);
  }

  // Debug log — the native tail plus the frontend call ring buffer, both
  // copyable for support.
  let showLog = $state(false);
  let nativeLog = $state("");
  // The frontend ring buffer isn't reactive, so it's read on Show, Refresh and
  // Copy rather than once.
  let frontendLog = $state("");
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
      frontendLog = frontendLogText();
      logLoading = false;
    }
  }

  const combinedLog = $derived(
    `# Frontend calls\n${frontendLog}\n\n# Native log\n${nativeLog}`,
  );

  async function copyLog() {
    frontendLog = frontendLogText();
    try {
      await navigator.clipboard.writeText(combinedLog);
      showToast("Debug log copied.", "success");
    } catch {
      showToast("Couldn't copy to clipboard.", "error");
    }
  }

  // Bug report — the same narrow bundle desktop builds, reviewed on screen
  // before anything reaches the clipboard. Nothing is sent.
  let bugReport = $state("");
  let reportIsPrivate = $state(false);
  let bugBusy = $state(false);
  let bugError = $state("");
  let bugRequest = 0;

  async function prepareBugReport(privateReport: boolean) {
    if (bugBusy) return;
    const request = ++bugRequest;
    bugBusy = true;
    bugError = "";
    bugReport = "";
    reportIsPrivate = privateReport;
    const publicHistory = privateReport ? "" : publicConnectionHistory();
    try {
      const text = await api.collectDiagnostics(session.isConnected ? session.serial : null, !privateReport);
      if (request === bugRequest) {
        bugReport = text + (publicHistory ? `\n### Recent connection actions\n\n${publicHistory}\n` : "");
      }
    } catch (e) {
      if (request === bugRequest) bugError = String(e);
    } finally {
      if (request === bugRequest) bugBusy = false;
    }
  }

  // A report describes the TV it was built for; a switch, reconnect or lost
  // link drops it (and any read still in flight) rather than offering a
  // stale one for copying.
  let bugConnection = `${session.serial}:${session.generation}:${session.isConnected}`;
  $effect(() => {
    const key = `${session.serial}:${session.generation}:${session.isConnected}`;
    if (key === bugConnection) return;
    bugConnection = key;
    ++bugRequest;
    bugReport = "";
    bugError = "";
    bugBusy = false;
  });

  async function copyBugReport() {
    try {
      await navigator.clipboard.writeText(bugReport);
      showToast("Bug report copied.", "success");
    } catch {
      showToast("Couldn't copy to clipboard. Select the text and copy it instead.", "error");
    }
  }
</script>

<div class="support-reports">
  <!-- Anonymous reports -->
  <div class="more-card" data-testid="telemetry-card">
    <span class="card-label">Privacy</span>
    <button class="ghost-btn" onclick={openPrivacyPolicy}>Privacy policy<span class="msr">open_in_new</span></button>
    {#if telemetry}
      <label class="telemetry-toggle">
        <span class="tt-copy">
          <span class="tt-title">{TOGGLE_LABEL}</span>
          <span class="card-desc">
            {#if !telemetry.available}
              This build doesn't send reports.
            {:else if telemetry.enabled}
              On. Turning it off stops sending immediately and discards anything not yet sent.
            {:else}
              Off. Nothing is sent.
            {/if}
          </span>
        </span>
        <input
          type="checkbox"
          role="switch"
          checked={telemetry.available && telemetry.enabled}
          disabled={!telemetry.available || telemetryBusy}
          onchange={toggleTelemetry}
          aria-label={TOGGLE_LABEL}
        />
      </label>
      {#if telemetryOffUnsaved && telemetry.enabled === false}
        <div class="telemetry-unsaved" role="alert">
          <p class="card-desc">
            Reports are off for now, but this choice couldn't be saved. They may turn back on
            the next time the app starts.
          </p>
          <button class="ghost-btn compact" disabled={telemetryBusy} onclick={() => setTelemetry(false)}>
            <span class="msr">refresh</span>Try saving again
          </button>
        </div>
      {/if}
      <HelpSheet title="What's sent">
        <p class="card-desc">{MOBILE_NOTICE_TEXT}</p>
        <p class="card-desc">
          Reports go to a server the app's developer runs; raw reports are kept for 30 days.
          Turning reports back on starts a new random install ID.
        </p>
      </HelpSheet>
      {#if telemetry.support_id}
        <p class="card-desc">Support ID: <code class="support-id">{telemetry.support_id}</code></p>
        <button class="ghost-btn" onclick={copySupportId}>Copy Support ID</button>
        <p class="card-desc">Include this ID in a privacy or deletion request. Save it before turning reports off; turning them back on creates a new ID.</p>
      {/if}
    {/if}
  </div>

  <!-- Bug report -->
  <div class="more-card">
    <span class="card-label">Bug report</span>
    <p class="card-desc">Use the public report for GitHub. Copy puts the preview on your clipboard.</p>
    <HelpSheet title="About bug reports">
      <p class="card-desc">
        The public report includes app and platform details and recent connection outcomes.
        For private support, review the full report before emailing it to the developer.
      </p>
    </HelpSheet>
    <div class="log-toolbar">
      <button class="ghost-btn compact" onclick={() => prepareBugReport(false)} disabled={bugBusy}>
        <span class="msr" aria-hidden="true">bug_report</span>Review bug report
      </button>
      <button class="ghost-btn compact" onclick={() => prepareBugReport(true)} disabled={bugBusy}>Review private report</button>
      {#if bugReport}
        <button class="ghost-btn compact" onclick={copyBugReport}>
          <span class="msr" aria-hidden="true">content_copy</span>{reportIsPrivate ? "Copy private report" : "Copy bug report"}
        </button>
      {/if}
    </div>
    {#if bugError}
      <p class="log-error mono" role="alert">{bugError}</p>
    {/if}
    {#if bugReport}
      {#if reportIsPrivate}<p class="card-desc">For private email. May include TV names, addresses, serial numbers and app logs.</p>{/if}
      <pre class="log-view mono" aria-label="Bug report">{bugReport}</pre>
    {/if}
  </div>

  <!-- Debug log -->
  <div class="more-card">
    <span class="card-label">Debug log</span>
    <p class="card-desc">For private support. The log may contain TV names, addresses and app activity. Copy puts log text on your clipboard; it does not save a file.</p>
    <HelpSheet title="About debug logs"><p class="card-desc">Recent app activity and the device transport log — useful for support.</p></HelpSheet>
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
</div>
<Toast message={toast} type={toastType} />

<style>
  .support-reports {
    display: flex;
    flex-direction: column;
    gap: 16px;
  }
  .support-id { overflow-wrap: anywhere; user-select: text; }
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
  .telemetry-toggle {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    cursor: pointer;
  }
  .tt-copy {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tt-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .telemetry-toggle input {
    width: 22px;
    height: 22px;
    flex: none;
    accent-color: var(--accent);
  }
  .card-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    margin: 0;
  }
  .telemetry-unsaved {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, var(--danger) 40%, transparent);
    background: color-mix(in srgb, var(--danger) 8%, transparent);
  }
  .telemetry-unsaved .card-desc {
    color: var(--text-soft);
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
  .log-toolbar {
    display: flex;
    flex-wrap: wrap;
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
</style>
