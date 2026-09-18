<script lang="ts">
  import { onMount } from "svelte";
  import { open as openDialog } from "@tauri-apps/plugin-dialog";
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import type { IconName } from "$lib/icons";
  import appFilesCatalog from "$lib/app-files-catalog.json";
  import type { Device, FileEntry, FindResult } from "$lib/types";

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
  /// catalog id → search result (null until that app was searched). Keyed by
  /// the catalog's own id, not the package name, so the rows stay stable if a
  /// package id is corrected.
  let appFilesResults = $state<Record<string, FindResult | null>>({});
  let appFilesBusy = $state<string | null>(null);
  let filesRequest = 0;
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
    const request = ++filesRequest;
    const allowSystemPaths = powerUserPaths;
    filesLoading = true;
    filesErr = null;
    filesMessage = "";
    try {
      const entries = await api.listDir(serial, path, allowSystemPaths);
      if (request !== filesRequest) return;
      filesEntries = entries;
      filesPath = path;
    } catch (e) {
      if (request !== filesRequest) return;
      filesErr = String(e);
    } finally {
      if (request === filesRequest) filesLoading = false;
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
    appFilesBusy = entry.id;
    filesMessage = "";
    try {
      appFilesResults[entry.id] = await api.findFiles(serial, entry.search_dirs, entry.pattern);
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

  /// Board 11.8 gives each row a glyph for what the file is. Extension-based,
  /// because that is all `ls` tells us — an unrecognised extension falls back
  /// to the generic document rather than guessing.
  function fileIcon(name: string): IconName {
    const ext = name.slice(name.lastIndexOf(".") + 1).toLowerCase();
    if (ext === "apk") return "android";
    if (["png", "jpg", "jpeg", "webp", "gif", "bmp"].includes(ext)) return "image";
    if (["txt", "log"].includes(ext)) return "terminal";
    return "description";
  }
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-files" aria-labelledby="tab-files">
  <div class="card-header">
    <div class="header-title">
      <h2><Icon name="folder" size={17} /> Files</h2>
      <p class="muted small mono header-sub">
        {#if filesEntries}
          {filesEntries.length} item{filesEntries.length === 1 ? "" : "s"} · {formatSize(
            filesEntries.reduce((n, f) => n + (f.is_dir ? 0 : (f.size_bytes ?? 0)), 0),
          )} in this folder
        {:else}
          browsing the device
        {/if}
      </p>
    </div>
    <div class="header-actions">
      <button class="primary" onclick={uploadToCurrentDir} disabled={filesBusy !== null} title="Upload a file from this computer into the current folder">
        <Icon name="upload" size={15} /> {filesBusy === "__upload__" ? "Uploading…" : "Upload here"}
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
    {#each appFilesCatalog as entry (entry.id)}
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
          {appFilesBusy === entry.id ? "Searching…" : "Find backup files"}
        </button>
      </div>
      {#if appFilesResults[entry.id]}
        {@const result = appFilesResults[entry.id]}
        {@const found = result?.hits ?? []}
        {@const unsearched = result?.unsearched ?? []}
        {#if unsearched.length > 0}
          <!-- The search never ran against these, so "no matches" would be a
               claim we cannot make. Say what actually happened instead. -->
          <p class="muted small found-list">
            Couldn't search {unsearched.join(", ")} — the TV didn't answer. Check the
            connection and try again.
          </p>
        {/if}
        {#if found.length === 0 && unsearched.length === 0}
          <p class="muted small found-list">No matches — export from the app first, then search again.</p>
        {:else if found.length > 0}
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
      <Icon name="arrow_upward" size={15} /> Up
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
        <tr><th>Name</th><th class="num">Size</th><th class="num">Modified</th><th class="num">Actions</th></tr>
      </thead>
      <tbody>
        {#each filesEntries as f (f.name)}
          <tr>
            <td class="file-name">
              {#if f.is_dir}
                <button class="dir-link" onclick={() => loadFiles(`${filesPath}/${f.name}`)}>
                  <Icon name="folder" size={16} fill /> {f.name}
                </button>
              {:else}
                <span>
                  <Icon name={f.is_symlink ? "link" : fileIcon(f.name)} size={16} />
                  {f.name}
                </span>
              {/if}
            </td>
            <td class="num muted mono">{f.is_dir ? "—" : formatSize(f.size_bytes)}</td>
            <td class="num muted small mono">{f.modified}</td>
            <!-- Icon-only, per the board: three verbs spelled out on every row
                 crowded out the names they belonged to. Each keeps its title
                 and gains an aria-label, and the legend is under the table. -->
            <td class="row-actions">
              {#if f.is_dir}
                <button
                  class="file-tool"
                  onclick={() => loadFiles(`${filesPath}/${f.name}`)}
                  title="Open {f.name}"
                  data-tip="Open folder"
                  aria-label={`Open the folder ${f.name}`}
                ><Icon name="chevron_right" size={16} /></button>
              {:else if !f.is_symlink}
                <button
                  class="file-tool"
                  onclick={() => downloadFile(f.name)}
                  disabled={filesBusy !== null}
                  title="Save this file to a folder on this computer"
                  data-tip="Download"
                  aria-label={`Download ${f.name}`}
                ><Icon name="download" size={16} /></button>
                <button
                  class="file-tool"
                  onclick={() => startFileCopy(f.name)}
                  disabled={filesBusy !== null}
                  title="Copy this file to another connected device"
                  data-tip="Copy to device"
                  aria-label={`Copy ${f.name} to another device`}
                ><Icon name="swap_horiz" size={16} /></button>
              {/if}
              <button
                class="file-tool danger"
                onclick={() => deleteEntry(f)}
                disabled={filesBusy !== null}
                title="Delete from the device{f.is_dir ? ' (recursive!)' : ''}"
                  data-tip="Delete from TV" data-tip-align="end"
                aria-label={`Delete ${f.name} from the device`}
              ><Icon name="delete" size={16} /></button>
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
    <p class="tool-legend">
      <span><Icon name="download" size={14} /> download</span>
      <span><Icon name="swap_horiz" size={14} /> copy to device</span>
      <span><Icon name="delete" size={14} /> delete from TV</span>
    </p>
  {/if}
</div>

<style>
  /* --- Shared scoped utilities, duplicated from the page (see CLAUDE.md note
         on component CSS). Global rules (.muted, button, input) live in the
         layout and are inherited. --- */
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
    border-bottom: 1px solid var(--border);
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
    font-family: var(--mono);
    text-align: right;
    width: 100px;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-family: var(--mono);
    font-size: 0.85rem;
  }
  .files-table .row-actions {
    display: flex;
    justify-content: flex-end;
    gap: 0.2rem;
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
  /* Icon-only row tools. Borderless until hovered, so a long listing is a list
     of files rather than a wall of buttons. */
  .file-tool {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.3rem;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .file-tool:hover:not(:disabled) {
    border-color: var(--border);
    background: var(--bg-button-hover);
    color: var(--fg-primary);
  }
  .file-tool.danger {
    color: var(--danger);
  }
  .file-tool.danger:hover:not(:disabled) {
    border-color: var(--danger);
    background: var(--danger-surface);
    color: var(--danger-surface-text);
  }
  .tool-legend {
    display: flex;
    justify-content: flex-end;
    gap: 1rem;
    margin-top: 0.5rem;
    font-family: var(--mono);
    font-size: 0.72rem;
    color: var(--fg-muted);
  }
  .tool-legend span {
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
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
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-sm);
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
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 0.85em;
  }

  /* --- Files-tab–specific styles. --- */
  .app-backups {
    margin: 0.6rem 0 1rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
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
    font-family: var(--mono);
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
    border-bottom: 1px solid var(--border);
  }
  .files-table .num { text-align: right; white-space: nowrap; }
  .files-table .row-actions { text-align: right; white-space: nowrap; }
  /* Icon + name share a baseline row; the icon is the file-type cue so it
     takes the muted colour until the row is hovered. */
  .dir-link,
  .file-name > span {
    display: inline-flex;
    align-items: center;
    gap: 0.4rem;
  }
  .dir-link {
    background: none;
    border: none;
    padding: 0;
    color: var(--fg-primary);
    cursor: pointer;
    font-size: 0.95rem;
  }
  .dir-link:hover { color: var(--accent); }
</style>
