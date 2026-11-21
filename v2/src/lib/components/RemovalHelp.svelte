<script lang="ts">
  import HelpTip from "./HelpTip.svelte";

  // Why a row suggests Disable or Uninstall, on demand next to the lists that
  // suggest them. The rule itself is `suggestedMethod` in recommendation.ts.
</script>

<div class="removal-help">
  <HelpTip label="Disable or uninstall">
    <p>
      <strong>Disable</strong> is suggested for every review app. It stops the app while keeping
      its installed copy. Android's third-party app list does not prove Play Store origin.
    </p>
    <p>
      <strong>Uninstall</strong> remains an explicit choice where offered. It removes the app for
      this TV's user. Getting it back depends on an available APK or store listing; deleted app data
      is not restored. Built-in apps remain in system storage either way.
    </p>
    <p>
      Snapshots record disabled and absent apps. Recorded absent apps are left unchanged because
      saved-TV hardware identity is unverified. Restore can disable recorded disabled apps and
      restore supported settings; it never uninstalls.
    </p>
  </HelpTip>
</div>

<style>
  .removal-help { display: inline-flex; align-items: center; }
  .removal-help :global(.help-title) { font-size: 0.9rem; font-weight: 500; color: var(--fg-secondary); }
</style>
