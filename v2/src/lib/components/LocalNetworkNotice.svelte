<script lang="ts">
  import HelpTip from "./HelpTip.svelte";
  import Icon from "./Icon.svelte";
  import { LOCAL_NETWORK_BLOCKED } from "$lib/adb-errors";
  import { localNetwork, openLocalNetworkSettings } from "$lib/local-network.svelte";

  /// Built-in ADB connects from the app itself, so there is no server to restart.
  let { hostServer = true }: { hostServer?: boolean } = $props();
</script>

<!-- Only on the probe's evidence (status "blocked"); see local-network.svelte.ts. -->
{#if localNetwork.status === "blocked"}
  <div class="callout callout-warn local-network-notice" role="status">
    <Icon name="warning" size={16} />
    <div class="notice-body">
      <HelpTip label={LOCAL_NETWORK_BLOCKED}>
        <p>If macOS is asking right now, choose Allow.</p>
        <p>
          Otherwise, click Open Privacy &amp; Security → Local Network and turn ATV Optimizer on.
          If it's already on, turn it off and on again. If it's listed more than once, turn all of
          them on. {hostServer ? "Then Restart ADB." : "Then try again."}
        </p>
        <p>
          If you clicked Don't Allow earlier or dismissed the prompt, macOS won't ask again.
        </p>
        <p>A VPN or a firewall app such as Little Snitch or LuLu can also block local connections.</p>
      </HelpTip>
      <button class="notice-action" onclick={openLocalNetworkSettings}>
        <Icon name="open_in_new" size={14} /> Open Privacy &amp; Security
      </button>
      {#if localNetwork.error}
        <p class="notice-error" role="alert">{localNetwork.error}</p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .local-network-notice {
    margin-bottom: 1rem;
    font-size: 0.9rem;
  }
  .notice-body {
    flex: 1;
    min-width: 0;
  }
  .notice-action {
    margin-top: 0.6rem;
  }
  .notice-error {
    margin: 0.5rem 0 0;
  }
</style>
