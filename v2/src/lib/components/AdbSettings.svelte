<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  let { onbusy = (_busy: boolean) => {} } = $props<{ onbusy?: (busy: boolean) => void }>();
  let info = $state<{ path: string | null; version: string | null } | null>(null);
  let busy = $state(false);
  let message = $state("");
  let before = $state<string | null>(null);

  async function inspect() {
    try { info = await api.adbBuildInfo(); }
    catch (e) { info = null; message = String(e); }
  }
  onMount(() => { void inspect(); });

  async function reinstall() {
    if (busy || !confirm("Re-download the latest official Google platform-tools? This replaces the app-managed tools, not saved TVs or pairing keys. The running shared ADB server is not restarted automatically. Close active TV operations first.")) return;
    busy = true; onbusy(true); before = info?.version ?? null;
    message = "Downloading the latest platform-tools from Google…";
    try {
      const result = await api.installAdb();
      message = result.message;
      if (result.ok) { await inspect(); window.dispatchEvent(new CustomEvent("adb-tools-changed", { detail: "installed" })); }
    } catch (e) { message = String(e); }
    finally { busy = false; onbusy(false); }
  }

  async function restart() {
    if (busy || !confirm("Restart the shared ADB server? Other ADB tools will disconnect. Existing network connections will be retried. This does not restart your TV.")) return;
    busy = true; onbusy(true);
    try { message = (await api.restartAdb()).message; window.dispatchEvent(new Event("adb-tools-changed")); }
    catch (e) { message = String(e); }
    finally { busy = false; onbusy(false); }
  }
</script>

<section aria-labelledby="adb-settings-title">
  <h3 id="adb-settings-title">ADB connection tools</h3>
  <p>The version below is the selected ADB client. An already-running shared server may be from another installation.</p>
  <pre aria-label="Selected ADB version">{info?.version ?? "Version unavailable"}</pre>
  {#if info?.path}<p class="path">{info.path}</p>{/if}
  {#if before}<p>Before re-download:</p><pre>{before}</pre>{/if}
  <div class="actions">
    <button onclick={reinstall} disabled={busy}>Re-download latest ADB</button>
    <button onclick={restart} disabled={busy}>Restart shared ADB server</button>
    <button onclick={inspect} disabled={busy}>Refresh version</button>
  </div>
  <p>For a comparison: note the version, re-download, test the connection, then explicitly restart ADB and test again. The latest download may be the same version.</p>
  {#if message}<p role="status">{message}</p>{/if}
</section>

<style>
  section { border-top: 1px solid var(--border); margin-top: 16px; padding-top: 8px; }
  p { font-size: 0.85rem; line-height: 1.5; }
  pre, .path { white-space: pre-wrap; overflow-wrap: anywhere; font-size: 0.8rem; }
  .actions { display: flex; flex-wrap: wrap; gap: 8px; }
</style>
