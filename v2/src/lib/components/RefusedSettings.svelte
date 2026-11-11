<script lang="ts">
  import HelpDetails from "$lib/components/HelpDetails.svelte";
  import Icon from "$lib/components/Icon.svelte";

  let { keys }: { keys: string[] } = $props();
</script>

{#if keys.length > 0}
  <div class="refused-settings" data-plan-item="refused">
    <p>
      <Icon name="warning" size={16} />
      {keys.length === 1
        ? "1 setting in this file is not one snapshots restore and will be skipped."
        : `${keys.length} settings in this file are not ones snapshots restore and will be skipped.`}
    </p>
    <HelpDetails label="Skipped settings">
      <p>Snapshots restore only the settings this app saves. These are never written:</p>
      <ul>
        {#each keys as key, i (i)}
          <li class="mono">{key}</li>
        {/each}
      </ul>
    </HelpDetails>
  </div>
{/if}

<style>
  .refused-settings {
    background: var(--warn-surface);
    border: 1px solid var(--warn-border);
    color: var(--warn);
    padding: 0.6rem 0.9rem;
    border-radius: var(--radius-md);
    margin: 0.6rem 0;
    font-size: 0.9rem;
  }
  .refused-settings > p {
    margin: 0;
    display: flex;
    align-items: center;
    gap: 0.4rem;
  }
  .refused-settings :global(.help-details) {
    margin: 0.4rem 0 0;
  }
  ul {
    margin: 0.3rem 0 0;
    padding-left: 1.2rem;
    overflow-wrap: anywhere;
  }
  .mono {
    font-family: var(--mono);
  }
</style>
