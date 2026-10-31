<script lang="ts">
  // The connected TV's support tier: a badge plus its one-line explanation,
  // or a prominent banner when the TV isn't supported. Informational only —
  // it never gates an action. `unknown` says "not verified", nothing more.
  import { TIER_LABEL, supportLine, type SupportInfo } from "../../../shared/support";

  let { support, compact = false }: { support: SupportInfo; compact?: boolean } = $props();

  const line = $derived(supportLine(support));
  const icon = $derived(
    support.tier === "not_supported"
      ? "block"
      : support.tier === "beta"
        ? "report"
        : support.tier === "unknown"
          ? "help"
          : "check_circle",
  );
</script>

{#if support.tier === "not_supported"}
  <div class="support-banner" role="alert" data-tier={support.tier}>
    <span class="msr" aria-hidden="true">{icon}</span>
    <div class="sb-body">
      <span class="tier-badge not_supported">{TIER_LABEL[support.tier]}</span>
      <span class="sb-line">{line}</span>
    </div>
  </div>
{:else}
  <div class="support-row" class:compact data-tier={support.tier}>
    <span class={`tier-badge ${support.tier}`}>
      <span class="msr" aria-hidden="true">{icon}</span>{TIER_LABEL[support.tier]}
    </span>
    {#if !compact}<span class="sr-line">{line}</span>{/if}
  </div>
{/if}

<style>
  .support-banner {
    display: flex;
    gap: 12px;
    align-items: flex-start;
    padding: 14px 16px;
    border-radius: 16px;
    background: color-mix(in srgb, var(--danger) 12%, transparent);
    border: 1px solid color-mix(in srgb, var(--danger) 45%, transparent);
    color: var(--text);
    font-size: 13px;
    line-height: 1.45;
  }
  .support-banner > .msr {
    color: var(--danger);
    font-size: 22px;
    flex: none;
  }
  .sb-body {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .sb-line {
    font-weight: 600;
  }
  .support-row {
    display: flex;
    align-items: center;
    gap: 10px;
    flex-wrap: wrap;
    font-size: 12.5px;
    color: var(--text-soft);
    line-height: 1.4;
    margin-bottom: 6px;
  }
  .sr-line {
    flex: 1;
    min-width: 0;
  }
  .tier-badge {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.02em;
    white-space: nowrap;
    border: 1px solid var(--line);
    background: var(--surface-2);
    color: var(--muted);
    flex: none;
    align-self: flex-start;
  }
  .tier-badge .msr {
    font-size: 14px;
  }
  .tier-badge.fully_supported,
  .tier-badge.supported {
    color: var(--teal);
    border-color: color-mix(in srgb, var(--teal) 35%, transparent);
  }
  .tier-badge.beta {
    color: var(--amber);
    border-color: color-mix(in srgb, var(--amber) 35%, transparent);
  }
  .tier-badge.not_supported {
    color: var(--danger);
    border-color: color-mix(in srgb, var(--danger) 45%, transparent);
  }
</style>
