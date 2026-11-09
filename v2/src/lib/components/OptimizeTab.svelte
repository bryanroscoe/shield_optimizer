<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
  import { api, send } from "$lib/api";
  import { confirmAction } from "$lib/confirm";
  import ErrorNotice from "$lib/components/ErrorNotice.svelte";
  import HelpDetails from "$lib/components/HelpDetails.svelte";
  import RemovalHelp from "$lib/components/RemovalHelp.svelte";
  import { destructiveWarning, type SupportInfo } from "../../../shared/support";
  import { recordAction } from "../../../shared/telemetry";
  import type {
    DeviceType,
    OptimizeMode,
    OptimizePlan,
    OptimizePlanItem,
    AppUsage,
    Safety,
    SnapshotSaveOutcome,
  } from "$lib/types";
  import AppRow from "$lib/components/AppRow.svelte";
  import type { AppDetailInputs } from "$lib/app-details";
  import {
    CHECKING_LABEL,
    UNAVAILABLE_LABEL,
    isBlocked,
    verdictLabel,
    type SafetyStatus,
  } from "$lib/safety";
  import {
    canOfferUninstall,
    isReinstallable,
    recommendation,
    reviewLabel,
    suggestedMethod,
    uninstallNote,
    uninstallOutcome,
  } from "$lib/recommendation";

  let {
    serial,
    deviceType,
    appUsage,
    appDetails,
    keptPackages,
    resetToken,
    pageEpoch,
    active,
    onRunStarted,
    onStatesChanged,
    onPlanLoaded,
    onSaveSnapshot = null,
    support = null,
  }: {
    serial: string;
    deviceType: DeviceType;
    appUsage: Record<string, AppUsage>;
    /// The page's shared measurements for one package, so a row opened here
    /// shows the same RAM, storage and last-used figures as the App List.
    appDetails: (pkg: string) => AppDetailInputs;
    /// Packages the user marked "keep" in the App List. The wizard must not
    /// recommend removing something they already said they use.
    keptPackages: Set<string>;
    resetToken: number;
    pageEpoch: number;
    /// Whether the tab is on screen. The plan loads itself when it is.
    active: boolean;
    /// Called before the run's first mutation, so the page reports no
    /// cached state while packages change under it.
    onRunStarted: () => void;
    onStatesChanged: () => void;
    onPlanLoaded: () => void;
    /// Saves a snapshot of the TV as it is now, offered after a run changed it.
    onSaveSnapshot?: (() => Promise<SnapshotSaveOutcome>) | null;
    /// The TV's support tier: adds a warning before an Optimize run on an
    /// unsupported TV. Never applied to Restore.
    support?: SupportInfo | null;
  } = $props();

  /// The first failure of the last run, shown through ErrorNotice.
  let runFailure = $state<{ name: string; message: string } | null>(null);

  let optimizeMode = $state<OptimizeMode>("optimize");
  let optimizePlan = $state<OptimizePlan | null>(null);
  let optimizePlanLoading = $state(false);
  let optimizePlanErr = $state<string | null>(null);
  /// Per-package action override. A package absent from the map follows the
  /// safety-gated default; a present value is the user's explicit pick
  /// from the per-row action pills (including "skip"). The execute loop dispatches
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
  /// Changes the last Optimize run made to the TV. A snapshot taken before a
  /// run cannot undo it, so saving one is offered only after a run changed it.
  let lastOptimizeChanged = $state(0);
  let snapshotSaving = $state(false);
  let snapshotResult = $state<{ ok: boolean; message: string } | null>(null);
  let snapshotRequest = 0;
  /// What each package was last changed to under the loaded plan. The plan is
  /// not reloaded after a run, so a row run twice reads "done" twice; only the
  /// first one changed the TV.
  let appliedInPlan = new Map<string, RowAction>();
  type RunItem = {
    package: string;
    name: string;
    action: RowAction;
    verdict: Safety | null;
  };
  let safetyByPackage = $state<Record<string, SafetyStatus>>({});
  /// Package whose safety detail is expanded, or null. The wizard's own copy
  /// tells you to review safety before choosing, so the reason has to be
  /// reachable here.
  let expandedSafety = $state<string | null>(null);

  let componentEpoch = 0;
  let loadRequest = 0;
  let runRequest = 0;
  let performanceRequest = 0;
  let destroyed = false;

  type Context = {
    serial: string;
    deviceType: DeviceType;
    resetToken: number;
    pageEpoch: number;
    componentEpoch: number;
  };

  function captureContext(): Context {
    return { serial, deviceType, resetToken, pageEpoch, componentEpoch };
  }

  function contextIsCurrent(context: Context): boolean {
    return !destroyed
      && context.serial === serial
      && context.deviceType === deviceType
      && context.resetToken === resetToken
      && context.pageEpoch === pageEpoch
      && context.componentEpoch === componentEpoch;
  }

  // Bulk mutations elsewhere (App List actions, snapshot apply, panic
  // recovery) change the installed/disabled sets the plan baked in — the
  // parent bumps resetToken so the plan drops and reloads fresh next run.
  // First run just records the baseline; only a later change clears the plan.
  let seenContext: string | undefined;
  $effect(() => {
    const contextKey = `${serial}\u0000${deviceType}\u0000${resetToken}\u0000${pageEpoch}`;
    if (seenContext !== undefined && contextKey !== seenContext) {
      componentEpoch++;
      loadRequest++;
      runRequest++;
      performanceRequest++;
      optimizeAbort = true;
      optimizePlan = null;
      optimizePlanLoading = false;
      optimizeRunning = false;
      optimizeCurrent = null;
      safetyByPackage = {};
      appliedInPlan = new Map();
      resetSnapshotOffer();
    }
    seenContext = contextKey;
  });

  // Load the plan for the current mode whenever the tab is showing without
  // one — on first visit and after a reset dropped it. Optimize is already
  // the selected mode, so asking the user to click it to see anything read
  // as a toggle that did nothing. A failed load waits for an explicit click.
  $effect(() => {
    if (active && !optimizePlan && !optimizePlanLoading && !optimizePlanErr && !optimizeRunning) {
      untrack(() => void loadOptimizePlan(optimizeMode));
    }
  });

  onDestroy(() => {
    destroyed = true;
    componentEpoch++;
    loadRequest++;
    runRequest++;
    performanceRequest++;
    optimizeAbort = true;
  });

  async function loadOptimizePlan(mode: OptimizeMode) {
    if (optimizeRunning) return;
    const context = captureContext();
    const request = ++loadRequest;
    optimizeMode = mode;
    optimizePlanLoading = true;
    optimizePlanErr = null;
    optimizePlan = null;
    optimizeOverrides = {};
    optimizeProgress = {};
    optimizeFailureMessages = {};
    optimizeSummary = "";
    optimizePerfApplied = false;
    appliedInPlan = new Map();
    resetSnapshotOffer();
    safetyByPackage = {};
    try {
      const plan = await api.prepareOptimize(context.serial, context.deviceType, mode);
      if (!contextIsCurrent(context) || request !== loadRequest) return;
      optimizePlan = plan;
      // Every package the device actually has, not just the ones the plan
      // proposes to remove. A catalog package that is already disabled (such
      // as com.android.providers.tv, on the Caution list) still needs its
      // verdict; a package left out here reads "unavailable" at the row.
      const packages = plan.items
        .filter((item) => rowState(item) !== "missing")
        .map((item) => item.entry.package);
      safetyByPackage = Object.fromEntries(packages.map((pkg) => [pkg, { status: "checking" }]));
      const results = await Promise.allSettled(packages.map((pkg) => api.safetyInfo(pkg)));
      // Deliberately not comparing `optimizePlan` to `plan`: `optimizePlan` is
      // $state, so assigning an object stores a deep proxy and the identity
      // check never matches. Bailing out on it would leave every row on
      // "Checking safety", which forces Skip. The request token already
      // proves this load is the current one.
      if (!contextIsCurrent(context) || request !== loadRequest) return;
      safetyByPackage = Object.fromEntries(packages.map((pkg, index) => {
        const result = results[index];
        return result.status === "fulfilled"
          ? [pkg, { status: "ready", verdict: result.value } satisfies SafetyStatus]
          : [pkg, { status: "unavailable", reason: String(result.reason) } satisfies SafetyStatus];
      }));
      onPlanLoaded();
    } catch (e) {
      if (!contextIsCurrent(context) || request !== loadRequest) return;
      optimizePlanErr = String(e);
    } finally {
      if (contextIsCurrent(context) && request === loadRequest) optimizePlanLoading = false;
    }
  }

  /// The natural action the engine computed for an actionable row (disable /
  /// uninstall in optimize mode, enable in restore mode), or null for rows the
  /// backend marked skip (not installed / already in target state) — those
  /// aren't actionable and get no action pills.
  function naturalAction(item: OptimizePlanItem): RowAction | null {
    if (item.action.kind === "skip") return null;
    // The engine already applies `safe_method`; this repeats it so a plan
    // from any source can never make an unrecoverable uninstall the default.
    if (item.action.kind === "uninstall" && !isReinstallable(item.entry)) return "disable";
    return item.action.kind;
  }

  /// The action a row is armed with by default. This mirrors v1's per-app defaults: only
  /// apps flagged default_optimize / default_restore are pre-selected for
  /// action; everything else defaults to Skip so the wizard never removes a
  /// streaming app (or anything not on the curated default list) unless the
  /// user explicitly chooses to. Returns null for non-actionable rows.
  function defaultAction(item: OptimizePlanItem): RowAction | null {
    const natural = naturalAction(item);
    if (natural === null) return null;
    if (natural === "disable" || natural === "uninstall") {
      // A keep decision outranks the catalog's recommendation: the user has
      // already answered the question the wizard is about to ask.
      if (keptPackages.has(item.entry.package)) return "skip";
      // The same suggestion the App List and Health show, so the three
      // screens cannot disagree about what we recommend for one app.
      const rec = recommendation(
        item.entry,
        rowState(item),
        safetyByPackage[item.entry.package],
        item.install_source ?? "unknown",
      );
      return rec.kind === "act" ? rec.action : "skip";
    }
    return item.entry.default_restore ? natural : "skip";
  }

  /// The action that will actually run: the user's pick if they made
  /// one, otherwise the per-app default (or skip for non-actionable rows).
  function effectiveAction(item: OptimizePlanItem): RowAction {
    const action = optimizeOverrides[item.entry.package] ?? defaultAction(item) ?? "skip";
    if (action === "disable" || action === "uninstall") {
      const safety = safetyByPackage[item.entry.package];
      if (safety?.status !== "ready" || isBlocked(safety.verdict)) return "skip";
    }
    return action;
  }

  /// Action choices for a row, in mode-appropriate order. Restore only ever
  /// produces enable rows, so its choices are Keep / Enable; optimize rows can
  /// be downgraded (uninstall→disable) or upgraded (disable→uninstall).
  /// Keep first, then the actions that change the device — read in order,
  /// that is "do nothing, do a little, do the irreversible one".
  function actionOptions(item: OptimizePlanItem): RowAction[] {
    if (naturalAction(item) === "enable") return ["skip", "enable"];
    const safety = safetyByPackage[item.entry.package];
    if (safety?.status !== "ready" || isBlocked(safety.verdict)) return ["skip"];
    // No Uninstall on an app the store can't give back, unless it is a
    // sideload whose source the confirm prompt can name.
    return canOfferUninstall(item.entry) ? ["skip", "disable", "uninstall"] : ["skip", "disable"];
  }

  // There is deliberately no "arm everything safe": most Safe rows are apps
  // someone here chose to install, and only they know whether anyone uses
  // them. Each one is armed by hand.

  function keepAll() {
    if (!optimizePlan || optimizeRunning) return;
    for (const item of optimizePlan.items) setOptimizeAction(item.entry.package, "skip");
  }

  /// Restore only. Turning apps back on is the safe direction, so every
  /// disabled catalog app can be armed at once; removals never can.
  function reenableAll() {
    if (!optimizePlan || optimizeRunning || optimizeMode !== "restore") return;
    for (const item of optimizePlan.items) {
      if (naturalAction(item) === "enable") setOptimizeAction(item.entry.package, "enable");
    }
  }

  function removalReviewIsAvailable(item: OptimizePlanItem): boolean {
    const action = naturalAction(item);
    const safety = safetyByPackage[item.entry.package];
    return (action === "disable" || action === "uninstall")
      && safety?.status === "ready"
      && !isBlocked(safety.verdict);
  }

  function actionLabel(_item: OptimizePlanItem, action: RowAction): string {
    return { disable: "Disable", uninstall: "Uninstall", enable: "Enable", skip: "Keep" }[action];
  }

  /// Which option we recommend, shown as a small "Recommended" pill beside
  /// that option's label. A review row gets none: it carries its own amber
  /// "Review: … if unused" pill instead, because "if you don't use it" is the
  /// whole of the advice and "Recommended" would overstate it.
  function isRecommended(item: OptimizePlanItem, action: RowAction): boolean {
    if (isReviewRow(item)) return false;
    return action === defaultAction(item);
  }

  function isReviewRow(item: OptimizePlanItem): boolean {
    return !!item.entry.review && removalReviewIsAvailable(item);
  }

  /// Why a row offers no choice. A lone disabled "Keep" pill is a control
  /// that refuses to say what is wrong with it; the sentence is the point.
  function singleOptionReason(item: OptimizePlanItem): string {
    const safety = safetyByPackage[item.entry.package];
    if (!safety || safety.status === "unavailable") return UNAVAILABLE_LABEL;
    if (safety.status === "checking") return CHECKING_LABEL;
    if (isBlocked(safety.verdict)) return "Protected — never removed";
    return "No action available";
  }

  /// A failure message is arbitrary text from the device. `data-tip` is a
  /// single nowrap line, so it carries the opening clause and `title` keeps
  /// the whole thing.
  function shortTip(text: string | undefined): string | undefined {
    const trimmed = text?.trim();
    if (!trimmed) return undefined;
    return trimmed.length <= 56 ? trimmed : `${trimmed.slice(0, 55)}…`;
  }

  function setOptimizeAction(pkg: string, action: RowAction) {
    if (optimizeRunning || !optimizePlan) return;
    const item = optimizePlan.items.find((candidate) => candidate.entry.package === pkg);
    if (!item || !actionOptions(item).includes(action)) return;
    optimizeOverrides[pkg] = action;
  }

  function readySafety(pkg: string): Safety | null {
    const safety = safetyByPackage[pkg];
    return safety?.status === "ready" ? safety.verdict : null;
  }

  const CONFIRM_DETAIL_LIMIT = 8;

  async function executeOptimize() {
    // A snapshot being saved records the TV as it is, so nothing changes it
    // until the save is done.
    if (!optimizePlan || optimizeRunning || snapshotSaving) return;
    const context = captureContext();
    const plan = optimizePlan;
    const mode = optimizeMode;
    const runItems: RunItem[] = plan.items.map((item) => {
      const action = effectiveAction(item);
      const safety = safetyByPackage[item.entry.package];
      return {
        package: item.entry.package,
        name: item.entry.name,
        action,
        verdict: action === "disable" || action === "uninstall"
          ? safety?.status === "ready" ? { ...safety.verdict } : null
          : null,
      };
    });
    const selected = runItems.filter((item) => item.action !== "skip");
    if (selected.length === 0) {
      optimizeSummary = "No actions are selected.";
      return;
    }
    if (selected.some((item) => (item.action === "disable" || item.action === "uninstall") && !item.verdict)) {
      optimizeSummary = "Safety is unavailable for a selected removal. Reload the plan and review it again.";
      return;
    }
    const label = mode === "optimize" ? "Optimize" : "Restore";
    // The desktop app shows this in a native alert, which grows rather than
    // scrolls, so a long plan names the first few and points at the rows.
    const removals = selected.filter((item) => item.verdict);
    const shown = removals.length > CONFIRM_DETAIL_LIMIT ? removals.slice(0, CONFIRM_DETAIL_LIMIT - 1) : removals;
    const removalDetails = shown
      .map((item) => {
        const entry = plan.items.find((i) => i.entry.package === item.package)?.entry ?? null;
        const note = item.action === "uninstall" ? uninstallNote(item.package, entry) : null;
        return `${item.action.toUpperCase()} ${item.name} (${item.package})\nSafety: ${verdictLabel(item.verdict!)}\nReason: ${item.verdict!.reason}${note ? `\n${note}` : ""}`;
      })
      .join("\n\n")
      + (removals.length > shown.length
        ? `\n\n…and ${removals.length - shown.length} more. Each is listed with its safety and reason in the plan.`
        : "");
    const tierWarning = mode === "optimize" ? destructiveWarning(support) : null;
    const confirmation = (removalDetails
      ? `Run ${label} on ${selected.length} package(s)?\n\n${removalDetails}`
      : `Run ${label} on ${selected.length} package(s)?`) + (tierWarning ? `\n\n${tierWarning}` : "");
    if (!(await confirmAction(confirmation))
      || !contextIsCurrent(context)
      || optimizePlan !== plan
      || optimizeMode !== mode
      || optimizeRunning
      || snapshotSaving) return;

    const request = ++runRequest;
    optimizeRunning = true;
    optimizeAbort = false;
    optimizeProgress = {};
    optimizeFailureMessages = {};
    runFailure = null;
    resetSnapshotOffer();
    onRunStarted();

    let done = 0, skipped = 0, failed = 0, changed = 0;
    let safetyStopped = false;
    for (const item of runItems) {
      if (optimizeAbort || !contextIsCurrent(context) || request !== runRequest) break;
      const pkg = item.package;
      const action = item.action;
      if (action === "skip") {
        optimizeProgress[pkg] = "skipped";
        skipped++;
        continue;
      }
      optimizeCurrent = pkg;
      optimizeProgress[pkg] = "pending";
      try {
        if (action === "disable" || action === "uninstall") {
          const currentSafety = await api.safetyInfo(pkg);
          if (!contextIsCurrent(context) || request !== runRequest) return;
          if (optimizeAbort) break;
          if (!item.verdict
            || isBlocked(currentSafety)
            || currentSafety.kind !== item.verdict.kind
            || currentSafety.reason !== item.verdict.reason) {
            optimizeProgress[pkg] = "failed";
            optimizeFailureMessages[pkg] = isBlocked(currentSafety)
              ? `Protected: ${currentSafety.reason}`
              : "Safety changed after confirmation. Reload and review the plan.";
            failed++;
            safetyStopped = true;
            optimizeAbort = true;
            break;
          }
        }
        if (!contextIsCurrent(context) || request !== runRequest) return;
        if (optimizeAbort) break;
        let r: { ok: boolean; message: string };
        if (action === "disable") r = await api.disablePackage(context.serial, pkg);
        else if (action === "uninstall") r = await api.uninstallPackage(context.serial, pkg);
        else r = await api.enablePackage(context.serial, pkg);
        if (!contextIsCurrent(context) || request !== runRequest) return;
        if (r.ok) {
          optimizeProgress[pkg] = "done";
          done++;
          if (appliedInPlan.get(pkg) !== action) changed++;
          appliedInPlan.set(pkg, action);
        } else {
          optimizeProgress[pkg] = "failed";
          optimizeFailureMessages[pkg] = r.message;
          failed++;
        }
      } catch (e) {
        if (!contextIsCurrent(context) || request !== runRequest) return;
        optimizeProgress[pkg] = "failed";
        optimizeFailureMessages[pkg] = String(e);
        failed++;
        if (action === "disable" || action === "uninstall") {
          safetyStopped = true;
          optimizeAbort = true;
          break;
        }
      }
    }
    if (!contextIsCurrent(context) || request !== runRequest) return;
    optimizeCurrent = null;
    optimizeRunning = false;
    optimizeSummary = safetyStopped
      ? `Stopped before further removals. ${done} applied, ${failed} failed, ${skipped} skipped. Reload and review safety.`
      : optimizeAbort
      ? `Aborted. ${done} applied, ${failed} failed, ${skipped} skipped.`
      : `${label} complete: ${done} applied, ${failed} failed, ${skipped} skipped.`;
    if (mode === "optimize") lastOptimizeChanged = changed;
    const firstFailed = runItems.find((item) => optimizeProgress[item.package] === "failed");
    runFailure = firstFailed
      ? { name: firstFailed.name, message: optimizeFailureMessages[firstFailed.package] ?? "failed" }
      : null;
    // The per-package calls were reported one by one; this is the run as a
    // whole. Counts only, never the package names.
    if (mode === "optimize" && done + failed > 0) {
      recordAction(send, "optimize", context.serial, failed > 0
        ? { ok: false, code: "partial_failure", message: `${failed} of ${done + failed} changes failed` }
        : { ok: true });
    }
    // Keep the App List in parity — it cached states before this run.
    onStatesChanged();
  }

  async function applyPerformanceSettings() {
    if (!optimizePlan || optimizeRunning) return;
    const context = captureContext();
    const plan = optimizePlan;
    const mode = optimizeMode;
    const request = ++performanceRequest;
    const profile = optimizeMode === "optimize" ? "optimized" : "default";
    try {
      const r = await api.applyPerformanceSettings(context.serial, profile);
      if (!contextIsCurrent(context) || request !== performanceRequest || optimizePlan !== plan || optimizeMode !== mode) return;
      optimizePerfApplied = r.ok;
      optimizeSummary = optimizeSummary
        ? `${optimizeSummary} Performance: ${r.message.trim()}.`
        : `Performance: ${r.message.trim()}.`;
    } catch (e) {
      if (!contextIsCurrent(context) || request !== performanceRequest || optimizePlan !== plan || optimizeMode !== mode) return;
      optimizeSummary = optimizeSummary
        ? `${optimizeSummary} Performance failed: ${e}.`
        : `Performance failed: ${e}.`;
    }
  }

  /// Drops the offer and any save still in flight for it: a result that
  /// lands after this belongs to an offer that is gone.
  function resetSnapshotOffer() {
    snapshotRequest++;
    lastOptimizeChanged = 0;
    snapshotSaving = false;
    snapshotResult = null;
  }

  async function saveSetupSnapshot() {
    if (!onSaveSnapshot || snapshotSaving || optimizeRunning) return;
    const context = captureContext();
    const request = ++snapshotRequest;
    snapshotSaving = true;
    snapshotResult = null;
    try {
      const outcome = await onSaveSnapshot();
      if (!contextIsCurrent(context) || request !== snapshotRequest) return;
      // Cancel leaves the offer as it was, with nothing to report.
      snapshotResult = outcome.kind === "cancelled"
        ? null
        : { ok: outcome.kind === "saved", message: outcome.message };
    } catch (e) {
      if (contextIsCurrent(context) && request === snapshotRequest) {
        snapshotResult = { ok: false, message: `Failed: ${e}` };
      }
    } finally {
      if (contextIsCurrent(context) && request === snapshotRequest) snapshotSaving = false;
    }
  }

  function cancelOptimize() {
    if (!optimizeRunning) return;
    optimizeAbort = true;
  }

  /// On-device state for an Optimize row, read off the plan's skip reason so
  /// StateBadge renders meaningfully here: a not-installed skip ⇒ missing, an
  /// already-disabled skip ⇒ disabled, everything else ⇒ enabled.
  function rowState(item: OptimizePlanItem): "enabled" | "disabled" | "missing" {
    if (item.action.kind === "skip") {
      if (item.action.reason === "not_installed") return "missing";
      if (item.action.reason === "already_disabled") return "disabled";
    }
    return "enabled";
  }

  // Mirror the App List's default-on filter: most catalog apps aren't on any
  // given device, so an unfiltered plan is mostly un-actionable "Missing" rows.
  // Filters only the rendered rows — the plan, summary, and counts are untouched.
  let optimizeHideNotInstalled = $state(true);
  let visibleOptimizeItems = $derived(
    optimizePlan
      ? optimizePlan.items.filter(
          (i) => !(optimizeHideNotInstalled && rowState(i) === "missing"),
        )
      : [],
  );

  /// What the plan is made of, in the safety vocabulary — the first thing the
  /// guided header says, because "N safe, M caution, K protected" is the shape
  /// of the decision you are about to make. Counted over the rows that are
  /// actually on the device; a package the TV does not have is not a risk.
  let planVerdicts = $derived.by(() => {
    const tally = { safe: 0, caution: 0, protected: 0, unknown: 0, pending: 0 };
    for (const item of optimizePlan?.items ?? []) {
      if (rowState(item) === "missing") continue;
      const safety = safetyByPackage[item.entry.package];
      if (safety?.status !== "ready") {
        tally.pending++;
      } else if (safety.verdict.kind === "never_disable") {
        tally.protected++;
      } else if (safety.verdict.kind === "caution") {
        tally.caution++;
      } else if (safety.verdict.kind === "safe") {
        tally.safe++;
      } else {
        tally.unknown++;
      }
    }
    return tally;
  });

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
    <h2><Icon name="auto_fix_high" size={20} /> Optimize / Restore Wizard</h2>
    <div class="header-actions">
      <!-- A mode switch, not an action pair. Lime means "press this", so it
           never marks the current mode. A segmented control shows which mode
           you are in without borrowing the action colour, and matches the
           two-choice settings in Tweaks. -->
      <span class="mode-label" id="plan-mode-label">Mode</span>
      <div class="mode-box" role="group" aria-labelledby="plan-mode-label">
        <button
          class="mode-btn"
          class:active={optimizeMode === "optimize"}
          aria-pressed={optimizeMode === "optimize"}
          onclick={() => loadOptimizePlan("optimize")}
          disabled={optimizePlanLoading || optimizeRunning || snapshotSaving}
        >Optimize</button>
        <button
          class="mode-btn"
          class:active={optimizeMode === "restore"}
          aria-pressed={optimizeMode === "restore"}
          onclick={() => loadOptimizePlan("restore")}
          disabled={optimizePlanLoading || optimizeRunning || snapshotSaving}
        >Restore</button>
      </div>
    </div>
  </div>
  {#if optimizePlan}
    {@const actionableNow = optimizePlan.items.filter((i) => naturalAction(i) !== null).length}
    {@const ramNow = optimizePlan.items
      .filter((i) => naturalAction(i) !== null)
      .reduce((acc, i) => acc + (i.memory_mb ?? 0), 0)}
    <p class="muted small mono header-sub">
      {actionableNow} app{actionableNow === 1 ? "" : "s"} actionable
      {#if ramNow > 0} · ≈{ramNow.toFixed(0)} MB of RAM in play{/if}
    </p>
  {/if}
  <p class="muted small">
    {optimizeMode === "optimize"
      ? "Check each app's safety before choosing Disable or Uninstall. Anything we can't vouch for is set to Keep until you say otherwise."
      : "Re-enable catalog apps that are disabled on this TV. Only the ones we recommend bringing back start armed; Re-enable all disabled arms the rest."}
  </p>
  {#if optimizeMode === "restore"}
    <HelpDetails label="What Restore covers">
      <p>Restore lists the apps in our catalog that are disabled on this TV. It never reinstalls an app that was uninstalled.</p>
      <p>To turn on a disabled app that isn't in the catalog, use Enable on its row under Everything else in the App List. Emergency Recovery on the Overview tab re-enables every disabled package on the TV at once.</p>
    </HelpDetails>
  {/if}

  {#if optimizePlanErr}
    <div class="error">{optimizePlanErr}</div>
  {/if}

  {#if optimizePlanLoading || (!optimizePlan && !optimizePlanErr)}
    <p class="muted">Querying device…</p>
  {:else if !optimizePlan}
    <p class="muted">Pick Optimize or Restore to try again.</p>
  {:else}
    {@const actionable = optimizePlan.items.filter((i) => effectiveAction(i) !== "skip").length}
    {@const totalRunning = optimizePlan.items
      .filter((i) => naturalAction(i) !== null)
      .reduce((acc, i) => acc + (i.memory_mb ?? 0), 0)}
    <!-- Two steps, numbered, so the screen says the table comes before the
         Run button at its foot. -->
    <ol class="plan-steps">
      <li class="plan-step">
        <span class="step-num mono" aria-hidden="true">1</span>
        <div class="step-body">
          <p class="step-text">
            Review the plan below — {planVerdicts.safe} safe to remove, {planVerdicts.caution} caution,
            {planVerdicts.protected} protected{#if planVerdicts.unknown > 0}, {planVerdicts.unknown} unknown{/if}
          </p>
          <div class="plan-bulk">
            {#if optimizeMode === "restore"}
              {@const disabledRows = optimizePlan.items.filter((i) => naturalAction(i) === "enable").length}
              <button
                class="small-action subtle"
                data-tip="Arm Enable on every disabled app in this list"
                onclick={reenableAll}
                disabled={optimizeRunning || disabledRows === 0}
              >Re-enable all disabled</button>
            {/if}
            <button
              class="small-action subtle"
              data-tip="Set every row back to Keep"
              onclick={keepAll}
              disabled={optimizeRunning}
            >Keep all</button>
          </div>
        </div>
      </li>
      <li class="plan-step">
        <span class="step-num mono" aria-hidden="true">2</span>
        <div class="step-body">
          <p class="step-text">
            Run — the button is under the table. {actionable} of {optimizePlan.items.length} rows are armed.
          </p>
        </div>
      </li>
    </ol>
    {#if optimizeRunning}
      {@const done = Object.values(optimizeProgress).filter((v) => v !== "pending").length}
      <!-- The current package lives in this panel, not in a button label, so
           nothing reflows per step and Cancel stays in one place. -->
      <div class="run-card" role="status" aria-live="polite">
        <div class="run-count"><strong>{done}</strong> of {actionable} applied</div>
        <div class="run-bar"><span style={`width:${actionable ? (done / actionable) * 100 : 0}%`}></span></div>
        {#if optimizeCurrent}
          <p class="run-current">
            <span class="muted small">Current package</span>
            <span class="mono">{optimizeCurrent}</span>
          </p>
        {/if}
        <button class="run-cancel" onclick={cancelOptimize}>Cancel remaining</button>
      </div>
    {/if}

    <div class="app-toolbar">
      <label class="inline-check">
        <input type="checkbox" bind:checked={optimizeHideNotInstalled} />
        Hide not installed
      </label>
      {#if optimizeMode === "optimize"}
        <RemovalHelp />
      {/if}
    </div>

    <!-- svelte-ignore a11y_no_noninteractive_tabindex (Keyboard users need to focus the overflow region to scroll the table.) -->
    <div class="plan-table-scroll" role="region" aria-label="Optimize app choices" tabindex="0">
    <table class="optimize-table">
      <thead>
        <tr>
          <th><span class="sr-only">Details</span></th>
          <th>App</th>
          <th>Verdict &amp; source</th>
          <th class="right">RAM</th>
          <th class="right">Last used</th>
          <th>Action</th>
        </tr>
      </thead>
      <tbody>
        {#each visibleOptimizeItems as item (item.entry.package)}
          {@const skip = skipReasonLabel(item)}
          {@const progress = optimizeProgress[item.entry.package]}
          {@const eff = effectiveAction(item)}
          <AppRow
            name={item.entry.name}
            description={item.entry.optimize_description}
            package={item.entry.package}
            review={item.entry.review}
            reviewedAt={item.entry.reviewed_at}
            state={rowState(item)}
            mb={(() => {
              const d = appDetails(item.entry.package);
              return d.measures.memory.status === "ready" ? d.memoryMb : (item.memory_mb ?? undefined);
            })()}
            usage={appUsage[item.entry.package]}
            showUsage={naturalAction(item) !== null}
            safety={readySafety(item.entry.package)}
            safetyStatus={safetyByPackage[item.entry.package]?.status ?? "unavailable"}
            safetyUnavailableReason={(() => {
              const st = safetyByPackage[item.entry.package];
              return st?.status === "unavailable" ? st.reason : undefined;
            })()}
            columns={6}
            detailOpen={expandedSafety === item.entry.package}
            onToggleDetail={() =>
              (expandedSafety =
                expandedSafety === item.entry.package ? null : item.entry.package)}
            details={appDetails(item.entry.package)}
            rowClass={!skip && isReviewRow(item)
              ? eff === "skip" ? "review-flag" : "review-flag acting"
              : eff === "skip"
                ? "dim"
                : !skip
                  ? "acting"
                  : undefined}
          >
            {#snippet actions()}
            <td>
              {#if progress}
                <!-- The run's outcome sits with the choice it acted on: a
                     separate Result column scrolled off narrow windows. -->
                <div class="row-result">
                  {#if progress === "done"}
                    <span class="tag installed"><Icon name="check" size={14} /> DONE</span>
                  {:else if progress === "pending"}
                    <span class="muted small">…</span>
                  {:else if progress === "skipped"}
                    <span class="muted small">skipped</span>
                  {:else if progress === "failed"}
                    <span
                      class="tag"
                      style="background:var(--danger-surface); color:var(--danger-text)"
                      data-tip={shortTip(optimizeFailureMessages[item.entry.package])}
                      data-tip-align="end"
                      title={optimizeFailureMessages[item.entry.package] ?? ""}
                    >FAILED</span>
                  {/if}
                </div>
              {/if}
              {#if skip}
                <span class="terminal-reason">{skip}</span>
              {:else}
                {@const opts = actionOptions(item)}
                {#if opts.length === 1}
                  <!-- One option is not a choice, so instead of a lone greyed-out
                       "Keep" pill the sentence says why the row has no choice. -->
                  <span class="terminal-reason">{singleOptionReason(item)}</span>
                {:else}
                  <!-- One pill per option with a radio mark, so the armed state
                       is legible before you read the fill colour — a segmented
                       control relies on the fill alone, and on a table of
                       sixteen rows that is a lot of colour to decode. -->
                  <div class="action-radio" role="group" aria-label={`Action for ${item.entry.name}`}>
                    {#each opts as opt (opt)}
                      <button
                        class="radio-pill"
                        class:active={eff === opt}
                        class:will-remove={opt === "uninstall"}
                        class:will-keep={opt === "skip"}
                        aria-pressed={eff === opt}
                        disabled={optimizeRunning}
                        onclick={() => setOptimizeAction(item.entry.package, opt)}
                      >
                        <Icon
                          name={eff === opt ? "radio_button_checked" : "radio_button_unchecked"}
                          size={15}
                          fill={eff === opt}
                        />
                        <span class="radio-label">{actionLabel(item, opt)}</span>
                        {#if isRecommended(item, opt)}
                          <span class="plan-caption mono" data-tip="What we recommend for this app">Recommended</span>
                        {/if}
                      </button>
                    {/each}
                  </div>
                {/if}
                {#if isReviewRow(item)}
                  <span
                    class="review-pill mono"
                    data-tip="Check the last-used column — act only if nobody here uses it"
                  >{reviewLabel(suggestedMethod(item.entry, item.install_source ?? "unknown"))}</span>
                {:else if safetyByPackage[item.entry.package]?.status === "unavailable"}
                  <div class="muted small review-hint">Reload the plan to retry safety.</div>
                {/if}
                {#if eff === "uninstall"}
                  <!-- Whether an uninstall can be taken back depends on the
                       app, so the row armed with it says which case it is. -->
                  <div class="small uninstall-note">{uninstallOutcome(item.entry.package, item.entry).short}</div>
                {/if}
              {/if}
            </td>
            {/snippet}
          </AppRow>
        {/each}
      </tbody>
    </table>
    </div>

    <!-- The action sits after the list it acts on, so Run is not reachable
         before the plan has been read. -->
    <div class="apply-foot">
    {#if optimizeMode === "optimize"}
      <!-- What can be taken back, said once, where the run starts. -->
      <p class="undo-line small">
        <Icon name="restore" size={15} />
        <span>Undo a Disable with Restore mode. Uninstall can't always be undone, so check the row's note before you run.</span>
      </p>
      <HelpDetails label="Undoing a run">
        <p><strong>Disable:</strong> switch Mode to Restore, choose Re-enable all disabled or Enable on the rows you want, then Run plan. Restore covers catalog apps only; Emergency Recovery on the Overview tab re-enables every disabled package on the TV.</p>
        <p><strong>Uninstall:</strong> removes the app for this TV's user, and Restore can't bring it back. A row set to Uninstall says how to get it back: from the Play Store, from the site a sideloaded app came from, or not at all for a discontinued service.</p>
      </HelpDetails>
    {/if}
    <div class="apply-row">
      <!-- A disabled button never gets :hover, so the reason it is disabled has
           to hang off something that does. -->
      <span
        class="tip-wrap"
        data-tip={actionable === 0
          ? "Nothing is armed — choose an action on a row"
          : snapshotSaving
            ? "Waiting for the snapshot to finish saving"
            : `Apply ${actionable} change${actionable === 1 ? "" : "s"} to this TV`}
      >
      <button
        class="primary"
        onclick={executeOptimize}
        disabled={optimizeRunning || actionable === 0 || snapshotSaving}
      >
        {optimizeRunning
          ? "Running…"
          : `Run plan · ${actionable} item${actionable === 1 ? "" : "s"}`}
      </button>
      </span>
      {#if optimizeSummary && !optimizeRunning}
        <button
          onclick={applyPerformanceSettings}
          disabled={optimizePerfApplied}
          data-tip={optimizeMode === "optimize" ? "Set animation scales to 0.5×" : "Reset animation scales to 1×"}
        >
          {optimizePerfApplied ? "Performance applied" : (optimizeMode === "optimize" ? "Apply 0.5× animations" : "Reset animations to 1×")}
        </button>
      {/if}
    </div>
    </div>
    {#if optimizeSummary}
      <p class="muted small mono action-message">{optimizeSummary}</p>
    {/if}
    {#if onSaveSnapshot && optimizeMode === "optimize" && lastOptimizeChanged > 0 && !optimizeRunning}
      <!-- Offered after the run, not before: a snapshot records the result,
           so it can set the TV up this way again but never take this run back. -->
      <div class="snapshot-offer">
        <button onclick={saveSetupSnapshot} disabled={snapshotSaving || snapshotResult?.ok === true}>
          <Icon name="save" size={16} />
          {snapshotSaving ? "Saving…" : snapshotResult?.ok ? "Snapshot saved" : "Save this setup as a snapshot"}
        </button>
        {#if snapshotResult}
          <p class="small snapshot-result" class:failed={!snapshotResult.ok} role="status">{snapshotResult.message}</p>
        {/if}
        <HelpDetails label="What a snapshot is for">
          <p>Use it to set this TV up the same way again, for example after a factory reset: Restore this snapshot on the Snapshot tab disables the apps it recorded and puts back its Home app and settings.</p>
          <p>It disables every app it recorded, with no per-app choice, on whichever TV you apply it to. Don't apply it to another TV unless nobody there uses those apps.</p>
          <p>It can't undo this run. It never re-enables or reinstalls an app, and apps this run uninstalled aren't in it. To turn apps back on, use Restore mode here.</p>
        </HelpDetails>
      </div>
    {/if}
    {#if runFailure && !optimizeRunning}
      <ErrorNotice error={runFailure.message} action={`${runFailure.name}`} {support} />
    {/if}
  {/if}
</div>

<style>
  .card {
    min-width: 0;
  }
  /* Shared scoped utilities duplicated from the page; global muted and button
     rules live in the layout and are inherited. */
  .header-actions {
    display: flex;
    gap: 0.8rem;
    align-items: center;
  }
  th.right {
    text-align: right;
  }

  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-family: var(--mono);
    font-size: 0.85rem;
  }
  table {
    width: 100%;
    border-collapse: collapse;
    font-size: 0.9rem;
  }
  .plan-table-scroll {
    width: 100%;
    min-width: 0;
    max-width: 100%;
    overflow-x: auto;
  }
  .plan-table-scroll:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
  }
  .optimize-table {
    min-width: 56rem;
  }
  .row-result {
    margin-bottom: 0.35rem;
  }
  th, td {
    text-align: left;
    padding: 0.5rem 0.6rem;
    border-bottom: 1px solid var(--border);
    vertical-align: middle;
  }
  th {
    color: var(--fg-muted);
    font-weight: 500;
    font-size: 0.8rem;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .tag {
    font-size: 0.7rem;
    padding: 0.15rem 0.5rem;
    border-radius: var(--radius-sm);
    letter-spacing: 0.04em;
  }
  .tag.installed { background: var(--ok-surface); color: var(--ok); }
  .action-message {
    margin-top: 0.4rem;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    word-break: break-word;
  }
  .apply-row {
    display: flex;
    align-items: center;
    gap: 0.8rem;
    margin: 0.8rem 0 0.4rem;
    flex-wrap: wrap;
  }
  .app-toolbar {
    display: flex;
    align-items: center;
    gap: 1rem;
    flex-wrap: wrap;
    margin: 0.6rem 0;
  }
  .inline-check {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.9rem;
    white-space: nowrap;
  }

  /* A pill track with a raised thumb, so it reads as a two-way switch rather
     than as two buttons that happen to sit together. */
  .mode-box {
    display: inline-flex;
    align-items: center;
    gap: 2px;
    padding: 3px;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-pill);
  }
  .mode-btn {
    border: 1px solid transparent;
    background: none;
    border-radius: var(--radius-pill);
    color: var(--fg-muted);
    padding: 0.25rem 0.9rem;
    font-size: 0.85rem;
    transition: background 0.15s, color 0.15s;
  }
  .mode-btn:hover:not(.active):not(:disabled) {
    background: var(--bg-button-hover);
    color: var(--fg-primary);
  }
  /* A segmented control with both halves in surface grey reads as chrome, not
     as a choice you have made. The armed half carries a visible edge and a
     shadow so it looks pressed, and the label beside it says what is being
     chosen. */
  .mode-btn.active {
    background: var(--bg-surface);
    border-color: var(--accent);
    color: var(--fg-primary);
    font-weight: 600;
    box-shadow: 0 1px 0 0 color-mix(in srgb, var(--accent) 45%, transparent);
  }
  .mode-label {
    font-family: var(--mono);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-muted);
  }
  .header-sub {
    margin: 0 0 0.3rem;
  }
  .apply-foot {
    margin-top: 1.2rem;
    padding-top: 1rem;
    border-top: 1px solid var(--border);
  }
  .apply-foot .apply-row {
    margin-top: 0.6rem;
  }
  .undo-line {
    display: flex;
    align-items: flex-start;
    gap: 0.4rem;
    margin: 0;
    color: var(--fg-secondary);
  }
  .undo-line > :first-child {
    flex: none;
    margin-top: 0.1rem;
  }
  .snapshot-offer {
    margin-top: 0.8rem;
    padding: 0.7rem 0.9rem;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-inset);
  }
  .snapshot-offer :global(.help-details) {
    margin: 0.5rem 0 0;
  }
  .snapshot-result {
    margin: 0.5rem 0 0;
    color: var(--fg-secondary);
    overflow-wrap: anywhere;
  }
  .snapshot-result.failed {
    color: var(--danger-text);
  }
  /* The guided strip. Numbered, because the order matters and the Run button
     is a screen away from the table it applies. */
  .plan-steps {
    display: flex;
    flex-wrap: wrap;
    gap: 0.6rem 1.5rem;
    margin: 1rem 0 0.5rem;
    padding: 0.7rem 0.9rem;
    list-style: none;
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    background: var(--bg-inset);
  }
  .plan-step {
    display: flex;
    align-items: flex-start;
    gap: 0.5rem;
    min-width: 0;
  }
  .step-num {
    flex: none;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    width: 1.3rem;
    height: 1.3rem;
    border-radius: 50%;
    background: var(--bg-button);
    color: var(--fg-secondary);
    font-size: 0.72rem;
  }
  .step-body {
    min-width: 0;
  }
  .step-text {
    margin: 0.1rem 0 0;
    font-size: 0.85rem;
  }
  /* Visually hidden, still announced — the caret column has no visible head. */
  .sr-only {
    position: absolute;
    width: 1px;
    height: 1px;
    overflow: hidden;
    clip-path: inset(50%);
    white-space: nowrap;
  }
  .tip-wrap {
    display: inline-flex;
  }
  .plan-bulk {
    display: flex;
    gap: 0.5rem;
    flex-wrap: wrap;
    margin-top: 0.4rem;
  }
  .small-action {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
  }
  .small-action.subtle {
    background: transparent;
    border-color: var(--border);
    color: var(--fg-muted);
  }
  .small-action.subtle:hover:not(:disabled) {
    background: var(--bg-button);
    color: var(--fg-secondary);
  }
  /* Progress as a panel, not as a reflowing button label. */
  .run-card {
    margin: 0.5rem 0 1rem;
    padding: 1rem 1.1rem;
    border: 1px solid color-mix(in srgb, var(--accent) 40%, transparent);
    border-radius: var(--radius-lg);
    background: var(--bg-inset);
  }
  .run-count {
    font-size: 1.1rem;
  }
  .run-count strong {
    font-family: var(--mono);
    font-size: 1.6rem;
  }
  .run-bar {
    height: 6px;
    margin: 0.6rem 0;
    border-radius: var(--radius-pill);
    background: var(--bg-button);
    overflow: hidden;
  }
  .run-bar span {
    display: block;
    height: 100%;
    background: var(--accent-strong);
    transition: width 0.2s;
  }
  .run-current {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    margin: 0 0 0.8rem;
  }
  .run-current .mono {
    color: var(--accent);
    overflow-wrap: anywhere;
  }
  .run-cancel {
    width: 100%;
    justify-content: center;
  }

  .uninstall-note {
    margin-top: 0.25rem;
    max-width: 14rem;
    color: var(--fg-secondary);
    line-height: 1.35;
  }
  /* Width-capped to the action pills' footprint so the text centers under the
     control rather than the (wider) table cell. */
  .review-hint {
    margin-top: 0.2rem;
    font-size: 0.72rem;
    max-width: 11rem;
    text-align: center;
  }
  /* A radio group, not a segmented control: the mark says which one is chosen
     before the fill colour does, which is what makes a column of sixteen rows
     scannable. The pills stack rather than sitting in a trough, so the labels
     stay readable when the Action column narrows. */
  .action-radio {
    display: flex;
    flex-direction: column;
    gap: 0.15rem;
    align-items: flex-start;
  }
  .radio-pill {
    display: inline-flex;
    align-items: center;
    gap: 0.35rem;
    padding: 0.18rem 0.55rem 0.18rem 0.35rem;
    border: 1px solid transparent;
    border-radius: var(--radius-pill);
    background: none;
    color: var(--fg-muted);
    font-size: 0.78rem;
    white-space: nowrap;
    cursor: pointer;
  }
  .radio-pill:hover:not(.active):not(:disabled) {
    background: var(--bg-button-hover);
    color: var(--fg-primary);
  }
  /* Lime is this app's one action colour: it marks the choice you have made,
     never every choice you could make. */
  .action-radio .radio-pill.active {
    background: var(--accent-strong);
    border-color: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  /* Keeping an app is not an action, so an armed Keep is a neutral surface. */
  .action-radio .radio-pill.will-keep.active {
    background: var(--bg-surface);
    border-color: var(--border);
    color: var(--fg-primary);
  }
  /* Uninstall is the one that does not come back; armed, it says so. */
  .action-radio .radio-pill.will-remove.active {
    background: var(--danger);
    border-color: var(--danger);
    color: var(--danger-ink);
  }
  .action-radio .radio-pill.active:disabled {
    opacity: 1;
  }
  /* Our pick, named rather than dotted. A small outlined pill so it reads as
     a label on the option, not as a second option. */
  .plan-caption {
    padding: 0 0.3rem;
    border: 1px solid currentColor;
    border-radius: var(--radius-pill);
    font-size: 0.58rem;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    opacity: 0.8;
  }
  /* The review advice, in the same amber as the row it tints. */
  .review-pill {
    display: inline-block;
    margin-top: 0.3rem;
    padding: 0.1rem 0.5rem;
    border: 1px solid var(--warn-border);
    border-radius: var(--radius-pill);
    background: var(--warn-surface-2);
    color: var(--warn);
    font-size: 0.68rem;
    white-space: nowrap;
  }
  /* Terminal rows (not installed / already in target state) can't be acted on —
     a neutral pill, distinct from an armed Keep pill so "nothing to do here"
     doesn't read like "you chose to skip this". */
  .terminal-reason {
    display: inline-block;
    font-size: 0.74rem;
    padding: 0.15rem 0.5rem;
    border-radius: var(--radius-sm);
    background: var(--bg-muted);
    color: var(--fg-faint);
    letter-spacing: 0.02em;
  }
</style>
