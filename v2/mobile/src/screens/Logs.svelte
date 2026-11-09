<script lang="ts">
  import HelpSheet from "../components/HelpSheet.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import { onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { LogPoller, type LogPollState } from "../../../shared/log-poller";
  import {
    LOG_LINE_CHOICES,
    LOG_PRIORITIES,
    checkLogRead,
    filterLogLines,
    logOptions,
    partialReason,
    type DeviceLogOptions,
    type DeviceLogResult,
  } from "../../../shared/device-logs";

  let { back }: { back: () => void } = $props();

  let priority = $state<DeviceLogOptions["priority"]>("info");
  let lines = $state(200);
  let tag = $state("");
  let packageName = $state("");
  let search = $state("");
  let visible = $state(typeof document === "undefined" || document.visibilityState !== "hidden");
  type Snapshot = { data: DeviceLogResult; options: DeviceLogOptions; at: string };
  let pollState = $state<LogPollState<Snapshot>>({ busy: false, running: false, value: null, error: null });

  // The read is pinned to the TV and connection it started on: a TV switch,
  // reconnect or lost link changes the context, so its late result is dropped.
  const context = () =>
    visible && session.isConnected && session.serial
      ? `${session.serial}:${session.generation}`
      : null;
  const poller = new LogPoller<Snapshot>(async () => {
    const options = logOptions(priority, lines, tag, packageName);
    const data = checkLogRead(await api.readDeviceLogs(session.serial, options));
    return { data, options, at: new Date().toLocaleTimeString() };
  }, context, (next) => { pollState = next; });

  const text = $derived(pollState.value?.data.output.stdout ?? "");
  const displayed = $derived(filterLogLines(text, search));
  const partial = $derived(pollState.value ? partialReason(pollState.value.data) : null);
  const locked = $derived(pollState.busy || pollState.running);
  const canRead = $derived(session.isConnected && !!session.serial && visible);

  // Leaving the TV (switch, reconnect, lost link) stops refreshing and
  // clears a snapshot that belonged to the old connection.
  let watched = `${session.serial}:${session.generation}:${session.isConnected}`;
  $effect(() => {
    const key = `${session.serial}:${session.generation}:${session.isConnected}`;
    if (key === watched) return;
    watched = key;
    poller.stop();
    pollState = { busy: false, running: false, value: null, error: null };
  });

  function visibilityChanged() {
    visible = document.visibilityState !== "hidden";
    if (!visible) poller.stop();
  }
  onDestroy(() => poller.dispose());
</script>

<svelte:document onvisibilitychange={visibilityChanged} />
<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Device logs</h3>
    <span style="width:44px"></span>
  </div>

  <div class="logs-card">
    <span class="card-label">Recent logcat snapshot</span>
    <p class="card-desc privacy">
      Logs can contain account details, URLs and tokens. Review before sharing.
      This view does not save or upload them.
    </p>
    <HelpSheet title="Capture limits and app scope">
      <p class="card-desc">App filtering covers the currently running main process only; helper processes are excluded.</p>
      <p class="card-desc">Refreshes 3 seconds after each read finishes. Each read is limited to 30 seconds and 256 KiB per output stream; resolving an app adds a separate bounded read. Stop prevents further refreshes; an in-flight read finishes within its limits. Leaving this screen, switching TVs or putting the app in the background stops refreshes.</p>
      <p class="card-desc">Android can reuse process IDs, so older entries may belong to a previous process. Filters and logcat options depend on the TV's Android version. Snapshots can overlap or miss high-volume traffic; this is not a lossless recording. Raw logs are not added to the bug report.</p>
    </HelpSheet>

    <fieldset disabled={locked}>
      <label>Minimum severity
        <select bind:value={priority}>
          {#each LOG_PRIORITIES as p (p.value)}<option value={p.value}>{p.label}</option>{/each}
        </select>
      </label>
      <label>Recent entries
        <select bind:value={lines}>
          {#each LOG_LINE_CHOICES as n (n)}<option value={n}>{n.toLocaleString()}</option>{/each}
        </select>
      </label>
      <label>Tag (optional)<input bind:value={tag} placeholder="ActivityManager" maxlength="64" autocapitalize="off" autocomplete="off" spellcheck="false" /></label>
      <label>App package (optional)<input bind:value={packageName} placeholder="com.example.app" maxlength="255" autocapitalize="off" autocomplete="off" spellcheck="false" /></label>
    </fieldset>

    <div class="actions">
      <button class="primary small" onclick={() => poller.once()} disabled={locked || !canRead}>
        <span class="msr">description</span>Read logs
      </button>
      {#if pollState.running}
        <button class="ghost small" onclick={() => poller.stop()}>Stop refreshing</button>
      {:else}
        <button class="ghost small" onclick={() => poller.start()} disabled={pollState.busy || !canRead}>Auto-refresh</button>
      {/if}
    </div>
    <span class="status" role="status">
      {#if !session.isConnected}Connect a TV to read its logs.
      {:else}{pollState.busy ? "Reading…" : pollState.running ? "Waiting for next snapshot…" : "Stopped"}{/if}
    </span>
  </div>

  {#if pollState.error}<p class="error" role="alert">{pollState.error}</p>{/if}
  {#if pollState.value}
    <div class="logs-card">
      <p class="card-desc snapshot-meta">
        Snapshot at {pollState.value.at} · {pollState.value.options.priority} and above ·
        {pollState.value.options.tag ? `tag ${pollState.value.options.tag}` : "all tags"} ·
        {pollState.value.data.package ? `${pollState.value.data.package} (PID ${pollState.value.data.pid})` : "all processes"}
      </p>
      {#if partial}
        <p class="error" role="alert">Partial snapshot: {partial}.</p>
      {/if}
      {#if pollState.value.data.output.stderr}<pre class="log-view mono diagnostic">{pollState.value.data.output.stderr}</pre>{/if}
      <label class="search">Find in this snapshot
        <input type="search" bind:value={search} placeholder="Filter displayed lines" autocapitalize="off" autocomplete="off" spellcheck="false" />
      </label>
      <pre class="log-view mono" aria-label="Log output">{displayed || (text ? "No displayed lines match your search." : "No entries matched these filters in the recent log buffer.")}</pre>
    </div>
  {/if}
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
  .logs-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
    min-width: 0;
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
  .privacy {
    color: var(--text-soft);
  }
  fieldset {
    border: 0;
    padding: 0;
    margin: 0;
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(140px, 1fr));
    gap: 10px;
    min-width: 0;
  }
  fieldset:disabled {
    opacity: 0.7;
  }
  label {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-soft);
    min-width: 0;
  }
  input,
  select {
    min-height: 44px;
    width: 100%;
    box-sizing: border-box;
    padding: 0 12px;
    border-radius: 11px;
    border: 1px solid var(--line);
    background: var(--canvas);
    color: var(--text);
    font-family: var(--sans);
    font-size: 14px;
  }
  .actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
    align-items: center;
  }
  .actions > button {
    flex: 1 1 140px;
  }
  .status {
    font-size: 12px;
    color: var(--muted);
  }
  .snapshot-meta {
    overflow-wrap: anywhere;
  }
  .log-view {
    max-height: 60vh;
    overflow: auto;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 12px;
    font-size: 11px;
    line-height: 1.5;
    color: var(--text-soft);
    white-space: pre-wrap;
    overflow-wrap: anywhere;
    margin: 0;
    user-select: text;
  }
  .log-view.diagnostic {
    max-height: 8rem;
  }
  .error {
    margin: 0;
  }
</style>
