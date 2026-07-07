<script lang="ts">
  import type { Screen } from "../lib/router.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  // Static explainer only — it describes the tiers the core `safety_info`
  // classifier assigns. It never classifies packages itself.
  const tiers: { key: string; label: string; icon: string; desc: string }[] = [
    {
      key: "safe",
      label: "Safe",
      icon: "check_circle",
      desc: "Bloat with no system role — telemetry, defunct apps, unused services. Remove freely.",
    },
    {
      key: "review",
      label: "Review",
      icon: "visibility",
      desc: "Something you might actually use — a streaming app or store. Kept unless you opt in.",
    },
    {
      key: "advanced",
      label: "Advanced",
      icon: "tune",
      desc: "Affects behavior — launchers, input hooks. Fine for power users who know the trade-off.",
    },
    {
      key: "blocked",
      label: "Blocked",
      icon: "shield",
      desc: "Brick-tier system packages. Guarded from every path — you can't disable these.",
    },
  ];

  const actions: { label: string; icon: string; desc: string }[] = [
    {
      label: "Disable",
      icon: "block",
      desc: "Hides & stops the app but keeps it on disk. Instant re-enable.",
    },
    {
      label: "Uninstall",
      icon: "delete",
      desc: "Removes it for the current user & frees storage. Reinstall from Play Store or Restore.",
    },
  ];
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={() => navigate("more")} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Risk guide</h3>
    <span style="width:44px"></span>
  </div>

  <div class="guide-content">
    <p class="lede">
      Every app is scored by the same audited classifier the optimizer uses. The tier tells you how
      safe an action is.
    </p>

    <div class="tiers">
      {#each tiers as t (t.key)}
        <div class="tier-card tier-{t.key}">
          <span class="tier-icon msr">{t.icon}</span>
          <div class="tier-body">
            <span class="tier-label">{t.label}</span>
            <span class="tier-desc">{t.desc}</span>
          </div>
        </div>
      {/each}
    </div>

    <span class="section-label">Disable vs uninstall</span>
    <div class="action-cards">
      {#each actions as a (a.label)}
        <div class="action-card">
          <span class="action-icon msr">{a.icon}</span>
          <span class="action-label">{a.label}</span>
          <span class="action-desc">{a.desc}</span>
        </div>
      {/each}
    </div>

    <div class="callout teal">
      <span class="msr">verified_user</span>
      <span class="callout-text">
        Blocked packages can never be disabled from this app — the guard runs before anything is
        sent to the TV, so a mistap can't brick your device.
      </span>
    </div>
  </div>

  <div class="spacer"></div>
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

  .guide-content {
    display: flex;
    flex-direction: column;
    gap: 12px;
  }

  .tiers {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .tier-card {
    display: flex;
    gap: 13px;
    align-items: flex-start;
    padding: 15px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .tier-icon {
    font-size: 24px;
    flex: none;
    margin-top: 1px;
  }
  .tier-body {
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .tier-label {
    font-size: 15px;
    font-weight: 700;
  }
  .tier-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
  }
  .tier-safe {
    border-color: color-mix(in srgb, var(--teal) 30%, transparent);
  }
  .tier-safe .tier-icon {
    color: var(--teal);
  }
  .tier-review {
    border-color: color-mix(in srgb, var(--amber) 30%, transparent);
  }
  .tier-review .tier-icon {
    color: var(--amber);
  }
  .tier-advanced {
    border-color: color-mix(in srgb, var(--advanced) 30%, transparent);
  }
  .tier-advanced .tier-icon {
    color: var(--advanced);
  }
  .tier-blocked {
    border-color: color-mix(in srgb, var(--danger) 30%, transparent);
  }
  .tier-blocked .tier-icon {
    color: var(--danger);
  }

  .action-cards {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 10px;
  }
  .action-card {
    display: flex;
    flex-direction: column;
    gap: 7px;
    padding: 15px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .action-icon {
    font-size: 24px;
    color: var(--text-soft);
  }
  .action-label {
    font-size: 14px;
    font-weight: 700;
  }
  .action-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
  }

  .callout-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.45;
  }
</style>
