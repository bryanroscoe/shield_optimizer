<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import { isBlocked, reasonOf, tierOf } from "../lib/safety";
  import type {
    OptimizeMode,
    OptimizePlan,
    OptimizePlanItem,
    Safety,
  } from "../lib/types";
  import BottomTabs from "../components/BottomTabs.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  let loading = $state(true);
  let error = $state("");
  // Free tier: prepare_optimize returns "LOCKED:<feature>". We surface a
  // clearly-labeled locked state — never a fabricated debloat list.
  let locked = $state(false);
  let plan = $state<OptimizePlan | null>(null);
  let mode = $state<OptimizeMode>("optimize");
  let showPaywall = $state(false);

  let activeTab = $state<"recommended" | "optional" | "all">("recommended");
  let tabTouched = $state(false);
  // Local selection (which rows are ticked). The backend safety gate remains
  // authoritative for every individual mutation during apply.
  let selected = $state<Set<string>>(new Set());
  let selectionVersion = $state(0);
  let selectionTouched = new Set<string>();
  let applying = $state(false);
  let cancelRequested = $state(false);
  let applyDone = $state(0);
  let applyTotal = $state(0);
  let applyCurrent = $state("");

  type PlanIdentity = {
    serial: string;
    generation: number;
    mode: OptimizeMode;
    token: number;
  };
  type FrozenItem = { item: OptimizePlanItem; verdict: Safety | null };
  type ApplyIntent = PlanIdentity & {
    selectionVersion: number;
    items: FrozenItem[];
    message: string;
    warning: string;
  };
  let planIdentity = $state<PlanIdentity | null>(null);
  let applyIntent = $state<ApplyIntent | null>(null);
  let planRequest = 0;
  let applyRequest = 0;
  let destroyed = false;
  let observedSession = "";

  // Core tiers for the plan's actionable packages. The catalog's own `risk`
  // tag is editorial metadata and disagrees with the classifier (e.g.
  // com.google.android.feedback is catalog "safe" but core "caution"), so the
  // chip a row renders always comes from here.
  let safetyMap = $state<Record<string, Safety>>({});
  let safetyLoading = $state(false);
  let safetyFailed = $state(false);
  let safetyRequest = 0;

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  function showToast(message: string, type: "success" | "error" | "info" = "info") {
    toast = message;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 4200);
  }

  onDestroy(() => {
    destroyed = true;
    ++planRequest;
    ++safetyRequest;
    ++applyRequest;
    applyIntent = null;
    cancelRequested = true;
    if (applying) session.applyInProgress = false;
    clearTimeout(toastTimer);
  });

  async function loadPlan(nextMode: OptimizeMode = mode) {
    const device = session.connectedDevice;
    const serial = session.serial;
    const generation = session.generation;
    const request = ++planRequest;
    mode = nextMode;
    activeTab = "recommended";
    tabTouched = false;
    loading = true;
    error = "";
    locked = false;
    plan = null;
    planIdentity = null;
    applyIntent = null;
    selected = new Set();
    selectionTouched = new Set();
    ++selectionVersion;
    safetyMap = {};
    safetyFailed = false;
    ++safetyRequest;
    if (!device || !serial || !session.isConnected) {
      error = "No TV connected.";
      loading = false;
      return;
    }
    try {
      const p = await api.prepareOptimize(
        serial,
        device.device_type,
        nextMode,
      );
      if (
        destroyed ||
        request !== planRequest ||
        serial !== session.serial ||
        generation !== session.generation ||
        nextMode !== mode ||
        !session.isConnected
      ) return;
      if (p.mode !== nextMode) {
        error = "The optimizer returned a plan for a different mode. Retry.";
        return;
      }
      plan = p;
      const identity = { serial, generation, mode: nextMode, token: request };
      planIdentity = identity;
      const sel = new Set<string>();
      for (const it of p.items) {
        if (it.action.kind === "skip") continue;
        if (it.action.kind === "enable" && it.entry.default_restore) sel.add(it.entry.package);
      }
      selected = sel;
      void loadSafety(p, identity);
    } catch (e) {
      if (
        destroyed ||
        request !== planRequest ||
        serial !== session.serial ||
        generation !== session.generation ||
        nextMode !== mode
      ) return;
      const s = String(e);
      if (s.includes("LOCKED:")) locked = true;
      else error = s;
    } finally {
      if (request === planRequest) loading = false;
    }
  }

  // `safety_info` is a pure in-process lookup, so one batch for the whole plan
  // is cheap. Anything unresolved renders as "unverified" and blocks apply.
  async function loadSafety(p: OptimizePlan, identity: PlanIdentity | null = planIdentity) {
    if (!identity || !planCurrent(identity)) return;
    const pkgs = p.items
      .filter((it) => it.action.kind === "disable" || it.action.kind === "uninstall")
      .map((it) => it.entry.package);
    const request = ++safetyRequest;
    applyIntent = null;
    ++selectionVersion;
    selected = new Set(
      p.items
        .filter((item) => item.action.kind === "enable" && selected.has(item.entry.package))
        .map((item) => item.entry.package),
    );
    safetyMap = {};
    safetyFailed = false;
    if (pkgs.length === 0) {
      safetyLoading = false;
      chooseInitialTab(p, identity);
      return;
    }
    safetyLoading = true;
    try {
      const pairs = await Promise.all(
        pkgs.map(async (pkg) => [pkg, await api.safetyInfo(pkg)] as const),
      );
      if (request !== safetyRequest || !planCurrent(identity)) return;
      const map: Record<string, Safety> = {};
      for (const [pkg, s] of pairs) map[pkg] = s;
      safetyMap = map;
      // Only resolved Caution defaults are recommendations. Unknown always
      // starts unchecked, and a user's explicit choice is never resurrected.
      const next = new Set(selected);
      for (const item of p.items) {
        if (
          item.action.kind !== "enable" &&
          item.action.kind !== "skip" &&
          item.entry.default_optimize &&
          map[item.entry.package]?.kind === "caution" &&
          !selectionTouched.has(item.entry.package)
        ) next.add(item.entry.package);
      }
      selected = next;
      chooseInitialTab(p, identity);
    } catch {
      if (request !== safetyRequest || !planCurrent(identity)) return;
      safetyMap = {};
      safetyFailed = true;
      chooseInitialTab(p, identity);
    } finally {
      if (request === safetyRequest && planCurrent(identity)) safetyLoading = false;
    }
  }

  function chooseInitialTab(p: OptimizePlan, identity: PlanIdentity) {
    if (identity.mode !== "optimize" || tabTouched || !planCurrent(identity)) return;
    const hasRecommendation = p.items.some(
      (item) => item.action.kind !== "skip" && recommendedFlag(item),
    );
    activeTab = hasRecommendation ? "recommended" : "optional";
  }

  function chooseTab(tab: "recommended" | "optional" | "all") {
    tabTouched = true;
    activeTab = tab;
  }

  function sessionCurrent(serial: string, generation: number): boolean {
    return (
      !destroyed &&
      session.serial === serial &&
      session.generation === generation &&
      session.isConnected
    );
  }

  function planCurrent(identity: PlanIdentity | null = planIdentity): identity is PlanIdentity {
    return (
      identity !== null &&
      planIdentity !== null &&
      identity.serial === planIdentity.serial &&
      identity.generation === planIdentity.generation &&
      identity.mode === planIdentity.mode &&
      identity.token === planIdentity.token &&
      identity.mode === mode &&
      sessionCurrent(identity.serial, identity.generation)
    );
  }

  // Observe reactive connection state as well as the plain generation getter,
  // so a same-serial reconnect cannot leave an old plan actionable.
  $effect(() => {
    const device = session.connectedDevice;
    const liveness = session.liveness;
    const generation = session.generation;
    const key = `${device?.serial ?? ""}|${generation}|${liveness}`;
    if (key === observedSession) return;
    observedSession = key;
    untrack(() => {
      ++planRequest;
      ++safetyRequest;
      applyIntent = null;
      planIdentity = null;
      plan = null;
      selected = new Set();
      ++selectionVersion;
      if (device && liveness === "live") void loadPlan(mode);
      else {
        loading = false;
        error = "No TV connected.";
      }
    });
  });

  const actionable = $derived(
    (plan?.items ?? []).filter((it) => it.action.kind !== "skip"),
  );
  const recommendedFlag = (it: OptimizePlanItem) => {
    if (mode === "restore") return it.action.kind === "enable" && it.entry.default_restore;
    return (
      it.action.kind !== "enable" &&
      it.entry.default_optimize &&
      safetyMap[it.entry.package]?.kind === "caution"
    );
  };
  const optionalItems = $derived(
    actionable.filter(
      (it) =>
        mode === "optimize" &&
        (!it.entry.default_optimize ||
          (!safetyLoading && !recommendedFlag(it))),
    ),
  );
  const visibleItems = $derived(
    activeTab === "recommended"
      ? actionable.filter(recommendedFlag)
      : activeTab === "optional"
        ? optionalItems
        : actionable,
  );

  // Enabling is never destructive, so the never-disable guard only applies to
  // the disable / uninstall directions.
  function isHardBlocked(it: OptimizePlanItem): boolean {
    return it.action.kind !== "enable" && isBlocked(safetyMap[it.entry.package]);
  }

  function isSelectable(it: OptimizePlanItem): boolean {
    if (!planCurrent() || it.action.kind === "skip") return false;
    if (it.action.kind === "enable") return true;
    const verdict = safetyMap[it.entry.package];
    return verdict?.kind === "caution" || verdict?.kind === "unknown";
  }

  const selectedItems = $derived(
    actionable.filter((it) => selected.has(it.entry.package)),
  );
  const selectedCount = $derived(selectedItems.length);
  const recommendedSelected = $derived(
    selectedItems.filter((item) => item.entry.default_optimize).length,
  );
  const optionalSelected = $derived(selectedCount - recommendedSelected);
  const runningMb = $derived(
    selectedItems.reduce((acc, it) => acc + (it.memory_mb ?? 0), 0),
  );
  // Readiness is per selected destructive item; a batch flag cannot bless a
  // missing, rejected or protected verdict.
  const safetyReady = $derived(
    selectedItems.every(isSelectable),
  );

  function warningFor(items: FrozenItem[]): string {
    const parts: string[] = [];
    const disables = items.filter(({ item }) => item.action.kind === "disable").length;
    const uninstalls = items.filter(({ item }) => item.action.kind === "uninstall").length;
    if (disables > 0) {
      parts.push(`${disables} app${disables === 1 ? "" : "s"} will be disabled.`);
    }
    if (uninstalls > 0) {
      parts.push(
        `${uninstalls} app${uninstalls === 1 ? "" : "s"} will be uninstalled for this TV's user. Reinstall works only while an APK remains on the TV; otherwise use the Play Store.`,
      );
    }
    const destructive = items.filter(({ item }) => item.action.kind !== "enable");
    if (destructive.length > 0) {
      parts.push(
        destructive
          .map(({ item, verdict }) => `${verdict?.kind === "unknown" ? "Unknown" : "Caution"}: ${item.entry.name} — ${reasonOf(verdict)}`)
          .join(" "),
      );
    }
    return parts.join(" ");
  }

  function confirmationFor(items: FrozenItem[], intentMode: OptimizeMode): string {
    const changes = items
      .map(({ item }) => `${item.entry.name} — ${actionLabel(item)}`)
      .join("; ");
    const postPass =
      intentMode === "optimize"
        ? "Afterward, animation scales are set to 0.5×."
        : "Afterward, animation scales are reset to 1×.";
    return `${changes}. ${postPass}`;
  }

  function selectionChanged(pkg?: string) {
    if (pkg) selectionTouched.add(pkg);
    ++selectionVersion;
    applyIntent = null;
  }

  function toggle(it: OptimizePlanItem) {
    if (!isSelectable(it)) {
      const verdict = safetyMap[it.entry.package];
      showToast(
        verdict?.kind === "never_disable"
          ? `Protected: ${verdict.reason}`
          : "Safety unavailable. Retry before selecting this action.",
        verdict?.kind === "never_disable" ? "error" : "info",
      );
      return;
    }
    const next = new Set(selected);
    if (next.has(it.entry.package)) next.delete(it.entry.package);
    else next.add(it.entry.package);
    selected = next;
    selectionChanged(it.entry.package);
  }

  function keepOptional(it: OptimizePlanItem) {
    const next = new Set(selected);
    next.delete(it.entry.package);
    selected = next;
    selectionChanged(it.entry.package);
  }

  function chooseOptionalAction(it: OptimizePlanItem) {
    if (!selected.has(it.entry.package)) toggle(it);
  }

  function selectAll() {
    const next = new Set(selected);
    for (const it of visibleItems) {
      selectionTouched.add(it.entry.package);
      if (isSelectable(it)) next.add(it.entry.package);
      else next.delete(it.entry.package);
    }
    selected = next;
    selectionChanged();
  }

  function actionLabel(it: OptimizePlanItem): string {
    switch (it.action.kind) {
      case "uninstall":
        return "Uninstall for this user";
      case "enable":
        return "Re-enable";
      default:
        return "Disable";
    }
  }

  function handleApply() {
    if (!session.isPro) {
      showPaywall = true;
      return;
    }
    const identity = planIdentity;
    if (!identity || !planCurrent(identity) || selectedCount === 0 || !safetyReady) return;
    const items: FrozenItem[] = selectedItems.map((item) => ({
      item,
      verdict: item.action.kind === "enable" ? null : safetyMap[item.entry.package] ?? null,
    }));
    if (items.some(({ item, verdict }) => item.action.kind !== "enable" && !verdict)) return;
    applyIntent = {
      ...identity,
      selectionVersion,
      items,
      message: confirmationFor(items, identity.mode),
      warning: warningFor(items),
    };
  }

  async function runApply() {
    const intent = applyIntent;
    applyIntent = null;
    if (
      applying ||
      !intent ||
      !planCurrent(intent) ||
      intent.selectionVersion !== selectionVersion ||
      intent.items.length === 0 ||
      !session.isPro
    ) return;
    const serial = intent.serial;
    const generation = intent.generation;
    const items = intent.items;
    const runMode = intent.mode;
    const request = ++applyRequest;

    applying = true;
    session.applyInProgress = true;
    cancelRequested = false;
    applyTotal = items.length;
    applyDone = 0;
    applyCurrent = items[0].item.entry.name;
    toast = "";

    const failures: string[] = [];
    let completed = 0;
    let entitlementLost = false;
    let canceled = false;
    let safetyStopped = false;
    const runCurrent = () =>
      request === applyRequest &&
      sessionCurrent(serial, generation) &&
      planCurrent(intent) &&
      intent.selectionVersion === selectionVersion &&
      session.isPro;
    try {
      for (const [index, frozen] of items.entries()) {
        const item = frozen.item;
        if (cancelRequested || !runCurrent()) {
          entitlementLost = !session.isPro;
          canceled = true;
          break;
        }
        applyDone = index;
        applyCurrent = item.entry.name;
        try {
          if (item.action.kind === "disable" || item.action.kind === "uninstall") {
            let fresh: Safety;
            try {
              fresh = await api.safetyInfo(item.entry.package);
            } catch {
              if (runCurrent()) {
                safetyMap = { ...safetyMap };
                delete safetyMap[item.entry.package];
                showToast(`Safety unavailable for ${item.entry.name}. Review and retry.`, "error");
              }
              safetyStopped = true;
              break;
            }
            if (!runCurrent() || cancelRequested) {
              canceled = true;
              break;
            }
            if (fresh.kind === "never_disable") {
              safetyMap = { ...safetyMap, [item.entry.package]: fresh };
              showToast(`Protected: ${fresh.reason}`, "error");
              safetyStopped = true;
              break;
            }
            if (
              !frozen.verdict ||
              fresh.kind !== frozen.verdict.kind ||
              fresh.reason !== frozen.verdict.reason
            ) {
              safetyMap = { ...safetyMap, [item.entry.package]: fresh };
              showToast(`Safety guidance changed for ${item.entry.name}. Review it and apply again.`, "info");
              safetyStopped = true;
              break;
            }
          }
          const result =
            item.action.kind === "disable"
              ? await api.disablePackage(serial, item.entry.package)
              : item.action.kind === "uninstall"
                ? await api.uninstallPackage(serial, item.entry.package)
                : item.action.kind === "enable"
                  ? await api.enablePackage(serial, item.entry.package)
                  : null;
          if (!runCurrent()) {
            canceled = true;
            break;
          }
          if (!result?.ok) failures.push(item.entry.name);
          else completed += 1;
          if (cancelRequested) {
            canceled = true;
            break;
          }
        } catch (e) {
          if (!runCurrent()) {
            canceled = true;
            break;
          }
          if (String(e).includes("LOCKED:")) {
            showPaywall = true;
            entitlementLost = true;
            break;
          }
          failures.push(item.entry.name);
        }
      }
      if (!runCurrent() && !entitlementLost) canceled = true;
      if (request === applyRequest) applyDone = items.length;

      canceled ||= cancelRequested || !sessionCurrent(serial, generation);
      if (!entitlementLost && !canceled && !safetyStopped && runCurrent()) {
        applyCurrent = "animation settings";
        try {
          const performance = await api.applyPerformanceSettings(
            serial,
            runMode === "optimize" ? "optimized" : "default",
          );
          if (!runCurrent() || cancelRequested) {
            canceled = true;
          }
          if (!performance.ok) failures.push("animation settings");
        } catch {
          if (runCurrent()) failures.push("animation settings");
          else canceled = true;
        }
      }

      if (request !== applyRequest || !sessionCurrent(serial, generation)) return;
      session.invalidateAll();
      const verb = runMode === "optimize" ? "Optimization" : "Restore";
      if (safetyStopped) {
        // The specific safety message was already shown above.
      } else if (canceled) {
        showToast(
          `Stopped after ${completed} of ${items.length}. ${items.length - completed - failures.length} left untouched.`,
          "info",
        );
      } else if (entitlementLost) {
        showToast(`Stopped — Pro is required. ${completed} app${completed === 1 ? "" : "s"} updated.`, "info");
      } else if (failures.length === 0) {
        showToast(`${verb} complete — ${completed} app${completed === 1 ? "" : "s"} updated.`, "success");
      } else {
        showToast(
          `${completed} updated; ${failures.length} failed (${failures.slice(0, 3).join(", ")}${failures.length > 3 ? ", …" : ""}).`,
          "info",
        );
      }
      if (!safetyStopped) await loadPlan(runMode);
    } finally {
      if (request === applyRequest) {
        applying = false;
        session.applyInProgress = false;
        cancelRequested = false;
        applyCurrent = "";
        applyDone = 0;
        applyTotal = 0;
      }
    }
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button
        class="iconbtn"
        onclick={() => navigate("dashboard")}
        disabled={applying}
        aria-label="Back"
      >
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Optimize</h3>
    <span style="width:44px"></span>
  </div>

  {#if !locked && !loading}
    <div class="mode-box" role="group" aria-label="Plan mode">
      <button
        class="mode-btn"
        class:active={mode === "optimize"}
        disabled={applying}
        onclick={() => mode !== "optimize" && loadPlan("optimize")}
      >
        Optimize
      </button>
      <button
        class="mode-btn"
        class:active={mode === "restore"}
        disabled={applying}
        onclick={() => mode !== "restore" && loadPlan("restore")}
      >
        Restore
      </button>
    </div>
  {/if}

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Building plan…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={() => loadPlan(mode)}>Retry</button>
    <div class="spacer"></div>
  {:else if locked}
    <!-- Locked (Free tier): no fabricated list, an honest upsell. -->
    <div class="locked-card">
      <span class="locked-icon msr">lock</span>
      <h2>Debloat is a Pro feature</h2>
      <p class="locked-desc">
        Pro unlocks the curated debloat plan for this TV, with every disable
        and uninstall checked by the shared safety engine.
      </p>
      <button class="primary" onclick={() => (showPaywall = true)}>
        <span class="msr">star</span>Unlock Pro
      </button>
      <button class="ghost-link" onclick={() => navigate("more")}>
        Already have a key? Enter it in More
      </button>
    </div>
    <div class="spacer"></div>
  {:else if plan}
    <p class="mode-hint">
      {mode === "optimize"
        ? "Disables or uninstalls curated bloat for this TV. Restore re-enables disabled items; uninstalled apps may require Reinstall or the Play Store."
        : "Re-enables curated apps that are currently disabled on this TV."}
    </p>

    <div class="tab-pill-box">
      <button class="tab-pill" class:active={activeTab === "recommended"} onclick={() => chooseTab("recommended")}>
        Recommended
      </button>
      <button
        class="tab-pill"
        class:active={activeTab === (mode === "optimize" ? "optional" : "all")}
        onclick={() => chooseTab(mode === "optimize" ? "optional" : "all")}
      >
        {mode === "optimize" ? "Optional apps" : "All curated"}
      </button>
    </div>
    <p class="tab-hint">
      {mode === "optimize" && activeTab === "optional"
        ? "Do you use these apps? Keep anything you use. Nothing changes until you confirm."
        : "These are curated choices for this TV. Everything else installed on the device lives in the Apps tab."}
    </p>

    <div class="optimize-summary-card">
      <span class="summary-text">
        <span class="summary-count">{selectedCount}</span> selected{mode === "optimize"
          ? ` · ${recommendedSelected} recommended · ${optionalSelected} optional`
          : ""}{runningMb > 0
          ? ` · ≈ ${Math.round(runningMb)} MB of RAM in play`
          : ""}
      </span>
      {#if activeTab !== "optional"}
        <button class="select-all-btn" onclick={selectAll} disabled={applying}>Select all</button>
      {/if}
    </div>
    {#if runningMb > 0}
      <p class="mb-note">
        That's what these processes are using right now, not a guaranteed saving — Android reclaims
        and re-spawns memory on its own.
      </p>
    {/if}

    {#if safetyFailed}
      <div class="stale-warning" role="alert">
        <span class="msr">warning</span>
        <span>Safety tiers couldn't be loaded, so nothing will be applied.</span>
        <button class="retry-link" onclick={() => plan && loadSafety(plan)}>Retry</button>
      </div>
    {/if}

    {#if visibleItems.length === 0}
      {#if mode === "optimize" && activeTab === "recommended" && safetyLoading}
        <p class="lede empty">Checking safety before showing recommendations…</p>
      {:else if mode === "optimize" && activeTab === "recommended"}
        <p class="lede empty">
          No recommended app changes are available. You can still review optional apps.
        </p>
        <button class="ghost" onclick={() => chooseTab("optional")}>Review optional apps</button>
      {:else if mode === "optimize" && activeTab === "optional"}
        <p class="lede empty">No optional app changes are available for this TV.</p>
      {:else}
        <p class="lede empty">
          Nothing to restore — none of the curated apps are disabled.
        </p>
      {/if}
    {:else}
      <div class="optimize-list">
        {#each visibleItems as item (item.entry.package)}
          {@const tier = tierOf(safetyMap[item.entry.package])}
          {@const hardBlocked = isHardBlocked(item)}
          <div class="optimize-item" class:blocked-row={hardBlocked}>
            <div class="item-details">
              <span class="item-name">{item.entry.name}</span>
              {#if mode === "optimize" && activeTab === "optional"}
                <span class="mono item-package">{item.entry.package}</span>
              {/if}
              <div class="item-meta">
                {#if item.action.kind === "enable"}
                  <span class="tier-chip restore">Restore</span>
                {:else if safetyLoading}
                  <span class="tier-chip pending">Checking safety…</span>
                {:else if tier}
                  <span class="tier-chip {tier.cls}">{tier.label}</span>
                {:else}
                  <span class="tier-chip pending">Safety unavailable</span>
                {/if}
                <span class="mono meta-text">
                  {actionLabel(item)}{item.memory_mb != null
                    ? ` · ${Math.round(item.memory_mb)} MB`
                    : ""}
                </span>
              </div>
              {#if item.action.kind !== "enable" && tier}
                <span class="row-reason {tier.cls}">{reasonOf(safetyMap[item.entry.package])}</span>
              {:else if item.action.kind !== "enable" && safetyFailed}
                <span class="row-reason">Safety guidance could not be loaded. Retry before selecting this action.</span>
              {/if}
              <span class="row-purpose">
                {mode === "optimize" ? item.entry.optimize_description : item.entry.restore_description}
              </span>
            </div>
            {#if mode === "optimize" && activeTab === "optional"}
              <div class="choice-group" role="group" aria-label="Choice for {item.entry.name}">
                <button
                  class="choice-btn"
                  class:active={!selected.has(item.entry.package)}
                  disabled={applying}
                  aria-pressed={!selected.has(item.entry.package)}
                  onclick={() => keepOptional(item)}
                >Keep</button>
                <button
                  class="choice-btn action"
                  class:active={selected.has(item.entry.package) && isSelectable(item)}
                  disabled={applying || !isSelectable(item)}
                  aria-pressed={selected.has(item.entry.package) && isSelectable(item)}
                  onclick={() => chooseOptionalAction(item)}
                >{item.action.kind === "uninstall" ? "Uninstall for this user" : "Disable"}</button>
              </div>
            {:else}
              <button
                class="toggle-switch"
                class:checked={selected.has(item.entry.package) && isSelectable(item)}
                disabled={applying || !isSelectable(item)}
                onclick={() => toggle(item)}
                aria-label="Toggle {item.entry.name}"
              >
                <span class="toggle-knob"></span>
              </button>
            {/if}
          </div>
        {/each}
      </div>
    {/if}
    <div class="spacer"></div>

    {#if applying}
      <div class="progress-card" role="status">
        <span class="progress-line">
          {Math.min(applyDone + 1, applyTotal)} of {applyTotal}
          {applyCurrent ? `· ${applyCurrent}` : ""}
        </span>
        <div class="progress-track">
          <div
            class="progress-fill"
            style="width: {applyTotal ? Math.round((applyDone / applyTotal) * 100) : 0}%"
          ></div>
        </div>
        <button
          class="ghost small"
          disabled={cancelRequested}
          onclick={() => (cancelRequested = true)}
        >
          {cancelRequested ? "Stopping after this app…" : "Cancel"}
        </button>
      </div>
    {:else}
      <button
        class="primary"
        disabled={selectedCount === 0 || !safetyReady}
        onclick={handleApply}
      >
        <span class="msr">{mode === "optimize" ? "auto_fix_high" : "restore"}</span>
        {mode === "optimize" ? "Apply optimization" : "Apply restore"}{session.isPro ? "" : " (Pro)"}
      </button>
    {/if}
  {/if}

  <PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />

  <ConfirmDialog
    open={applyIntent !== null}
    icon={applyIntent?.mode === "optimize" ? "auto_fix_high" : "restore"}
    title={applyIntent?.mode === "optimize"
      ? `Apply ${applyIntent.items.length} selected change${applyIntent.items.length === 1 ? "" : "s"}?`
      : `Re-enable ${applyIntent?.items.length ?? 0} app${applyIntent?.items.length === 1 ? "" : "s"}?`}
    warning={applyIntent?.warning ?? ""}
    message={applyIntent?.message ?? ""}
    confirmLabel={applyIntent?.mode === "optimize" ? "Apply" : "Restore"}
    onConfirm={runApply}
    onCancel={() => (applyIntent = null)}
  />

  <Toast message={toast} type={toastType} />

  <BottomTabs active="optimize" {navigate} />
</div>

<style>
  .header-left {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  /* Locked state */
  .locked-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 12px;
    padding: 32px 20px;
    border-radius: 22px;
    background: var(--surface);
    border: 1px solid var(--line);
    margin-top: 20px;
  }
  .locked-icon {
    font-size: 44px;
    color: var(--accent);
  }
  .locked-card h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
  }
  .locked-desc {
    margin: 0 0 8px;
    font-size: 13px;
    color: var(--muted);
    line-height: 1.5;
  }
  .locked-card .primary {
    max-width: 260px;
  }

  /* Mode + tabs */
  .mode-box {
    display: flex;
    gap: 8px;
    margin-bottom: 10px;
  }
  .mode-btn {
    flex: 1;
    padding: 10px;
    border-radius: 12px;
    background: var(--surface);
    border: 1px solid var(--line);
    color: var(--muted);
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .mode-btn.active {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }
  .mode-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .mode-hint {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
  }
  .tab-pill-box {
    display: flex;
    padding: 4px;
    border-radius: 13px;
    background: #131519;
    border: 1px solid var(--line);
    gap: 4px;
    margin-bottom: 8px;
  }
  .tab-pill {
    flex: 1;
    text-align: center;
    padding: 9px;
    border-radius: 10px;
    background: transparent;
    border: none;
    color: var(--muted);
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 500;
    cursor: pointer;
  }
  .tab-pill.active {
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .tab-hint {
    margin: 0 0 12px;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }

  /* Summary */
  .optimize-summary-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 15px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
    margin-bottom: 8px;
  }
  .summary-text {
    font-size: 13px;
    color: var(--text-soft);
  }
  .summary-count {
    color: var(--accent);
    font-weight: 700;
  }
  .select-all-btn {
    background: transparent;
    border: none;
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    color: var(--accent);
    cursor: pointer;
    padding: 0;
  }
  .select-all-btn:disabled {
    opacity: 0.5;
  }
  .mb-note {
    margin: 0 0 12px;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }

  .stale-warning {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-bottom: 12px;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--amber) 35%, transparent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--amber) 9%, transparent);
    color: var(--amber);
    font-size: 12px;
    line-height: 1.4;
  }
  .stale-warning .msr {
    font-size: 18px;
    flex: none;
  }
  .retry-link {
    margin-left: auto;
    background: transparent;
    border: none;
    color: var(--amber);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
    padding: 0;
  }

  /* List */
  .optimize-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
    overflow-y: auto;
  }
  .optimize-item {
    content-visibility: auto;
    contain-intrinsic-size: auto 66px;
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 13px 14px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .optimize-item.blocked-row {
    border-color: color-mix(in srgb, var(--danger) 30%, transparent);
  }
  .item-details {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 4px;
    min-width: 0;
  }
  .item-name {
    font-size: 14px;
    font-weight: 600;
  }
  .item-package {
    color: var(--muted);
    font-size: 10px;
    overflow-wrap: anywhere;
  }
  .item-meta {
    display: flex;
    align-items: center;
    gap: 7px;
    flex-wrap: wrap;
  }
  .tier-chip {
    font-size: 10px;
    font-weight: 700;
    padding: 2px 7px;
    border-radius: 5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    flex: none;
  }
  .tier-chip.unknown,
  .tier-chip.restore {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }
  .tier-chip.caution {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .tier-chip.blocked {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .tier-chip.pending {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 7%, transparent);
    text-transform: none;
  }
  .meta-text {
    font-size: 10px;
    color: var(--muted);
  }
  .row-reason {
    font-size: 11px;
    color: var(--muted);
    line-height: 1.35;
  }
  .row-reason.caution {
    color: var(--amber);
  }
  .row-reason.blocked {
    color: var(--danger);
  }
  .row-purpose {
    font-size: 11px;
    color: var(--muted);
    line-height: 1.35;
  }
  .choice-group {
    width: min(138px, 42%);
    display: flex;
    flex-direction: column;
    gap: 6px;
    flex: none;
  }
  .choice-btn {
    min-height: 34px;
    padding: 6px 8px;
    border-radius: 9px;
    border: 1px solid var(--line);
    background: transparent;
    color: var(--muted);
    font-family: var(--sans);
    font-size: 10px;
    font-weight: 650;
    line-height: 1.2;
  }
  .choice-btn.active {
    border-color: color-mix(in srgb, var(--teal) 55%, transparent);
    background: color-mix(in srgb, var(--teal) 10%, transparent);
    color: var(--teal);
  }
  .choice-btn.action.active {
    border-color: color-mix(in srgb, var(--accent) 55%, transparent);
    background: color-mix(in srgb, var(--accent) 12%, transparent);
    color: var(--accent);
  }
  .choice-btn:disabled {
    opacity: 0.45;
  }

  /* Toggle */
  .toggle-switch {
    position: relative;
    width: 44px;
    height: 26px;
    border-radius: 999px;
    background: var(--surface-2);
    border: 1px solid var(--line);
    flex: none;
    cursor: pointer;
    padding: 0;
    transition: background-color 0.2s;
  }
  .toggle-switch:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .toggle-switch.checked {
    background: var(--accent);
    border-color: transparent;
  }
  .toggle-knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: var(--text-soft);
    transition: transform 0.2s;
  }
  .toggle-switch.checked .toggle-knob {
    transform: translateX(18px);
    background: var(--accent-ink);
  }

  /* Apply progress */
  .progress-card {
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 14px 15px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid color-mix(in srgb, var(--accent) 30%, transparent);
  }
  .progress-line {
    font-family: var(--mono);
    font-size: 12px;
    color: var(--text-soft);
  }
  .progress-track {
    height: 7px;
    border-radius: 4px;
    background: var(--canvas);
    overflow: hidden;
  }
  .progress-fill {
    height: 100%;
    background: var(--accent);
    transition: width 0.2s ease;
  }
</style>
