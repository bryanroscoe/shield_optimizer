<script lang="ts">
  import { onMount } from "svelte";
  import { openPath } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import type { Device, SnapshotFile } from "$lib/types";
  import { deviceTypeLabel } from "$lib/types";

  let snapshots = $state<SnapshotFile[]>([]);
  let loading = $state(true);
  let error = $state<string | null>(null);
  let snapshotDir = $state<string>("");

  let devices = $state<Device[]>([]);
  let actionBusy = $state<string | null>(null);
  let actionMsg = $state<string>("");

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

  /// Pick a connected device, then apply this snapshot to it. The Snapshot tab
  /// inside that device already supports cross-device apply via the preview
  /// step; this is a one-click shortcut from the global view.
  async function applyTo(snap: SnapshotFile, serial: string) {
    if (!confirm(`Apply ${snap.filename} to ${serial}?\n\nThis will disable ${snap.disabled_count} package(s) and write ${snap.settings_count} setting(s).`)) return;
    actionBusy = snap.path;
    actionMsg = "";
    try {
      const r = await api.applySnapshot(serial, snap.path);
      actionMsg = `${snap.filename} → ${serial}: ${r.summary}`;
    } catch (e) {
      actionMsg = `Apply failed: ${e}`;
    } finally {
      actionBusy = null;
    }
  }

  async function previewTo(snap: SnapshotFile, serial: string) {
    actionBusy = snap.path;
    actionMsg = "";
    try {
      const plan = await api.previewApply(serial, snap.path);
      const lines = [
        `Preview ${snap.filename} → ${serial}:`,
        `  ${plan.packages_to_disable.length} packages would be disabled`,
        `  ${plan.packages_already_disabled.length} already disabled`,
        `  ${plan.packages_not_installed.length} not installed on target`,
        `  Launcher: ${plan.launcher_to_set ?? "(unchanged)"}`,
        `  ${Object.keys(plan.settings_to_write).length} settings would be written`,
      ];
      if (plan.cross_device_warning) lines.push(`⚠ ${plan.cross_device_warning}`);
      actionMsg = lines.join("\n");
    } catch (e) {
      actionMsg = String(e);
    } finally {
      actionBusy = null;
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
      await openPath(snapshotDir);
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
  <h1>Snapshots</h1>
  <div class="header-actions">
    {#if snapshotDir}
      <button onclick={revealFolder} title={snapshotDir}>Open folder</button>
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
  <ul class="snap-list">
    {#each snapshots as s (s.path)}
      <li>
        <div class="snap-main">
          <div class="snap-title">
            <strong>{s.device_name}</strong>
            <span class="tag installed">{deviceTypeLabel(s.device_type).toUpperCase()}</span>
            <span class="muted small mono">{s.device_serial}</span>
          </div>
          <div class="muted small">
            {formatTimestamp(s.saved_at)} ·
            {s.disabled_count} disabled,
            {s.settings_count} settings,
            launcher {s.launcher ?? "—"}
          </div>
          <div class="muted small mono">{s.filename}</div>
        </div>
        <div class="snap-actions">
          {#if authorizedDevices().length > 0}
            <select
              disabled={actionBusy === s.path}
              onchange={(e) => {
                const target = (e.target as HTMLSelectElement);
                const v = target.value;
                if (!v) return;
                const [verb, serial] = v.split("|");
                if (verb === "preview") previewTo(s, serial);
                else if (verb === "apply") applyTo(s, serial);
                target.value = "";
              }}
            >
              <option value="">Apply to device…</option>
              {#each authorizedDevices() as d}
                <option value={`preview|${d.serial}`}>Preview → {d.name}</option>
                <option value={`apply|${d.serial}`}>Apply → {d.name}</option>
              {/each}
            </select>
          {/if}
          <button class="small-action danger" onclick={() => deleteSnap(s)}>Delete</button>
        </div>
      </li>
    {/each}
  </ul>
{/if}

<style>
  .header-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    margin-bottom: 0.6rem;
  }
  .header-actions {
    display: flex;
    gap: 0.5rem;
  }
  h1 {
    margin: 0;
    font-size: 1.4rem;
  }
  .snap-list {
    list-style: none;
    padding: 0;
    margin: 1rem 0 0;
  }
  .snap-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.7rem 1rem;
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 6px;
    margin-bottom: 0.5rem;
  }
  .snap-main {
    flex: 1;
    min-width: 0;
  }
  .snap-title {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-bottom: 0.2rem;
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
    border-radius: 6px;
  }
  .action-msg {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.6rem 0.8rem;
    margin: 0.5rem 0;
    font-family: ui-monospace, monospace;
    font-size: 0.82rem;
    white-space: pre-wrap;
    word-break: break-word;
  }
  .tag {
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    letter-spacing: 0.04em;
  }
  .tag.installed { background: var(--ok-surface); color: var(--ok); }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .small-action {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
  }
  .small-action.danger {
    background: var(--bg-button);
    border-color: var(--danger-surface);
    color: var(--danger-strong);
  }
  .small-action.danger:hover {
    background: var(--danger-surface);
    color: var(--danger-surface-text);
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
  }
</style>
