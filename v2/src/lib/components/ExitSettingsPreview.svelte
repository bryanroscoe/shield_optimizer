<script lang="ts">
  import { onMount } from "svelte";
  import { exitPreview, previewPolicies, type ExitPolicy, type ExitPreviewAction, type ExitPreviewSnapshot } from "$lib/exit-preview";

  let snapshot = $state<ExitPreviewSnapshot | null>(null);
  let policy = $state<ExitPolicy>("always_leave_running");
  let confirmPolicy = $state<ExitPolicy>("always_leave_running");
  let remember = $state(false);
  let busy = $state(false);
  let failure = $state("");
  let dialog = $state<HTMLDialogElement | null>(null);
  const unsupported = $derived(policy === "always_stop");
  const confirmUnsupported = $derived(confirmPolicy === "always_stop");

  async function perform(action: ExitPreviewAction) {
    if (busy) return;
    busy = true; failure = "";
    try {
      const next = await exitPreview(action);
      if (next.prompt_open && (!snapshot?.prompt_open || next.prompt_generation !== snapshot.prompt_generation)) {
        confirmPolicy = next.preference.policy; remember = false;
      }
      snapshot = next; policy = next.preference.policy;
    } catch (error) { failure = String(error); }
    finally { busy = false; }
  }
  onMount(() => { void perform({ type: "read" }); });
  $effect(() => {
    if (!dialog) return;
    if (snapshot?.prompt_open && !dialog.open) dialog.showModal();
    else if (!snapshot?.prompt_open && dialog.open) dialog.close();
  });
  const savedLabel = $derived(snapshot?.preference.remembered
    ? previewPolicies.find((choice) => choice.value === snapshot?.preference.policy)?.title
    : "None — Leave running is the default");
</script>

<section class="exit-preview" aria-labelledby="exit-preview-title">
  <div class="eyebrow">Development preview · simulation only</div>
  <h1 id="exit-preview-title">When this app closes</h1>
  <p class="intro">Choose how shared ADB should be handled. This preview never closes the app or stops ADB.</p>
  <fieldset disabled={busy || snapshot?.phase !== "running"}>
    <legend>Shared ADB server</legend>
    {#each previewPolicies as choice}
      <label class="policy-card" class:selected={policy === choice.value} class:warning={choice.value === "always_stop"}>
        <input type="radio" name="preview-policy" value={choice.value} bind:group={policy} />
        <span><strong>{choice.title}</strong><span class="detail">{choice.detail}</span></span>
      </label>
    {/each}
  </fieldset>
  <div class="save-row">
    <button class="primary" onclick={() => perform({ type: "save", policy })} disabled={busy || unsupported || snapshot?.phase !== "running"}>Save preview choice</button>
    <span class="muted">Uses separate development preferences. Real quit behavior is unchanged.</span>
  </div>
  {#if unsupported}<p class="unavailable" role="status">Always stop cannot be enabled: its executor is unavailable.</p>{/if}
  <div class="state-panel">
    <div><span class="muted">Saved preview choice</span><strong data-testid="saved-policy">{savedLabel}</strong></div>
    <div><span class="muted">Simulated operation gate</span><strong data-testid="admission">{snapshot?.admission_open ? "Open" : snapshot ? "Closed" : "Loading…"}</strong></div>
  </div>
  <div class="actions">
    <button onclick={() => perform({ type: "request_quit" })} disabled={busy || !snapshot}>Simulate normal quit</button>
    <button onclick={() => perform({ type: "updater_restart" })} disabled={busy || !snapshot}>Simulate updater restart</button>
    <button onclick={() => perform({ type: "restart_simulation" })} disabled={busy || !snapshot}>Simulate app restart</button>
  </div>
  <p class="result" role="status">{snapshot?.message ?? "Loading development preview…"}</p>
  {#if snapshot?.storage_warning}<p class="unavailable" role="status">{snapshot.storage_warning}</p>{/if}
  {#if failure && !snapshot?.prompt_open}<p class="unavailable" role="alert">{failure}</p>{/if}
</section>

{#each [snapshot?.prompt_generation ?? 0] as promptGeneration (promptGeneration)}
<dialog bind:this={dialog} aria-labelledby="quit-preview-title" oncancel={(event) => { event.preventDefault(); void perform({ type: "cancel", prompt_generation: promptGeneration }); }}>
  <div class="eyebrow">First-quit prompt · simulated</div>
  <h2 id="quit-preview-title">Leave shared ADB running?</h2>
  <p>Stopping shared ADB disconnects other apps and all attached devices.</p>
  <fieldset disabled={busy}>
    <legend>For this simulated quit</legend>
    {#each previewPolicies as choice}
      <label class="policy-card" class:selected={confirmPolicy === choice.value}>
        <input type="radio" name="confirm-policy" value={choice.value} bind:group={confirmPolicy} />
        <span><strong>{choice.title}</strong><span class="detail">{choice.detail}</span></span>
      </label>
    {/each}
  </fieldset>
  <label class="remember"><input type="checkbox" bind:checked={remember} disabled={busy} />Remember my choice in this preview</label>
  {#if confirmUnsupported}<p class="unavailable">Stop is unavailable. No preference or shutdown will be applied.</p>{/if}
  {#if failure}<p class="unavailable" role="alert">{failure}</p>{/if}
  <div class="actions">
    <button onclick={() => perform({ type: "cancel", prompt_generation: promptGeneration })} disabled={busy}>Cancel</button>
    <button class="primary" onclick={() => perform({ type: "confirm", prompt_generation: promptGeneration, policy: confirmPolicy, remember })} disabled={busy || confirmUnsupported}>
      {confirmUnsupported ? "Stop ADB — unavailable" : confirmPolicy === "auto" ? "Continue with Auto (leaves running)" : "Leave ADB running"}
    </button>
  </div>
  <div class="simulation-actions">
    <button onclick={() => perform({ type: "request_quit" })} disabled={busy}>Repeat close request</button>
    <button onclick={() => perform({ type: "updater_restart" })} disabled={busy}>Simulate updater during prompt</button>
  </div>
  <p class="muted small">Simulation only. The real app and shared server remain running.</p>
</dialog>
{/each}

<style>
  .exit-preview { max-width: 860px; margin: 30px auto; padding: 28px; border: 1px solid var(--border); border-radius: 18px; background: var(--bg-surface); }
  .eyebrow { text-transform: uppercase; letter-spacing: .12em; font-size: 11px; font-weight: 700; color: var(--accent); }
  h1 { margin: 10px 0; font-size: 32px; } h2 { margin: 12px 0; font-size: 25px; }
  .intro { color: var(--fg-secondary); margin-bottom: 24px; }
  fieldset { border: 0; padding: 0; margin: 0; min-width: 0; } legend { font-size: 13px; color: var(--fg-muted); margin-bottom: 10px; }
  .policy-card { display: flex; gap: 14px; align-items: flex-start; padding: 16px; margin: 9px 0; border: 1px solid var(--border-strong); border-radius: 12px; background: var(--bg-inset); cursor: pointer; }
  .policy-card.selected { border-color: var(--accent); background: var(--accent-surface); } .policy-card.warning { border-color: var(--warn-border); }
  .policy-card input { margin-top: 3px; flex-shrink: 0; } .detail { display: block; color: var(--fg-secondary); font-size: 13px; line-height: 1.5; margin-top: 5px; }
  .save-row,.actions { display: flex; align-items: center; gap: 12px; flex-wrap: wrap; margin-top: 18px; } .save-row .muted { flex: 1; font-size: 12px; min-width: 200px; }
  .state-panel { display: grid; grid-template-columns: 2fr 1fr; gap: 18px; padding: 18px; margin-top: 24px; border-radius: 12px; background: var(--bg-surface-2); }
  .state-panel span,.state-panel strong { display: block; } .state-panel strong { margin-top: 6px; font-size: 14px; }
  .result { line-height: 1.6; padding-top: 10px; color: var(--fg-secondary); } .unavailable { padding: 12px; background: var(--warn-surface); color: var(--warn); border-radius: 8px; font-size: 13px; line-height: 1.5; }
  dialog { width: min(650px,calc(100vw - 40px)); border: 1px solid var(--border-strong); border-radius: 18px; padding: 26px; background: var(--bg-surface); color: var(--fg-primary); box-shadow: 0 24px 100px #0008; }
  dialog::backdrop { background: var(--scrim); backdrop-filter: blur(3px); } .remember { display: flex; align-items: center; gap: 9px; margin-top: 18px; font-size: 13px; }
  .simulation-actions { display: flex; gap: 8px; margin-top: 16px; flex-wrap: wrap; }.simulation-actions button { font-size: 11px; color: var(--fg-muted); }
  @media(max-width:600px) { .exit-preview { margin: 14px; padding: 18px; } .state-panel { grid-template-columns: 1fr; } h1 { font-size: 26px; } }
</style>
