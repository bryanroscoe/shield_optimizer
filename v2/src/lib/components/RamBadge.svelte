<script lang="ts">
  // Resident RAM for an app that's running right now. Renders nothing when the
  // app isn't resident (mb falsy) — most apps aren't, and that's the signal.
  let { mb }: { mb?: number } = $props();
</script>

{#if mb}
  <span
    class="ram-tag"
    class:warn={mb >= 200}
    class:caution={mb >= 100 && mb < 200}
    title="Resident RAM right now (dumpsys meminfo)"
  >{mb.toFixed(0)} MB</span>
{/if}

<style>
  .ram-tag {
    font-size: 0.72rem;
    font-family: ui-monospace, monospace;
    color: var(--fg-muted);
  }
  .ram-tag.caution {
    color: var(--warn);
  }
  .ram-tag.warn {
    color: var(--danger-strong);
  }
</style>
