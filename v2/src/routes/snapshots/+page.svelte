<script lang="ts">
  import AbsentSnapshotApps from "../../../shared/components/AbsentSnapshotApps.svelte";
  import HelpDetails from "$lib/components/HelpDetails.svelte";
  import RefusedSettings from "$lib/components/RefusedSettings.svelte";
  import { onMount } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import { confirmAction } from "$lib/confirm";
  import type { Device, SnapshotFile, SnapshotApplyPlan, ApplyResult } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";
  import { homeAlreadyMatches, launcherSkipReason, splitRestoreFailures } from "$lib/snapshot-restore";
  import { destructiveWarning } from "../../../shared/support";
  import { onSupportTiersUpdated } from "$lib/support-updates";

  let snapshots = $state<SnapshotFile[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let snapshotDir = $state<string>("");

  let devices = $state<Device[]>([]);
  let actionBusy = $state<string | null>(null);
  let deletingPath = $state<string | null>(null);
  let actionMsg = $state<string>("");

  // Structured preview: the computed plan for applying `snap` to `serial`,
  // rendered as a table (a la the Optimize wizard) instead of a text dump.
  type PreviewState = {
    snap: SnapshotFile;
    serial: string;
    deviceName: string;
    plan: SnapshotApplyPlan;
  };
  let previewState = $state<PreviewState | null>(null);
  let applyBusy = $state(false);
  let applyResult = $state<ApplyResult | null>(null);
  let applyErr = $state<string | null>(null);

  async function load() {
    if (actionBusy || applyBusy || deletingPath) return;
    loading = true;
    error = null;
    try {
      const [snaps, dir, devs] = await Promise.all([
        api.listSnapshots(),
        api.snapshotDirPath().catch(() => ""),
        api.listDevices().catch(() => [] as Device[]),
      ]);
      snapshots = snaps;
      snapshotDir = dir;
      devices = devs;
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  /// Compute the plan for applying `snap` to `serial` and open the preview
  /// panel. Apply happens from the panel after the user reviews the table.
  async function previewTo(snap: SnapshotFile, serial: string) {
    if (loading || actionBusy || applyBusy || deletingPath) return;
    actionBusy = snap.path;
    actionMsg = "";
    applyResult = null;
    applyErr = null;
    previewState = null;
    try {
      const plan = await api.previewApply(serial, snap.path);
      const deviceName = devices.find((d) => d.serial === serial)?.name ?? serial;
      previewState = { snap, serial, deviceName, plan };
    } catch (e) {
      actionMsg = String(e);
    } finally {
      actionBusy = null;
    }
  }

  function cancelPreview() {
    previewState = null;
    applyResult = null;
    applyErr = null;
  }

  async function confirmApply() {
    if (!previewState || actionBusy || applyBusy || deletingPath) return;
    const { snap, serial, plan } = previewState;
    const total =
      plan.packages_to_disable.length +
      Object.keys(plan.settings_to_write).length +
      plan.settings_to_delete.length +
      (plan.launcher_to_set ? 1 : 0);
    const tierWarning = destructiveWarning(devices.find((d) => d.serial === serial)?.support);
    const current = previewState;
    if (!(await confirmAction(`Restore ${snap.filename} to ${current.deviceName}?\n\n${total} change(s). Disabled packages can be re-enabled via Emergency Recovery.${tierWarning ? `\n\n${tierWarning}` : ""}`))
      || previewState !== current || actionBusy || applyBusy || deletingPath) return;
    applyBusy = true;
    applyErr = null;
    try {
      applyResult = await api.applySnapshot(serial, snap.path);
    } catch (e) {
      applyErr = String(e);
    } finally {
      applyBusy = false;
    }
  }

  async function deleteSnap(snap: SnapshotFile) {
    if (loading || actionBusy || applyBusy || deletingPath) return;
    if (!(await confirmAction(`Delete snapshot "${snap.label ?? snap.device_name}"?\n\n${snap.filename}\n\nThis permanently deletes the saved file from this computer. It does not change the TV.`))
      || loading || actionBusy || applyBusy || deletingPath) return;
    deletingPath = snap.path;
    actionMsg = "";
    try {
      await api.deleteSnapshot(snap.path);
      snapshots = snapshots.filter((s) => s.path !== snap.path);
      if (previewState?.snap.path === snap.path) cancelPreview();
      actionMsg = `Deleted ${snap.label ?? snap.filename}.`;
    } catch (e) {
      actionMsg = `Delete failed: ${e}`;
    } finally {
      deletingPath = null;
    }
  }

  async function revealFolder() {
    if (!snapshotDir) return;
    try {
      // reveal_item_in_dir needs no path scope (unlike open_path) and opens the
      // system file manager at the snapshot folder. The backend ensures the dir
      // exists when it hands us the path, so this works even before the first
      // snapshot is saved.
      await revealItemInDir(snapshotDir);
    } catch (e) {
      actionMsg = `Open folder failed: ${e}`;
    }
  }

  function authorizedDevices(): Device[] {
    return devices.filter((d) => d.status === "device");
  }

  function formatTimestamp(iso: string): string {
    // Snapshot saved_at is "YYYY-MM-DDTHH:MM:SSZ". Replace T with a space and
    // Z with " UTC" for a slightly friendlier display while staying sortable.
    return iso.replace("T", " ").replace("Z", " UTC");
  }

  onMount(() => {
    void load();
    // Fetched support tiers replaced the ones this list was read with; only
    // the devices need re-reading for the restore warning to be current.
    return onSupportTiersUpdated(() => {
      void api
        .listDevices()
        .then((devs) => (devices = devs))
        .catch(() => {});
    });
  });
</script>

<section class="header-row">
  <div class="header-title">
    <h1>Snapshots <span class="beta-tag">Beta</span></h1>
    <p class="muted small mono header-sub">
      all devices · {snapshots.length} saved
    </p>
  </div>
  <div class="header-actions">
    {#if snapshotDir}
      <button onclick={revealFolder} title={snapshotDir}>
        <Icon name="folder_open" size={16} /> Open folder
      </button>
    {/if}
    <button onclick={load} disabled={loading || !!actionBusy || applyBusy || !!deletingPath}>{loading ? "Loading…" : "Refresh"}</button>
  </div>
</section>

<HelpDetails label="Snapshot help">
  <p class="muted">
    A snapshot records which apps are disabled, which known apps weren't installed, your Home app and a set of display, audio, HDMI-CEC and screensaver settings. Restore disables any of those apps it finds turned on; it never uninstalls.
    Restore to the same device, or to another
    one to copy its setup. Files live at <code>{snapshotDir || "(unknown)"}</code> — copy them anywhere to share.
  </p>
</HelpDetails>
<p class="muted small">Restore disables recorded apps and restores settings. It never re-enables or reinstalls apps. Delete removes only the saved file from this computer.</p>

{#if actionMsg}
  <pre class="action-msg">{actionMsg}</pre>
{/if}

{#if previewState}
  {@const plan = previewState.plan}
  {@const settingKeys = Object.keys(plan.settings_to_write)}
  {@const nowValues = plan.current_values ?? {}}
  {@const launcherUnchanged = homeAlreadyMatches(plan, previewState.snap.launcher)}
  {@const launcherSkip = launcherSkipReason(plan)}
  <div class="preview-panel">
    <div class="preview-head">
      <div>
        <h2>Restore to {previewState.deviceName}</h2>
        <p class="muted small mono">{previewState.snap.filename} → {previewState.serial}</p>
      </div>
      <button onclick={cancelPreview} disabled={applyBusy || !!deletingPath}>Close</button>
    </div>

    {#if plan.cross_device_warning}
      <div class="warning"><Icon name="warning" size={16} /> {plan.cross_device_warning}</div>
    {/if}
    <RefusedSettings keys={plan.settings_refused ?? []} />
        <AbsentSnapshotApps packages={plan.packages_absent_skipped ?? []} />

    <div class="plan-summary">
      <strong>{plan.packages_to_disable.length}</strong> to disable ·
      {plan.packages_already_disabled.length} already disabled ·
      {plan.packages_not_installed.length} not on device ·
      <strong>{settingKeys.length}</strong> setting{settingKeys.length === 1 ? "" : "s"}
      to write · <strong>{plan.settings_to_delete.length}</strong> to reset
      {#if plan.packages_kept.length > 0}· {plan.packages_kept.length} kept on for Home{/if}
      {#if plan.launcher_to_set}· Home app{/if}
    </div>

    <table class="plan-table">
      <thead><tr><th>Item</th><th>Now</th><th>What happens</th></tr></thead>
      <tbody>
        {#each plan.packages_to_disable as pkg (pkg)}
          <tr class="acting" ><td class="mono small">{pkg}</td><td class="mono small">enabled</td><td><span class="plan-act disable">Disable</span></td></tr>
        {/each}
        {#if plan.launcher_to_set}
          <tr class="acting"><td class="mono small">Home app</td><td class="mono small">{plan.current_launcher ?? "—"}</td><td><span class="plan-act launcher">Set Home → {plan.launcher_to_set}</span></td></tr>
        {/if}
        {#each settingKeys as k (k)}
          <tr class="acting"><td class="mono small">{k}</td><td class="mono small">{nowValues[k] ?? "unset"}</td><td><span class="plan-act setting">Set → {plan.settings_to_write[k]}</span></td></tr>
        {/each}
        {#each plan.settings_to_delete as k (k)}
          <tr class="acting"><td class="mono small">{k}</td><td class="mono small">{nowValues[k] ?? "—"}</td><td><span class="plan-act setting">Reset → device default (delete override)</span></td></tr>
        {/each}
        {#if launcherUnchanged}
          <tr class="dim"><td class="mono small">Home app</td><td class="mono small">{plan.current_launcher ?? "—"}</td><td><span class="terminal-reason">Already Home</span></td></tr>
        {:else if launcherSkip}
          <tr class="dim" data-plan-item="launcher"><td class="mono small">Home app</td><td class="mono small">{plan.current_launcher ?? "—"}</td><td><span class="terminal-reason">{launcherSkip}</span></td></tr>
        {/if}
        {#each plan.packages_kept as kept (kept.package)}
          <tr class="dim" data-plan-item="kept"><td class="mono small">{kept.package}</td><td class="mono small">enabled</td><td><span class="terminal-reason">Kept on: {kept.reason}</span></td></tr>
        {/each}
        {#each plan.settings_already_set as k (k)}
          <tr class="dim"><td class="mono small">{k}</td><td class="mono small">{nowValues[k] ?? "unset"}</td><td><span class="terminal-reason">Already set</span></td></tr>
        {/each}
        {#each plan.packages_already_disabled as pkg (pkg)}
          <tr class="dim"><td class="mono small">{pkg}</td><td class="mono small">disabled</td><td><span class="terminal-reason">Already disabled</span></td></tr>
        {/each}
        {#each plan.packages_not_installed as pkg (pkg)}
          <tr class="dim"><td class="mono small">{pkg}</td><td class="mono small">not installed</td><td><span class="terminal-reason">Not on device</span></td></tr>
        {/each}
      </tbody>
    </table>

    <div class="apply-row">
      <button class="primary" onclick={confirmApply} disabled={applyBusy || !!actionBusy || !!deletingPath || applyResult !== null}>
        {applyBusy ? "Restoring…" : applyResult ? "Restored" : "Restore snapshot"}
      </button>
      <span class="muted small">Disable is reversible via Emergency Recovery on the device.</span>
    </div>
    {#if applyErr}<div class="error">{applyErr}</div>{/if}
    {#if applyResult}
      {@const outcome = splitRestoreFailures(applyResult)}
      <div class="apply-result">
        <p><strong>{applyResult.summary}</strong></p>
        {#if !applyResult.launcher_set && applyResult.launcher_message}
          <p>Home app: {applyResult.launcher_message}</p>
        {/if}
        {#if outcome.kept.length > 0}
          <p>Kept on: {outcome.kept.map((k) => `${k.package} (${k.reason})`).join(", ")}</p>
        {/if}
        {#if outcome.failed.length > 0}
          <p class="warn-text">{outcome.failed.length} failed: {outcome.failed.join(", ")}</p>
        {/if}
        {#if applyResult.settings_failed.length > 0}
          <p class="warn-text">{applyResult.settings_failed.length} setting(s) failed: {applyResult.settings_failed.join(", ")}</p>
        {/if}
      </div>
    {/if}
  </div>
{/if}

{#if error}
  <div class="error">{error}</div>
{:else if loading && snapshots.length === 0}
  <div class="muted">Loading…</div>
{:else if snapshots.length === 0}
  <div class="empty">
    <h2>No snapshots yet.</h2>
    <p class="muted">Open a device and save a snapshot from its Snapshot tab.</p>
  </div>
{:else}
  <!-- Board 11.12's table: what it is, where it came from, when, and what you
       can do with it. -->
  <div class="snap-table-box">
    <table class="snap-table">
      <thead>
        <tr>
          <th>Snapshot</th>
          <th>Source device</th>
          <th class="right">Saved</th>
          <th class="right">Action</th>
        </tr>
      </thead>
      <tbody>
        {#each snapshots as s (s.path)}
          <tr>
            <td>
              <div class="snap-name">{s.label ?? s.device_name}</div>
              <div class="muted small mono snap-sub">
                {s.disabled_count} disabled · {s.settings_count} settings ·
                launcher {s.launcher ?? "—"} · {s.filename}
              </div>
            </td>
            <td class="src-cell">
              <div class="mono">{s.device_name}</div>
              <div class="muted small mono">
                {deviceTypeLabel(s.device_type).toLowerCase()} · {s.device_serial}
              </div>
            </td>
            <td class="right mono nowrap">{formatTimestamp(s.saved_at)}</td>
            <td class="right">
              <div class="snap-actions">
              {#if authorizedDevices().length > 0}
                <select
                  disabled={loading || !!actionBusy || applyBusy || !!deletingPath}
                  aria-label={`Restore ${s.label ?? s.device_name} to a device`}
                  onchange={(e) => {
                    const target = e.target as HTMLSelectElement;
                    const serial = target.value;
                    if (serial) previewTo(s, serial);
                    target.value = "";
                  }}
                >
                  <option value="">Restore to device…</option>
                  {#each authorizedDevices() as d}
                    <option value={d.serial}>Preview restore → {d.name}</option>
                  {/each}
                </select>
              {/if}
              <button
                class="snap-tool danger"
                onclick={() => deleteSnap(s)}
                disabled={loading || !!actionBusy || applyBusy || !!deletingPath}
                title="Delete this snapshot file from disk"
                aria-label={`Delete ${s.filename}`}
              ><Icon name="delete" size={16} /> {deletingPath === s.path ? "Deleting…" : "Delete"}</button>
              </div>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <HelpDetails label="Copying a setup">
    <div class="callout snapshots-note">
      <Icon name="content_copy" size={16} />
      <span>
        Restoring a snapshot to a different device is a clone — the restore plan names
        every package the target is missing before anything runs.
      </span>
    </div>
  </HelpDetails>
{/if}

<style>
  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.6rem;
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
  .snap-table-box {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    overflow-x: auto;
  }
  .snap-table {
    width: 100%;
    border-collapse: collapse;
  }
  .snap-table th {
    padding: 0.6rem 1rem;
    border-bottom: 1px solid var(--border);
    color: var(--fg-muted);
    font-weight: 500;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    text-align: left;
  }
  .snap-table td {
    padding: 0.7rem 1rem;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  .snap-table tr:last-child td {
    border-bottom: none;
  }
  .snap-table th.right,
  .snap-table td.right {
    text-align: right;
  }
  .nowrap {
    white-space: nowrap;
  }
  .snap-name {
    font-weight: 600;
  }
  .snap-sub {
    margin-top: 0.15rem;
    /* break-word, not anywhere: a package id should break when it has to, but
       "23 disabled" should not be split across three lines. */
    overflow-wrap: break-word;
  }
  /* The name column takes the room; the rest claim only what they need. The
     `max-width: 0` trick used elsewhere does not work here — the Action cell
     holds a <select> whose min-content is its longest option, so the name
     column collapses to a word per line. Capping the select is what
     actually frees the width. */
  .snap-table th:first-child,
  .snap-table td:first-child {
    width: auto;
  }
  .src-cell,
  .snap-table th.right,
  .snap-table td.right {
    width: 1%;
    white-space: nowrap;
  }
  .snap-actions select {
    max-width: 11rem;
  }
  .snap-tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.3rem;
    padding: 0.3rem 0.5rem;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
    vertical-align: middle;
  }
  .snap-tool.danger {
    color: var(--danger);
  }
  .snap-tool.danger:hover {
    border-color: var(--danger);
    background: var(--danger-surface);
    color: var(--danger-surface-text);
  }
  .snapshots-note {
    margin-top: 1rem;
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: 0.5rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  .beta-tag {
    margin-left: 0.4rem;
    padding: 0.05rem 0.4rem;
    border-radius: var(--radius-sm);
    background: var(--warn-surface-2);
    color: var(--warn);
    font-size: 0.65rem;
    font-weight: 600;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    vertical-align: middle;
  }
  /* A wrapper, not the cell itself: display:flex on a <td> takes it out of
     table layout, and its row border stops short of the table edge. */
  .snap-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.4rem;
    align-items: center;
  }
  .empty {
    text-align: center;
    padding: 3rem 1rem;
  }
  .empty h2 {
    margin: 0 0 0.4rem;
    font-size: 1.1rem;
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
  }
  .action-msg {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0.6rem 0.8rem;
    margin: 0.5rem 0;
    font-family: var(--mono);
    font-size: 0.82rem;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 0.85em;
  }

  /* Snapshot-apply preview — same visual language as the Optimize wizard. */
  .preview-panel {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-surface);
    padding: 1rem 1.2rem;
    margin: 0.5rem 0 1rem;
  }
  .preview-head {
    display: flex;
    flex-wrap: wrap;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
  }
  .preview-head h2 {
    margin: 0;
    font-size: 1.1rem;
  }
  .preview-head > div {
    min-width: 0;
    overflow-wrap: anywhere;
  }
  .preview-head > button {
    margin-left: auto;
  }
  .warning {
    background: var(--warn-surface);
    border: 1px solid var(--warn-border);
    color: var(--warn);
    padding: 0.6rem 0.9rem;
    border-radius: var(--radius-md);
    margin: 0.6rem 0;
    font-size: 0.9rem;
  }
  .plan-summary {
    margin: 0.6rem 0;
    font-size: 0.9rem;
  }
  .plan-table {
    width: 100%;
    border-collapse: collapse;
    margin: 0.4rem 0 0.8rem;
  }
  .plan-table th {
    text-align: left;
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-muted);
    padding: 0.3rem 0.6rem;
    border-bottom: 1px solid var(--border);
  }
  .plan-table td {
    padding: 0.35rem 0.6rem;
    border-bottom: 1px solid var(--border);
    overflow-wrap: anywhere;
  }
  .plan-table tr.dim {
    opacity: 0.5;
  }
  .plan-table tr.acting td:first-child {
    box-shadow: inset 3px 0 0 var(--accent-strong);
  }
  .plan-act {
    font-weight: 500;
    font-size: 0.85rem;
  }
  .plan-act.disable { color: var(--accent); }
  .plan-act.launcher { color: var(--accent); }
  .plan-act.setting { color: var(--fg-secondary); font-weight: 400; }
  .terminal-reason {
    display: inline-block;
    font-size: 0.74rem;
    padding: 0.15rem 0.5rem;
    border-radius: var(--radius-sm);
    background: var(--bg-muted);
    color: var(--fg-faint);
  }
  .apply-row {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    flex-wrap: wrap;
    margin-top: 0.4rem;
  }
  .apply-result {
    margin-top: 0.8rem;
    padding: 0.6rem 0.9rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .warn-text { color: var(--warn); }
</style>
