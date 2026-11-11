<script lang="ts">
  import type { AppUsage } from "$lib/types";
  import { tip } from "$lib/tip";
  import { isStaleUsage, usageLabel, usageLabelBare } from "$lib/usage";

  // "last used" cue for the Review / remove-if-unused decision. Renders nothing
  // until usage data is loaded. Never-opened and long-idle apps read as stale
  // (highlighted) — the candidates to remove. Interpretation lives in
  // $lib/usage so the Optimize review callout can't drift from this badge.
  // A row badge: the Last used column header holds the keyboard explanation.
  let { usage, bare = false }: { usage?: AppUsage; bare?: boolean } = $props();
</script>

{#if usage}
  <span
    class="usage-tag"
    class:stale={isStaleUsage(usage)}
    use:tip={{ text: "Last foreground use · Android keeps about a year of history", row: true }}
  >
    {bare ? usageLabelBare(usage) : usageLabel(usage)}
  </span>
{/if}

<style>
  .usage-tag {
    font-size: 0.72rem;
    color: var(--fg-muted);
  }
  .usage-tag.stale {
    color: var(--warn);
  }
</style>
