<script lang="ts">
  import HelpSheet from "./HelpSheet.svelte";
  import { modal } from "../lib/modal";
  // More › Report a bug. Send uploads the description and a short summary
  // to the developer. Rust builds the summary once (it reads the TV) and
  // keeps it; the preview redacts the description against it as it is typed
  // and Send uploads that same summary by id, so what it shows is what goes.
  // GitHub stays as the public alternative.
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { formatSupportId, type TelemetrySettings } from "../../../shared/telemetry";
  import {
    BUG_NOTE_MAX,
    BUG_NOTE_WARN,
    bugIssueUrl,
    connectionSteps,
    previewText,
    type BugReportSummary,
  } from "../lib/bug-report";
  import { publicConnectionHistory } from "../lib/log";

  let { onClose }: { onClose: () => void } = $props();

  let note = $state("");
  let telemetry = $state<TelemetrySettings | null>(null);
  /// Unknown until the backend answers; Send waits for it rather than
  /// offering an upload this build may not have.
  const canSend = $derived(telemetry?.available === true);
  let sending = $state(false);
  let sent = $state<{ reportId: number | null; shortId: string } | null>(null);
  let error = $state("");
  let message = $state("");

  let previewOpen = $state(false);
  /// Built once per report, when the preview first opens or on Send.
  let summary = $state<BugReportSummary | null>(null);
  let summaryLoad: Promise<BugReportSummary> | null = null;
  /// The description as Send would upload it; undefined until redacted.
  let previewNote = $state<string | null | undefined>(undefined);
  const preview = $derived(
    summary && previewNote !== undefined ? { note: previewNote, text: summary.text } : null,
  );
  let previewError = $state("");
  let previewRequest = 0;
  let previewTimer: ReturnType<typeof setTimeout> | undefined;
  const noteNearLimit = $derived(note.length > BUG_NOTE_WARN);

  const serial = () => (session.isConnected ? session.serial : null);

  $effect(() => {
    void api.telemetrySettings().then(
      (settings) => (telemetry = settings),
      () => (telemetry = null),
    );
    return () => clearTimeout(previewTimer);
  });

  function ensureSummary(): Promise<BugReportSummary> {
    if (summary) return Promise.resolve(summary);
    summaryLoad ??= api.prepareBugReport(serial(), connectionSteps()).then(
      (result) => (summary = result),
      (e) => {
        summaryLoad = null;
        throw e;
      },
    );
    return summaryLoad;
  }

  async function loadPreview() {
    const request = ++previewRequest;
    previewError = "";
    try {
      const prepared = await ensureSummary();
      const redacted = await api.redactBugNote(prepared.id, note);
      if (request === previewRequest) previewNote = redacted;
    } catch (e) {
      if (request === previewRequest) {
        previewNote = undefined;
        previewError = String(e);
      }
    }
  }

  function togglePreview() {
    previewOpen = !previewOpen;
    if (previewOpen) void loadPreview();
  }

  // An open preview follows the description as it is typed: only the
  // description is redacted again, the summary stays as built.
  function onInput() {
    sent = null;
    if (!previewOpen) return;
    clearTimeout(previewTimer);
    previewTimer = setTimeout(() => void loadPreview(), 350);
  }

  async function send() {
    if (sending || !note.trim() || !canSend) return;
    sending = true;
    error = "";
    message = "";
    sent = null;
    try {
      const prepared = await ensureSummary();
      const result = await api.sendBugReport(prepared.id, note);
      sent = { reportId: result.report_id ?? null, shortId: formatSupportId(result.support_id) };
      note = "";
      previewOpen = false;
      previewRequest++;
      previewNote = undefined;
      summary = null;
      summaryLoad = null;
    } catch (e) {
      // Rust no longer holds that summary: the next Send or preview builds a new one.
      if (/the report changed/.test(String(e))) {
        summary = null;
        summaryLoad = null;
        previewNote = undefined;
        if (previewOpen) void loadPreview();
      }
      error = String(e).replace(/^.*Error:\s*/, "");
    } finally {
      sending = false;
    }
  }

  async function copyId(id: string) {
    try {
      await navigator.clipboard.writeText(id);
      message = "Support ID copied.";
    } catch {
      message = `Couldn't copy. Your Support ID is ${id}.`;
    }
  }

  /// The public form, with the description and the allowlisted public
  /// report filled in. The user still has to submit it.
  async function openGitHub() {
    error = "";
    message = "";
    let diagnostics = "";
    try {
      diagnostics = await api.collectDiagnostics(serial(), true);
      const history = publicConnectionHistory();
      if (history) diagnostics += `\n### Recent connection actions\n\n${history}\n`;
    } catch {
      // The form still opens; the description is the important part.
    }
    try {
      await api.openUrl(bugIssueUrl(note, diagnostics));
      message = "The GitHub form opened. Review it there, then submit it yourself.";
    } catch {
      error = "Couldn't open the browser. Try again, or visit the project's GitHub issues page.";
    }
  }
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="sheet-overlay" use:modal={{ onClose }} onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="sheet"
    role="dialog"
    aria-modal="true"
    aria-labelledby="bug-report-title"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="sheet-head">
      <h3 id="bug-report-title">Report a bug</h3>
      <button class="close" onclick={onClose} aria-label="Close"><span class="msr">close</span></button>
    </div>

    <label class="field">
      <span>What went wrong?</span>
      <textarea
        rows="5"
        maxlength={BUG_NOTE_MAX}
        bind:value={note}
        oninput={onInput}
        placeholder="What you did, what you expected, and what happened instead"
        autocomplete="off"
      ></textarea>
    </label>
    <!-- Always rendered, so the first count near the limit is announced. -->
    <p class="count" class:idle={!noteNearLimit} aria-live="polite">
      {#if noteNearLimit}{note.length.toLocaleString()} / {BUG_NOTE_MAX.toLocaleString()} characters{/if}
    </p>

    {#if telemetry && !canSend}
      <p class="fine">This build can't send reports to the developer. Report it on GitHub instead.</p>
    {:else}
      <p class="fine">Send shares your description and a short summary of this app, this phone and the connected TV with the developer.</p>
      <HelpSheet title="What's sent">
        <p>
          Your description, the app version, this phone's Android version and model, your Support
          ID, whether automatic reports are on, and for the connected TV its make, model and Android
          version and the outcome of recent connection steps.
        </p>
        <p>
          No app list, TV names, addresses or serial numbers. Addresses and serial numbers in your
          description are removed too. Reports are kept for 30 days and sent whether or not automatic
          reports are on.
        </p>
        <p>
          The Google Play version also attaches a Google Play Integrity check, Google's verdict on
          whether this app and phone are genuine. It is only recorded and never changes what the app
          does.
        </p>
      </HelpSheet>
      <button class="preview-toggle" onclick={togglePreview} aria-expanded={previewOpen}>
        {previewOpen ? "Hide" : "Show"} exactly what's sent
        <span class="msr" class:open={previewOpen}>expand_more</span>
      </button>
      {#if previewOpen}
        {#if previewError}
          <p class="alert" role="alert">{previewError}</p>
        {:else if preview}
          <textarea class="preview mono" readonly aria-label="Report preview" value={previewText(preview)}></textarea>
        {:else}
          <p class="fine">Building the preview…</p>
        {/if}
      {/if}
    {/if}

    {#if sent}
      <div class="sent" role="status">
        <span class="msr" aria-hidden="true">check_circle</span>
        <span>
          Sent{sent.reportId != null ? ` as report #${sent.reportId}` : ""}. Thank you.
          {#if sent.shortId}Your Support ID is <code class="mono">{sent.shortId}</code>.{/if}
        </span>
        {#if sent.shortId}
          <button class="act-btn small" onclick={() => copyId(sent!.shortId)}>
            <span class="msr">content_copy</span>Copy ID
          </button>
        {/if}
      </div>
    {/if}
    {#if error}
      <p class="alert" role="alert">{error}</p>
    {/if}
    {#if message}
      <p class="message" role="status">{message}</p>
    {/if}

    <div class="actions">
      <button class="act-btn" onclick={openGitHub}>
        Report on GitHub instead<span class="msr">open_in_new</span>
      </button>
      {#if !telemetry || canSend}
        <button class="act-btn primary" disabled={!canSend || sending || !note.trim()} onclick={send}>
          {sending ? "Sending…" : "Send"}
        </button>
      {/if}
    </div>
  </div>
</div>

<style>
  .sheet-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 320;
    display: flex;
    align-items: flex-end;
  }
  .sheet {
    width: 100%;
    max-height: 92vh;
    overflow-y: auto;
    overscroll-behavior-y: contain;
    background: var(--surface-2);
    border-top: 1px solid var(--line);
    border-radius: 22px 22px 0 0;
    padding: 20px 20px calc(env(safe-area-inset-bottom) + 20px);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 12px;
    animation: sheetUp 0.25s ease-out;
  }
  .sheet:focus {
    outline: none;
  }
  .sheet-head {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  h3 {
    flex: 1;
    margin: 0;
    font-size: 17px;
  }
  .close {
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 0;
    min-width: 44px;
    min-height: 44px;
    cursor: pointer;
  }
  .close .msr {
    font-size: 22px;
  }
  .fine,
  .message,
  .alert,
  .count {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
  }
  .count {
    text-align: right;
  }
  /* Out of the flex flow while empty, so it adds no gap. */
  .count.idle {
    position: absolute;
  }
  .alert {
    color: var(--danger);
  }
  .message {
    color: var(--text-soft);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 13px;
    font-weight: 600;
    color: var(--text-soft);
  }
  .field textarea,
  .preview {
    width: 100%;
    box-sizing: border-box;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    padding: 10px 12px;
    font-family: var(--sans);
    font-size: 14px;
    font-weight: 400;
    resize: vertical;
  }
  .preview {
    font-family: var(--mono);
    font-size: 11px;
    min-height: 200px;
    user-select: text;
    -webkit-user-select: text;
  }
  .preview-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    min-height: 44px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .preview-toggle .msr {
    font-size: 16px;
    transition: transform 0.15s ease;
  }
  .preview-toggle .msr.open {
    transform: rotate(180deg);
  }
  .sent {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 8px 10px;
    padding: 10px 12px;
    border-radius: 12px;
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
    font-size: 13px;
    line-height: 1.45;
    color: var(--text-soft);
  }
  .sent > span:nth-child(2) {
    flex: 1 1 180px;
  }
  .sent .msr {
    color: var(--accent);
    font-size: 18px;
  }
  .sent code {
    overflow-wrap: anywhere;
    user-select: text;
    -webkit-user-select: text;
  }
  .actions {
    display: flex;
    gap: 9px;
    flex-wrap: wrap;
  }
  .act-btn {
    flex: 1 1 140px;
    min-height: 46px;
    border: 1px solid var(--line);
    border-radius: 13px;
    background: var(--surface);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
    padding: 0 10px;
  }
  .act-btn.small {
    flex: none;
    min-height: 40px;
    font-size: 12px;
  }
  .act-btn.primary {
    background: var(--accent);
    color: var(--accent-ink);
    border-color: transparent;
  }
  .act-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .act-btn .msr {
    font-size: 18px;
  }
  @keyframes sheetUp {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
</style>
