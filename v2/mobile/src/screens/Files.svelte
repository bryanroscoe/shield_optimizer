<script lang="ts">
  import { onDestroy, onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { FileEntry, FindResult, PulledFile } from "../lib/types";
  import {
    NO_MATCHES_MESSAGE,
    appFilesCatalog,
    baseName,
    canDeleteOnTv,
    fileConnectionMatches,
    parentDir,
    summarizeFind,
    unsearchedMessage,
    type AppFilesEntry,
    type FileConnection,
  } from "../lib/appFiles";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import Toast from "../components/Toast.svelte";

  let { back }: { back: () => void } = $props();

  // Start at the TV's user storage. Both paths resolve to the same place on
  // Android TV; /sdcard is the friendlier one to show.
  const ROOT = "/sdcard";

  let path = $state(ROOT);
  // The directory `entries` actually came from. Pull targets are built from
  // this, never from `path` — a slow listing must not hand a newer directory's
  // name to an older row.
  let loadedPath = $state(ROOT);
  let loading = $state(true);
  let error = $state("");
  let entries = $state<FileEntry[]>([]);
  let loadGeneration = 0;
  let watchedSerial = session.serial;
  let watchedGeneration = session.generation;
  let watchedLive = session.isConnected;
  let destroyed = false;

  function current(target: FileConnection) {
    return !destroyed && fileConnectionMatches(target, session);
  }

  onDestroy(() => {
    destroyed = true;
    ++loadGeneration;
    clearTimeout(toastTimer);
  });

  // Files pulled this session, newest first, plus the set of remote paths we've
  // pulled so rows can show a check.
  let pulled = $state<PulledFile[]>([]);
  let pulledRemotes = $state<Set<string>>(new Set());
  let busyName = $state("");

  // App-files catalog (#86): per-entry search results for the current TV, and
  // which entry or found path is busy.
  let appResults = $state<Record<string, FindResult>>({});
  let appBusy = $state("");

  // A delete waiting on the confirmation dialog. Carries its own serial so a
  // TV switch while the dialog is open can't redirect it.
  let pendingDelete = $state<(FileConnection & { path: string; name: string; isDir: boolean }) | null>(
    null,
  );
  let deleting = $state(false);
  let showCatalog = $state(false);

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3600);
  }

  async function load(target: string) {
    const serial = session.serial;
    const connection = { serial, generation: session.generation };
    const generation = ++loadGeneration;
    path = target;
    if (!serial || !session.isConnected) {
      error = "No TV connected. Go back and connect first.";
      entries = [];
      loading = false;
      return;
    }
    loading = true;
    error = "";
    try {
      const rows = await api.listRemoteDir(serial, target);
      if (generation !== loadGeneration || !current(connection)) return;
      entries = rows;
      loadedPath = target;
    } catch (e) {
      if (generation !== loadGeneration || !current(connection)) return;
      error = String(e);
      entries = [];
    } finally {
      if (generation === loadGeneration && current(connection)) loading = false;
    }
  }

  onMount(() => load(ROOT));

  // A TV switch invalidates everything on this screen: the listing, the
  // breadcrumb and the pulled-this-session markers all belonged to the old TV.
  $effect(() => {
    const serial = session.serial;
    const generation = session.generation;
    const live = session.isConnected;
    if (serial === watchedSerial && generation === watchedGeneration && live === watchedLive) return;
    watchedSerial = serial;
    watchedGeneration = generation;
    watchedLive = live;
    loadedPath = ROOT;
    entries = [];
    pulled = [];
    pulledRemotes = new Set();
    busyName = "";
    appResults = {};
    appBusy = "";
    pendingDelete = null;
    deleting = false;
    void load(ROOT);
  });

  function joinPath(dir: string, name: string): string {
    return dir === "/" ? `/${name}` : `${dir}/${name}`;
  }

  function openDir(name: string) {
    void load(joinPath(loadedPath, name));
  }

  const segments = $derived(
    path === "/" ? [] : path.split("/").filter((s) => s.length > 0),
  );

  function crumbPath(index: number): string {
    return "/" + segments.slice(0, index + 1).join("/");
  }

  function goTo(p: string) {
    const target = p || "/";
    if (target === path) return;
    void load(target);
  }

  function remotePathOf(e: FileEntry): string {
    return joinPath(loadedPath, e.name);
  }

  async function pull(e: FileEntry) {
    if (busyName || e.is_dir || !session.isConnected) return;
    const target = { serial: session.serial, generation: session.generation };
    const remote = remotePathOf(e);
    busyName = e.name;
    try {
      const file = await api.pullFile(target.serial, remote);
      if (!current(target)) return;
      recordPulled(file, remote);
      showToast(`Copied ${file.name} into this app's storage.`, "success");
    } catch (err) {
      if (current(target)) showToast(String(err), "error");
    } finally {
      if (current(target)) busyName = "";
    }
  }

  function recordPulled(file: PulledFile, remote: string) {
    pulled = [file, ...pulled.filter((f) => f.path !== file.path)];
    pulledRemotes = new Set([...pulledRemotes, remote]);
  }

  async function findAppFiles(entry: AppFilesEntry) {
    const serial = session.serial;
    const target = { serial, generation: session.generation };
    if (appBusy || !serial || !session.isConnected) return;
    appBusy = entry.id;
    try {
      const result = await api.findFiles(serial, entry.search_dirs, entry.pattern);
      if (!current(target)) return;
      appResults = { ...appResults, [entry.id]: result };
    } catch (err) {
      if (!current(target)) return;
      // A refused request (or a lost connection) is not a search result — drop
      // any older answer rather than leave it looking current.
      const { [entry.id]: _dropped, ...rest } = appResults;
      appResults = rest;
      showToast(String(err), "error");
    } finally {
      if (current(target)) appBusy = "";
    }
  }

  async function pullFound(remote: string) {
    const serial = session.serial;
    const target = { serial, generation: session.generation };
    if (appBusy || busyName || !serial || !session.isConnected) return;
    appBusy = remote;
    try {
      const file = await api.pullFile(serial, remote);
      if (!current(target)) return;
      recordPulled(file, remote);
      showToast(`Copied ${file.name} into this app's storage.`, "success");
    } catch (err) {
      if (current(target)) showToast(String(err), "error");
    } finally {
      if (current(target)) appBusy = "";
    }
  }

  function askDelete(remote: string, isDir: boolean) {
    const serial = session.serial;
    if (!serial || !session.isConnected || deleting || !canDeleteOnTv(remote)) return;
    pendingDelete = { serial, generation: session.generation, path: remote, name: baseName(remote), isDir };
  }

  async function confirmDelete() {
    const target = pendingDelete;
    pendingDelete = null;
    if (!target || deleting) return;
    if (!current(target)) {
      showToast("The TV changed before the delete was confirmed. Nothing was deleted.", "error");
      return;
    }
    deleting = true;
    busyName = target.name;
    try {
      const r = await api.deletePath(target.serial, target.path);
      if (!current(target)) return;
      showToast(r.message, r.ok ? "success" : "error");
      // Found-file lists may now point at a file that is gone.
      appResults = {};
      if (r.ok) await load(loadedPath);
    } catch (err) {
      if (current(target)) showToast(String(err), "error");
    } finally {
      if (current(target)) {
        deleting = false;
        busyName = "";
      }
    }
  }

  function iconFor(e: FileEntry): string {
    if (e.is_dir) return "folder";
    const n = e.name.toLowerCase();
    if (n.endsWith(".apk")) return "android";
    if (/\.(png|jpg|jpeg|gif|webp|bmp)$/.test(n)) return "image";
    if (/\.(mp4|mkv|avi|mov|webm)$/.test(n)) return "movie";
    if (/\.(mp3|flac|wav|ogg|m4a)$/.test(n)) return "music_note";
    if (/\.(xml|json|txt|log|conf|cfg|ini)$/.test(n)) return "description";
    if (/\.(zip|tar|gz|7z|rar)$/.test(n)) return "folder_zip";
    return "draft";
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
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Files</h3>
    <span style="width:44px"></span>
  </div>

  <!-- From → To -->
  <div class="fromto">
    <div class="ft-box">
      <span class="ft-label">From</span>
      <span class="ft-value">{session.deviceLabel}</span>
    </div>
    <span class="msr ft-arrow">arrow_forward</span>
    <div class="ft-box to">
      <span class="ft-label">To</span>
      <span class="ft-value">App storage</span>
    </div>
  </div>

  <!-- App-files catalog (#86) -->
  <div class="catalog">
    <button
      class="catalog-toggle"
      aria-expanded={showCatalog}
      onclick={() => (showCatalog = !showCatalog)}
    >
      <span class="msr">backup</span>
      <span class="catalog-title">Find app backups</span>
      <span class="msr">{showCatalog ? "expand_less" : "expand_more"}</span>
    </button>
    {#if showCatalog}
      <p class="catalog-lede">
        Most apps can export their settings to the TV's storage. Export in the app first, then
        find the file here and copy it to this phone.
      </p>
      {#each appFilesCatalog as entry (entry.id)}
        {@const result = appResults[entry.id]}
        {@const summary = result ? summarizeFind(result) : null}
        <div class="app-entry">
          <div class="app-head">
            <div class="f-body">
              <span class="f-name">{entry.name}</span>
              <span class="app-hint">{entry.hint}</span>
            </div>
            <button
              class="ghost small"
              aria-label={`Find ${entry.name} backups`}
              disabled={appBusy !== "" || !session.serial}
              onclick={() => findAppFiles(entry)}
            >
              <span class="msr">search</span>{appBusy === entry.id ? "Searching…" : "Find"}
            </button>
          </div>
          {#if summary}
            {#if summary.kind === "unsearched" || (summary.kind === "found" && summary.unsearched.length > 0)}
              <!-- The search never ran against these, so "no matches" would be a
                   claim we cannot make. -->
              <p class="app-note warn" role="alert">
                <span class="msr">warning</span>{unsearchedMessage(summary.unsearched)}
              </p>
            {/if}
            {#if summary.kind === "none"}
              <p class="app-note">{NO_MATCHES_MESSAGE}</p>
            {:else if summary.kind === "found"}
              <div class="found-list">
                {#each summary.hits as hit (hit)}
                  <div class="found-row">
                    <span class="found-path mono">{hit}</span>
                    <div class="found-actions">
                      <button
                        class="ghost small"
                        aria-label={`Copy ${baseName(hit)} to this phone`}
                        disabled={appBusy !== "" || busyName !== ""}
                        onclick={() => pullFound(hit)}
                      >
                        {#if appBusy === hit}
                          <span class="pdot blink"></span>Copying…
                        {:else if pulledRemotes.has(hit)}
                          <span class="msr fill">check_circle</span>Copied
                        {:else}
                          <span class="msr">download</span>Copy to phone
                        {/if}
                      </button>
                      <button
                        class="ghost small"
                        disabled={appBusy !== ""}
                        onclick={() => void load(parentDir(hit))}
                      >
                        <span class="msr">folder_open</span>Open folder
                      </button>
                    </div>
                  </div>
                {/each}
              </div>
            {/if}
          {/if}
        </div>
      {/each}
    {/if}
  </div>

  <!-- Breadcrumb -->
  <div class="crumbs mono">
    <button class="crumb" class:active={segments.length === 0} onclick={() => goTo("/")}>
      <span class="msr">folder_open</span>
    </button>
    {#each segments as seg, i (i)}
      <span class="sep">/</span>
      <button class="crumb" class:active={i === segments.length - 1} onclick={() => goTo(crumbPath(i))}>
        {seg}
      </button>
    {/each}
  </div>

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Reading folder…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={() => load(path)}>Retry</button>
    <div class="spacer"></div>
  {:else}
    {#if entries.length === 0}
      <p class="lede empty">This folder is empty.</p>
    {:else}
      <div class="file-list">
        {#each entries as e (e.name)}
          {@const remote = remotePathOf(e)}
          {@const done = pulledRemotes.has(remote)}
          {@const busy = busyName === e.name}
          {@const deletable = canDeleteOnTv(remote)}
          <div class="row-wrap">
            {#if e.is_dir}
              <button class="file-row" onclick={() => openDir(e.name)}>
                <span class="msr f-icon dir">{iconFor(e)}</span>
                <div class="f-body">
                  <span class="f-name">{e.name}</span>
                  <span class="f-sub mono">folder</span>
                </div>
                <span class="msr f-chevron">chevron_right</span>
              </button>
            {:else}
              <button class="file-row" class:done disabled={busyName !== ""} onclick={() => pull(e)}>
                <span class="msr f-icon">{iconFor(e)}</span>
                <div class="f-body">
                  <span class="f-name">{e.name}</span>
                  <span class="f-sub mono">{fmtSize(e.size_bytes)}</span>
                </div>
                {#if busy}
                  <span class="pdot blink"></span>
                {:else if done}
                  <span class="msr f-done fill">check_circle</span>
                {:else}
                  <span class="msr f-dl">download</span>
                {/if}
              </button>
            {/if}
            {#if deletable}
              <button
                class="iconbtn row-delete"
                aria-label={`Delete ${e.name} from the TV`}
                disabled={busyName !== "" || deleting}
                onclick={() => askDelete(remote, e.is_dir)}
              >
                <span class="msr">delete</span>
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}

    <div class="callout accent">
      <span class="msr">bolt</span>
      <span class="callout-text">
        Files transfer over your LAN — no cloud round-trip. Tap a file to copy it into ATV
        Optimizer's private storage on this phone (not yet exportable to Downloads or other apps).
        Items under /sdcard can be deleted from the TV with the bin icon.
      </span>
    </div>

    {#if pulled.length > 0}
      <span class="section-label">Copied this session</span>
      <div class="dl-list">
        {#each pulled as f (f.path)}
          <div class="dl-row">
            <span class="msr dl-icon fill">check_circle</span>
            <div class="f-body">
              <span class="f-name">{f.name}</span>
              <span class="f-sub mono">{fmtSize(f.size_bytes)} · {f.path}</span>
            </div>
          </div>
        {/each}
      </div>
    {/if}

    <div class="spacer"></div>
  {/if}

  <ConfirmDialog
    open={pendingDelete !== null}
    title={pendingDelete?.isDir ? "Delete folder from the TV?" : "Delete file from the TV?"}
    message={pendingDelete
      ? pendingDelete.isDir
        ? `"${pendingDelete.name}" and everything in it will be deleted from ${pendingDelete.path}.`
        : `"${pendingDelete.name}" will be deleted from ${pendingDelete.path}.`
      : ""}
    warning="This cannot be undone. Copies already on this phone are kept."
    confirmLabel="Delete"
    danger
    icon="delete"
    onConfirm={confirmDelete}
    onCancel={() => (pendingDelete = null)}
  />

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

  .fromto {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 12px;
  }
  .ft-box {
    flex: 1;
    min-width: 0;
    padding: 11px 13px;
    border-radius: 13px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .ft-box.to {
    background: color-mix(in srgb, var(--accent) 9%, var(--surface));
    border-color: color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .ft-label {
    font-size: 10px;
    color: var(--dim);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .ft-value {
    font-size: 13px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .ft-arrow {
    font-size: 22px;
    color: var(--accent);
    flex: none;
  }

  .crumbs {
    display: flex;
    align-items: center;
    gap: 4px;
    flex-wrap: wrap;
    margin-bottom: 12px;
    font-size: 12px;
  }
  .crumb {
    display: inline-flex;
    align-items: center;
    min-height: 44px;
    min-width: 44px;
    justify-content: center;
    background: transparent;
    border: none;
    color: var(--muted);
    font-family: var(--mono);
    font-size: 12px;
    padding: 0 8px;
    border-radius: 10px;
    cursor: pointer;
  }
  .crumb:active {
    background: var(--surface-2);
  }
  .crumb.active {
    color: var(--accent);
    font-weight: 600;
  }
  .crumb .msr {
    font-size: 16px;
    vertical-align: middle;
  }
  .sep {
    color: var(--dim);
  }

  .file-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .file-row {
    display: flex;
    align-items: center;
    gap: 12px;
    width: 100%;
    text-align: left;
    padding: 12px 13px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    color: var(--text);
    font-family: var(--sans);
    cursor: pointer;
  }
  .file-row:active {
    background: var(--surface-2);
  }
  .file-row.done {
    background: color-mix(in srgb, var(--accent) 8%, var(--surface));
    border-color: color-mix(in srgb, var(--accent) 28%, transparent);
  }
  .file-row:disabled {
    cursor: default;
  }
  .f-icon {
    font-size: 24px;
    color: var(--muted);
    flex: none;
  }
  .f-icon.dir {
    color: var(--text-soft);
  }
  .f-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .f-name {
    font-size: 14px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .f-sub {
    font-size: 10px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .f-chevron {
    font-size: 20px;
    color: var(--dim);
    flex: none;
  }
  .f-dl {
    font-size: 20px;
    color: var(--accent);
    flex: none;
  }
  .f-done {
    font-size: 22px;
    color: var(--accent);
    flex: none;
  }

  .callout.accent {
    display: flex;
    align-items: center;
    gap: 11px;
    margin-top: 14px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .callout.accent .msr {
    color: var(--accent);
    font-size: 20px;
    flex: none;
  }
  .callout-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
  }

  .catalog {
    margin-bottom: 12px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
    padding: 4px 13px;
  }
  .catalog-toggle {
    display: flex;
    align-items: center;
    gap: 10px;
    width: 100%;
    min-height: 44px;
    background: transparent;
    border: none;
    color: var(--text);
    font-family: var(--sans);
    padding: 0;
    cursor: pointer;
  }
  .catalog-toggle .msr {
    font-size: 20px;
    color: var(--accent);
  }
  .catalog-title {
    flex: 1;
    text-align: left;
    font-size: 14px;
    font-weight: 600;
  }
  .catalog-lede {
    margin: 0 0 10px;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
  }
  .app-entry {
    border-top: 1px solid var(--line);
    padding: 10px 0;
  }
  .app-head {
    display: flex;
    align-items: flex-start;
    gap: 10px;
  }
  .app-head .ghost {
    flex: none;
  }
  .app-hint {
    font-size: 11px;
    line-height: 1.4;
    color: var(--muted);
  }
  .app-note {
    margin: 8px 0 0;
    font-size: 12px;
    line-height: 1.4;
    color: var(--text-soft);
    display: flex;
    gap: 6px;
    align-items: flex-start;
  }
  .app-note.warn .msr {
    font-size: 16px;
    color: var(--amber);
    flex: none;
  }
  .found-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    margin-top: 8px;
  }
  .found-row {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .found-path {
    font-size: 11px;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .found-actions {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }

  .row-wrap {
    display: flex;
    align-items: center;
    gap: 6px;
  }
  .row-wrap .file-row {
    flex: 1;
    min-width: 0;
  }
  .row-delete {
    flex: none;
  }
  .row-delete .msr {
    color: var(--muted);
  }

  .dl-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .dl-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px 13px;
    border-radius: 14px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .dl-icon {
    font-size: 22px;
    color: var(--teal);
    flex: none;
  }
</style>
