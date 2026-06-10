<script lang="ts">
  import type { AppUsage } from "$lib/types";
  import { isStaleUsage, usageLabel } from "$lib/usage";

  // "last used" cue for the Review / remove-if-unused decision. Renders nothing
  // until usage data is loaded. Never-opened and long-idle apps read as stale
  // (highlighted) — the candidates to remove. Interpretation lives in
  // $lib/usage so the Optimize review callout can't drift from this badge.
  let { usage }: { usage?: AppUsage } = $props();
</script>

{#if usage}
  <span
    class="usage-tag"
    class:stale={isStaleUsage(usage)}
    title="Last foreground use from usagestats. History is limited (~1 year of rolling buckets) and resets on a factory wipe, so 'no recent use' may just mean it aged out."
  >
    {usageLabel(usage)}
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
