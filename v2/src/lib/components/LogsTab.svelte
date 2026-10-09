<script lang="ts">
  import { onDestroy } from "svelte";
  import { api } from "$lib/api";
  import Icon from "$lib/components/Icon.svelte";
  import { LogPoller, type LogPollState } from "$lib/log-poller";
  import type { DeviceLogOptions, DeviceLogResult } from "$lib/types";

  let { serial, resetToken = 0, active = true }: {
    serial: string; resetToken?: number; active?: boolean;
  } = $props();
  let priority = $state<DeviceLogOptions["priority"]>("info");
  let lines = $state(200);
  let tag = $state("");
  let packageName = $state("");
  let search = $state("");
  let visible = $state(true);
  type Snapshot = { data: DeviceLogResult; options: DeviceLogOptions; at: string };
  let pollState = $state<LogPollState<Snapshot>>({ busy: false, running: false, value: null, error: null });
  const poller = new LogPoller<Snapshot>(async () => {
    const options: DeviceLogOptions = {
      priority, lines, tag: tag.trim() || null, package: packageName.trim() || null,
    };
    const data = await api.readDeviceLogs(serial, options);
    if (data.output.exit_code !== 0 && data.output.termination === "completed") {
      // Retain raw diagnostics on screen; this component never exports them.
      throw new Error(data.output.stderr || `logcat exited with code ${data.output.exit_code ?? "unknown"}`);
    }
    return { data, options, at: new Date().toLocaleTimeString() };
  }, () => active && visible ? `${serial}:${resetToken}` : null, next => { pollState = next; });
  const text = $derived(pollState.value?.data.output.stdout ?? "");
  const displayed = $derived(search ? text.split("\n").filter(line => line.toLowerCase().includes(search.toLowerCase())).join("\n") : text);

  function visibilityChanged() {
    visible = document.visibilityState !== "hidden";
    if (!visible) poller.stop();
  }
  onDestroy(() => poller.dispose());
</script>

<svelte:document onvisibilitychange={visibilityChanged} />
<div class="card" role="tabpanel" tabindex={0} id="tabpanel-logs" aria-labelledby="tab-logs">
  <div class="card-header">
    <div>
      <h2><Icon name="description" size={20} /> Device logs</h2>
      <p class="muted small">Recent logcat snapshots, read only</p>
    </div>
  </div>
  <p class="muted small">
    Logs can contain account details, URLs and tokens. Review before sharing.
    This view does not save or upload them.
  </p>
  <fieldset disabled={pollState.busy || pollState.running}>
    <label>Minimum severity
      <select bind:value={priority}>
        <option value="verbose">Verbose</option><option value="debug">Debug</option>
        <option value="info">Info</option><option value="warning">Warning</option>
        <option value="error">Error</option><option value="fatal">Fatal</option>
      </select>
    </label>
    <label>Recent entries
      <select bind:value={lines}><option value={100}>100</option><option value={200}>200</option><option value={500}>500</option><option value={1000}>1,000</option></select>
    </label>
    <label>Tag (optional)<input bind:value={tag} placeholder="ActivityManager" maxlength="64" /></label>
    <label>App package (optional)<input bind:value={packageName} placeholder="com.example.app" maxlength="255" /></label>
  </fieldset>
  <p class="muted small">App filtering covers the currently running main process only; helper processes are excluded.</p>
  <div class="actions">
    <button onclick={() => poller.once()} disabled={pollState.busy || pollState.running || !visible}>Read logs</button>
    {#if pollState.running}
      <button onclick={() => poller.stop()}>Stop refreshing</button>
    {:else}
      <button onclick={() => poller.start()} disabled={pollState.busy || !visible}>Auto-refresh</button>
    {/if}
    <span class="muted small" role="status">{pollState.busy ? "Reading…" : pollState.running ? "Waiting for next snapshot…" : "Stopped"}</span>
  </div>
  <details class="capture-details small">
    <summary>Capture limits and app scope</summary>
    <p>Refreshes 3 seconds after each read finishes. Each read is limited to 30 seconds and 256 KiB per output stream; resolving an app adds a separate bounded read. Stop prevents further refreshes; an in-flight read finishes within its limits. Leaving this tab or hiding the app stops refreshes.</p>
    <p>Android can reuse process IDs, so older entries may belong to a previous process. Filters and logcat options depend on the TV's Android version. Snapshots can overlap or miss high-volume traffic; this is not a lossless recording. Raw logs are not added to automatic bug reports.</p>
  </details>
  {#if pollState.error}<p class="error" role="alert">{pollState.error}</p>{/if}
  {#if pollState.value}
    <p class="muted small">
      Snapshot at {pollState.value.at} · {pollState.value.options.priority} and above ·
      {pollState.value.options.tag ? `tag ${pollState.value.options.tag}` : "all tags"} ·
      {pollState.value.data.package ? `${pollState.value.data.package} (PID ${pollState.value.data.pid})` : "all processes"}
    </p>
    {#if pollState.value.data.output.termination !== "completed"}
      <p class="error" role="alert">Partial snapshot: {pollState.value.data.output.termination === "timeout" ? "the read timed out" : "the output limit was reached"}.</p>
    {/if}
    {#if pollState.value.data.output.stderr}<pre class="diagnostic">{pollState.value.data.output.stderr}</pre>{/if}
    <label class="search">Find in this snapshot<input bind:value={search} placeholder="Filter displayed lines" /></label>
    <pre class="log-output" aria-label="Log output">{displayed || (text ? "No displayed lines match your search." : "No entries matched these filters in the recent log buffer.")}</pre>
  {/if}
</div>

<style>
  .card-header { margin-bottom: .8rem; padding-bottom: .7rem; }
  .card-header h2 { margin: 0; }
  .card-header p { margin: .3rem 0 0; }
  .small { font-size: .8rem; line-height: 1.5; }
  .capture-details { margin: .8rem 0; color: var(--fg-secondary); }
  .capture-details summary { cursor: pointer; }
  h2 { display: flex; align-items: center; gap: .5rem; }
  fieldset { border: 0; padding: 0; margin: 1rem 0; display: flex; flex-wrap: wrap; gap: .8rem; }
  label { display: flex; flex-direction: column; gap: .35rem; font-size: .85rem; }
  input, select { min-width: 10rem; }
  .actions { display: flex; flex-wrap: wrap; align-items: center; gap: .6rem; }
  .search { max-width: 28rem; margin: .8rem 0; }
  pre { white-space: pre-wrap; overflow-wrap: anywhere; overflow: auto; max-height: 32rem; padding: 1rem; border: 1px solid var(--border); border-radius: .5rem; font-size: .8rem; user-select: text; }
  .diagnostic { max-height: 8rem; }
</style>
