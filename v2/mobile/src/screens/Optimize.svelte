<script lang="ts">
  import { onDestroy, onMount } from "svelte";
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

  let activeTab = $state<"recommended" | "optional">("recommended");
  let optionalChoices = $state<Record<string, "keep" | "disable" | "uninstall">>({});
  let planOrigin = $state<{ serial: string; generation: number } | null>(null);
  let planRequest = 0;
  // Local selection (which rows are ticked). The backend safety gate remains
  // authoritative for every individual mutation during apply.
  let selected = $state<Set<string>>(new Set());
  let confirmApply = $state(false);
  let applying = $state(false);
  let cancelRequested = $state(false);
  let applyDone = $state(0);
  let applyTotal = $state(0);
  let applyCurrent = $state("");

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
    cancelRequested = true;
    clearTimeout(toastTimer);
  });

  async function loadPlan(nextMode: OptimizeMode = mode) {
    const request = ++planRequest;
    const serial = session.serial;
    const generation = session.generation;
    mode = nextMode;
    loading = true;
    error = "";
    locked = false;
    plan = null;
    optionalChoices = {};
    planOrigin = null;
    safetyMap = {};
    safetyFailed = false;
    ++safetyRequest;
    if (!session.connectedDevice) {
      error = "No TV connected.";
      loading = false;
      return;
    }
    try {
      const p = await api.prepareOptimize(
        session.serial,
        session.connectedDevice.device_type,
        nextMode,
      );
      if (request !== planRequest || session.serial !== serial || session.generation !== generation) return;
      plan = p;
      planOrigin = { serial, generation };
      const sel = new Set<string>();
      for (const it of p.items) {
        if (it.action.kind === "skip") continue;
        const on =
          nextMode === "optimize" ? it.entry.default_optimize : it.entry.default_restore;
        if (on) sel.add(it.entry.package);
      }
      selected = sel;
      void loadSafety(p);
    } catch (e) {
      const s = String(e);
      if (s.includes("LOCKED:")) locked = true;
      else error = s;
    } finally {
      if (request === planRequest) loading = false;
    }
  }

  // `safety_info` is a pure in-process lookup, so one batch for the whole plan
  // is cheap. Anything unresolved renders as "unverified" and blocks apply.
  async function loadSafety(p: OptimizePlan) {
    const pkgs = p.items
      .filter((it) => it.action.kind !== "skip")
      .map((it) => it.entry.package);
    const request = ++safetyRequest;
    safetyFailed = false;
    if (pkgs.length === 0) {
      safetyLoading = false;
      return;
    }
    safetyLoading = true;
    try {
      const pairs = await Promise.all(
        pkgs.map(async (pkg) => [pkg, await api.safetyInfo(pkg)] as const),
      );
      if (request !== safetyRequest) return;
      const map: Record<string, Safety> = {};
      for (const [pkg, s] of pairs) map[pkg] = s;
      safetyMap = map;
    } catch {
      if (request !== safetyRequest) return;
      safetyFailed = true;
    } finally {
      if (request === safetyRequest) safetyLoading = false;
    }
  }

  onMount(() => void loadPlan("optimize"));

  const actionable = $derived(
    (plan?.items ?? []).filter((it) => it.action.kind !== "skip"),
  );
  const recommendedFlag = (it: OptimizePlanItem) =>
    mode === "optimize" ? it.entry.default_optimize : it.entry.default_restore;
  const optionalItems = $derived(
    actionable.filter((it) => mode === "optimize" && !it.entry.default_optimize),
  );
  const visibleItems = $derived(
    activeTab === "recommended" ? actionable.filter(recommendedFlag) : optionalItems,
  );

  // Enabling is never destructive, so the never-disable guard only applies to
  // the disable / uninstall directions.
  function isHardBlocked(it: OptimizePlanItem): boolean {
    return it.action.kind !== "enable" && isBlocked(safetyMap[it.entry.package]);
  }

  const selectedItems = $derived(
    actionable.filter((it) => {
      if (isHardBlocked(it)) return false;
      if (optionalItems.includes(it)) return optionalChoices[it.entry.package] !== undefined && optionalChoices[it.entry.package] !== "keep";
      return selected.has(it.entry.package);
    }),
  );
  const selectedCount = $derived(selectedItems.length);
  const runningMb = $derived(
    selectedItems.reduce((acc, it) => acc + (it.memory_mb ?? 0), 0),
  );
  const selectedUninstalls = $derived(
    selectedItems.filter((it) => it.action.kind === "uninstall").length,
  );
  const cautionSelected = $derived(
    selectedItems.filter(
      (it) => it.action.kind !== "enable" && safetyMap[it.entry.package]?.kind === "caution",
    ),
  );
  // Fail closed: a disable/uninstall run waits for the classifier.
  const safetyReady = $derived(
    mode === "restore" || (!safetyLoading && !safetyFailed),
  );

  const warningText = $derived.by(() => {
    const parts: string[] = [];
    if (selectedUninstalls > 0) {
      parts.push(
        `${selectedUninstalls} app${selectedUninstalls === 1 ? "" : "s"} will be uninstalled for this TV's user. Reinstall brings back an APK still on the TV; anything else needs the Play Store.`,
      );
    }
    if (cautionSelected.length > 0) {
      const shown = cautionSelected
        .slice(0, 2)
        .map((it) => `${it.entry.name} — ${reasonOf(safetyMap[it.entry.package])}`);
      const rest = cautionSelected.length - shown.length;
      parts.push(
        `Caution tier: ${shown.join(" ")}${rest > 0 ? ` Plus ${rest} more caution app${rest === 1 ? "" : "s"} selected.` : ""}`,
      );
    }
    return parts.join(" ");
  });

  function toggle(it: OptimizePlanItem) {
    if (isHardBlocked(it)) {
      showToast(`Protected: ${reasonOf(safetyMap[it.entry.package])}`, "error");
      return;
    }
    const next = new Set(selected);
    if (next.has(it.entry.package)) next.delete(it.entry.package);
    else next.add(it.entry.package);
    selected = next;
  }

  function selectAll() {
    const next = new Set(selected);
    for (const it of visibleItems) {
      if (!isHardBlocked(it)) next.add(it.entry.package);
    }
    selected = next;
  }

  function setOptionalChoice(it: OptimizePlanItem, value: string) {
    if (isHardBlocked(it)) return;
    const next = { ...optionalChoices };
    if (value === "keep") delete next[it.entry.package];
    else next[it.entry.package] = value as "disable" | "uninstall";
    optionalChoices = next;
  }

  const confirmationNames = $derived(
    selectedItems.map((it) => `${it.entry.name} — ${actionLabel(it)}`).join(", "),
  );

  function actionLabel(it: OptimizePlanItem): string {
    switch (it.action.kind) {
      case "uninstall":
        return "uninstall";
      case "enable":
        return "re-enable";
      default:
        return "disable";
    }
  }

  function handleApply() {
    if (!session.isPro) {
      showPaywall = true;
      return;
    }
    if (selectedCount > 0) confirmApply = true;
  }

  async function runApply() {
    confirmApply = false;
    if (applying) return;
    // One serial for the whole run: if the TV changes underneath us we must
    // not fire the rest of the plan at a different device.
    const serial = session.serial;
    const generation = session.generation;
    if (!planOrigin || planOrigin.serial !== serial || planOrigin.generation !== generation) return;
    if (!serial) {
      showToast("No TV connected.", "error");
      return;
    }
    const items = [...selectedItems];
    if (items.length === 0) return;
    const runMode = mode;

    applying = true;
    session.applyInProgress = true;
    cancelRequested = false;
    applyTotal = items.length;
    applyDone = 0;
    applyCurrent = items[0].entry.name;
    toast = "";

    const failures: string[] = [];
    let completed = 0;
    let entitlementLost = false;
    let canceled = false;
    try {
      for (const [index, item] of items.entries()) {
        if (cancelRequested || generation !== session.generation || !session.isConnected) {
          canceled = true;
          break;
        }
        applyDone = index;
        applyCurrent = item.entry.name;
        try {
          const result =
            item.action.kind === "disable"
              ? await api.disablePackage(serial, item.entry.package)
              : item.action.kind === "uninstall"
                ? await api.uninstallPackage(serial, item.entry.package)
                : item.action.kind === "enable"
                  ? await api.enablePackage(serial, item.entry.package)
                  : null;
          if (!result?.ok) failures.push(item.entry.name);
          else completed += 1;
        } catch (e) {
          if (String(e).includes("LOCKED:")) {
            showPaywall = true;
            entitlementLost = true;
            break;
          }
          failures.push(item.entry.name);
        }
      }
      applyDone = items.length;

      canceled ||= cancelRequested || generation !== session.generation || !session.isConnected;
      if (!entitlementLost && !canceled) {
        applyCurrent = "animation settings";
        try {
          const performance = await api.applyPerformanceSettings(
            serial,
            runMode === "optimize" ? "optimized" : "default",
          );
          if (!performance.ok) failures.push("animation settings");
        } catch {
          failures.push("animation settings");
        }
      }

      session.invalidateAll();
      const verb = runMode === "optimize" ? "Optimization" : "Restore";
      if (canceled) {
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
      await loadPlan(runMode);
    } finally {
      applying = false;
      session.applyInProgress = false;
      cancelRequested = false;
      applyCurrent = "";
      applyDone = 0;
      applyTotal = 0;
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
        ? "Disables or uninstalls curated bloat for this TV. Run Restore to put any of it back."
        : "Re-enables curated apps that are currently disabled on this TV."}
    </p>

    <div class="tab-pill-box">
      <button class="tab-pill" class:active={activeTab === "recommended"} onclick={() => (activeTab = "recommended")}>
        Recommended
      </button>
      <button class="tab-pill" class:active={activeTab === "optional"} onclick={() => (activeTab = "optional")}>
        Optional apps
      </button>
    </div>
    <p class="tab-hint">
      {#if activeTab === "optional"}Keep anything you use. Nothing changes until you confirm.{:else}Recommended choices come from the audited catalog. Everything else installed lives in Apps.{/if}
    </p>

    <div class="optimize-summary-card">
      <span class="summary-text">
        <span class="summary-count">{selectedCount}</span> selected{runningMb > 0
          ? ` · ≈ ${Math.round(runningMb)} MB of RAM in play`
          : ""}
      </span>
      {#if activeTab === "recommended"}<button class="select-all-btn" onclick={selectAll} disabled={applying}>Select all</button>{/if}
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
      <p class="lede empty">
          {mode === "optimize"
          ? activeTab === "optional" ? "No optional apps to review in this TV’s curated list" : "Nothing to optimize here."
          : "Nothing to restore — none of the curated apps are disabled."}
      </p>
      <div class="spacer"></div>
    {:else}
      <div class="optimize-list">
        {#each visibleItems as item (item.entry.package)}
          {@const tier = tierOf(safetyMap[item.entry.package])}
          {@const hardBlocked = isHardBlocked(item)}
          <div class="optimize-item" class:blocked-row={hardBlocked}>
            <div class="item-details">
              <span class="item-name">{item.entry.name}</span>
              <div class="item-meta">
                {#if safetyLoading}
                  <span class="tier-chip pending">checking…</span>
                {:else if tier}
                  <span class="tier-chip {tier.cls}">{tier.label}</span>
                {:else}
                  <span class="tier-chip pending">unverified</span>
                {/if}
                <span class="mono meta-text">
                  {actionLabel(item)}{item.memory_mb != null
                    ? ` · ${Math.round(item.memory_mb)} MB`
                    : ""} · catalog: {item.entry.risk}
                </span>
              </div>
              {#if hardBlocked}
                <span class="row-reason">{reasonOf(safetyMap[item.entry.package])}</span>
              {/if}
              {#if activeTab === "optional" && !hardBlocked}
                <label class="choice-label">Choice
                  <select value={optionalChoices[item.entry.package] ?? "keep"} disabled={applying} onchange={(e) => setOptionalChoice(item, e.currentTarget.value)}>
                    <option value="keep">Keep</option>
                    {#if item.action.kind === "disable"}<option value="disable">Disable for this user</option>{/if}
                    {#if item.action.kind === "uninstall"}<option value="uninstall">Uninstall for this user</option>{/if}
                  </select>
                </label>
              {/if}
            </div>
            {#if activeTab === "recommended"}<button
              class="toggle-switch"
              class:checked={selected.has(item.entry.package) && !hardBlocked}
              disabled={applying || hardBlocked}
              onclick={() => toggle(item)}
              aria-label="Toggle {item.entry.name}"
            >
              <span class="toggle-knob"></span>
            </button>{/if}
          </div>
        {/each}
      </div>
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
  {/if}

  <PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />

  <ConfirmDialog
    open={confirmApply}
    icon={mode === "optimize" ? "auto_fix_high" : "restore"}
    title={mode === "optimize"
      ? `Apply ${selectedCount} selected change${selectedCount === 1 ? "" : "s"}?`
      : `Re-enable ${selectedCount} app${selectedCount === 1 ? "" : "s"}?`}
    warning={warningText}
    message={mode === "optimize"
      ? `Selected: ${confirmationNames}. Each app is processed in turn, then optimized animation scales are written. Protected system packages stay blocked by the safety engine.`
      : "Each selected app is re-enabled in turn, then animation scales are reset to 1×."}
    confirmLabel={mode === "optimize" ? "Apply" : "Restore"}
    onConfirm={runApply}
    onCancel={() => (confirmApply = false)}
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
    color: var(--danger);
    line-height: 1.35;
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
