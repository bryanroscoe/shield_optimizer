<script lang="ts">
  import { tierOf, type Safety } from "../lib/safety";
  import type { UninstallOutcome } from "../lib/recommendation";
  import Unfurl from "./Unfurl.svelte";

  // The body of a single-app Disable or Uninstall confirmation: the engine's
  // tier as a chip, then what the action does and how the app comes back as
  // bullets. Both bullets are the consequence, so they stay visible; only the
  // longer how-to for getting an app back folds away.
  let {
    verdict,
    action,
    outcome = null,
  }: {
    verdict: Safety | null;
    action: "disable" | "uninstall";
    /// Required for Uninstall: what it removes and how the app comes back.
    outcome?: UninstallOutcome | null;
  } = $props();

  const tier = $derived(tierOf(verdict));
  let helpOpen = $state(false);
</script>

<div class="removal-detail">
  {#if tier}
    <span class="tier-chip {tier.cls}" data-testid="removal-tier">
      <span class="msr" aria-hidden="true">{tier.icon}</span>{tier.label}
    </span>
  {/if}
  <ul class="removal-points" aria-label="What happens">
    {#if action === "disable"}
      <li>Disable is reversible with Enable</li>
    {:else if outcome}
      <li>{outcome.removes}</li>
      <li>{outcome.recover}</li>
    {/if}
  </ul>
  {#if action === "uninstall" && outcome && outcome.help.length > 0}
    <Unfurl label="Getting it back" open={helpOpen} onToggle={() => (helpOpen = !helpOpen)}>
      {#each outcome.help as line (line)}
        <p>{line}</p>
      {/each}
    </Unfurl>
  {/if}
</div>

<style>
  .removal-detail {
    align-self: stretch;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 8px;
    text-align: left;
  }
  .tier-chip {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    max-width: 100%;
    box-sizing: border-box;
    padding: 3px 8px;
    border-radius: 6px;
    font-size: 12px;
    font-weight: 700;
    overflow-wrap: anywhere;
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }
  .tier-chip .msr {
    flex: none;
    font-size: 15px;
  }
  .tier-chip.safe {
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
  }
  .tier-chip.caution {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .tier-chip.blocked {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .removal-points {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding-left: 18px;
    font-size: 13px;
    color: var(--text-soft);
    line-height: 1.45;
    overflow-wrap: anywhere;
  }
</style>
