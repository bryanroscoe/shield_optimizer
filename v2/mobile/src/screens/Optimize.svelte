<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { OptimizePlan, OptimizePlanItem, RiskTier } from "../lib/types";
  import BottomTabs from "../components/BottomTabs.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  let loading = $state(true);
  let error = $state("");
  // Free tier: prepare_optimize returns "LOCKED:<feature>". We surface a
  // clearly-labeled locked state — never a fabricated debloat list.
  let locked = $state(false);
  let plan = $state<OptimizePlan | null>(null);
  let showPaywall = $state(false);

  let activeTab = $state<"recommended" | "everything">("recommended");
  // Local selection (which rows are ticked). Applying is Pro; there's no
  // apply endpoint on mobile yet, so this drives the count/paywall only.
  let selected = $state<Set<string>>(new Set());

  async function loadPlan() {
    loading = true;
    error = "";
    locked = false;
    if (!session.connectedDevice) {
      error = "No TV connected.";
      loading = false;
      return;
    }
    try {
      const p = await api.prepareOptimize(
        session.serial,
        session.connectedDevice.device_type,
        "optimize",
      );
      plan = p;
      // Default-select the actionable recommended items.
      const sel = new Set<string>();
      for (const it of p.items) {
        if (it.entry.default_optimize && it.action.kind !== "skip") {
          sel.add(it.entry.package);
        }
      }
      selected = sel;
    } catch (e) {
      const s = String(e);
      if (s.includes("LOCKED:")) locked = true;
      else error = s;
    } finally {
      loading = false;
    }
  }

  onMount(loadPlan);

  const actionable = $derived(
    (plan?.items ?? []).filter((it) => it.action.kind !== "skip"),
  );
  const visibleItems = $derived(
    activeTab === "recommended"
      ? actionable.filter((it) => it.entry.default_optimize)
      : actionable,
  );

  const selectedCount = $derived(
    visibleItems.filter((it) => selected.has(it.entry.package)).length,
  );
  const totalSavings = $derived(
    visibleItems
      .filter((it) => selected.has(it.entry.package))
      .reduce((acc, it) => acc + (it.memory_mb ?? 0), 0),
  );

  function toggle(pkg: string) {
    const next = new Set(selected);
    if (next.has(pkg)) next.delete(pkg);
    else next.add(pkg);
    selected = next;
  }

  function selectAll() {
    const next = new Set(selected);
    for (const it of visibleItems) next.add(it.entry.package);
    selected = next;
  }

  function methodLabel(it: OptimizePlanItem): string {
    return it.action.kind === "uninstall" ? "uninstall" : "disable";
  }

  function riskClass(r: RiskTier): string {
    return `risk-${r}`;
  }

  function handleApply() {
    // Applying the debloat is a Pro feature — route the locked action to the
    // paywall rather than performing anything.
    showPaywall = true;
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={() => navigate("dashboard")} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Optimize</h3>
    <span style="width:44px"></span>
  </div>

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Building plan…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={loadPlan}>Retry</button>
    <div class="spacer"></div>
  {:else if locked}
    <!-- Locked (Free tier): no fabricated list, an honest upsell. -->
    <div class="locked-card">
      <span class="locked-icon msr">lock</span>
      <h2>Debloat is a Pro feature</h2>
      <p class="locked-desc">
        Pro unlocks the curated debloat plan for this TV — safely disable or
        uninstall bloat with one-tap rollback via snapshots.
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
    <div class="tab-pill-box">
      <button class="tab-pill" class:active={activeTab === "recommended"} onclick={() => (activeTab = "recommended")}>
        Recommended
      </button>
      <button class="tab-pill" class:active={activeTab === "everything"} onclick={() => (activeTab = "everything")}>
        Everything
      </button>
    </div>

    <div class="optimize-summary-card">
      <span class="summary-text">
        <span class="summary-count">{selectedCount}</span> selected{totalSavings > 0 ? ` · frees ~${totalSavings} MB` : ""}
      </span>
      <button class="select-all-btn" onclick={selectAll}>Select all</button>
    </div>

    {#if visibleItems.length === 0}
      <p class="lede empty">Nothing to optimize here — this TV is already clean.</p>
      <div class="spacer"></div>
    {:else}
      <div class="optimize-list">
        {#each visibleItems as item (item.entry.package)}
          <div class="optimize-item">
            <div class="item-details">
              <span class="item-name">{item.entry.name}</span>
              <div class="item-meta">
                <span class="risk-tag {riskClass(item.entry.risk)}">{item.entry.risk}</span>
                <span class="mono meta-text">
                  {methodLabel(item)}{item.memory_mb != null ? ` · ${item.memory_mb} MB` : ""}
                </span>
              </div>
            </div>
            <button
              class="toggle-switch"
              class:checked={selected.has(item.entry.package)}
              onclick={() => toggle(item.entry.package)}
              aria-label="Toggle {item.entry.name}"
            >
              <span class="toggle-knob"></span>
            </button>
          </div>
        {/each}
      </div>
      <div class="spacer"></div>
      <button class="primary" onclick={handleApply}>
        <span class="msr">auto_fix_high</span>Apply optimization{session.isPro ? "" : " (Pro)"}
      </button>
    {/if}
  {/if}

  {#if showPaywall}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="paywall-overlay" onclick={() => (showPaywall = false)}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="paywall-card" onclick={(e) => e.stopPropagation()}>
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="paywall-close msr" onclick={() => (showPaywall = false)}>close</span>
        <div class="paywall-header">
          <span class="logo"><span class="msr">star</span></span>
          <h2>Unlock Pro Features</h2>
          <p class="paywall-lede">One-time purchase. Lifetime safety &amp; control.</p>
        </div>
        <div class="paywall-features">
          <div class="feature-row">
            <span class="msr teal-color">done</span>
            <div class="feature-info">
              <span class="feature-title">Curated Debloat</span>
              <span class="feature-desc">Safely disable/uninstall known TV bloat packages.</span>
            </div>
          </div>
          <div class="feature-row">
            <span class="msr teal-color">done</span>
            <div class="feature-info">
              <span class="feature-title">Automatic Snapshots</span>
              <span class="feature-desc">Roll back any change instantly or clone to other TVs.</span>
            </div>
          </div>
          <div class="feature-row">
            <span class="msr teal-color">done</span>
            <div class="feature-info">
              <span class="feature-title">Launcher &amp; Tweaks</span>
              <span class="feature-desc">Set a custom launcher and tune system settings.</span>
            </div>
          </div>
        </div>
        <button class="primary" onclick={() => { showPaywall = false; navigate("more"); }}>
          Enter license key
        </button>
        <button class="ghost paywall-close-btn" onclick={() => (showPaywall = false)}>
          Maybe later
        </button>
      </div>
    </div>
  {/if}

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

  /* Tabs */
  .tab-pill-box {
    display: flex;
    padding: 4px;
    border-radius: 13px;
    background: #131519;
    border: 1px solid var(--line);
    gap: 4px;
    margin-bottom: 12px;
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

  /* Summary */
  .optimize-summary-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 15px;
    border-radius: 14px;
    background: color-mix(in srgb, var(--accent) 9%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
    margin-bottom: 14px;
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

  /* List */
  .optimize-list {
    display: flex;
    flex-direction: column;
    gap: 9px;
    overflow-y: auto;
  }
  .optimize-item {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 13px 14px;
    border-radius: 15px;
    background: var(--surface);
    border: 1px solid var(--line);
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
  }
  .risk-tag {
    font-size: 10px;
    font-weight: 700;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
    padding: 2px 7px;
    border-radius: 5px;
    text-transform: uppercase;
    letter-spacing: 0.04em;
  }
  .risk-tag.risk-medium {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .risk-tag.risk-high {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .risk-tag.risk-advanced {
    color: var(--advanced);
    background: color-mix(in srgb, var(--advanced) 14%, transparent);
  }
  .meta-text {
    font-size: 10px;
    color: var(--muted);
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

  /* Paywall */
  .paywall-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.85);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 200;
  }
  .paywall-card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 26px;
    width: 100%;
    max-width: 340px;
    padding: 24px;
    position: relative;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.8);
    display: flex;
    flex-direction: column;
    align-items: center;
    box-sizing: border-box;
    gap: 4px;
  }
  .paywall-close {
    position: absolute;
    top: 18px;
    right: 18px;
    font-size: 22px;
    color: var(--muted);
    cursor: pointer;
  }
  .paywall-header {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 8px;
    margin-bottom: 20px;
  }
  .paywall-header h2 {
    margin: 0;
    font-size: 24px;
    font-weight: 700;
  }
  .paywall-lede {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
  }
  .paywall-features {
    display: flex;
    flex-direction: column;
    gap: 16px;
    width: 100%;
    margin-bottom: 20px;
  }
  .feature-row {
    display: flex;
    gap: 12px;
    align-items: flex-start;
  }
  .feature-row .msr {
    font-size: 20px;
    flex-shrink: 0;
  }
  .teal-color {
    color: var(--teal);
  }
  .feature-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .feature-title {
    font-size: 14px;
    font-weight: 600;
    color: var(--text);
  }
  .feature-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.4;
  }
  .paywall-close-btn {
    width: 100%;
    min-height: 50px;
    margin-top: 10px;
  }
</style>
