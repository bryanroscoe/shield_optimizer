<script lang="ts">
  // Resident RAM for an app that's running right now. Renders nothing when the
  // app isn't resident (mb falsy) — most apps aren't, and that's the signal.
  // `label` prefixes a small "RAM" tag so the number is unambiguous; turn it off
  // where a column header already says RAM (the Optimize table).
  let { mb, label = true }: { mb?: number; label?: boolean } = $props();
</script>

{#if mb}
  <span
    class="ram-tag"
    class:warn={mb >= 200}
    class:caution={mb >= 100 && mb < 200}
    title="Using this much RAM right now — the app is currently running (dumpsys meminfo)"
  >
    {#if label}<span class="ram-label">RAM</span>{/if}{mb.toFixed(0)} MB
  </span>
{/if}

<style>
  .ram-tag {
    font-size: 0.72rem;
    font-family: var(--mono);
    color: var(--fg-muted);
    white-space: nowrap;
  }
  .ram-label {
    font-size: 0.62rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    opacity: 0.7;
    margin-right: 0.28rem;
  }
  .ram-tag.caution {
    color: var(--warn);
  }
  .ram-tag.warn {
    color: var(--danger-strong);
  }
</style>
