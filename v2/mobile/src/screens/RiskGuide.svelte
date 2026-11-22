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

  const actions: { label: string; icon: string; points: string[] }[] = [
    {
      label: "Disable",
      icon: "block",
      points: [
        "Stops the app and hides it, but leaves it on disk",
        "Enable normally puts it back without reinstalling it",
        "Suggested for every review app: Android’s third-party app list does not prove Play Store origin",
      ],
    },
    {
      label: "Uninstall",
      icon: "delete",
      points: [
        "Removes the app for the TV's current user: an explicit choice, and only where offered",
        "Getting it back depends on an available APK or store listing",
        "Deleted app data is not restored",
        "A preinstalled system app stays in the read-only system image, so uninstalling it frees little or no storage",
      ],
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
      Every package is checked by the same audited classifier the optimizer uses. Known protection
      rules are mandatory, but no match does not prove every other package safe.
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
          <ul class="action-desc">
            {#each a.points as point (point)}
              <li>{point}</li>
            {/each}
          </ul>
        </div>
      {/each}
    </div>

    <div class="callout">
      <span class="msr">verified_user</span>
      <span class="callout-text">
        Protected packages cannot be disabled or uninstalled from this app. Caution and Unknown
        packages require you to review the classifier's reason and confirm the exact action.
      </span>
    </div>

    <div class="callout">
      <span class="msr">restore</span>
      <ul class="callout-text callout-list" aria-label="Getting apps back">
        <li>
          Reinstall runs <span class="mono">install-existing</span> and works only while an APK is
          still on the TV; otherwise, look for the app in the Play Store
        </li>
        <li>Snapshots never reinstall apps: they record disabled and absent packages, the launcher and tracked settings</li>
        <li>Restoring a snapshot leaves recorded absent apps unchanged, because a saved TV's hardware identity is unverified</li>
      </ul>
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
  .tier-unknown {
    border-color: var(--line);
  }
  .tier-unknown .tier-icon {
    color: var(--muted);
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
    grid-template-columns: repeat(auto-fit, minmax(min(140px, 100%), 1fr));
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
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding-left: 18px;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    overflow-wrap: anywhere;
  }

  .callout-text {
    flex: 1;
    font-size: 12px;
    line-height: 1.45;
  }
  .callout-list {
    display: flex;
    flex-direction: column;
    gap: 4px;
    margin: 0;
    padding-left: 18px;
  }
</style>
