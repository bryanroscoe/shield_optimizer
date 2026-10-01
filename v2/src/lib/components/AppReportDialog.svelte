<script lang="ts">
  import { onMount } from "svelte";
  import { getVersion } from "@tauri-apps/api/app";
  import { save as saveDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import type { Safety } from "$lib/types";
  import {
    APP_REPORT_NOTE_MAX,
    APP_REPORT_REASONS,
    appReportIssueUrl,
    buildAppReport,
    reportFileName,
    reportText,
    type AppReportDevice,
    type AppReportReason,
    type AppReportState,
  } from "$lib/app-report";

  // The preview is the report: the text in the box is exactly what Copy,
  // Save and the GitHub form carry. Nothing leaves the machine unless the
  // user presses one of those, and the GitHub form still has to be submitted.
  let {
    package: pkg,
    appName,
    verdict,
    device,
    appState,
    onClose,
  }: {
    package: string;
    appName: string | null;
    verdict: Safety | null;
    device: AppReportDevice;
    appState: AppReportState;
    onClose: () => void;
  } = $props();

  /// A package the lists know is most likely reported for its verdict; one
  /// they don't, for being missing. The user can pick anything.
  function defaultReason(): AppReportReason {
    return verdict && verdict.source !== "no_record" ? "wrong_verdict" : "not_listed";
  }
  let reason = $state<AppReportReason>(defaultReason());
  let note = $state("");
  let includeState = $state(false);
  let appVersion = $state<string | null>(null);
  let copied = $state(false);
  let message = $state("");
  // Fixed when the dialog opens so the preview doesn't tick while it is read.
  const openedAt = new Date();

  onMount(() => {
    getVersion().then(
      (v) => (appVersion = v),
      () => (appVersion = null),
    );
  });

  let report = $derived(
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
  let text = $derived(report ? reportText(report) : "");

  async function copy() {
    message = "";
    try {
      await navigator.clipboard.writeText(text);
      copied = true;
      setTimeout(() => (copied = false), 2000);
    } catch (e) {
      message = `Couldn't reach the clipboard (${e}). Select the text above and copy it.`;
    }
  }

  async function saveFile() {
    message = "";
    try {
      const path = await saveDialog({
        defaultPath: reportFileName(pkg, openedAt),
        filters: [{ name: "App report", extensions: ["json"] }],
      });
      if (!path) return;
      await api.saveAppReport(path, text);
      message = `Saved to ${path}.`;
    } catch (e) {
      message = `Couldn't save the report: ${e}`;
    }
  }

  async function openIssue() {
    const { url, prefilled } = appReportIssueUrl(pkg, reason, text);
    message = "";
    try {
      await openUrl(url);
    } catch (e) {
      message = `Couldn't open the browser (${e}). Click Copy, then open ${url.split("?")[0]} and paste it there.`;
      return;
    }
    message = prefilled
      ? "The GitHub form opened with this report filled in. Review it there, then submit it yourself."
      : "This report is too long to fill in for you. Click Copy, then paste it into the Report field on GitHub.";
  }

  function onKeydown(e: KeyboardEvent) {
    if (e.key !== "Escape") return;
    e.preventDefault();
    e.stopPropagation();
    onClose();
  }
</script>

<svelte:window onkeydown={onKeydown} />

<div class="report-backdrop" role="presentation" onclick={onClose}></div>
<div class="report-dialog" role="dialog" aria-modal="true" aria-labelledby="app-report-title">
  <h2 id="app-report-title">Report this app</h2>
  <p class="muted small">
    Nothing is sent automatically. Below is everything this report contains. Copy it, save it
    as a file, or open a GitHub issue that you review and submit yourself. A report asks for a
    review; it never changes how the app rates this package.
  </p>
  {#if !report}
    <p class="small" role="alert">
      <span class="mono">{pkg}</span> is not a package id a report can carry.
    </p>
  {:else}
    <fieldset class="reasons">
      <legend class="small">What's wrong</legend>
      {#each APP_REPORT_REASONS as r (r.id)}
        <label class="small">
          <input type="radio" name="app-report-reason" value={r.id} bind:group={reason} />
          {r.label}
        </label>
      {/each}
    </fieldset>
    <label class="field small">
      Note (optional)
      <textarea
        class="note"
        rows="2"
        maxlength={APP_REPORT_NOTE_MAX}
        bind:value={note}
        placeholder="What you expected, or where you saw it"
      ></textarea>
    </label>
    <label class="toggle small">
      <input type="checkbox" bind:checked={includeState} />
      Include installed and enabled state, and the measured RAM and storage
    </label>
    <textarea class="preview mono" readonly aria-label="Report preview" value={text}></textarea>
    <p class="muted small">
      It carries no serial number, no IP address and no other installed packages. An address
      or this TV's serial typed or pasted into the note is replaced with [redacted].
    </p>
  {/if}
  {#if message}
    <p class="message muted small" role="status">{message}</p>
  {/if}
  <div class="report-actions">
    <button onclick={copy} disabled={!report}>
      <Icon name="content_copy" size={14} /> {copied ? "Copied" : "Copy"}
    </button>
    <button onclick={saveFile} disabled={!report}>
      <Icon name="download" size={14} /> Save as file
    </button>
    <span class="spacer"></span>
    <button onclick={onClose}>Close</button>
    <button class="primary" onclick={openIssue} disabled={!report}>
      Open GitHub issue <Icon name="open_in_new" size={14} />
    </button>
  </div>
</div>

<style>
  .report-backdrop {
    position: fixed;
    inset: 0;
    background: var(--scrim);
    z-index: 10;
  }
  .report-dialog {
    position: fixed;
    z-index: 11;
    top: 50%;
    left: 50%;
    transform: translate(-50%, -50%);
    width: min(38rem, calc(100vw - 3rem));
    max-height: calc(100vh - 4rem);
    overflow-y: auto;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
    padding: 1.25rem;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: var(--radius-xl);
    box-shadow: var(--shadow-modal);
    text-align: left;
  }
  h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  p {
    margin: 0;
  }
  .reasons {
    display: flex;
    flex-wrap: wrap;
    gap: 0.3rem 1rem;
    margin: 0;
    padding: 0;
    border: 0;
  }
  .reasons legend {
    width: 100%;
    margin-bottom: 0.2rem;
    color: var(--fg-muted);
  }
  .reasons label,
  .toggle {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
  }
  .field {
    display: flex;
    flex-direction: column;
    gap: 0.2rem;
    color: var(--fg-muted);
  }
  .note {
    resize: vertical;
    font: inherit;
  }
  .preview {
    min-height: 12rem;
    resize: vertical;
    font-size: 0.78rem;
  }
  .report-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    padding-top: 0.25rem;
    border-top: 1px solid var(--border);
  }
  .spacer {
    flex: 1;
  }
  .small {
    font-size: 0.85rem;
  }
  .mono {
    font-family: var(--mono);
  }
</style>
