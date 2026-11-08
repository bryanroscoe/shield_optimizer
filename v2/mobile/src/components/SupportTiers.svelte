<script lang="ts">
  import { billing } from "../lib/billing.svelte";
  import HelpSheet from "./HelpSheet.svelte";
  // Pay-more Play products that unlock exactly the same Pro as Buy Pro. Shown
  // only for the tiers Play returned a price for.
  let { onPurchased }: { onPurchased?: () => void } = $props();

  const tiers = $derived(billing.supportOffers);

  async function buy(productId: string) {
    const outcome = await billing.purchase(productId);
    if (outcome && billing.status?.entitlement === "pro") onPurchased?.();
  }
</script>

{#if tiers.length > 0}
  <div class="support-tiers">
    <div class="support-head">
      <span class="support-label">Support me <span class="support-note">· same Pro</span></span>
      <HelpSheet title="Why pay more?">
        <p class="support-help">
          These unlock exactly the same Pro as Buy Pro, nothing extra. The higher price is a way to
          help keep ATV Optimizer developed. Buy only one: any of them unlocks Pro for good.
        </p>
      </HelpSheet>
    </div>
    <div class="support-buttons">
      {#each tiers as tier (tier.product_id)}
        <button
          class="ghost support-btn"
          aria-label="Support me with {tier.formatted_price}"
          disabled={billing.purchasing || billing.restoring || billing.pending}
          onclick={() => buy(tier.product_id)}
        >
          {billing.purchasingId === tier.product_id ? "Waiting…" : tier.formatted_price}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .support-tiers {
    width: 100%;
    margin-top: 12px;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .support-head {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    justify-content: space-between;
    gap: 4px 12px;
  }
  .support-label {
    font-size: 13px;
    font-weight: 600;
    color: var(--text);
  }
  .support-note {
    font-weight: 400;
    color: var(--muted);
  }
  .support-buttons {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }
  .support-btn {
    flex: 1 1 0;
    min-width: 96px;
    min-height: 44px;
  }
  .support-help {
    margin: 0;
  }
</style>
