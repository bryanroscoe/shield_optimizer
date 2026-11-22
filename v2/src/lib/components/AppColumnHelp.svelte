<script lang="ts">
  import HelpTip from "./HelpTip.svelte";

  // The one keyboard-reachable explanation per App List column. The per-row
  // badges it explains (REVIEW, RAM, last used) are hover/click tips with no
  // Tab stop, so a long list doesn't gain a Tab stop per row.
  let { column }: { column: "app" | "ram" | "usage" } = $props();
</script>

{#if column === "app"}
  <HelpTip label="App" inline>
    <p><strong>REVIEW</strong> marks an app to check before removing it: keep it if anyone uses it.</p>
  </HelpTip>
{:else if column === "ram"}
  <HelpTip label="RAM" inline>
    <p>RAM in use right now. A number means the app is running; a dash means it isn't in memory.</p>
  </HelpTip>
{:else}
  <HelpTip label="Last used" inline>
    <p>Last foreground use, from Android's usage history.</p>
    <p>Android keeps about a year of history and clears it on a factory reset, so a dash doesn't prove the app was never opened.</p>
  </HelpTip>
{/if}
