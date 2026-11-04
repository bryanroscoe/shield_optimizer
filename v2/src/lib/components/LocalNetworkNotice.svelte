<script lang="ts">
  import HelpTip from "./HelpTip.svelte";
  import Icon from "./Icon.svelte";
  import { LOCAL_NETWORK_BLOCKED } from "$lib/adb-errors";
  import { localNetwork, openLocalNetworkSettings } from "$lib/local-network.svelte";
</script>

<!-- Only on the probe's evidence (status "blocked"); see local-network.svelte.ts. -->
{#if localNetwork.status === "blocked"}
  <div class="callout callout-warn local-network-notice" role="status">
    <Icon name="warning" size={16} />
    <div class="notice-body">
      <HelpTip label={LOCAL_NETWORK_BLOCKED}>
        <p>If macOS is asking right now, choose Allow.</p>
        <p>
          Otherwise, in System Settings → Privacy &amp; Security → Local Network, find ATV Optimizer
          and turn it on. If it's already on, turn it off and back on. Then quit and reopen ATV
          Optimizer.
        </p>
        <p>
          macOS asks once and remembers the answer, so an earlier Don't Allow, or a prompt that was
          dismissed, keeps the app blocked until it is changed there. After an update, the switch
          can show on without taking effect.
        </p>
        <p>A VPN or a firewall app such as Little Snitch or LuLu can also block local connections.</p>
      </HelpTip>
      <button class="notice-action" onclick={openLocalNetworkSettings}>
        <Icon name="open_in_new" size={14} /> Open Local Network settings
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
