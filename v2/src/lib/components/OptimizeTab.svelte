<script lang="ts">
  import { api } from "$lib/api";
  import type { DeviceType, OptimizeMode, OptimizePlan, OptimizePlanItem, AppUsage } from "$lib/types";
  import RamBadge from "$lib/components/RamBadge.svelte";
  import UsageBadge from "$lib/components/UsageBadge.svelte";

  let {
    serial,
    deviceType,
    appUsage,
    resetToken,
    onStatesChanged,
    onPlanLoaded,
  }: {
    serial: string;
    deviceType: DeviceType;
    appUsage: Record<string, AppUsage>;
    resetToken: number;
    onStatesChanged: () => void;
    onPlanLoaded: () => void;
  } = $props();

  let optimizeMode = $state<OptimizeMode>("optimize");
  let optimizePlan = $state<OptimizePlan | null>(null);
  let optimizePlanLoading = $state(false);
  let optimizePlanErr = $state<string | null>(null);
  /// Per-package action override. A package absent from the map follows the
  /// plan's recommended action; a present value is the user's explicit pick
  /// from the per-row dropdown (including "skip"). The execute loop dispatches
  /// on effectiveAction(), so disable/uninstall/enable/skip all just work.
  type RowAction = "disable" | "uninstall" | "enable" | "skip";
  let optimizeOverrides = $state<Record<string, RowAction>>({});
  let optimizeRunning = $state(false);
  let optimizeCurrent = $state<string | null>(null); // package currently being acted on
  let optimizeProgress = $state<Record<string, "pending" | "done" | "skipped" | "failed">>({});
  let optimizeFailureMessages = $state<Record<string, string>>({});
  let optimizeAbort = $state(false);
  let optimizeSummary = $state<string>("");
  let optimizePerfApplied = $state<boolean>(false);

  // Bulk mutations elsewhere (App List actions, snapshot apply, panic
  // recovery) change the installed/disabled sets the plan baked in — the
  // parent bumps resetToken so the plan drops and reloads fresh next run.
  // First run just records the baseline; only a later change clears the plan.
  let seenResetToken: number | undefined;
  $effect(() => {
    const token = resetToken;
    if (seenResetToken !== undefined && token !== seenResetToken) {
      optimizePlan = null;
    }
    seenResetToken = token;
  });

  async function loadOptimizePlan(mode: OptimizeMode) {
    optimizeMode = mode;
    optimizePlanLoading = true;
    optimizePlanErr = null;
    optimizePlan = null;
    optimizeOverrides = {};
    optimizeProgress = {};
    optimizeFailureMessages = {};
    optimizeSummary = "";
    optimizePerfApplied = false;
    try {
      optimizePlan = await api.prepareOptimize(serial, deviceType, mode);
    } catch (e) {
      optimizePlanErr = String(e);
    } finally {
      optimizePlanLoading = false;
    }
    // Lazy "last used" cues for the Review rows (shared with the App List).
    onPlanLoaded();
  }

  /// The natural action the engine computed for an actionable row (disable /
  /// uninstall in optimize mode, enable in restore mode), or null for rows the
  /// backend marked skip (not installed / already in target state) — those
  /// aren't actionable and get no dropdown.
  function naturalAction(item: OptimizePlanItem): RowAction | null {
    return item.action.kind === "skip" ? null : item.action.kind;
  }

  /// What the dropdown defaults to. This mirrors v1's per-app defaults: only
  /// apps flagged default_optimize / default_restore are pre-selected for
  /// action; everything else defaults to Skip so the wizard never removes a
  /// streaming app (or anything not on the curated default list) unless the
  /// user explicitly chooses to. Returns null for non-actionable rows.
  function defaultAction(item: OptimizePlanItem): RowAction | null {
    const natural = naturalAction(item);
    if (natural === null) return null;
    const isDefault =
      optimizeMode === "optimize" ? item.entry.default_optimize : item.entry.default_restore;
    return isDefault ? natural : "skip";
  }

  /// The action that will actually run: the user's dropdown pick if they made
  /// one, otherwise the per-app default (or skip for non-actionable rows).
  function effectiveAction(item: OptimizePlanItem): RowAction {
    return optimizeOverrides[item.entry.package] ?? defaultAction(item) ?? "skip";
  }

  /// Dropdown choices for a row, in mode-appropriate order. Restore only ever
  /// produces enable rows, so its menu is Enable / Skip; optimize rows can be
  /// downgraded (uninstall→disable) or upgraded (disable→uninstall).
  function actionOptions(item: OptimizePlanItem): RowAction[] {
    return naturalAction(item) === "enable"
      ? ["enable", "skip"]
      : ["disable", "uninstall", "skip"];
  }

  function actionLabel(item: OptimizePlanItem, action: RowAction): string {
    const base = { disable: "Disable", uninstall: "Uninstall", enable: "Enable", skip: "Skip" }[action];
    return action === defaultAction(item) ? `${base} (recommended)` : base;
  }

  function setOptimizeAction(pkg: string, action: RowAction) {
    optimizeOverrides[pkg] = action;
  }

  async function executeOptimize() {
    if (!optimizePlan) return;
    const total = optimizePlan.items.filter((i) => effectiveAction(i) !== "skip").length;
    if (total === 0) {
      optimizeSummary = "Nothing to do — every item is in its target state.";
      return;
    }
    const label = optimizeMode === "optimize" ? "Optimize" : "Restore";
    if (!confirm(`Run ${label} on ${total} package(s)? Disabled packages can be re-enabled via Emergency Recovery.`)) return;

    optimizeRunning = true;
    optimizeAbort = false;
    optimizeProgress = {};
    optimizeFailureMessages = {};

    let done = 0, skipped = 0, failed = 0;
    for (const item of optimizePlan.items) {
      if (optimizeAbort) break;
      const pkg = item.entry.package;
      const action = effectiveAction(item);
      if (action === "skip") {
        optimizeProgress[pkg] = "skipped";
        skipped++;
        continue;
      }
      optimizeCurrent = pkg;
      optimizeProgress[pkg] = "pending";
      try {
        let r: { ok: boolean; message: string };
        if (action === "disable") r = await api.disablePackage(serial, pkg);
        else if (action === "uninstall") r = await api.uninstallPackage(serial, pkg);
        else r = await api.enablePackage(serial, pkg);
        if (r.ok) {
          optimizeProgress[pkg] = "done";
          done++;
        } else {
          optimizeProgress[pkg] = "failed";
          optimizeFailureMessages[pkg] = r.message;
          failed++;
        }
      } catch (e) {
        optimizeProgress[pkg] = "failed";
        optimizeFailureMessages[pkg] = String(e);
        failed++;
      }
    }
    optimizeCurrent = null;
    optimizeRunning = false;
    optimizeSummary = optimizeAbort
      ? `Aborted. ${done} applied, ${failed} failed, ${skipped} skipped.`
      : `${label} complete: ${done} applied, ${failed} failed, ${skipped} skipped.`;
    // Keep the App List in parity — it cached states before this run.
    onStatesChanged();
  }

  async function applyPerformanceSettings() {
    if (!optimizePlan) return;
    const profile = optimizeMode === "optimize" ? "optimized" : "default";
    try {
      const r = await api.applyPerformanceSettings(serial, profile);
      optimizePerfApplied = r.ok;
      optimizeSummary = optimizeSummary
        ? `${optimizeSummary} Performance: ${r.message.trim()}.`
        : `Performance: ${r.message.trim()}.`;
    } catch (e) {
      optimizeSummary = optimizeSummary
        ? `${optimizeSummary} Performance failed: ${e}.`
        : `Performance failed: ${e}.`;
    }
  }

  function skipReasonLabel(item: OptimizePlanItem): string | null {
    if (item.action.kind !== "skip") return null;
    switch (item.action.reason) {
      case "not_installed": return "Not installed";
      case "already_disabled": return "Already disabled";
      case "already_enabled": return "Already enabled";
      case "user_choice": return "Skipped";
    }
  }
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-optimize" aria-labelledby="tab-optimize">
  <div class="card-header">
    <h2>Optimize / Restore Wizard</h2>
    <div class="header-actions">
      <button
        class:primary={optimizeMode === "optimize"}
        onclick={() => loadOptimizePlan("optimize")}
        disabled={optimizePlanLoading || optimizeRunning}
      >Optimize</button>
      <button
        class:primary={optimizeMode === "restore"}
        onclick={() => loadOptimizePlan("restore")}
        disabled={optimizePlanLoading || optimizeRunning}
      >Restore</button>
    </div>
  </div>
  <p class="muted small">
    {optimizeMode === "optimize"
      ? "Disable or uninstall bloat per the device's app catalog. Each row defaults to the recommended action — change it (Disable / Uninstall / Skip) per row, then Run."
      : "Re-enable everything that's currently disabled per the device's app catalog. Set any row to Skip to leave it, then Run. Restore is reversible by running Optimize again."}
  </p>

  {#if optimizePlanErr}
    <div class="error">{optimizePlanErr}</div>
  {/if}

  {#if optimizePlanLoading}
    <p class="muted">Querying device…</p>
  {:else if !optimizePlan}
    <p class="muted">Pick Optimize or Restore to load the plan.</p>
  {:else}
    {@const actionable = optimizePlan.items.filter((i) => effectiveAction(i) !== "skip").length}
    {@const totalRunning = optimizePlan.items
      .filter((i) => naturalAction(i) !== null)
      .reduce((acc, i) => acc + (i.memory_mb ?? 0), 0)}
    <div class="plan-summary">
      <strong>{actionable}</strong> of {optimizePlan.items.length} items will be acted on.
      {#if totalRunning > 0}
        <span class="muted">≈ {totalRunning.toFixed(0)} MB of RAM in play.</span>
      {/if}
    </div>
    <div class="apply-row">
      <button
        class="primary"
        onclick={executeOptimize}
        disabled={optimizeRunning || actionable === 0}
      >
        {optimizeRunning ? `Running… (${optimizeCurrent ?? ""})` : `Run ${optimizeMode === "optimize" ? "Optimize" : "Restore"}`}
      </button>
      {#if optimizeRunning}
        <button onclick={() => (optimizeAbort = true)}>Abort</button>
      {/if}
      {#if optimizeSummary && !optimizeRunning}
        <button
          onclick={applyPerformanceSettings}
          disabled={optimizePerfApplied}
          title={optimizeMode === "optimize" ? "Set animation scales to 0.5×" : "Reset animation scales to 1×"}
        >
          {optimizePerfApplied ? "Performance applied" : (optimizeMode === "optimize" ? "Apply 0.5× animations" : "Reset animations to 1×")}
        </button>
      {/if}
    </div>
    {#if optimizeSummary}
      <p class="muted small mono action-message">{optimizeSummary}</p>
    {/if}

    <table class="optimize-table">
      <thead>
        <tr>
          <th>App</th>
          <th>RAM</th>
          <th>Risk</th>
          <th>Action</th>
          <th>Result</th>
        </tr>
      </thead>
      <tbody>
        {#each optimizePlan.items as item (item.entry.package)}
          {@const skip = skipReasonLabel(item)}
          {@const progress = optimizeProgress[item.entry.package]}
          {@const eff = effectiveAction(item)}
          <tr class:dim={eff === "skip"} class:acting={!skip && eff !== "skip"}>
            <td>
              <div class="app-name">
                {item.entry.name}
                {#if item.entry.default_optimize}
                  <span class="tag installed">DEFAULT</span>
                {:else if item.entry.review}
                  <span class="tag review" title="Remove if you don't use it">REVIEW</span>
                {/if}
              </div>
              {#if item.entry.optimize_description}
                <div class="muted small app-desc">{item.entry.optimize_description}</div>
              {/if}
              <div class="muted small mono">{item.entry.package}</div>
              {#if appUsage[item.entry.package] && naturalAction(item) !== null}
                <div class="cell-cue"><UsageBadge usage={appUsage[item.entry.package]} /></div>
              {/if}
            </td>
            <td class="num">
              <!-- Live RAM for installed + enabled rows only (naturalAction
                   null ⇒ not-installed / already-disabled, where a residual
                   process isn't reclaimable). -->
              {#if item.memory_mb && naturalAction(item) !== null}
                <RamBadge mb={item.memory_mb} label={false} />
              {:else}
                <span class="muted">—</span>
              {/if}
            </td>
            <td class={`risk risk-${item.entry.risk}`}>{item.entry.risk.toUpperCase()}</td>
            <td>
              {#if skip}
                <span class="terminal-reason">{skip}</span>
              {:else}
                <select
                  class="action-select"
                  class:will-skip={eff === "skip"}
                  class:will-remove={eff === "uninstall"}
                  class:will-act={eff === "disable" || eff === "enable"}
                  value={eff}
                  onchange={(e) =>
                    setOptimizeAction(
                      item.entry.package,
                      (e.currentTarget as HTMLSelectElement).value as RowAction,
                    )}
                  disabled={optimizeRunning}
                >
                  {#each actionOptions(item) as opt (opt)}
                    <option value={opt}>{actionLabel(item, opt)}</option>
                  {/each}
                </select>
              {/if}
            </td>
            <td>
              {#if progress === "done"}
                <span class="tag installed">✓ DONE</span>
              {:else if progress === "pending"}
                <span class="muted small">…</span>
              {:else if progress === "skipped"}
                <span class="muted small">skipped</span>
              {:else if progress === "failed"}
                <span class="tag" style="background:var(--danger-surface); color:var(--danger-text)" title={optimizeFailureMessages[item.entry.package] ?? ""}>FAILED</span>
              {/if}
            </td>
          </tr>
        {/each}
      </tbody>
    </table>
  {/if}
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button, .risk-* colors) live in the layout and are inherited. */
  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.2rem;
  }
  .card h2 {
    margin: 0 0 0.8rem;
    font-size: 1.1rem;
  }
  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .header-actions {
    display: flex;
    gap: 0.8rem;
    align-items: center;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  th, td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--bg-button);
    vertical-align: middle;
  }
  th {
    color: var(--fg-muted);
    font-weight: 500;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  td.num {
    font-family: ui-monospace, monospace;
    text-align: right;
    width: 100px;
  }
  td.risk {
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
  .tag.installed { background: var(--ok-surface); color: var(--ok); }
  .tag.review { background: var(--warn-surface-2); color: var(--warn); }
  /* Small stacked cue (RAM / last-used badge) under a row's state badge. */
  .cell-cue {
    margin-top: 0.2rem;
  }
  .action-message {
    margin-top: 0.4rem;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    word-break: break-word;
  }
  .apply-row {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    margin: 0.8rem 0 0.4rem;
    flex-wrap: wrap;
  }

  /* Optimize-specific styles. */
  .plan-summary {
    margin: 0.4rem 0;
    padding: 0.5rem 0.8rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    font-size: 0.9rem;
  }
  /* Skipped rows recede; rows that WILL be acted on stand out with a left
     accent bar and a faint tint so the consequential rows are obvious at a
     glance (the dim-everything approach was too subtle to read). */
  .optimize-table tr.dim {
    opacity: 0.78;
  }
  .optimize-table tr.acting td {
    background: color-mix(in srgb, var(--accent-strong) 8%, transparent);
  }
  .optimize-table tr.acting td:first-child {
    box-shadow: inset 3px 0 0 var(--accent-strong);
  }
  .action-select {
    font-size: 0.85rem;
    padding: 0.25rem 0.5rem;
    min-width: 9.5rem;
  }
  /* Color the dropdown by what it will do, so each row's intent is legible at
     a glance: muted italic for Skip, accent for disable/enable, danger for the
     destructive uninstall. */
  .action-select.will-skip {
    color: var(--fg-muted);
    font-style: italic;
  }
  .action-select.will-act {
    color: var(--accent);
    font-weight: 500;
  }
  .action-select.will-remove {
    color: var(--danger-strong);
    font-weight: 500;
  }
  /* Terminal rows (not installed / already in target state) can't be acted on —
     a neutral pill, distinct from the italic "Skip (recommended)" dropdown so
     "nothing to do here" doesn't read like "you chose to skip this". */
  .terminal-reason {
    display: inline-block;
    font-size: 0.74rem;
    padding: 0.15rem 0.5rem;
    border-radius: 4px;
    background: var(--bg-muted);
    color: var(--fg-faint);
    letter-spacing: 0.02em;
  }
</style>
