<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import appFilesCatalog from "$lib/app-files-catalog.json";
  import type { Device, FileEntry } from "$lib/types";

  let { serial }: { serial: string } = $props();

  let filesPath = $state("/sdcard");
  let filesEntries = $state<FileEntry[] | null>(null);
  let filesLoading = $state(false);
  let filesErr = $state<string | null>(null);
  /// Power-user opt-in: browse the whole filesystem, not just /sdcard. Most
  /// system paths are permission-denied without root; deletes outside /sdcard
  /// get an extra confirmation and protected mounts are refused outright.
  let powerUserPaths = $state(false);
  let filesBusy = $state<string | null>(null); // entry name currently being acted on
  let filesMessage = $state("");
  /// package → found backup-file paths (null until that app was searched).
  let appFilesResults = $state<Record<string, string[] | null>>({});
  let appFilesBusy = $state<string | null>(null);
  /// File name the "copy to another device" picker is open for, plus targets.
  let fileCopyName = $state<string | null>(null);
  let fileCopyTargets = $state<Device[]>([]);
  let crumbs = $derived(
    filesPath
      .split("/")
      .filter(Boolean)
      .map((seg, i, all) => ({ label: seg, path: "/" + all.slice(0, i + 1).join("/") })),
  );

  async function loadFiles(path: string) {
    filesLoading = true;
    filesErr = null;
    filesMessage = "";
    try {
      filesEntries = await api.listDir(serial, path, powerUserPaths);
      filesPath = path;
    } catch (e) {
      filesErr = String(e);
    } finally {
      filesLoading = false;
    }
  }

  async function startFileCopy(name: string) {
    filesMessage = "";
    try {
      const all = await api.listDevices();
      fileCopyTargets = all.filter((d) => d.status === "device" && d.serial !== serial);
    } catch (e) {
      filesMessage = String(e);
      return;
    }
    if (fileCopyTargets.length === 0) {
      filesMessage = "No other connected device to copy to — connect the target device first.";
      return;
    }
    fileCopyName = name;
  }

  async function copyFileTo(target: Device) {
    if (!fileCopyName) return;
    const name = fileCopyName;
    filesBusy = name;
    // Land it in the same path on the target so a /sdcard/Download file
    // arrives in /sdcard/Download there too.
    filesMessage = `Copying ${name} to ${target.name}…`;
    try {
      const r = await api.copyFileToDevice(serial, `${filesPath}/${name}`, target.serial, filesPath);
      filesMessage = r.message;
      if (r.ok) fileCopyName = null;
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function downloadFile(name: string) {
    const folder = await openDialog({ directory: true, title: "Choose a download folder" });
    if (!folder) return;
    filesBusy = name;
    filesMessage = "";
    try {
      const r = await api.pullFile(serial, `${filesPath}/${name}`, folder as string, powerUserPaths);
      filesMessage = r.message;
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function uploadToCurrentDir() {
    const file = await openDialog({ title: "Choose a file to upload" });
    if (!file) return;
    filesBusy = "__upload__";
    filesMessage = "";
    try {
      const r = await api.pushFile(serial, file as string, filesPath, powerUserPaths);
      filesMessage = r.message;
      if (r.ok) await loadFiles(filesPath);
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function deleteEntry(entry: FileEntry) {
    const what = entry.is_dir ? "folder AND EVERYTHING IN IT" : "file";
    const outsideSdcard = !filesPath.startsWith("/sdcard");
    const prefix = outsideSdcard
      ? `⚠️ SYSTEM PATH (${filesPath}). Deleting here can break the device.\n\n`
      : "";
    if (!confirm(`${prefix}Delete ${what} "${entry.name}" from the device? This cannot be undone.`))
      return;
    filesBusy = entry.name;
    filesMessage = "";
    try {
      const r = await api.deletePath(serial, `${filesPath}/${entry.name}`, powerUserPaths);
      filesMessage = r.message;
      if (r.ok) await loadFiles(filesPath);
    } catch (e) {
      filesMessage = String(e);
    } finally {
      filesBusy = null;
    }
  }

  async function findAppFiles(entry: (typeof appFilesCatalog)[number]) {
    appFilesBusy = entry.package;
    filesMessage = "";
    try {
      appFilesResults[entry.package] = await api.findFiles(serial, entry.search_dirs, entry.pattern);
    } catch (e) {
      filesMessage = String(e);
    } finally {
      appFilesBusy = null;
    }
  }

  async function downloadFoundFile(path: string) {
    const folder = await openDialog({ directory: true, title: "Choose a folder for the backup" });
    if (!folder) return;
    appFilesBusy = path;
    filesMessage = "";
    try {
      const r = await api.pullFile(serial, path, folder as string);
      filesMessage = r.message;
    } catch (e) {
      filesMessage = String(e);
    } finally {
      appFilesBusy = null;
    }
  }

  function goToFolder(path: string) {
    // Parent dir. Below /sdcard normally; in power-user mode you can climb to
    // the filesystem root.
    const floor = powerUserPaths ? "/" : "/sdcard";
    const dir = path.slice(0, path.lastIndexOf("/")) || floor;
    loadFiles(dir);
  }

  function formatSize(bytes: number): string {
    if (bytes >= 1 << 30) return `${(bytes / (1 << 30)).toFixed(2)} GB`;
    if (bytes >= 1 << 20) return `${(bytes / (1 << 20)).toFixed(1)} MB`;
    if (bytes >= 1 << 10) return `${(bytes / (1 << 10)).toFixed(0)} KB`;
    return `${bytes} B`;
  }

  onMount(() => loadFiles(filesPath));
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-files" aria-labelledby="tab-files">
  <div class="card-header">
    <h2>Files</h2>
    <div class="header-actions">
      <button onclick={uploadToCurrentDir} disabled={filesBusy !== null} title="Upload a file from this computer into the current folder">
        {filesBusy === "__upload__" ? "Uploading…" : "Upload here"}
      </button>
      <button onclick={() => loadFiles(filesPath)} disabled={filesLoading}>
        {filesLoading ? "Loading…" : "Refresh"}
      </button>
    </div>
  </div>
  <p class="muted small files-intro">
    {#if powerUserPaths}
      Browsing the whole filesystem. Most system paths are read-only without root;
      deletes outside <code>/sdcard</code> are double-confirmed and critical mounts are refused.
    {:else}
      Browsing the device's user storage (<code>/sdcard</code>).
    {/if}
    <label class="inline-check">
      <input
        type="checkbox"
        checked={powerUserPaths}
        onchange={(e) => {
          powerUserPaths = e.currentTarget.checked;
          loadFiles(powerUserPaths ? filesPath : "/sdcard");
        }}
      />
      Show system paths (power user)
    </label>
  </p>

  <details class="app-backups">
    <summary>App file backups — find &amp; save exports (Projectivy theme, SmartTube settings, …)</summary>
    <p class="muted small">
      App settings live in protected storage, but most apps can export a backup to
      <code>/sdcard</code>. Export in the app first, then find the file here and save it to
      this computer. To restore later: browse to the folder below and use <strong>Upload here</strong>,
      then import it in the app.
    </p>
    {#each appFilesCatalog as entry (entry.package)}
      <div class="app-backup-row">
        <div>
          <div class="apk-name">{entry.name}</div>
          <div class="muted small">{entry.hint}</div>
        </div>
        <button
          class="small-action"
          onclick={() => findAppFiles(entry)}
          disabled={appFilesBusy !== null}
        >
          {appFilesBusy === entry.package ? "Searching…" : "Find backup files"}
        </button>
      </div>
      {#if appFilesResults[entry.package]}
        {@const found = appFilesResults[entry.package] ?? []}
        {#if found.length === 0}
          <p class="muted small found-list">No matches — export from the app first, then search again.</p>
        {:else}
          <ul class="found-list">
            {#each found as path (path)}
              <li>
                <span class="mono small">{path}</span>
                <span>
                  <button class="small-action" onclick={() => downloadFoundFile(path)} disabled={appFilesBusy !== null}>
                    Save to computer
                  </button>
                  <button class="small-action subtle" onclick={() => goToFolder(path)} disabled={appFilesBusy !== null}>
                    Go to folder
                  </button>
                </span>
              </li>
            {/each}
          </ul>
        {/if}
      {/if}
    {/each}
  </details>

  <nav class="crumbs" aria-label="Path">
    <button
      class="small-action subtle"
      onclick={() => goToFolder(filesPath)}
      disabled={filesPath === "/" || (filesPath === "/sdcard" && !powerUserPaths) || filesLoading}
      title="Up one level"
    >
      ↑ Up
    </button>
    {#each crumbs as c, i (c.path)}
      {#if i > 0}<span class="muted">/</span>{/if}
      {#if i === crumbs.length - 1}
        <span class="crumb-current">{c.label}</span>
      {:else}
        <button class="crumb" onclick={() => loadFiles(c.path)}>{c.label}</button>
      {/if}
    {/each}
  </nav>
  {#if filesMessage}
    <p class="muted small mono action-message">{filesMessage}</p>
  {/if}
  {#if fileCopyName}
    <div class="clone-panel">
      <span>Copy <code>{fileCopyName}</code> to:</span>
      {#each fileCopyTargets as t (t.serial)}
        <button class="small-action" onclick={() => copyFileTo(t)} disabled={filesBusy !== null}>
          {filesBusy !== null ? "Copying…" : `${t.name} (${t.serial})`}
        </button>
      {/each}
      <button class="small-action subtle" onclick={() => (fileCopyName = null)} disabled={filesBusy !== null}>
        Cancel
      </button>
    </div>
  {/if}
  {#if filesErr}
    <div class="error">{filesErr}</div>
  {:else if filesEntries === null}
    <div class="muted">{filesLoading ? "Loading…" : "—"}</div>
  {:else if filesEntries.length === 0}
    <p class="muted">Empty folder.</p>
  {:else}
    <table class="files-table">
      <thead>
        <tr><th>Name</th><th class="num">Size</th><th>Modified</th><th></th></tr>
      </thead>
      <tbody>
        {#each filesEntries as f (f.name)}
          <tr>
            <td class="file-name">
              {#if f.is_dir}
                <button class="dir-link" onclick={() => loadFiles(`${filesPath}/${f.name}`)}>
                  📁 {f.name}
                </button>
              {:else}
                <span>{f.is_symlink ? "🔗" : "📄"} {f.name}</span>
              {/if}
            </td>
            <td class="num muted">{f.is_dir ? "—" : formatSize(f.size_bytes)}</td>
            <td class="muted small">{f.modified}</td>
            <td class="row-actions">
              {#if !f.is_dir && !f.is_symlink}
                <button
                  class="small-action"
                  onclick={() => downloadFile(f.name)}
                  disabled={filesBusy !== null}
                  title="Save this file to a folder on this computer"
                >
                  {filesBusy === f.name ? "…" : "Download"}
                </button>
                <button
                  class="small-action subtle"
                  onclick={() => startFileCopy(f.name)}
                  disabled={filesBusy !== null}
                  title="Copy this file to another connected device"
                >
                  Copy to…
                </button>
              {/if}
              <button
                class="small-action subtle danger"
                onclick={() => deleteEntry(f)}
                disabled={filesBusy !== null}
                title="Delete from the device{f.is_dir ? ' (recursive!)' : ''}"
              >
                Delete
              </button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  /* --- Shared scoped utilities, duplicated from the page (see CLAUDE.md note
         on component CSS). Global rules (.muted, button, input) live in the
         layout and are inherited. --- */
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
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  th, td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--bg-button);
    vertical-align: middle;
  }
  th {
    color: var(--fg-muted);
    font-weight: 500;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  td.num {
    font-family: ui-monospace, monospace;
    text-align: right;
    width: 100px;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  .row-actions {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
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
    border-color: var(--danger-strong);
  }
  .small-action.subtle {
    background: transparent;
    border-color: var(--border);
    color: var(--fg-muted);
  }
  .small-action.subtle:hover:not(:disabled) {
    background: var(--bg-button);
    color: var(--fg-secondary);
  }
  .action-message {
    margin-top: 0.4rem;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    word-break: break-word;
  }
  .clone-panel {
    display: flex;
    align-items: center;
    gap: 0.6rem;
    flex-wrap: wrap;
    margin: 0.4rem 0;
    padding: 0.5rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.9rem;
  }
  .inline-check {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.9rem;
    white-space: nowrap;
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
  }

  /* --- Files-tab–specific styles. --- */
  .app-backups {
    margin: 0.6rem 0 1rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 6px;
  }
  .app-backups summary {
    cursor: pointer;
    font-weight: 600;
  }
  .app-backup-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
    padding: 0.5rem 0;
    border-top: 1px solid var(--bg-button);
    margin-top: 0.5rem;
  }
  .apk-name {
    font-family: ui-monospace, monospace;
    font-size: 0.88rem;
    word-break: break-all;
  }
  .found-list {
    list-style: none;
    padding: 0 0 0 0.8rem;
    margin: 0.2rem 0 0.6rem;
  }
  .found-list li {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.8rem;
    padding: 0.25rem 0;
  }
  .crumbs {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin: 0.4rem 0 0.8rem;
    flex-wrap: wrap;
  }
  .crumb {
    background: none;
    border: none;
    padding: 0.1rem 0.2rem;
    color: var(--accent);
    cursor: pointer;
    font-size: 0.95rem;
  }
  .crumb:hover { text-decoration: underline; }
  .crumb-current { font-weight: 600; }
  .files-table {
    width: 100%;
    border-collapse: collapse;
  }
  .files-table th, .files-table td {
    text-align: left;
    padding: 0.4rem 0.6rem;
    border-bottom: 1px solid var(--bg-button);
  }
  .files-table .num { text-align: right; white-space: nowrap; }
  .files-table .row-actions { text-align: right; white-space: nowrap; }
  .dir-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--fg);
    cursor: pointer;
    font-size: 0.95rem;
  }
  .dir-link:hover { color: var(--accent); }
</style>
