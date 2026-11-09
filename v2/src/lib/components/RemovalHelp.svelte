<script lang="ts">
  import HelpTip from "./HelpTip.svelte";

  // Why a row suggests Disable or Uninstall, on demand next to the lists that
  // suggest them. The rule itself is `suggestedMethod` in recommendation.ts.
</script>

<div class="removal-help">
  <HelpTip label="Disable or uninstall">
    <p>
      <strong>Uninstall</strong> is suggested for apps you installed from the Play Store. You can
      reinstall them there any time, and uninstalling frees their storage.
    </p>
    <p>
      <strong>Disable</strong> is suggested for apps that came with the TV or were sideloaded. It is
      fully reversible, and those apps can't be downloaded again from the store.
    </p>
    <p>
      Snapshots record both: the apps you disabled, and known apps that weren't on the TV. Restoring
      one disables any of those apps that are back. It never uninstalls.
    </p>
  </HelpTip>
</div>

<style>
  .removal-help { display: inline-flex; align-items: center; }
  .removal-help :global(.help-title) { font-size: 0.9rem; font-weight: 500; color: var(--fg-secondary); }
</style>
