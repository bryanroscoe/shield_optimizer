<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { FileEntry, PulledFile } from "../lib/types";
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

  // Files pulled this session, newest first, plus the set of remote paths we've
  // pulled so rows can show a check.
  let pulled = $state<PulledFile[]>([]);
  let pulledRemotes = $state<Set<string>>(new Set());
  let busyName = $state("");

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
    const generation = ++loadGeneration;
    path = target;
    if (!serial) {
      error = "No TV connected. Go back and connect first.";
      entries = [];
      loading = false;
      return;
    }
    loading = true;
    error = "";
    try {
      const rows = await api.listRemoteDir(serial, target);
      if (generation !== loadGeneration || serial !== session.serial) return;
      entries = rows;
      loadedPath = target;
    } catch (e) {
      if (generation !== loadGeneration || serial !== session.serial) return;
      error = String(e);
      entries = [];
    } finally {
      if (generation === loadGeneration && serial === session.serial) loading = false;
    }
  }

  onMount(() => load(ROOT));

  // A TV switch invalidates everything on this screen: the listing, the
  // breadcrumb and the pulled-this-session markers all belonged to the old TV.
  $effect(() => {
    const serial = session.serial;
    if (serial === watchedSerial) return;
    watchedSerial = serial;
    loadedPath = ROOT;
    entries = [];
    pulled = [];
    pulledRemotes = new Set();
    busyName = "";
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
    if (busyName || e.is_dir || !session.serial) return;
    const remote = remotePathOf(e);
    busyName = e.name;
    try {
      const file = await api.pullFile(session.serial, remote);
      pulled = [file, ...pulled.filter((f) => f.path !== file.path)];
      pulledRemotes = new Set([...pulledRemotes, remote]);
      showToast(`Copied ${file.name} into this app's storage.`, "success");
    } catch (err) {
      showToast(String(err), "error");
    } finally {
      busyName = "";
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
        {/each}
      </div>
    {/if}

    <div class="callout accent">
      <span class="msr">bolt</span>
      <span class="callout-text">
        Files transfer over your LAN — no cloud round-trip. Tap a file to copy it into ATV
        Optimizer's private storage on this phone (not yet exportable to Downloads or other apps).
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
