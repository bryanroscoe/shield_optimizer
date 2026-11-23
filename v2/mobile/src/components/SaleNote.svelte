<script lang="ts">
  import type { PlayOffer } from "../lib/types";

  // A Play Console sale on Pro: the full price, percentage and end date all
  // come from Play. Nothing shows once the sale has ended or for no sale.
  let { offer, centered = false }: { offer: PlayOffer | null | undefined; centered?: boolean } = $props();

  // Re-checked each minute so a sale that ends while the screen is open goes.
  let now = $state(Date.now());
  $effect(() => {
    const timer = setInterval(() => (now = Date.now()), 60_000);
    return () => clearInterval(timer);
  });

  const sale = $derived.by(() => {
    const s = offer?.sale;
    if (!s || (s.ends_at_ms != null && s.ends_at_ms <= now)) return null;
    if (!s.full_formatted_price && s.percent_off == null) return null;
    return s;
  });
  const ends = $derived(
    sale?.ends_at_ms != null
      ? new Date(sale.ends_at_ms).toLocaleDateString(undefined, { month: "short", day: "numeric" })
      : null,
  );
</script>

{#if sale}
  <p class="sale-note" class:centered>
    <span class="sale-badge">Sale</span>
    {#if sale.full_formatted_price}
      <s><span class="hidden-label">Regular price&nbsp;</span>{sale.full_formatted_price}</s>
    {/if}
    {#if sale.percent_off != null}<strong>{sale.percent_off}% off</strong>{/if}
    {#if ends}<span class="sale-ends">· ends {ends}</span>{/if}
  </p>
{/if}

<style>
  .sale-note {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 6px;
    margin: 0;
    color: var(--text-soft);
  }
  .sale-note.centered {
    justify-content: center;
  }
  .sale-badge {
    padding: 1px 8px;
    border-radius: 999px;
    color: var(--accent);
    background: color-mix(in srgb, var(--accent) 14%, transparent);
    font-weight: 600;
  }
  .hidden-label {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .sale-note s {
    color: var(--muted);
  }
  .sale-note strong {
    color: var(--text);
  }
</style>
