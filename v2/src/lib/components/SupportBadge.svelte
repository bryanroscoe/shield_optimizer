<script lang="ts">
  import { parseSupport, supportLine, TIER_LABEL } from "../../../shared/support";

  // How well the app is known to work on this TV. Informational only: it
  // never changes what an action is allowed to do.
  let {
    support,
    mode = "badge",
  }: {
    support: unknown;
    /// The badge, its one-line explanation, or both.
    mode?: "badge" | "line" | "both";
  } = $props();

  const info = $derived(parseSupport(support));
  const line = $derived(supportLine(info));
</script>

{#if mode !== "line"}
  <span class="support-badge tier-{info.tier}" data-tier={info.tier} data-tip={line} data-tip-wrap>
    {TIER_LABEL[info.tier]}
  </span>
{/if}
{#if mode !== "badge"}
  <span class="support-line small" class:warn={info.tier === "beta"}>{line}</span>
{/if}

<style>
  .support-badge {
    display: inline-block;
    padding: 0.08rem 0.45rem;
    border-radius: var(--radius-sm);
    font-size: 0.72rem;
    font-weight: 600;
    letter-spacing: 0.02em;
    white-space: nowrap;
    background: var(--bg-muted);
    color: var(--fg-secondary);
  }
  .tier-fully_supported,
  .tier-supported {
    background: var(--ok-surface);
    color: var(--ok);
  }
  .tier-beta {
    background: var(--warn-surface-2);
    color: var(--warn);
  }
  .tier-not_supported {
    background: var(--danger-surface);
    color: var(--danger-text);
  }
  .support-line {
    display: block;
    margin-top: 0.2rem;
    color: var(--fg-muted);
  }
  .support-line.warn {
    color: var(--warn);
  }
</style>
