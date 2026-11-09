<script lang="ts" module>
  // One save or share at a time across every instance: Android's picker or share sheet
  // covers the app, and the backend refuses a second request anyway.
  const lock = $state({ busy: false });
</script>

<script lang="ts">
  // Save a copy / Share for one file already in the app's private storage (a TV screenshot
  // or a file copied from the TV). Each tap hands that one file to Android's own UI; nothing
  // is sent anywhere until the person finishes it there.
  import { api } from "../lib/api";
  import type { ExportMode, ExportOutcome } from "../lib/types";

  let {
    path,
    label,
    compact = false,
  }: { path: string; label: string; compact?: boolean } = $props();

  let running = $state<ExportMode | null>(null);
  let status = $state("");
  let failed = $state(false);

  function describe(outcome: ExportOutcome): string {
    switch (outcome.status) {
      case "saved":
        return outcome.name ? `Saved a copy as ${outcome.name}.` : "Saved a copy.";
      case "canceled":
        return "Not saved.";
      case "share_opened":
        return "Share sheet opened.";
    }
  }

  async function run(mode: ExportMode) {
    if (lock.busy) return;
    lock.busy = true;
    running = mode;
    status = "";
    failed = false;
    try {
      status = describe(await api.exportFile(path, mode));
    } catch (e) {
      failed = true;
      status = String(e);
    } finally {
      lock.busy = false;
      running = null;
    }
  }
</script>

<div class="export" class:compact>
  <div class="export-buttons">
    <button
      type="button"
      class="ghost small export-btn"
      disabled={lock.busy}
      aria-label={compact ? `Save a copy of ${label}` : undefined}
      onclick={() => run("save")}
    >
      <span class="msr" aria-hidden="true">save</span>
      {#if !compact}<span>{running === "save" ? "Opening…" : "Save a copy"}</span>{/if}
    </button>
    <button
      type="button"
      class="ghost small export-btn"
      disabled={lock.busy}
      aria-label={compact ? `Share ${label}` : undefined}
      onclick={() => run("share")}
    >
      <span class="msr" aria-hidden="true">share</span>
      {#if !compact}<span>{running === "share" ? "Opening…" : "Share"}</span>{/if}
    </button>
  </div>
  <p class="export-status" class:error={failed} role="status">{status}</p>
</div>

<style>
  .export {
    display: flex;
    flex-direction: column;
    gap: 6px;
    min-width: 0;
  }
  .export-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .export-btn {
    gap: 8px;
    padding: 0 16px;
    min-width: 0;
  }
  .export-btn .msr {
    font-size: 20px;
  }
  /* In a row, the buttons sit at the trailing edge and the status takes a line below. */
  .export.compact {
    display: contents;
  }
  .compact .export-status {
    flex-basis: 100%;
  }
  .compact .export-btn {
    flex: none;
    width: 48px;
    padding: 0;
  }
  .export-status {
    margin: 0;
    font-size: .875rem;
    line-height: 1.5;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  /* Kept in the accessibility tree while empty, so screen readers announce the result. */
  .export-status:empty {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
  }
  .export-status.error {
    color: var(--danger, #ff6b6b);
  }
</style>
