<script lang="ts">
  import { billing } from "../lib/billing.svelte";
  // A quiet, optional tip for Pro owners in Google Play builds. It opens only
  // when tapped, lists only the tips Play returned a price for, and unlocks
  // nothing. The backend verifies and finishes every tip with Google Play.
  const id = $props.id();
  let open = $state(false);
  const tips = $derived(billing.tipOffers);
  const busy = $derived(!!billing.tippingId || billing.purchasing || billing.restoring);
</script>

{#if tips.length > 0}
  <div class="tip-jar">
    <button
      type="button"
      class="tip-link"
      aria-expanded={open}
      aria-controls="{id}-tips"
      onclick={() => (open = !open)}
    >
      <span>Leave a tip</span>
      <span class="msr" class:open aria-hidden="true">expand_more</span>
    </button>
    {#if open}
      <div class="tip-panel" id="{id}-tips">
        <p class="tip-note">Optional. A tip doesn't unlock anything.</p>
        <div class="tip-buttons">
          {#each tips as tip (tip.product_id)}
            <button
              class="ghost tip-btn"
              aria-label="Tip {tip.formatted_price}"
              disabled={busy}
              onclick={() => billing.tip(tip.product_id)}
            >
              {billing.tippingId === tip.product_id ? "Waiting…" : tip.formatted_price}
            </button>
          {/each}
        </div>
      </div>
    {/if}
    {#if billing.tipMessage}
      <p class="tip-status" class:success={billing.tipTone === "success"} class:error={billing.tipTone === "error"} role="status">
        {billing.tipMessage}
      </p>
    {:else if billing.tipPending}
      <p class="tip-status" role="status">A tip is pending. Google Play finishes it once the payment clears.</p>
    {/if}
  </div>
{/if}

<style>
  .tip-jar {
    container-type: inline-size;
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 100%;
  }
  .tip-link {
    box-sizing: border-box;
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 8px;
    min-height: 44px;
    max-width: 100%;
    margin: 0;
    padding: 10px 0;
    border: 0;
    background: transparent;
    color: var(--text-soft);
    font: inherit;
    font-size: 13px;
    cursor: pointer;
  }
  .tip-link .msr {
    font-size: 18px;
    transition: transform 0.15s ease;
  }
  .tip-link .msr.open {
    transform: rotate(180deg);
  }
  .tip-link:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 6px;
  }
  .tip-panel {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .tip-note,
  .tip-status {
    margin: 0;
    font-size: 12px;
    line-height: 1.4;
    color: var(--muted);
  }
  .tip-status.success {
    color: var(--teal);
  }
  .tip-status.error {
    color: var(--danger);
  }
  /* Two even columns, four only when a localized price surely fits. */
  .tip-buttons {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 8px;
  }
  @container (min-width: 440px) {
    .tip-buttons {
      grid-template-columns: repeat(4, minmax(0, 1fr));
    }
  }
  .tip-btn {
    min-height: 44px;
    overflow-wrap: anywhere;
  }
</style>
