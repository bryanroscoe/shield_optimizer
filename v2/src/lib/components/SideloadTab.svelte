<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { openUrl } from "@tauri-apps/plugin-opener";
  import { api } from "$lib/api";
  import sideloadCatalog from "$lib/sideload-catalog.json";
  import type { DiscoveredApk } from "$lib/types";

  let { serial }: { serial: string } = $props();

  /// Path of the APK currently installing (null when idle) — per-path so a
  /// multi-APK list only shows the spinner on the row actually installing.
  let sideloadBusy = $state<string | null>(null);
  let sideloadResult = $state<string>("");
  let sideloadHint = $state<string | null>(null);
  /// Path the current install result belongs to (so it renders under that
  /// row), whether it succeeded, and the raw adb output for the details line.
  let sideloadResultPath = $state<string | null>(null);
  let sideloadOk = $state(false);
  // Auto-discovered APK list — re-scanned whenever the user picks a folder
  // (or after a successful install in case files were added/removed).
  let discoveredApks = $state<DiscoveredApk[]>([]);
  let discoveredFolder = $state<string | null>(null);
  let discoveryBusy = $state(false);
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
      await scanApkFolder(folder);
    }
    await installApkPath(selected);
  }

  async function pickApkFolder() {
    const picked = await openDialog({ multiple: false, directory: true });
    if (!picked || Array.isArray(picked)) return;
    localStorage.setItem("shieldopt.lastApkFolder", picked);
    await scanApkFolder(picked);
  }

  async function scanApkFolder(folder: string) {
    discoveryBusy = true;
    try {
      discoveredApks = await api.listApksInFolder(folder);
      discoveredFolder = folder;
      const pkgs = discoveredApks.map((a) => a.package).filter((p): p is string => !!p);
      apkInstallState = pkgs.length ? await api.packageStates(serial, pkgs) : {};
    } catch (e) {
      sideloadResult = `Scan failed: ${e}`;
    } finally {
      discoveryBusy = false;
    }
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
    if (last) scanApkFolder(last);
  });
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-sideload" aria-labelledby="tab-sideload">
  <div class="card-header">
    <h2>Install APK</h2>
    <div class="header-actions">
      <button onclick={pickApkFolder} disabled={sideloadBusy !== null || discoveryBusy}>
        {discoveryBusy ? "Scanning…" : "Choose folder…"}
      </button>
      <button class="primary" onclick={pickAndInstallApk} disabled={sideloadBusy !== null}>
        {sideloadBusy !== null ? "Installing…" : "Pick file…"}
      </button>
    </div>
  </div>
  <p class="muted small">
    Pick a file directly, or point at a folder and we'll list every APK inside.
    Either way, install runs <code>adb install -r &lt;file&gt;</code>.
  </p>

  {#if discoveredFolder && discoveredApks.length > 0}
    <div class="apk-folder muted small mono">
      {discoveredFolder} — {discoveredApks.length} APK{discoveredApks.length === 1 ? "" : "s"} found
    </div>
    <ul class="apk-list">
      {#each discoveredApks as apk (apk.path)}
        <li>
          <div class="apk-row">
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
              {sideloadBusy === apk.path ? "Installing…" : "Install"}
            </button>
          </div>
          {#if sideloadResultPath === apk.path && sideloadResult}
            <div class="install-result" class:ok={sideloadOk} class:bad={!sideloadOk}>
              <span>{sideloadOk ? "✓" : "✕"} {sideloadResult}</span>
              {#if sideloadHint}<span class="muted small"> — {sideloadHint}</span>{/if}
            </div>
          {/if}
        </li>
      {/each}
    </ul>
  {:else if discoveredFolder}
    <p class="muted small">No <code>.apk</code> files in {discoveredFolder}.</p>
  {/if}

  {#if sideloadResult && !discoveredApks.some((a) => a.path === sideloadResultPath)}
    <div class="install-result" class:ok={sideloadOk} class:bad={!sideloadOk}>
      <span>{sideloadOk ? "✓" : "✕"} {sideloadResult}</span>
      {#if sideloadHint}<span class="muted small"> — {sideloadHint}</span>{/if}
    </div>
  {/if}

  <details class="sideload-catalog">
    <summary>Popular sideloads — common apps you download to install ({sideloadCatalog.length})</summary>
    <p class="muted small">
      Apps people commonly install that aren't on the Play Store. Links go to the
      official source only — download the APK there, then install it with the
      buttons above. You're sideloading third-party software; check it's the
      official release.
    </p>
    <ul class="catalog-list">
      {#each sideloadCatalog as entry (entry.package)}
        <li>
          <div>
            <div class="apk-name">{entry.name}</div>
            <div class="muted small">{entry.description}</div>
            <div class="muted small mono">{entry.package}</div>
          </div>
          <button
            class="small-action"
            onclick={() => openDownloadPage(entry.url)}
            title={entry.url}
          >
            Open download page
          </button>
        </li>
      {/each}
    </ul>
  </details>
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button, input) live in the layout and are inherited. */
  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.2rem;
  }
  .card h2 {
    margin: 0 0 0.8rem;
    font-size: 1.1rem;
  }
  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .header-actions {
    display: flex;
    gap: 0.8rem;
    align-items: center;
  }
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
  .tag {
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    letter-spacing: 0.04em;
  }
  .tag.installed { background: var(--ok-surface); color: var(--ok); }
  .tag.disabled { background: var(--warn-surface-2); color: var(--warn); }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
  }

  /* Install-APK–specific styles. */
  .apk-folder {
    margin: 0.4rem 0;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    word-break: break-all;
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
    font-family: ui-monospace, monospace;
    font-size: 0.88rem;
    word-break: break-all;
  }
  .sideload-catalog {
    margin-top: 1.5rem;
    padding-top: 1.2rem;
    border-top: 1px solid var(--border);
  }
  .sideload-catalog summary {
    cursor: pointer;
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
