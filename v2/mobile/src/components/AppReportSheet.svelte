<script lang="ts">
  // "Report this app" (#100), the phone's version of desktop's
  // AppReportDialog. The preview is the report: the text shown is exactly
  // what Copy and the GitHub form carry. Nothing is sent by the app, and the
  // GitHub form still has to be submitted by the user.
  import { invoke } from "@tauri-apps/api/core";
  import type { Safety } from "../lib/types";
  import {
    APP_REPORT_NOTE_MAX,
    APP_REPORT_REASONS,
    appReportIssueUrl,
    buildAppReport,
    reportText,
    type AppReportDevice,
    type AppReportReason,
    type AppReportState,
  } from "../lib/app-report";

  let {
    package: pkg,
    appName,
    verdict,
    device,
    appState,
    appVersion,
    onClose,
  }: {
    package: string;
    appName: string | null;
    /// Null while the lookup is pending or when it failed.
    verdict: Safety | null;
    device: AppReportDevice;
    appState: AppReportState;
    appVersion: string | null;
    onClose: () => void;
  } = $props();

  /// A package the lists know is most likely reported for its verdict; one
  /// they don't, for being missing. Without a verdict, neither is claimed.
  function defaultReason(v: Safety | null): AppReportReason {
    if (!v) return "other";
    return v.source !== "no_record" ? "wrong_verdict" : "not_listed";
  }
  let chosenReason = $state<AppReportReason | null>(null);
  const reason = $derived(chosenReason ?? defaultReason(verdict));
  let note = $state("");
  let includeState = $state(false);
  let previewOpen = $state(false);
  let message = $state("");
  let copied = $state(false);
  let fallbackUrl = $state("");
  let copiedTimer: ReturnType<typeof setTimeout> | undefined;
  const openedAt = new Date();

  const report = $derived(
    buildAppReport({
      package: pkg,
      appName,
      reason,
      note,
      appVersion,
      device,
      verdict,
      includeState,
      state: appState,
      now: openedAt,
    }),
  );
  const text = $derived(report ? reportText(report) : "");

  async function writeClipboard(value: string): Promise<boolean> {
    try {
      await navigator.clipboard.writeText(value);
      return true;
    } catch {
      // Older WebViews without the async clipboard: copy through a
      // throwaway selection instead.
      try {
        const area = document.createElement("textarea");
        area.value = value;
        area.setAttribute("readonly", "");
        area.style.position = "fixed";
        area.style.opacity = "0";
        document.body.appendChild(area);
        area.select();
        const ok = document.execCommand("copy");
        area.remove();
        return ok;
      } catch {
        return false;
      }
    }
  }

  async function copyReport(): Promise<boolean> {
    if (!text) return false;
    const ok = await writeClipboard(text);
    if (ok) {
      copied = true;
      clearTimeout(copiedTimer);
      copiedTimer = setTimeout(() => (copied = false), 2000);
    } else {
      previewOpen = true;
      message = "Couldn't reach the clipboard. Long-press the report below to select and copy it.";
    }
    return ok;
  }

  async function onCopy() {
    message = "";
    fallbackUrl = "";
    if (await copyReport()) message = "Report copied.";
  }

  async function openIssue() {
    if (!report) return;
    message = "";
    fallbackUrl = "";
    const { url, prefilled } = appReportIssueUrl(pkg, reason, text);
    // Too long to carry in the link: put it on the clipboard before the
    // browser takes over, so there is something to paste.
    const copiedFirst = prefilled ? false : await copyReport();
    try {
      await invoke("plugin:opener|open_url", { url });
    } catch {
      fallbackUrl = url;
      message = prefilled
        ? "Couldn't open the browser from here. Copy the link, open it in your browser, and review the filled-in report before submitting."
        : `Couldn't open the browser from here. ${copiedFirst ? "The report is on your clipboard. " : ""}Copy the link, open it in your browser, then paste the report into the Report field.`;
      return;
    }
    message = prefilled
      ? "The GitHub form opened with this report filled in. Review it there, then submit it yourself."
      : copiedFirst
        ? "This report is too long to fill in for you, so it's on your clipboard. Paste it into the Report field on GitHub."
        : "This report is too long to fill in for you. Copy it, then paste it into the Report field on GitHub.";
  }

  async function copyLink() {
    if (fallbackUrl && (await writeClipboard(fallbackUrl))) message = "Link copied. Open it in your browser.";
  }

  $effect(() => () => clearTimeout(copiedTimer));
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="sheet-overlay" onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events -->
  <div
    class="sheet"
    role="dialog"
    aria-modal="true"
    aria-labelledby="app-report-title"
    tabindex="-1"
    onclick={(e) => e.stopPropagation()}
  >
    <div class="sheet-head">
      <div class="head-body">
        <h3 id="app-report-title">Report this app</h3>
        <span class="mono pkg">{pkg}</span>
      </div>
      <button class="close" onclick={onClose} aria-label="Close"><span class="msr">close</span></button>
    </div>

    <p class="lede">
      Nothing is sent automatically. Copy the report, or open a GitHub issue you review and
      submit yourself. A report asks for a review; it never changes how the app rates this
      package.
    </p>

    {#if !report}
      <p class="alert" role="alert">This package id can't be carried by a report.</p>
    {:else}
      <fieldset class="reasons">
        <legend>What's wrong</legend>
        {#each APP_REPORT_REASONS as r (r.id)}
          <label class="reason" class:active={reason === r.id}>
            <input
              type="radio"
              name="app-report-reason"
              value={r.id}
              checked={reason === r.id}
              onchange={() => (chosenReason = r.id)}
            />
            {r.label}
          </label>
        {/each}
      </fieldset>

      <label class="field">
        <span>Note (optional)</span>
        <textarea
          rows="3"
          maxlength={APP_REPORT_NOTE_MAX}
          bind:value={note}
          placeholder="What you expected, or where you saw it"
          autocomplete="off"
        ></textarea>
      </label>

      <label class="toggle">
        <input type="checkbox" bind:checked={includeState} />
        <span>Include installed and enabled state, and the measured RAM and storage</span>
      </label>

      <p class="fine">
        It carries no serial number, no IP address and no other installed packages. An address,
        this TV's serial or another app's package id in the note is replaced with [redacted].
      </p>

      <button class="preview-toggle" onclick={() => (previewOpen = !previewOpen)} aria-expanded={previewOpen}>
        {previewOpen ? "Hide" : "Show"} the full report
        <span class="msr" class:open={previewOpen}>expand_more</span>
      </button>
      {#if previewOpen}
        <textarea
          class="preview mono"
          readonly
          aria-label="Report preview"
          value={text}
        ></textarea>
      {/if}
    {/if}

    {#if message}
      <p class="message" role="status">{message}</p>
    {/if}
    {#if fallbackUrl}
      <div class="link-row">
        <span class="mono link">{fallbackUrl.split("?")[0]}</span>
        <button class="act-btn small" onclick={copyLink}><span class="msr">content_copy</span>Copy link</button>
      </div>
    {/if}

    <div class="actions">
      <button class="act-btn" disabled={!report} onclick={onCopy}>
        <span class="msr">content_copy</span>{copied ? "Copied" : "Copy report"}
      </button>
      <button class="act-btn primary" disabled={!report} onclick={openIssue}>
        Open GitHub issue
      </button>
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
    align-items: flex-start;
    gap: 10px;
  }
  .head-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 3px;
  }
  h3 {
    margin: 0;
    font-size: 17px;
  }
  .pkg {
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .close {
    border: 0;
    background: transparent;
    color: var(--muted);
    padding: 0;
    cursor: pointer;
  }
  .close .msr {
    font-size: 22px;
  }
  .lede,
  .fine,
  .message,
  .alert {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
  }
  .alert {
    color: var(--danger);
  }
  .message {
    color: var(--text-soft);
  }
  .reasons {
    border: 0;
    margin: 0;
    padding: 0;
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .reasons legend {
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--muted);
    margin-bottom: 8px;
    padding: 0;
  }
  .reason {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    min-height: 36px;
    padding: 0 12px;
    border-radius: 999px;
    border: 1px solid var(--line);
    background: var(--surface);
    font-size: 12px;
    color: var(--text-soft);
  }
  .reason.active {
    border-color: var(--accent);
    color: var(--text);
  }
  .reason input {
    margin: 0;
    accent-color: var(--accent);
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    color: var(--muted);
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
    resize: vertical;
  }
  .preview {
    font-family: var(--mono);
    font-size: 11px;
    min-height: 180px;
    user-select: text;
    -webkit-user-select: text;
  }
  .toggle {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 12px;
    color: var(--text-soft);
    line-height: 1.4;
  }
  .toggle input {
    margin: 2px 0 0;
    accent-color: var(--accent);
    flex: none;
  }
  .preview-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
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
  .link-row {
    display: flex;
    align-items: center;
    gap: 8px;
  }
  .link {
    flex: 1;
    min-width: 0;
    font-size: 11px;
    color: var(--muted);
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
    min-height: 36px;
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
