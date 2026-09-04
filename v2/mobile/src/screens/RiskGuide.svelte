<script lang="ts">
  import type { Screen } from "../lib/router.svelte";
  import { SAFETY_TIER_LIST } from "../lib/safety";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";

  let {
    navigate,
    back,
  }: {
    navigate: (screen: Screen) => void;
    // Optional so this screen keeps working from a plain `navigate`-only host.
    back?: () => void;
  } = $props();

  const goBack = () => (back ? back() : navigate("more"));

  // The three tiers are core's, not ours — see src/lib/safety.ts.
  const tiers = SAFETY_TIER_LIST;

  const actions: { label: string; icon: string; desc: string }[] = [
    {
      label: "Disable",
      icon: "block",
      desc: "Stops the app and hides it, but leaves it on disk. Enable puts it back instantly.",
    },
    {
      label: "Uninstall",
      icon: "delete",
      desc: "Removes the app for the TV's current user. A preinstalled system app still sits in the read-only system image, so this frees little or no storage — it just takes the app away.",
    },
  ];
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={goBack} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Risk guide</h3>
    <span style="width:44px"></span>
  </div>

  <div class="guide-content">
    <p class="lede">
      Every package is checked by the same audited classifier the optimizer uses — three tiers, no
      others. The tier tells you how safe an action is.
    </p>

    <div class="tiers">
      {#each tiers as t (t.kind)}
        <div class="tier-card tier-{t.cls}">
          <span class="tier-icon msr">{t.icon}</span>
          <div class="tier-body">
            <span class="tier-label">{t.label}</span>
            <span class="tier-desc">{t.description}</span>
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

    <div class="callout">
      <span class="msr">restore</span>
      <span class="callout-text">
        Uninstalled an app you wanted? Open it in Apps and tap <strong>Reinstall</strong> — it runs
        <span class="mono">install-existing</span>, which restores the APK already on the TV. If the
        app was never preinstalled, get it from the Play Store instead. Snapshots don't reinstall
        anything: they record which packages are disabled, plus the launcher and tracked settings.
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
  .tier-caution {
    border-color: color-mix(in srgb, var(--amber) 30%, transparent);
  }
  .tier-caution .tier-icon {
    color: var(--amber);
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
