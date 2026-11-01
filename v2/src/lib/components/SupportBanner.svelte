<script lang="ts">
  import Icon from "./Icon.svelte";
  import { parseSupport } from "../../../shared/support";

  // The prominent "this TV isn't supported" banner. Renders nothing for any
  // other tier; their one-line explanation lives beside the badge.
  let { support }: { support: unknown } = $props();
  const info = $derived(parseSupport(support));
</script>

{#if info.tier === "not_supported"}
  <div class="support-banner" role="alert">
    <Icon name="warning" size={18} />
    <span>
      <strong>This TV isn't supported:</strong>
      {info.note ?? "the app can't make its changes on it."}
      You can still look around and run Recovery, but changes may fail on this TV.
    </span>
  </div>
{/if}

<style>
  .support-banner {
    display: flex;
    gap: 0.6rem;
    align-items: flex-start;
    padding: 0.7rem 0.9rem;
    margin: 0.6rem 0;
    border: 1px solid var(--danger-border);
    border-radius: var(--radius-sm);
    background: var(--danger-surface);
    color: var(--danger-surface-text);
  }
</style>
