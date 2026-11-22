<script lang="ts">
  import HelpTip from "./HelpTip.svelte";

  // Why a row suggests Disable or Uninstall, on demand next to the lists that
  // suggest them. The rule itself is `suggestedMethod` in recommendation.ts.
</script>

<div class="removal-help">
  <HelpTip label="Disable or uninstall">
    <p>
      <strong>Disable</strong> is suggested for every REVIEW app. It stops the app while keeping it
      installed. Android's list of user-installed apps doesn't prove an app came from the Play Store.
    </p>
    <p>
      <strong>Uninstall</strong> stays your explicit choice where it's offered. It removes the app
      for this TV's user. Getting it back needs an available APK or store listing, and deleted app
      data isn't restored. Preinstalled apps remain in system storage either way.
    </p>
    <p>
      Snapshots record disabled and absent apps. Recorded absent apps are left unchanged because
      the TV's hardware ID isn't verified against the snapshot. Restore can disable the recorded
      disabled apps and restore supported settings; it never uninstalls.
    </p>
  </HelpTip>
</div>

<style>
  .removal-help { display: inline-flex; align-items: center; }
  .removal-help :global(.help-title) { font-size: 0.9rem; font-weight: 500; color: var(--fg-secondary); }
</style>
