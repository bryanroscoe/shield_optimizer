<script lang="ts">
  import HelpDetails from "./HelpDetails.svelte";

  // A list of packages a run left alone or couldn't change, one per line. Up
  // to two keep their reason beside them; a longer list folds the reasons so
  // the names stay scannable.
  export type PackageOutcome = { package: string; reason?: string | null };

  let {
    label,
    items,
    warn = false,
    reasonsLabel = "Reasons",
  }: { label: string; items: PackageOutcome[]; warn?: boolean; reasonsLabel?: string } = $props();

  const INLINE_REASONS = 2;
  const withReasons = $derived(items.filter((item) => item.reason));
  const inline = $derived(items.length <= INLINE_REASONS);
</script>

<div class="package-outcome" class:warn-text={warn} data-package-outcome>
  <p class="label">{label}</p>
  <ul>
    {#each items as item, i (i)}
      <li><span class="mono">{item.package}</span>{#if inline && item.reason}{" "}({item.reason}){/if}</li>
    {/each}
  </ul>
  {#if !inline && withReasons.length > 0}
    <HelpDetails label={reasonsLabel}>
      <ul>
        {#each withReasons as item, i (i)}
          <li><span class="mono">{item.package}</span>: {item.reason}</li>
        {/each}
      </ul>
    </HelpDetails>
  {/if}
</div>

<style>
  .package-outcome {
    margin: 0.35rem 0;
    min-width: 0;
  }
  .warn-text {
    color: var(--warn);
  }
  .label {
    margin: 0;
  }
  ul {
    margin: 0.2rem 0 0;
    padding-left: 1.2rem;
    overflow-wrap: anywhere;
  }
  .package-outcome :global(.help-details) {
    margin: 0.3rem 0 0;
  }
</style>
