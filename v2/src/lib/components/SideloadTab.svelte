<script lang="ts">
  import { onMount } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import sideloadCatalog from "$lib/sideload-catalog.json";
  import type { DiscoveredApk } from "$lib/types";

  let { serial, deviceLabel = "" }: { serial: string; deviceLabel?: string } = $props();

  /// Path of the APK currently installing (null when idle) — per-path so a
  /// multi-APK list only shows the spinner on the row actually installing.
  let sideloadBusy = $state<string | null>(null);
  let sideloadResult = $state<string>("");
  let sideloadHint = $state<string | null>(null);
  /// Path the current install result belongs to (so it renders under that
  /// row), whether it succeeded, and the raw adb output for the details line.
  let sideloadResultPath = $state<string | null>(null);
  let sideloadOk = $state(false);
  // A remembered folder is only a string from a previous explicit selection.
  // Keep it separate from the folder backing the currently displayed results
  // so mounting this tab never probes a stale or removable-volume path.
  let savedFolder = $state<string | null>(null);
  let discoveredApks = $state<DiscoveredApk[]>([]);
  let discoveredFolder = $state<string | null>(null);
  let discoveryBusy = $state(false);
  let scanError = $state("");
  /// package id → state, for the discovered APKs, so each row can say whether
  /// it's already installed on this device.
  let apkInstallState = $state<Record<string, "enabled" | "disabled" | "missing">>({});

  async function pickAndInstallApk() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      filters: [{ name: "Android Packages", extensions: ["apk"] }],
    });
    if (!selected || Array.isArray(selected)) return;
    // Remember the folder the user picked from so we can show the
    // surrounding APKs as a quick-pick list.
    const lastSep = Math.max(selected.lastIndexOf("/"), selected.lastIndexOf("\\"));
    if (lastSep > 0) {
      const folder = selected.slice(0, lastSep);
      localStorage.setItem("shieldopt.lastApkFolder", folder);
      savedFolder = folder;
      await scanApkFolder(folder, "selected");
    }
    await installApkPath(selected);
  }

  async function pickApkFolder() {
    const picked = await openDialog({ multiple: false, directory: true });
    if (!picked || Array.isArray(picked)) return;
    localStorage.setItem("shieldopt.lastApkFolder", picked);
    savedFolder = picked;
    await scanApkFolder(picked, "selected");
  }

  async function scanApkFolder(folder: string, source: "saved" | "selected") {
    discoveryBusy = true;
    scanError = "";
    try {
      let apks: DiscoveredApk[];
      try {
        apks = await api.listApksInFolder(folder);
      } catch {
        scanError = source === "saved"
          ? "Couldn't scan the saved folder. It may be unavailable or permission was denied."
          : "Couldn't scan this folder. It may be unavailable or permission was denied.";
        return;
      }

      discoveredApks = apks;
      discoveredFolder = folder;
      apkInstallState = {};
      const pkgs = apks.map((a) => a.package).filter((p): p is string => !!p);
      if (pkgs.length) {
        try {
          apkInstallState = await api.packageStates(serial, pkgs);
        } catch {
          const folderLabel = source === "saved" ? "saved folder" : "selected folder";
          scanError = `APK files were read from the ${folderLabel}, but app status couldn't be checked on this TV.`;
        }
      }
    } finally {
      discoveryBusy = false;
    }
  }

  async function scanSavedFolder() {
    if (!savedFolder || discoveryBusy || sideloadBusy !== null) return;
    await scanApkFolder(savedFolder, "saved");
  }

  async function installApkPath(path: string) {
    sideloadBusy = path;
    sideloadResultPath = path;
    sideloadOk = false;
    sideloadResult = "";
    sideloadHint = null;
    try {
      const r = await api.installApk(serial, path, true);
      sideloadOk = r.ok;
      // Friendly summary; the raw adb output is kept for the details line.
      sideloadResult = r.ok
        ? "Installed."
        : installFailureSummary(r.message);
      sideloadHint = r.hint;
    } catch (e) {
      sideloadResult = String(e);
    } finally {
      sideloadBusy = null;
    }
  }

  /// Turn raw `adb install` failure output into a one-line summary. The full
  /// text still shows in the details line; this is the headline.
  function installFailureSummary(raw: string): string {
    const m = raw.match(/INSTALL_FAILED_[A-Z_]+|INSTALL_PARSE_FAILED[A-Z_]*/);
    if (m) {
      if (m[0].includes("ALREADY_EXISTS")) return "Already installed (same version).";
      if (m[0].includes("VERSION_DOWNGRADE")) return "A newer version is already installed.";
      if (m[0].includes("NO_MATCHING_ABIS")) return "Wrong CPU architecture for this device.";
      if (m[0].includes("OLDER_SDK")) return "Needs a newer Android version than this device.";
      return `Install failed (${m[0]}).`;
    }
    return "Install failed.";
  }

  function formatBytes(n: number): string {
    if (n < 1024) return `${n} B`;
    if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
    if (n < 1024 * 1024 * 1024) return `${(n / 1024 / 1024).toFixed(1)} MB`;
    return `${(n / 1024 / 1024 / 1024).toFixed(2)} GB`;
  }

  async function openDownloadPage(url: string) {
    try {
      await openUrl(url);
    } catch (e) {
      sideloadResult = `Open link failed: ${e}`;
    }
  }

  onMount(() => {
    const last = localStorage.getItem("shieldopt.lastApkFolder");
    if (last?.trim()) savedFolder = last;
  });
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-sideload" aria-labelledby="tab-sideload">
  <div class="card-header">
    <div class="header-title">
      <h2><Icon name="download" size={17} /> Install APK</h2>
      <p class="muted small mono header-sub">sideload to {deviceLabel || serial}</p>
    </div>
    <div class="header-actions">
      <button onclick={pickApkFolder} disabled={sideloadBusy !== null || discoveryBusy}>
        {discoveryBusy ? "Scanning…" : "Choose folder…"}
      </button>
      <button class="primary" onclick={pickAndInstallApk} disabled={sideloadBusy !== null || discoveryBusy}>
        {sideloadBusy !== null ? "Installing…" : "Pick file…"}
      </button>
    </div>
  </div>
  <p class="muted small">
    Pick a file directly, or point at a folder and we'll list every APK inside.
    Either way, install runs <code>adb install -r &lt;file&gt;</code>.
  </p>

  <div class="sideload-layout">
    <div class="sideload-main">
  {#if savedFolder}
    <!-- The board's watch-folder card: the path you scan, and the one control
         that changes it, on one line. -->
    <div class="saved-folder">
      <div class="saved-folder-head">
        <span class="folder-icon" aria-hidden="true"><Icon name="folder_open" size={18} /></span>
        <div class="saved-folder-path small">
          <strong>Watch folder</strong>
          <code>{savedFolder}</code>
        </div>
        <button
          class="small-action"
          onclick={scanSavedFolder}
          disabled={discoveryBusy || sideloadBusy !== null}
        >
          {discoveryBusy ? "Scanning…" : "Scan folder"}
        </button>
      </div>
      <p class="muted small">
        Scanning reads APK files in this folder and checks whether detected apps are installed on
        this TV. Scanning does not install apps.
      </p>
    </div>
  {/if}

  {#if scanError}
    <div class="install-result bad" role="alert"><span><Icon name="close" size={15} /> {scanError}</span></div>
  {/if}

  {#if discoveredFolder && discoveredApks.length > 0}
    <p class="rail-label">Discovered APKs · {discoveredApks.length}</p>
    <div class="apk-folder muted small mono">{discoveredFolder}</div>
    <ul class="apk-list">
      {#each discoveredApks as apk (apk.path)}
        <li>
          <div class="apk-row">
            <span class="apk-icon" aria-hidden="true"><Icon name="android" size={18} /></span>
            <div class="apk-meta">
              <div class="apk-name">{apk.name}</div>
              <div class="muted small">
                {formatBytes(apk.size_bytes)}
                {#if apk.package}
                  · {apk.package}
                  {#if apkInstallState[apk.package] === "enabled"}
                    <span class="tag installed">INSTALLED</span>
                  {:else if apkInstallState[apk.package] === "disabled"}
                    <span class="tag disabled">INSTALLED (disabled)</span>
                  {/if}
                {/if}
              </div>
            </div>
            <button
              class="small-action primary"
              onclick={() => installApkPath(apk.path)}
              disabled={sideloadBusy !== null}
            >
              {sideloadBusy === apk.path
                ? "Installing…"
                : apk.package && apkInstallState[apk.package]
                  ? "Reinstall"
                  : "Install"}
            </button>
          </div>
          {#if sideloadResultPath === apk.path && sideloadResult}
            <div class="install-result" class:ok={sideloadOk} class:bad={!sideloadOk}>
              <span><Icon name={sideloadOk ? "check" : "close"} size={15} /> {sideloadResult}</span>
              {#if sideloadHint}<span class="muted small"> — {sideloadHint}</span>{/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if discoveredFolder}
    <p class="muted small">No <code>.apk</code> files in the scanned folder: {discoveredFolder}.</p>
  {:else if !savedFolder}
    <div class="sideload-empty">
      <Icon name="download" size={28} />
      <strong>No folder scanned yet</strong>
      <span class="small">
        Choose folder… to list every APK inside it and keep it for next time, or
        Pick file… to install one straight away.
      </span>
    </div>
  {/if}

  {#if sideloadResult && !discoveredApks.some((a) => a.path === sideloadResultPath)}
    <div class="install-result" class:ok={sideloadOk} class:bad={!sideloadOk}>
      <span><Icon name={sideloadOk ? "check" : "close"} size={15} /> {sideloadResult}</span>
      {#if sideloadHint}<span class="muted small"> — {sideloadHint}</span>{/if}
    </div>
  {/if}

    </div>

    <!-- The board gives the catalog a permanent rail rather than a disclosure.
         It is the answer to "what do I even install", so it should not be a
         thing you have to know to open. -->
    <aside class="sideload-rail">
      <p class="rail-label">Suggested sideloads · {sideloadCatalog.length}</p>
      <ul class="catalog-list">
        {#each sideloadCatalog as entry (entry.package)}
          <li>
            <div class="catalog-text">
              <div class="apk-name">{entry.name}</div>
              <div class="muted small">{entry.description}</div>
              <div class="muted small mono catalog-pkg">{entry.package}</div>
            </div>
            {#if apkInstallState[entry.package] === "enabled"}
              <span class="tag installed">INSTALLED</span>
            {:else if apkInstallState[entry.package] === "disabled"}
              <span class="tag disabled">INSTALLED (disabled)</span>
            {/if}
            <button
              class="catalog-get"
              onclick={() => openDownloadPage(entry.url)}
              title={`Open the official download page — ${entry.url}`}
              aria-label={`Open the official download page for ${entry.name}`}
            ><Icon name="download" size={16} /></button>
          </li>
        {/each}
      </ul>
      <p class="muted small rail-note">
        Apps people commonly install that aren't on the Play Store. Links go to the
        official source only — download the APK there, then install it with the
        buttons on the left. You're sideloading third-party software; check it's the
        official release.
      </p>
    </aside>
  </div>
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button, input) live in the layout and are inherited. */
  .header-actions {
    display: flex;
    gap: 0.8rem;
    align-items: center;
  }
  .header-title {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .header-title h2 {
    margin: 0;
  }
  .header-sub {
    margin: 0;
  }
  /* What you have on the left, what you could have on the right — board 11.9.
     The catalog was a closed <details> at the bottom, which is the wrong place
     for the answer to "what do I even install". */
  .sideload-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 24rem);
    gap: 1.5rem;
    align-items: start;
    margin-top: 1rem;
  }
  .sideload-main,
  .sideload-rail {
    min-width: 0;
  }
  .rail-label {
    margin: 1rem 0 0.5rem;
    font-family: var(--mono);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-muted);
  }
  .sideload-rail .rail-label {
    margin-top: 0;
  }
  .rail-note {
    margin-top: 0.7rem;
  }
  .saved-folder-head {
    display: flex;
    align-items: center;
    gap: 0.7rem;
  }
  .folder-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 2.2rem;
    height: 2.2rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-inset);
    color: var(--fg-muted);
  }
  .saved-folder-head .saved-folder-path {
    flex: 1;
    min-width: 0;
  }
  .apk-icon {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    flex: none;
    width: 2.2rem;
    height: 2.2rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    background: var(--bg-inset);
    color: var(--ok);
  }
  .catalog-text {
    flex: 1;
    min-width: 0;
  }
  .catalog-pkg {
    overflow-wrap: anywhere;
  }
  /* A link out, not an install — the icon says "fetch it", and the title says
     where from, because sideloading somebody else's build deserves a name. */
  .catalog-get {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.3rem;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--accent);
    cursor: pointer;
  }
  .catalog-get:hover {
    border-color: var(--border);
    background: var(--bg-button-hover);
  }

  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
  .small-action {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
  }
  .tag {
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: var(--radius-sm);
    letter-spacing: 0.04em;
  }
  .tag.installed { background: var(--ok-surface); color: var(--ok); }
  .tag.disabled { background: var(--warn-surface-2); color: var(--warn); }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 0.85em;
  }

  /* Install-APK–specific styles. */
  .apk-folder {
    margin: 0.4rem 0;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    word-break: break-all;
  }
  .saved-folder {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 0.5rem 0.8rem;
    align-items: center;
    margin: 0.8rem 0;
    padding: 0.7rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
  }
  .saved-folder-path {
    display: flex;
    flex-direction: column;
    gap: 0.3rem;
    min-width: 0;
  }
  .saved-folder-path code {
    overflow-wrap: anywhere;
  }
  .saved-folder p {
    grid-column: 1 / -1;
    margin: 0;
  }
  .apk-list {
    list-style: none;
    padding: 0;
    margin: 0.4rem 0 0.8rem;
  }
  .apk-list li {
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--bg-button);
  }
  .apk-list li:last-child {
    border-bottom: none;
  }
  .apk-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 0.8rem;
  }
  .install-result {
    margin-top: 0.4rem;
    font-size: 0.88rem;
  }
  .install-result.ok { color: var(--ok); }
  .install-result.bad { color: var(--warn); }
  .apk-name {
    font-family: var(--mono);
    font-size: 0.88rem;
    word-break: break-all;
  }
  .catalog-text .apk-name {
    font-family: var(--sans);
    font-size: 0.92rem;
    font-weight: 600;
    word-break: normal;
  }
  /* Nothing scanned yet is a state, not a blank half-screen. */
  .sideload-empty {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.5rem;
    padding: 2.5rem 1.5rem;
    border: 1px dashed var(--border);
    border-radius: var(--radius-lg);
    text-align: center;
    color: var(--fg-muted);
  }
  .sideload-empty :global(.msr) {
    color: var(--fg-muted);
  }
  .sideload-empty strong {
    color: var(--fg-secondary);
    font-weight: 600;
  }
  .catalog-list {
    list-style: none;
    padding: 0;
    margin: 0.5rem 0 0;
  }
  .catalog-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.6rem 0;
    border-bottom: 1px solid var(--bg-button);
  }
  .catalog-list li button {
    white-space: nowrap;
    flex-shrink: 0;
  }
</style>
