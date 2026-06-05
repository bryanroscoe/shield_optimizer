<script lang="ts">
  // Enabled / disabled / missing pill, shared by the App List's curated and
  // "everything else" tables. Self-contained styling so it looks identical
  // wherever it's dropped in.
  let { state }: { state: "enabled" | "disabled" | "missing" } = $props();
  const labels = { enabled: "Enabled", disabled: "Disabled", missing: "Missing" } as const;
</script>

<span class={`state-badge state-${state}`}>{labels[state]}</span>

<style>
  .state-badge {
    display: inline-block;
    font-size: 0.74rem;
    padding: 0.15rem 0.55rem;
    border-radius: 4px;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    font-family: ui-monospace, monospace;
  }
  .state-badge.state-enabled {
    background: var(--ok-surface);
    color: var(--ok);
  }
  .state-badge.state-disabled {
    background: var(--warn-surface-2);
    color: var(--warn);
  }
  .state-badge.state-missing {
    background: var(--bg-muted);
    color: var(--fg-faint);
  }
</style>
