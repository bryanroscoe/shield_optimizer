<script lang="ts">
  // Install APK: pick .apk files on this phone (one app, or every part of a split app), read
  // what their native code targets, and install them on the connected TV after a confirm. The
  // picked copies live in the app only until the install ends or the user cancels.
  import HelpSheet from "../components/HelpSheet.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import { onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { ApkSetInspection, InstallApkResult } from "../lib/types";
  import {
    abiVerdict,
    deviceAbiLabel,
    formatBytes,
    installFailureSummary,
    nativeCodeLabel,
    plural,
  } from "../lib/sideload";
  import sideloadCatalog from "../../../src/lib/sideload-catalog.json";

  let { back, navigate }: { back: () => void; navigate: (screen: Screen) => void } = $props();

  type Connection = { serial: string; generation: number };
  type Staged = Connection & { batch: string; inspection: ApkSetInspection };

  let phase = $state<"idle" | "picking" | "reading" | "installing">("idle");
  let staged = $state<Staged | null>(null);
  /// The install's answer, with the TV it was for: a later TV switch must not relabel it.
  let result = $state<(InstallApkResult & { tv: string }) | null>(null);
  let installingOn = $state("");
  let notice = $state("");
  let noticeError = $state(false);
  let linkError = $state("");
  let destroyed = false;
  /// The batch the backend still holds for this screen, so leaving the screen frees it.
  let held: string | null = null;

  const busy = $derived(phase !== "idle");
  const canPick = $derived(session.isConnected && !!session.serial && !busy);

  function current(target: Connection): boolean {
    return !destroyed && session.isConnected && session.serial === target.serial && session.generation === target.generation;
  }

  function release(batch: string | null) {
    if (!batch) return;
    if (held === batch) held = null;
    api.discardImport(batch).catch(() => {});
  }

  onDestroy(() => {
    destroyed = true;
    if (phase !== "installing") release(held);
  });

  // A TV switch, reconnect or lost link: the staged set was checked against the old TV.
  let watched = `${session.serial}:${session.generation}:${session.isConnected}`;
  $effect(() => {
    const key = `${session.serial}:${session.generation}:${session.isConnected}`;
    if (key === watched) return;
    watched = key;
    result = null;
    if (staged) {
      release(staged.batch);
      staged = null;
      notice = "The TV changed, so the picked files were dropped. Nothing was installed.";
      noticeError = true;
    }
  });

  function say(message: string, error = false) {
    notice = message;
    noticeError = error;
  }

  async function pick() {
    if (!canPick) return;
    const target = { serial: session.serial, generation: session.generation };
    release(staged?.batch ?? null);
    staged = null;
    result = null;
    say("");
    phase = "picking";
    let batch: string | null = null;
    try {
      const picked = await api.importPick("apk");
      if (picked.status === "canceled") {
        say("No files picked. Nothing was installed.");
        return;
      }
      batch = picked.batch;
      held = batch;
      if (!current(target)) return;
      phase = "reading";
      const inspection = await api.inspectImportedApks(target.serial, batch);
      if (!current(target)) return;
      staged = { ...target, batch, inspection };
      batch = null;
    } catch (e) {
      if (!destroyed) say(String(e), true);
    } finally {
      release(batch);
      if (!destroyed) phase = "idle";
    }
  }

  function cancel() {
    if (!staged || busy) return;
    release(staged.batch);
    staged = null;
    say("Canceled. Nothing was installed, and the picked copies were removed from this app.");
  }

  async function install() {
    const target = staged;
    if (!target || busy) return;
    if (!current(target)) {
      cancel();
      return;
    }
    staged = null;
    phase = "installing";
    installingOn = session.deviceLabel;
    say("");
    // The backend uses the batch up whatever happens.
    if (held === target.batch) held = null;
    try {
      const reply = await api.installImportedApks(target.serial, target.batch);
      if (destroyed) return;
      // A switch while installing already cleared the screen; the answer belongs to the old TV.
      if (current(target)) result = { ...reply, tv: installingOn };
      else say(`The TV changed during the install. Check ${installingOn}'s apps to see whether it finished.`, true);
    } catch (e) {
      if (!destroyed) say(String(e), true);
    } finally {
      if (!destroyed) phase = "idle";
    }
  }

  async function openLink(url: string) {
    linkError = "";
    try {
      await api.openUrl(url);
    } catch (e) {
      linkError = `Couldn't open the page: ${e}`;
    }
  }

  const ISSUES_URL = "https://github.com/bryanroscoe/shield_optimizer/issues/new";
</script>

<div class="screen sideload-screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Install APK</h3>
    <span style="width:44px"></span>
  </div>

  <div class="sl-card">
    <span class="card-label">To {session.deviceLabel}</span>
    <div class="actions">
      <button class="primary small" onclick={pick} disabled={!canPick}>
        <span class="msr" aria-hidden="true">android</span>
        {phase === "picking" ? "Waiting for the picker…" : phase === "reading" ? "Reading…" : "Pick APK files"}
      </button>
      <button class="ghost small" onclick={() => navigate("backups")} disabled={busy}>
        <span class="msr" aria-hidden="true">cloud_sync</span>Restore a backup
      </button>
    </div>
    <HelpSheet title="About installing APKs">
      <p class="card-desc">
        Pick APK files opens Android's file picker. Pick one .apk, or every part of a split app
        (base.apk and its split_config files) together. The files are copied into this app, then
        removed when the install ends or you cancel. Up to 32 files and 2 GiB at a time.
      </p>
      <p class="card-desc">
        Nothing installs until you confirm. Install sends the files to the TV's package manager
        the way desktop's Install APK does (adb install -r, or install-multiple for a split app):
        an app that is already installed is updated and keeps its data, if the new file is signed
        by the same developer.
      </p>
      <p class="card-desc">
        The architecture check reads which CPUs the APK ships native code for and compares them
        with what the TV reports. It doesn't read the app's name or version, and it doesn't check
        the signature or what the app does.
      </p>
    </HelpSheet>
    {#if !session.isConnected}
      <span class="status" role="status">Connect a TV to install an app on it.</span>
    {:else if phase === "installing"}
      <span class="status live" role="status"><span class="pdot blink"></span>Installing on {installingOn}…</span>
    {:else}
      <span class="status" class:bad={noticeError} role="status">{notice}</span>
    {/if}
  </div>

  {#if staged}
    {@const inspection = staged.inspection}
    {@const verdict = abiVerdict(inspection)}
    <div class="sl-card staged" class:mismatch={verdict === "mismatch"}>
      <span class="card-label">Ready to install · {plural(inspection.files.length, "file")}</span>
      <ul class="apk-list">
        {#each inspection.files as file, i (i)}
          <li>
            <span class="msr apk-icon" aria-hidden="true">android</span>
            <span class="apk-name">{file.name}</span>
            <span class="apk-size mono">{formatBytes(file.size_bytes)}</span>
          </li>
        {/each}
      </ul>
      <dl class="facts">
        <dt>Native code</dt>
        <dd class="mono">{nativeCodeLabel(inspection)}</dd>
        <dt>This TV</dt>
        <dd class="mono">{deviceAbiLabel(inspection)}</dd>
      </dl>
      {#if verdict === "mismatch"}
        <p class="callout warn" role="alert">
          <span class="msr" aria-hidden="true">warning</span>
          <span>
            This app ships native code for {inspection.abis.join(", ")}, and this TV reports
            {inspection.device_abis.join(", ")}. It will probably fail to install, or install and
            crash on launch.
          </span>
        </p>
      {:else if verdict === "unknown"}
        <p class="callout">
          <span class="msr" aria-hidden="true">info</span>
          <span>Whether this app's native code matches the TV couldn't be checked.</span>
        </p>
      {/if}
      <p class="card-desc safety">
        Nothing has been installed. ATV Optimizer doesn't know where these files came from or what
        the app does. Install them only if you trust their source.
      </p>
      <div class="actions">
        <button class="primary small" onclick={install} disabled={busy}>
          <span class="msr" aria-hidden="true">download</span>Install
        </button>
        <button class="ghost small" onclick={cancel} disabled={busy}>Cancel</button>
      </div>
    </div>
  {/if}

  {#if result}
    <div class="sl-card result" class:ok={result.ok} class:bad={!result.ok} role="status">
      <span class="result-head">
        <span class="msr fill" aria-hidden="true">{result.ok ? "check_circle" : "error"}</span>
        {result.ok ? `Installed on ${result.tv}.` : installFailureSummary(result.message)}
      </span>
      {#if result.hint}<p class="card-desc">{result.hint}</p>{/if}
      {#if !result.ok && result.message}<pre class="reply mono">{result.message}</pre>{/if}
    </div>
  {/if}

  <span class="section-label">Popular sideloads</span>
  <ul class="catalog-list">
    {#each sideloadCatalog as entry (entry.package)}
      <li class="sl-card catalog-row">
        <div class="catalog-text">
          <span class="apk-name">{entry.name}</span>
          <span class="card-desc">{entry.description}</span>
          <span class="catalog-pkg mono">{entry.package}</span>
        </div>
        <button
          class="ghost small"
          aria-label={`Open the official download page for ${entry.name}`}
          onclick={() => openLink(entry.url)}
        ><span class="msr" aria-hidden="true">open_in_new</span>Get</button>
      </li>
    {/each}
  </ul>
  {#if linkError}<p class="error" role="alert">{linkError}</p>{/if}
  <p class="card-desc rail-note">
    Official download pages open in your browser. Download the APK on this phone, then pick it here.
    <button class="ghost-link" onclick={() => openLink(ISSUES_URL)}>Suggest one on GitHub</button>
  </p>
  <div class="spacer"></div>
</div>

<style>
  .sideload-screen {
    gap: 14px;
  }
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
  .sl-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
  }
  .sl-card.mismatch {
    border-color: color-mix(in srgb, var(--amber) 45%, transparent);
  }
  .sl-card.result.ok {
    border-color: color-mix(in srgb, var(--accent) 35%, transparent);
  }
  .sl-card.result.bad {
    border-color: color-mix(in srgb, var(--danger) 40%, transparent);
  }
  .card-label {
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
    overflow-wrap: anywhere;
  }
  .card-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    margin: 0;
  }
  .safety {
    color: var(--text-soft);
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .actions > button {
    flex: 1 1 150px;
  }
  .ghost {
    gap: 6px;
    padding: 0 14px;
  }
  .ghost .msr {
    font-size: 18px;
  }
  .status {
    font-size: 12px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .status:empty {
    display: none;
  }
  .status.bad {
    color: var(--text-soft);
  }
  .status.live {
    display: inline-flex;
    gap: 8px;
    align-items: center;
  }
  .apk-list,
  .catalog-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .apk-list li {
    display: flex;
    align-items: center;
    gap: 10px;
    min-width: 0;
  }
  .apk-icon {
    font-size: 20px;
    color: var(--muted);
    flex: none;
  }
  .apk-name {
    flex: 1;
    min-width: 0;
    font-size: 14px;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .apk-size {
    flex: none;
    font-size: 11px;
    color: var(--muted);
  }
  .facts {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 6px 12px;
    margin: 0;
    font-size: 12px;
  }
  .facts dt {
    color: var(--muted);
  }
  .facts dd {
    margin: 0;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .callout {
    display: flex;
    gap: 10px;
    align-items: flex-start;
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-soft);
  }
  .callout.warn .msr {
    color: var(--amber);
  }
  .result-head {
    display: flex;
    gap: 10px;
    align-items: center;
    font-size: 14px;
    font-weight: 600;
  }
  .result.ok .result-head .msr {
    color: var(--accent);
  }
  .result.bad .result-head .msr {
    color: var(--amber);
  }
  .reply {
    margin: 0;
    max-height: 10rem;
    overflow: auto;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 10px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-soft);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    user-select: text;
  }
  .catalog-row {
    flex-direction: row;
    flex-wrap: wrap;
    align-items: center;
  }
  .catalog-text {
    flex: 1 1 200px;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .catalog-row > button {
    flex: none;
  }
  .catalog-pkg {
    font-size: 11px;
    color: var(--dim);
    overflow-wrap: anywhere;
  }
  .rail-note {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    align-items: center;
  }
  .error {
    margin: 0;
  }
</style>
