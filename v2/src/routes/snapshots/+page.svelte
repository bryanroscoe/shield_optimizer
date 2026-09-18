<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
  import { revealItemInDir } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import type { Device, SnapshotFile, SnapshotApplyPlan, ApplyResult } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";

  let snapshots = $state<SnapshotFile[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let snapshotDir = $state<string>("");

  let devices = $state<Device[]>([]);
  let actionBusy = $state<string | null>(null);
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
    if (!previewState) return;
    const { snap, serial, plan } = previewState;
    const total =
      plan.packages_to_disable.length +
      Object.keys(plan.settings_to_write).length +
      plan.settings_to_delete.length +
      (plan.launcher_to_set ? 1 : 0);
    if (!confirm(`Apply ${snap.filename} to ${previewState.deviceName}?\n\n${total} change(s). Disabled packages can be re-enabled via Emergency Recovery.`)) return;
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
    if (!confirm(`Delete ${snap.filename}? This removes the file from disk.`)) return;
    try {
      await api.deleteSnapshot(snap.path);
      snapshots = snapshots.filter((s) => s.path !== snap.path);
    } catch (e) {
      actionMsg = `Delete failed: ${e}`;
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
    // strip Z for a slightly friendlier display while staying sortable.
    return iso.replace("T", " ").replace("Z", " UTC");
  }

  onMount(load);
</script>

<section class="header-row">
  <div class="header-title">
    <h1>Snapshots</h1>
    <p class="muted small mono header-sub">
      all devices · {snapshots.length} saved
    </p>
  </div>
  <div class="header-actions">
    {#if snapshotDir}
      <button onclick={revealFolder} title={snapshotDir}>
        <Icon name="folder_open" size={15} /> Open folder
      </button>
    {/if}
    <button onclick={load} disabled={loading}>{loading ? "Loading…" : "Refresh"}</button>
  </div>
</section>

<p class="muted">
  Snapshots capture a device's disabled packages, current launcher, and tracked settings.
  Apply them to the same device (rollback) or a different one (cross-device clone).
  Files live at <code>{snapshotDir || "(unknown)"}</code> — copy them anywhere to share.
</p>

{#if actionMsg}
  <pre class="action-msg">{actionMsg}</pre>
{/if}

{#if previewState}
  {@const plan = previewState.plan}
  {@const settingKeys = Object.keys(plan.settings_to_write)}
  <div class="preview-panel">
    <div class="preview-head">
      <div>
        <h2>Apply to {previewState.deviceName}</h2>
        <p class="muted small mono">{previewState.snap.filename} → {previewState.serial}</p>
      </div>
      <button onclick={cancelPreview}>Close</button>
    </div>

    {#if plan.cross_device_warning}
      <div class="warning"><Icon name="warning" size={15} /> {plan.cross_device_warning}</div>
    {/if}

    <div class="plan-summary">
      <strong>{plan.packages_to_disable.length}</strong> to disable ·
      {plan.packages_already_disabled.length} already disabled ·
      {plan.packages_not_installed.length} not on device ·
      <strong>{settingKeys.length}</strong> setting{settingKeys.length === 1 ? "" : "s"}
      to write · <strong>{plan.settings_to_delete.length}</strong> to reset
      {#if plan.launcher_to_set}· launcher{/if}
    </div>

    <table class="plan-table">
      <thead><tr><th>Package</th><th>What happens</th></tr></thead>
      <tbody>
        {#each plan.packages_to_disable as pkg (pkg)}
          <tr class="acting"><td class="mono small">{pkg}</td><td><span class="plan-act disable">Disable</span></td></tr>
        {/each}
        {#if plan.launcher_to_set}
          <tr class="acting"><td class="mono small">{plan.launcher_to_set}</td><td><span class="plan-act launcher">Set as launcher</span></td></tr>
        {/if}
        {#each settingKeys as k (k)}
          <tr class="acting"><td class="mono small">{k}</td><td><span class="plan-act setting">Set → {plan.settings_to_write[k]}</span></td></tr>
        {/each}
        {#each plan.settings_to_delete as k (k)}
          <tr class="acting"><td class="mono small">{k}</td><td><span class="plan-act setting">Reset → device default (delete override)</span></td></tr>
        {/each}
        {#each plan.packages_already_disabled as pkg (pkg)}
          <tr class="dim"><td class="mono small">{pkg}</td><td><span class="terminal-reason">Already disabled</span></td></tr>
        {/each}
        {#each plan.packages_not_installed as pkg (pkg)}
          <tr class="dim"><td class="mono small">{pkg}</td><td><span class="terminal-reason">Not on device</span></td></tr>
        {/each}
      </tbody>
    </table>

    <div class="apply-row">
      <button class="primary" onclick={confirmApply} disabled={applyBusy || applyResult !== null}>
        {applyBusy ? "Applying…" : applyResult ? "Applied" : "Apply snapshot"}
      </button>
      <span class="muted small">Disable is reversible via Emergency Recovery on the device.</span>
    </div>
    {#if applyErr}<div class="error">{applyErr}</div>{/if}
    {#if applyResult}
      <div class="apply-result">
        <p><strong>{applyResult.summary}</strong></p>
        {#if applyResult.packages_failed.length > 0}
          <p class="warn-text">{applyResult.packages_failed.length} failed: {applyResult.packages_failed.join(", ")}</p>
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
       can do with it. The old rows stacked four lines each and gave the file
       name as much weight as the device it came from. -->
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
              <div class="snap-name">{s.label ?? s.filename}</div>
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
            <td class="right snap-actions">
              {#if authorizedDevices().length > 0}
                <select
                  disabled={actionBusy === s.path}
                  aria-label={`Apply ${s.label ?? s.filename} to a device`}
                  onchange={(e) => {
                    const target = e.target as HTMLSelectElement;
                    const serial = target.value;
                    if (serial) previewTo(s, serial);
                    target.value = "";
                  }}
                >
                  <option value="">Apply to device…</option>
                  {#each authorizedDevices() as d}
                    <option value={d.serial}>Preview → {d.name}</option>
                  {/each}
                </select>
              {/if}
              <button
                class="snap-tool danger"
                onclick={() => deleteSnap(s)}
                title="Delete this snapshot file from disk"
                aria-label={`Delete ${s.filename}`}
              ><Icon name="delete" size={16} /></button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  </div>
  <div class="callout snapshots-note">
    <Icon name="content_copy" size={16} />
    <span>
      Applying a snapshot from a different device is a clone — the apply plan names
      every package the target is missing before anything runs.
    </span>
  </div>
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
    overflow: hidden;
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
    vertical-align: top;
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
    overflow-wrap: anywhere;
  }
  .src-cell {
    white-space: nowrap;
  }
  .snap-tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.3rem;
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
    gap: 0.5rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  .snap-actions {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-shrink: 0;
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
    justify-content: space-between;
    align-items: flex-start;
    gap: 1rem;
  }
  .preview-head h2 {
    margin: 0;
    font-size: 1.1rem;
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
