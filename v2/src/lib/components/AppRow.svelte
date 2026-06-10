<script lang="ts">
  import type { Snippet } from "svelte";
  import type { AppUsage, RiskTier } from "$lib/types";
  import StateBadge from "$lib/components/StateBadge.svelte";
  import RamBadge from "$lib/components/RamBadge.svelte";
  import UsageBadge from "$lib/components/UsageBadge.svelte";

  // One catalog-app table row, shared by the App List and the Optimize wizard.
  // Dumb on purpose: data in, an `actions` snippet for the per-tab buttons —
  // the row owns layout (name/desc/pkg, state+RAM+usage cluster, risk), never
  // behaviour. Whether a cue shows (`mb`, `usage`) is the caller's call; the
  // badges already self-hide when their value is falsy.
  let {
    name,
    description,
    package: pkg,
    review = false,
    state,
    mb,
    ramLabel = true,
    usage,
    showUsage = true,
    risk,
    rowClass,
    actions,
  }: {
    name: string;
    description?: string;
    package: string;
    review?: boolean;
    state: "enabled" | "disabled" | "missing";
    mb?: number;
    ramLabel?: boolean;
    usage?: AppUsage;
    showUsage?: boolean;
    risk: RiskTier;
    rowClass?: string;
    actions: Snippet;
  } = $props();
</script>

<tr class={rowClass}>
  <td class="app-cell">
    <div class="app-name-row">
      {name}
      {#if review}
        <span class="tag review" title="Remove if you don't use it">REVIEW</span>
      {/if}
    </div>
    {#if description}
      <div class="muted small app-desc">{description}</div>
    {/if}
    <div class="muted small mono pkg-id">{pkg}</div>
  </td>
  <td class="center cluster-cell">
    <StateBadge {state} />
    {#if mb}
      <div class="cell-cue"><RamBadge {mb} label={ramLabel} /></div>
    {/if}
    {#if usage && showUsage}
      <div class="cell-cue"><UsageBadge {usage} /></div>
    {/if}
  </td>
  <td class={`risk center risk-${risk}`}>{risk.toUpperCase()}</td>
  {@render actions()}
</tr>

<style>
  /* Risk colors (.risk-safe/-medium/-high/-advanced) are global, defined in
     +layout.svelte, so they reach this scoped row. The table chrome (th/td
     borders, padding, .center) is owned by the host table; this row only
     styles the cells it fully owns. */
  td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--bg-button);
    vertical-align: middle;
  }
  td.center {
    text-align: center;
  }
  .app-cell {
    line-height: 1.3;
    /* Long system package ids are one unbreakable token; without this they
       force the column — and the whole table — wider than the viewport.
       `anywhere` also shrinks the column's min-content width. Inherited by the
       child name/pkg rows. */
    overflow-wrap: anywhere;
  }
  .app-name-row {
    font-size: 0.95rem;
    font-weight: 500;
  }
  .app-desc {
    margin-top: 0.15rem;
    font-size: 0.82rem;
    max-width: 42rem;
  }
  .pkg-id {
    margin-top: 0.1rem;
    font-size: 0.78rem;
    opacity: 0.7;
  }
  /* Small stacked cue (RAM / last-used badge) under the row's state badge. */
  .cell-cue {
    margin-top: 0.2rem;
  }
  .risk {
    font-family: ui-monospace, monospace;
    font-size: 0.78rem;
    letter-spacing: 0.04em;
  }
  .tag {
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    letter-spacing: 0.04em;
  }
  .tag.review {
    background: var(--warn-surface-2);
    color: var(--warn);
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  /* Optimize-row emphasis (passed via rowClass): skipped rows recede; rows that
     WILL be acted on get a left accent bar and a faint tint. The action/result
     cells come in through the actions snippet, so the tint reaches them via
     :global. */
  tr.dim {
    opacity: 0.78;
  }
  tr.acting :global(td) {
    background: color-mix(in srgb, var(--accent-strong) 8%, transparent);
  }
  tr.acting td:first-child {
    box-shadow: inset 3px 0 0 var(--accent-strong);
  }
  /* Review rows awaiting a human call: full opacity (unlike other skipped
     rows) + a warn accent bar — the wizard wants eyes here. */
  tr.review-flag td:first-child {
    box-shadow: inset 3px 0 0 var(--warn);
  }
</style>
