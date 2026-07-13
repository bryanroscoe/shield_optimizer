<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { BackupEntry, OtherPackage } from "../lib/types";
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
    if (packages.length > 0 || !session.serial) return;
    pkgLoading = true;
    pkgError = "";
    try {
      packages = await api.listOtherPackages(session.serial);
    } catch (e) {
      pkgError = String(e);
    } finally {
      pkgLoading = false;
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
    if (busyPkg || !session.serial) return;
    busyPkg = p.package;
    try {
      const entry = await api.backupApk(session.serial, p.package);
      showToast(`Backed up ${p.name ?? p.package}.`, "success");
      backups = [entry, ...backups.filter((b) => b.path !== entry.path)];
      picking = false;
      search = "";
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busyPkg = "";
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
      <span class="dest-sub">Saved to app storage</span>
    </div>
    <div class="dest soon" aria-disabled="true">
      <span class="dest-badge">Soon</span>
      <span class="msr">add_to_drive</span>
      <span class="dest-title">Google Drive</span>
      <span class="dest-sub">Off-device sync</span>
    </div>
  </div>

  <!-- Back up an app -->
  <div class="create-card">
    <span class="card-label">Back up an app's APK</span>
    <p class="card-desc">
      Saves the installed APK to this phone so you can reinstall it later — handy before removing a
      sideloaded app.
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
        <div class="pkg-list">
          {#each filtered.slice(0, 60) as p (p.package)}
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
            <span class="b-sub mono">This phone · {fmtDate(b.saved_at)} · {fmtSize(b.size_bytes)}</span>
          </div>
          <span class="msr b-done fill">check_circle</span>
        </div>
      {/each}
    </div>
  {/if}

  <div class="callout accent drive-note">
    <span class="msr">cloud_sync</span>
    <span class="callout-text">
      Google Drive sync — back up and restore across phones — is coming in a future update. For now
      backups live in this app's storage on this phone.
    </span>
  </div>

  <div class="spacer"></div>
  <Toast message={toast} type={toastType} />
</div>

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
  .dest.soon {
    opacity: 0.62;
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
  .dest-badge {
    position: absolute;
    top: 10px;
    right: 10px;
    font-size: 9px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
    padding: 3px 7px;
    border-radius: 6px;
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
  .b-done {
    font-size: 22px;
    color: var(--teal);
    flex: none;
  }

  .callout.accent {
    display: flex;
    align-items: center;
    gap: 11px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .callout.accent .msr {
    color: var(--accent);
    font-size: 20px;
    flex: none;
  }
  .drive-note {
    margin-top: 14px;
  }
  .callout-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
  }
</style>
