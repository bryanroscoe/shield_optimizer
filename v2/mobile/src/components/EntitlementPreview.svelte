<script lang="ts">
  import { previewMessage, previewIsPro, type Preview } from "../lib/entitlementPreview";
  let { state, onBuy, onRestore, onRecovery }: { state: Preview; onBuy: () => void; onRestore: () => void; onRecovery: () => void } = $props();
  const busy = $derived(["buying", "restoring", "pending", "verifying"].includes(state.phase));
</script>
<section aria-label="Simulated entitlement preview">
  <h2>Simulated entitlement preview</h2>
  <p>No real purchase or session unlock occurs here.</p>
  <p role="status">{previewMessage(state)}</p>
  <button disabled={busy || previewIsPro(state)} onclick={onBuy}>Simulate Pro purchase</button>
  <button disabled={busy} onclick={onRestore}>Simulate restore</button>
  <button onclick={onRecovery}>Free recovery</button>
  <button disabled>Support tips unavailable</button>
</section>
