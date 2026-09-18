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
    /* Hosts set `overflow-wrap: anywhere` so long package ids can break, and it
       inherits. In a flex row that let ENABLED render as a vertical column of
       seven letters and tripled the row's height. */
    flex: none;
    white-space: nowrap;
    overflow-wrap: normal;
    word-break: keep-all;
    font-size: 0.74rem;
    padding: 0.15rem 0.55rem;
    border-radius: var(--radius-sm);
    letter-spacing: 0.04em;
    text-transform: uppercase;
    font-family: var(--mono);
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
    color: var(--fg-muted);
  }
</style>
