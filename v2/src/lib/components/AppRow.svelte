<script lang="ts">
  import type { Snippet } from "svelte";
  import type { AppUsage, Safety } from "$lib/types";
  import { safetySourceLabel } from "../../../shared/safety";
  import {
    safetyLabel as sharedSafetyLabel,
    safetyReason as sharedSafetyReason,
    statusOf,
  } from "$lib/safety";
  import StateBadge from "$lib/components/StateBadge.svelte";
  import RamBadge from "$lib/components/RamBadge.svelte";
  import UsageBadge from "$lib/components/UsageBadge.svelte";
  import Icon from "$lib/components/Icon.svelte";
  import AppDetailPanel from "$lib/components/AppDetailPanel.svelte";
  import type { AppDetailInputs } from "$lib/app-details";
  import { openContextMenu } from "$lib/contextmenu";

  // One catalog-app table row, shared by the App List and the Optimize wizard.
  // Dumb on purpose: data in, an `actions` snippet for the per-tab buttons —
  // the row owns layout (name/desc/pkg, state+RAM+usage cluster, safety), never
  // behaviour. Whether a cue shows (`mb`, `usage`) is the caller's call; the
  // badges already self-hide when their value is falsy.
  let {
    name,
    description,
    package: pkg,
    review = false,
    reviewedAt,
    extraTag,
    extraTagKind = "neutral",
    userInstalled = false,
    state: pkgState,
    mb,
    usage,
    showUsage = true,
    safety = null,
    safetyStatus = "unavailable",
    safetyUnavailableReason,
    detailOpen = false,
    onToggleDetail,
    columns = 6,
    rowClass,
    details,
    actions,
  }: {
    name: string;
    description?: string;
    package: string;
    review?: boolean;
    /// The catalog entry's `reviewed_at`. Shown only beside a verdict that
    /// came from the catalog, since a protected-list verdict was not that review.
    reviewedAt?: string;
    /// One extra tag beside the name — "SYSTEM" / "3RD-PARTY" for the
    /// non-catalog table. Kept generic so the row does not learn about
    /// package inventories.
    extraTag?: string;
    extraTagKind?: "neutral" | "ok";
    /// A package the device reports as non-system. It still gets no verdict —
    /// we have not reviewed it — but "not in any reviewed list" reads as a
    /// warning when the honest answer is "you put this here".
    userInstalled?: boolean;
    state: "enabled" | "disabled" | "missing" | null;
    mb?: number;
    usage?: AppUsage;
    showUsage?: boolean;
    safety?: Safety | null;
    safetyStatus?: "checking" | "ready" | "unavailable";
    /// Why the lookup failed, when it did. Shown instead of a generic
    /// sentence so an unavailable verdict is diagnosable rather than just
    /// alarming.
    safetyUnavailableReason?: string;
    /// Whether this row's safety detail is expanded. Owned by the caller so
    /// only one row opens at a time. The `state` prop is destructured to
    /// `pkgState` so nothing in this file is bound to the name `state`, which
    /// would otherwise make every `$state(...)` read as a store subscription.
    detailOpen?: boolean;
    onToggleDetail?: () => void;
    /// How many columns the host table has, so the detail row spans all of
    /// them. Six is the App List's shape (caret, app, verdict, RAM, last used,
    /// actions); the Optimize wizard adds a Result column and passes seven.
    columns?: number;
    rowClass?: string;
    /// What the shared detail panel shows under the verdict: the page's one
    /// set of measurements, each with when it was read. Every table passes the
    /// same object so the panel reads the same everywhere.
    details?: AppDetailInputs;
    actions: Snippet;
  } = $props();

  /// The whole row is the disclosure. Anything interactive inside it — the
  /// verbs, the tools, the copy button, the Optimize action pills, a link —
  /// answers for itself, so a click that landed on one of those is not a
  /// request to open the reason. Asking the event where it landed keeps that
  /// true for controls a future caller adds: a missed `stopPropagation` in
  /// some other file would otherwise silently toggle the row under the user.
  function onRowClick(event: MouseEvent) {
    const target = event.target as Element | null;
    if (target?.closest("button, a, input, select, textarea, label")) return;
    onToggleDetail?.();
  }

  let lookup = $derived(statusOf(safetyStatus, safety, safetyUnavailableReason));

  function safetyLabel(): string {
    return sharedSafetyLabel(lookup);
  }

  function safetyClass(): string {
    if (safetyStatus !== "ready" || !safety) return "unavailable";
    return safety.kind === "never_disable" ? "protected" : safety.kind;
  }

  /// Where the verdict came from, in words. A bare "Unknown" conflates "we
  /// rated this high risk" with "we have never seen this package"; those want
  /// very different treatment from the reader.
  function safetySource(): string {
    if (safetyStatus !== "ready" || !safety) {
      return safetyUnavailableReason?.trim()
        ? `Could not be checked — ${safetyUnavailableReason.trim()}`
        : "Could not be checked";
    }
    if (safety.source === "no_record" && userInstalled) {
      // Not an upgrade of the verdict — the kind stays Unknown. This only
      // says which kind of unknown it is, which is the part that decides
      // whether you can reason about it at all.
      return "Not in any reviewed list · you installed this, not the system";
    }
    return safetySourceLabel(safety.source);
  }

  function safetyReason(): string {
    return sharedSafetyReason(lookup);
  }

  let pkgCopied = $state(false);

  async function copyPkgFromMenu(): Promise<string> {
    await navigator.clipboard.writeText(pkg);
    return "Copied package name";
  }

  async function copyPkg() {
    try {
      await navigator.clipboard.writeText(pkg);
      pkgCopied = true;
      setTimeout(() => (pkgCopied = false), 1500);
    } catch {
      /* clipboard blocked — the id is still on screen to select by hand */
    }
  }
</script>

<!-- The row is clickable but it is not a control: the caret button in the first
     cell is the keyboard target and carries `aria-expanded`, so the row itself
     needs no key handler of its own. -->
<!-- svelte-ignore a11y_click_events_have_key_events -->
<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<tr
  class={rowClass}
  class:row-open={detailOpen}
  onclick={onRowClick}
  oncontextmenu={(e) => openContextMenu(e, [{ label: "Copy package name", run: copyPkgFromMenu }])}
>
  <!-- A disclosure looks like one: a caret in its own narrow column, pointing
       right when closed and down when open. It used to sit inside the verdict
       chip, where it read as decoration on the verdict rather than as the
       control for the row. -->
  <td class="caret-cell">
    <button
      class="row-caret"
      class:open={detailOpen}
      aria-expanded={detailOpen}
      aria-label={detailOpen ? `Hide why ${name} has this verdict` : `Show why ${name} has this verdict`}
      data-tip={detailOpen ? "Hide the full reason" : "Show the full reason"}
      onclick={() => onToggleDetail?.()}
    ><span class="caret-glyph" aria-hidden="true"><Icon name="chevron_right" size={16} /></span></button>
  </td>
  <!-- Board 11.5's app cell: identity on line one, what it does on line two,
       the id you paste into a bug report on line three. The state pill rides
       with the name rather than owning a column of its own. -->
  <td class="app-cell">
    <div class="app-name-row">
      <span class="app-name">{name}</span>
      {#if pkgState}
        <StateBadge state={pkgState} />
      {:else}
        <span class="state-unavailable">STATE UNAVAILABLE</span>
      {/if}
      {#if review}
        <span class="tag review" data-tip="Check whether you actually use this app">REVIEW</span>
      {/if}
      {#if extraTag}
        <span class={`tag tag-${extraTagKind}`}>{extraTag}</span>
      {/if}
    </div>
    {#if description}
      <!-- A full sentence, clipped to one line here: `title` rather than
           `data-tip`, which is a nowrap single-line strip. -->
      <div class="muted app-desc" title={description}>{description}</div>
    {/if}
    <div class="pkg-line">
      <span class="mono pkg-id">{pkg}</span>
      <button
        class="pkg-copy"
        data-tip={pkgCopied ? "Copied" : "Copy the package id"}
        aria-label={`Copy package id ${pkg}`}
        onclick={copyPkg}
      >
        <Icon name={pkgCopied ? "check" : "content_copy"} size={14} />
      </button>
    </div>
  </td>
  <!-- Verdict AND where it came from, as the board has it. A bare chip makes
       "we rated this" and "we have never seen it" look identical; the source
       line is the difference. Click still opens the full reason. -->
  <td
    class={`verdict-cell safety-${safetyClass()}`}
    data-verdict={lookup.status === "ready" ? lookup.verdict.kind : lookup.status}
  >
    <!-- Still a button: the verdict is the thing you came to read, so it opens
         the reason too. The caret it used to carry now lives in its own column
         at the head of the row. -->
    <button
      class="safety-toggle"
      class:open={detailOpen}
      tabindex={-1}
      onclick={() => onToggleDetail?.()}
    >
      <span class="verdict-top">
        <span class="verdict-chip">{safetyLabel()}</span>
      </span>
      <span class="verdict-source">{safetySource()}</span>
    </button>
  </td>
  <td class="num-cell">
    {#if mb && mb > 0}
      <RamBadge {mb} label={false} />
    {:else}
      <span class="muted dash">—</span>
    {/if}
  </td>
  <td class="num-cell">
    {#if usage && showUsage}
      <UsageBadge {usage} bare />
    {:else}
      <!-- A dash is "we have no record", which is not the same as "never
           opened" — usagestats ages out and resets on a wipe. -->
      <span
        class="muted dash"
        data-tip="No usage record · Android keeps about a year of history"
      >—</span>
    {/if}
  </td>
  {@render actions()}
</tr>
{#if detailOpen}
  <tr class="safety-detail-row">
    <td colspan={columns}>
      <AppDetailPanel
        package={pkg}
        kindLabel={safetyLabel()}
        kindClass={safetyClass()}
        reason={safetyReason()}
        {description}
        reviewedAt={lookup.status === "ready" && lookup.verdict.source === "reviewed_catalog"
          ? reviewedAt
          : undefined}
        serial={details?.serial}
        measures={details?.measures}
        memoryMb={details?.memoryMb}
        usage={details?.usage}
        storage={details?.storage}
        onRemeasure={details?.onRemeasure}
      />
    </td>
  </tr>
{/if}

<style>
  /* The table chrome (th/td borders, padding, .center) is owned by the host
     table; this row only styles the cells it fully owns.
     `--bg-button` is a button FILL, not a line — against a card it is about
     1.1:1 and effectively invisible. These cells drew their divider with it
     while the actions cell, which comes in from the host, drew the same
     divider with `--border`. The result was one visible short line under the
     actions and nothing under the rest, which reads as a stray line rather
     than as a row divider. */
  td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  td.center {
    text-align: center;
  }
  /* The whole row opens the reason, so it has to look like it does. The tint
     is deliberately below the hover tint of the controls inside it — the row
     saying "I can be clicked" must not outshout the button saying "click me". */
  tr:not(.safety-detail-row) {
    cursor: pointer;
  }
  tr:not(.safety-detail-row):hover {
    background: color-mix(in srgb, var(--fg-primary) 4%, transparent);
  }
  tr.row-open {
    background: color-mix(in srgb, var(--fg-primary) 6%, transparent);
  }
  /* A caret column wide enough for the button and nothing more. */
  .caret-cell {
    width: 1%;
    /* The column pays for itself out of the App column's share rather than
       widening the table — the App List already ran right up to the card's
       edge, and a table that overflows its card is worse than a tight caret. */
    padding-left: 0.25rem;
    padding-right: 0;
    text-align: center;
  }
  .row-caret {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    padding: 0.15rem;
    border: 1px solid transparent;
    border-radius: var(--radius-sm);
    background: none;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .row-caret:hover {
    background: var(--bg-button-hover);
    color: var(--fg-primary);
  }
  /* First cell in the table, so a centred tooltip hangs off the card's left
     edge. Anchor it to the button instead. */
  .row-caret::after {
    left: 0;
    transform: none;
  }
  /* The glyph turns, not the button: rotating the button rotates its `data-tip`
     pseudo-element with it, and the tooltip came out running down the page one
     letter per line. */
  .caret-glyph {
    display: inline-flex;
    transition: transform 0.15s;
  }
  /* Right when closed, down when open — the standard disclosure, rather than
     a second glyph that has to be learned. */
  .row-caret.open {
    color: var(--fg-secondary);
  }
  .row-caret.open .caret-glyph {
    transform: rotate(90deg);
  }
  /* Two lines per row, fixed. Three stacked lines showed about five apps at a
     time on a list that runs to hundreds. */
  .app-cell {
    /* The flexible column: every other cell is width:1% + nowrap, so this one
       takes what is left. Without max-width:0 its min-content is the full
       description — the table then grows past the card and pushes State,
       Safety and Action off the right edge. This is what makes the ellipsis
       fire instead of the table widening. */
    /* Claim the space as well as cap it: max-width alone lets the nowrap
       cells win every pixel and clips the name row to nothing. */
    width: 38%;
    max-width: 0;
    line-height: 1.35;
    overflow-wrap: anywhere;
  }
  .app-name-row {
    display: flex;
    align-items: center;
    gap: 0.5rem;
    font-size: 0.95rem;
    font-weight: 600;
    min-width: 0;
    /* At narrow widths the tags drop under the name instead of being clipped
       out of existence: ENABLED and REVIEW are the two things the row is read
       for, and a hidden overflow simply deleted them. The clip stays — it is
       what stops a long name widening the table. */
    flex-wrap: wrap;
    row-gap: 0.25rem;
    overflow: hidden;
  }
  /* The name never wraps and never shrinks: it is the one thing you read to
     decide, and `.app-cell`'s inherited `overflow-wrap: anywhere` will happily
     break "Funimation" across two lines if the item is allowed to narrow. The
     package id beside it absorbs whatever width is left and ellipsises. */
  .app-name {
    flex: none;
    white-space: nowrap;
    overflow-wrap: normal;
  }
  .app-desc {
    margin-top: 0.1rem;
    font-size: 0.8rem;
    /* One line — the first clause is what decides yes or no, and the rest is
       in the detail panel. */
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .pkg-line {
    display: flex;
    align-items: center;
    gap: 0.2rem;
    min-width: 0;
    margin-top: 0.1rem;
  }
  .pkg-id {
    flex: 0 1 auto;
    min-width: 0;
    font-size: 0.75rem;
    font-weight: 400;
    color: var(--fg-muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  /* Only on hover, so three hundred rows are not three hundred buttons. */
  .pkg-copy {
    flex: none;
    display: inline-flex;
    align-items: center;
    padding: 0.1rem 0.25rem;
    border: none;
    background: none;
    color: var(--fg-muted);
    opacity: 0;
    cursor: pointer;
  }
  tr:hover .pkg-copy,
  .pkg-copy:focus-visible {
    opacity: 1;
  }
  .pkg-copy:hover {
    color: var(--accent);
    background: none;
  }
  /* RAM and Last used get their own columns, as on board 11.5 — a number
     buried under a pill is a number nobody scans down. */
  .num-cell {
    width: 1%;
    text-align: right;
    white-space: nowrap;
    font-family: var(--mono);
    font-size: 0.78rem;
  }
  .num-cell .dash {
    opacity: 0.5;
  }
  .state-unavailable {
    font-family: var(--mono);
    font-size: 0.78rem;
    letter-spacing: 0.04em;
  }
  .state-unavailable,
  .safety-unavailable,
  .safety-unknown {
    color: var(--fg-muted);
  }
  .safety-protected {
    color: var(--danger);
  }
  .safety-caution {
    color: var(--warn);
  }
  .safety-safe {
    color: var(--ok);
  }
  .verdict-cell {
    width: 22%;
    max-width: 0;
  }
  /* The whole cell is the target, not just the chip: a two-line block with a
     click area the size of one word is a disclosure you have to aim at. */
  .safety-toggle {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 0.2rem;
    width: 100%;
    background: none;
    border: 1px solid transparent;
    padding: 0.4rem 0.5rem;
    margin: -0.4rem -0.5rem;
    font: inherit;
    color: inherit;
    text-align: left;
    cursor: pointer;
    border-radius: var(--radius-md);
  }
  .safety-toggle:hover {
    background: var(--bg-button-hover);
    border-color: var(--border);
  }
  .verdict-top {
    display: flex;
    align-items: center;
    gap: 0.3rem;
  }
  /* Filled chip, per the board — the old treatment coloured the text only, so
     PROTECTED and UNKNOWN were typographically identical at a glance. */
  .verdict-chip {
    display: inline-flex;
    align-items: center;
    padding: 0.12rem 0.5rem;
    border: 1px solid currentColor;
    border-radius: var(--radius-sm);
    background: color-mix(in srgb, currentColor 14%, transparent);
    font-family: var(--mono);
    font-size: 0.72rem;
    letter-spacing: 0.05em;
    text-transform: uppercase;
    white-space: nowrap;
  }
  /* Two lines of source, then clipped; the full sentence is one click away. */
  .verdict-source {
    display: -webkit-box;
    -webkit-line-clamp: 2;
    line-clamp: 2;
    -webkit-box-orient: vertical;
    overflow: hidden;
    color: var(--fg-muted);
    font-size: 0.75rem;
    line-height: 1.35;
    letter-spacing: normal;
    text-transform: none;
    overflow-wrap: anywhere;
  }
  .safety-detail-row td {
    padding-top: 0;
  }
  .tag {
    /* `.app-cell` sets `overflow-wrap: anywhere` so long package ids can break,
       and that inherits. In a flex row it let REVIEW break between every
       letter and render as a vertical column of six characters — so the tag
       opts back out and refuses to shrink. */
    flex: none;
    white-space: nowrap;
    overflow-wrap: normal;
    word-break: keep-all;
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: var(--radius-xs);
    letter-spacing: 0.04em;
  }
  .tag.review {
    background: var(--warn-surface-2);
    color: var(--warn);
  }
  .tag-neutral {
    background: var(--bg-button);
    color: var(--fg-muted);
  }
  .tag-ok {
    background: var(--ok-surface);
    color: var(--ok);
  }
  .mono {
    font-family: var(--mono);
  }
  /* Optimize-row emphasis (passed via rowClass): skipped rows recede; rows that
     WILL be acted on get a left accent bar. The full-row tint went with the
     select: now that the armed segment is visible on every row, tinting the
     row as well said the same thing twice and made a plan of twelve rows read
     as one solid block of lime. */
  tr.dim {
    opacity: 0.78;
  }
  tr.acting td:first-child {
    box-shadow: inset 3px 0 0 var(--accent-strong);
  }
  /* Review rows awaiting a human call: full opacity (unlike other skipped
     rows) + a warn accent bar — the wizard wants eyes here. */
  tr.review-flag {
    background: var(--warn-surface);
  }
  tr.review-flag:hover {
    background: color-mix(in srgb, var(--warn) 16%, transparent);
  }
  tr.review-flag td:first-child {
    box-shadow: inset 3px 0 0 var(--warn);
  }
</style>
