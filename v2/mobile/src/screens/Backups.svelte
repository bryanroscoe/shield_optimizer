<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { BackupEntry, OtherPackage } from "../lib/types";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import Toast from "../components/Toast.svelte";

  let { back }: { back: () => void } = $props();

  let loading = $state(true);
  let error = $state("");
  let backups = $state<BackupEntry[]>([]);

  // App picker (lazily loaded on first open).
  let picking = $state(false);
  let pkgLoading = $state(false);
  let pkgError = $state("");
  let packages = $state<OtherPackage[]>([]);
  let search = $state("");
  let busyPkg = $state("");
  let packagesSerial = $state("");
  let pickerGeneration = 0;
  let restoreTarget = $state<BackupEntry | null>(null);
  let busyRestore = $state("");
  let deleteTarget = $state<BackupEntry | null>(null);
  let busyDelete = $state("");

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3800);
  }

  async function loadBackups() {
    loading = backups.length === 0;
    error = "";
    try {
      backups = await api.listBackups();
    } catch (e) {
      error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(loadBackups);

  async function openPicker() {
    picking = true;
    const serial = session.serial;
    if (!serial) return;
    if (packages.length > 0 && packagesSerial === serial) return;
    const generation = ++pickerGeneration;
    packages = [];
    pkgLoading = true;
    pkgError = "";
    try {
      const rows = await api.listOtherPackages(serial);
      if (generation !== pickerGeneration || serial !== session.serial) return;
      packages = rows;
      packagesSerial = serial;
    } catch (e) {
      if (generation !== pickerGeneration || serial !== session.serial) return;
      pkgError = String(e);
    } finally {
      if (generation === pickerGeneration && serial === session.serial) pkgLoading = false;
    }
  }

  const filtered = $derived.by(() => {
    const q = search.trim().toLowerCase();
    const rows = q
      ? packages.filter(
          (p) =>
            p.package.toLowerCase().includes(q) ||
            (p.name ?? "").toLowerCase().includes(q),
        )
      : packages;
    return [...rows].sort((a, b) =>
      (a.name ?? a.package).localeCompare(b.name ?? b.package),
    );
  });

  async function backup(p: OtherPackage) {
    const targetSerial = session.serial;
    if (busyPkg || !targetSerial) return;
    busyPkg = p.package;
    try {
      const entry = await api.backupApk(targetSerial, p.package);
      showToast(
        `Backed up ${p.name ?? p.package} (${entry.apk_count} APK${entry.apk_count === 1 ? "" : "s"}).`,
        "success",
      );
      backups = [entry, ...backups.filter((b) => b.path !== entry.path)];
      picking = false;
      search = "";
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyPkg = "";
    }
  }

  async function confirmRestore() {
    const backup = restoreTarget;
    const targetSerial = session.serial;
    restoreTarget = null;
    if (!backup || !targetSerial || busyRestore) return;
    busyRestore = backup.path;
    try {
      const result = await api.restoreApkBackup(targetSerial, backup.path);
      showToast(result.message, result.ok ? "success" : "error");
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyRestore = "";
    }
  }

  async function confirmDelete() {
    const target = deleteTarget;
    deleteTarget = null;
    if (!target || busyDelete) return;
    busyDelete = target.path;
    try {
      const result = await api.deleteBackup(target.path);
      if (result.ok) {
        backups = backups.filter((b) => b.path !== target.path);
        showToast(result.message || "Backup deleted.", "success");
      } else {
        showToast(result.message || "Couldn't delete that backup.", "error");
      }
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyDelete = "";
    }
  }

  function fmtSize(bytes: number): string {
    if (!bytes || bytes < 0) return "0 B";
    const units = ["B", "KB", "MB", "GB", "TB"];
    let n = bytes;
    let i = 0;
    while (n >= 1024 && i < units.length - 1) {
      n /= 1024;
      i += 1;
    }
    return `${n < 10 && i > 0 ? n.toFixed(1) : Math.round(n)} ${units[i]}`;
  }

  function fmtDate(iso: string): string {
    if (!iso) return "—";
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return "—";
    return (
      d.toLocaleDateString(undefined, { month: "short", day: "numeric" }) +
      " · " +
      d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" })
    );
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Backups</h3>
    <span style="width:44px"></span>
  </div>

  <!-- Backup destination -->
  <span class="section-label nomargin">Backup destination</span>
  <div class="dest-row">
    <div class="dest active">
      <span class="msr">smartphone</span>
      <span class="dest-title">This phone</span>
      <span class="dest-sub">ATV Optimizer's private storage</span>
    </div>
  </div>

  <!-- Back up an app -->
  <div class="create-card">
    <span class="card-label">Back up an app's APK</span>
    <p class="card-desc">
      Saves every installed APK part to this phone so the app can be restored later. App data and
      sign-in details are not included.
    </p>
    {#if !picking}
      <button class="primary small-inline" disabled={!session.serial} onclick={openPicker}>
        <span class="msr">backup</span>Choose an app
      </button>
    {:else}
      <input
        class="label-input"
        bind:value={search}
        placeholder="Search installed apps…"
      />
      {#if pkgLoading}
        <div class="center small">
          <span class="statuspill live"><span class="pdot blink"></span>Reading installed apps…</span>
        </div>
      {:else if pkgError}
        <p class="error">{pkgError}</p>
        <button class="primary" onclick={openPicker}>Retry</button>
      {:else if filtered.length === 0}
        <p class="lede empty">No matching apps.</p>
      {:else}
        <span class="pkg-count">
          {filtered.length} of {packages.length} installed app{packages.length === 1 ? "" : "s"}
        </span>
        <div class="pkg-list">
          {#each filtered as p (p.package)}
            {@const busy = busyPkg === p.package}
            <button class="pkg-row" disabled={busyPkg !== ""} onclick={() => backup(p)}>
              <span class="msr pkg-icon">android</span>
              <div class="pkg-body">
                <span class="pkg-name">{p.name ?? p.package}</span>
                <span class="pkg-sub mono">{p.package}</span>
              </div>
              {#if busy}
                <span class="pdot blink"></span>
              {:else}
                <span class="msr pkg-dl">download</span>
              {/if}
            </button>
          {/each}
        </div>
      {/if}
      <button class="ghost-link" onclick={() => (picking = false)}>Cancel</button>
    {/if}
  </div>

  <span class="section-label">Recent backups</span>
  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading backups…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={loadBackups}>Retry</button>
  {:else if backups.length === 0}
    <p class="lede empty">No backups yet. Back up an app above to save its APK to this phone.</p>
  {:else}
    <div class="backup-list">
      {#each backups as b (b.path)}
        <div class="backup-row">
          <div class="b-icon"><span class="msr">android</span></div>
          <div class="b-body">
            <span class="b-name">{b.package}</span>
            <span class="b-sub mono">
              This phone · {fmtDate(b.saved_at)} · {fmtSize(b.size_bytes)} · {b.apk_count} APK{b.apk_count === 1 ? "" : "s"}
            </span>
          </div>
          <div class="b-actions">
            {#if b.complete}
              <button
                class="restore-btn"
                disabled={!session.serial || busyRestore !== "" || busyDelete !== ""}
                onclick={() => (restoreTarget = b)}
              >
                {#if busyRestore === b.path}
                  <span class="pdot blink"></span>
                {:else}
                  <span class="msr">restore</span>Restore
                {/if}
              </button>
            {:else}
              <span class="legacy-badge" title="This older backup may be missing split APKs">Base only</span>
            {/if}
            <button
              class="del-btn"
              disabled={busyDelete !== "" || busyRestore !== ""}
              onclick={() => (deleteTarget = b)}
              aria-label={`Delete the ${b.package} backup`}
            >
              {#if busyDelete === b.path}
                <span class="pdot blink"></span>
              {:else}
                <span class="msr">delete</span>
              {/if}
            </button>
          </div>
        </div>
      {/each}
    </div>
  {/if}

  <p class="drive-note">
    Backups stay in this app's private storage — exporting them or syncing to Drive isn't available
    yet.
  </p>

  <div class="spacer"></div>
  <Toast message={toast} type={toastType} />
</div>

<ConfirmDialog
  open={deleteTarget !== null}
  danger
  icon="delete"
  title={`Delete the ${deleteTarget?.package ?? "app"} backup?`}
  message="Removes the saved APK files from this phone. The app on your TV is not touched, and this can't be undone."
  confirmLabel="Delete"
  onConfirm={confirmDelete}
  onCancel={() => (deleteTarget = null)}
/>

<ConfirmDialog
  open={restoreTarget !== null}
  title={`Restore ${restoreTarget?.package ?? "app"}?`}
  message="Installs the saved APK bundle on the connected TV. This does not restore app data or sign-in details."
  warning="Android will reject the restore if the saved app signature is incompatible with the installed version."
  confirmLabel="Restore"
  icon="settings_backup_restore"
  onConfirm={confirmRestore}
  onCancel={() => (restoreTarget = null)}
/>

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

  .section-label.nomargin {
    margin-top: 4px;
  }

  .dest-row {
    display: flex;
    gap: 9px;
    margin-bottom: 6px;
  }
  .dest {
    flex: 1;
    position: relative;
    padding: 14px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .dest.active {
    background: color-mix(in srgb, var(--accent) 8%, var(--surface));
    border: 1.5px solid var(--accent);
  }
  .dest .msr {
    font-size: 22px;
    color: var(--text-soft);
  }
  .dest.active .msr {
    color: var(--accent);
  }
  .dest-title {
    font-size: 13px;
    font-weight: 600;
  }
  .dest-sub {
    font-size: 10px;
    color: var(--muted);
  }

  .create-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 10px;
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
  .small-inline {
    width: auto;
    min-height: 44px;
    padding: 0 18px;
    font-size: 14px;
    border-radius: 11px;
    gap: 6px;
    align-self: flex-start;
  }
  .small-inline .msr {
    font-size: 18px;
  }
  .label-input {
    min-height: 44px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 11px;
    padding: 0 12px;
    color: var(--text);
    font-size: 13px;
  }
  .label-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .center.small {
    min-height: 60px;
  }
  .ghost-link {
    background: transparent;
    border: none;
    color: var(--muted);
    font-family: var(--sans);
    font-size: 12px;
    cursor: pointer;
    align-self: flex-start;
    padding: 4px 0;
  }

  .pkg-count {
    font-size: 11px;
    color: var(--dim);
  }
  .pkg-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    max-height: 320px;
    overflow-y: auto;
  }
  .pkg-row {
    display: flex;
    align-items: center;
    gap: 11px;
    width: 100%;
    text-align: left;
    padding: 10px 11px;
    border-radius: 12px;
    background: var(--canvas);
    border: 1px solid var(--line);
    color: var(--text);
    font-family: var(--sans);
    cursor: pointer;
  }
  .pkg-row:active {
    background: var(--surface-2);
  }
  .pkg-row:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .pkg-icon {
    font-size: 22px;
    color: var(--muted);
    flex: none;
  }
  .pkg-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .pkg-name {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pkg-sub {
    font-size: 10px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pkg-dl {
    font-size: 20px;
    color: var(--accent);
    flex: none;
  }

  .backup-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .backup-row {
    display: flex;
    align-items: center;
    gap: 13px;
    padding: 14px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .b-icon {
    width: 40px;
    height: 40px;
    border-radius: 11px;
    background: var(--surface-2);
    display: grid;
    place-items: center;
    flex: none;
  }
  .b-icon .msr {
    font-size: 22px;
    color: var(--text-soft);
  }
  .b-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .b-name {
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .b-sub {
    font-size: 10px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .restore-btn {
    min-width: 74px;
    min-height: 34px;
    padding: 7px 10px;
    border: 1px solid color-mix(in srgb, var(--accent) 35%, transparent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
    font: inherit;
    font-size: 11px;
    font-weight: 650;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 4px;
    flex: none;
  }
  .restore-btn:disabled {
    opacity: 0.5;
  }
  .b-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .del-btn {
    display: grid;
    place-items: center;
    min-width: 44px;
    min-height: 44px;
    border: 1px solid var(--line);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--muted);
    cursor: pointer;
    flex: none;
  }
  .del-btn:active {
    color: var(--danger);
  }
  .del-btn:disabled {
    opacity: 0.5;
    cursor: default;
  }
  .del-btn .msr {
    font-size: 19px;
  }
  .restore-btn .msr {
    font-size: 16px;
  }
  .legacy-badge {
    flex: none;
    padding: 5px 7px;
    border-radius: 8px;
    background: color-mix(in srgb, var(--amber) 10%, transparent);
    color: var(--amber);
    font-size: 9px;
    font-weight: 650;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }

  .drive-note {
    margin: 16px 0 0;
    font-size: 11px;
    line-height: 1.45;
    color: var(--dim);
  }
</style>
