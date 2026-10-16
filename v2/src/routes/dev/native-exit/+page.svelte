<script lang="ts">
  import { onMount } from "svelte";
  import { invoke } from "@tauri-apps/api/core";
  type Snapshot = { preference: { policy: string; remembered: boolean }; phase: string; token: string | null; prompt_open: boolean; error: string | null; warning: string | null; stop_available: false };
  let snapshot = $state<Snapshot | null>(null);
  let policy = $state("always_leave_running");
  let remember = $state(false);
  let failure = $state("");
  let dialog = $state<HTMLDialogElement | null>(null);
  let busy = $state(false);
  async function action(value: Record<string, unknown>) {
    if (busy) return;
    busy = true;
    try {
      const next = await invoke<Snapshot>("dev_exit_action", { action: value });
      if (next.token !== snapshot?.token) { policy = next.preference.policy; remember = false; }
      snapshot = next; failure = "";
    } catch (error) { failure = String(error); }
    finally { busy = false; }
  }
  onMount(() => {
    void action({ type: "read" });
    const timer = setInterval(() => { if (!busy) void action({ type: "read" }); }, 250);
    return () => clearInterval(timer);
  });
  $effect(() => {
    if (!dialog) return;
    if (snapshot?.prompt_open && !dialog.open) dialog.showModal();
    else if (!snapshot?.prompt_open && dialog.open) dialog.close();
  });
</script>

<section>
  <p class="eyebrow">Isolated development app · actual native quit</p>
  <h1>When this app quits</h1>
  <p>Leave and Auto close this development app through native coordination. Shared ADB is left running. Device commands and updater installation are disabled.</p>
  <p>Saved choice: <strong>{snapshot?.preference.remembered ? snapshot.preference.policy : "None — Leave running is the default"}</strong></p>
  <fieldset disabled={busy || snapshot?.phase !== "Running"}>
    <legend>Shared ADB preference</legend>
    <label><input type="radio" bind:group={policy} value="always_leave_running" /> Always leave running — default</label>
    <label><input type="radio" bind:group={policy} value="auto" /> Auto — ownership unknown, leaves running</label>
    <label><input type="radio" disabled /> Always stop — unavailable pending executor review</label>
    <button onclick={() => action({ type: "save", policy })}>Save development preference</button>
  </fieldset>
  <button onclick={() => action({ type: "request_quit" })} disabled={busy || !snapshot}>Quit development app</button>
  <p>Native snapshot: {snapshot?.phase ?? "Connecting…"}</p>
  {#if snapshot?.warning}<p role="status">{snapshot.warning}</p>{/if}
  {#if snapshot?.error}<p role="alert">{snapshot.error}</p>{/if}
  {#if failure}<p role="alert">{failure}</p>{/if}
  {#if snapshot?.phase.startsWith("Failed") && snapshot.token}
    <button onclick={() => action({ type: "exit_after_failure", token: snapshot!.token })}>Exit without another stop attempt</button>
  {/if}
  {#if snapshot?.phase === "Finished" && snapshot.error && snapshot.token}
    <button onclick={() => action({ type: "retry_delivery", token: snapshot!.token })}>Retry completed exit delivery</button>
  {/if}
</section>
{#each snapshot?.token ? [snapshot.token] : [] as token (token)}
<dialog bind:this={dialog} oncancel={(event) => { event.preventDefault(); void action({ type: "cancel", token }); }} aria-labelledby="native-quit-title">
  <h2 id="native-quit-title">Leave shared ADB running?</h2>
  <p>This quits the development app. Stopping shared ADB would disconnect other apps and devices.</p>
  <fieldset disabled={busy}>
    <legend>Shared ADB on quit</legend>
    <label><input type="radio" bind:group={policy} value="always_leave_running" /> Always leave running — default</label>
    <label><input type="radio" bind:group={policy} value="auto" /> Auto — ownership unknown, leaves running</label>
    <label><input type="radio" disabled /> Always stop — unavailable pending executor review</label>
    <label><input type="checkbox" bind:checked={remember} /> Remember my choice in this development app</label>
  </fieldset>
  {#if failure}<p role="alert">{failure}</p>{/if}
  {#if snapshot?.error}<p role="alert">{snapshot.error}</p>{/if}
  <div class="actions">
    <button onclick={() => action({ type: "cancel", token })} disabled={busy}>Cancel</button>
    <button onclick={() => action({ type: "confirm", token, policy, remember })} disabled={busy}>Quit and leave ADB running</button>
  </div>
</dialog>
{/each}
<style>
  section { max-width: 760px; margin: 40px auto; padding: 28px; background: var(--bg-surface); border: 1px solid var(--border); border-radius: 16px; }
  .eyebrow { color: var(--accent); font-weight: 700; } p { line-height: 1.6; }
  dialog { max-width: 640px; padding: 26px; border: 1px solid var(--border); border-radius: 16px; background: var(--bg-surface); color: var(--fg-primary); }
  dialog::backdrop { background: #0009; } fieldset { border: 0; padding: 0; } label { display: block; margin: 18px 0; } .actions { display: flex; gap: 14px; margin-top: 24px; }
</style>
