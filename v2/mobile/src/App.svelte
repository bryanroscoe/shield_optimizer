<script lang="ts">
  import { onMount } from "svelte";
  import { router, initBackHandler, type Screen } from "./lib/router.svelte";
  import { session } from "./lib/session.svelte";
  import Onboarding from "./screens/Onboarding.svelte";
  import Dashboard from "./screens/Dashboard.svelte";
  import Diagnostics from "./screens/Diagnostics.svelte";
  import Optimize from "./screens/Optimize.svelte";
  import Apps from "./screens/Apps.svelte";
  import Remote from "./screens/Remote.svelte";
  import More from "./screens/More.svelte";
  import Launcher from "./screens/Launcher.svelte";
  import Tweaks from "./screens/Tweaks.svelte";
  import Snapshots from "./screens/Snapshots.svelte";
  import Devices from "./screens/Devices.svelte";
  import RiskGuide from "./screens/RiskGuide.svelte";

  function navigate(screen: Screen) {
    router.navigate(screen);
  }
  function back() {
    if (!router.back()) router.navigate("dashboard");
  }

  function handleConnected() {
    // Onboarding has already populated the session via session.connect().
    router.reset("dashboard");
  }
  function handleDisconnect() {
    session.disconnect();
    router.reset("onboarding");
  }

  onMount(() => {
    session.loadEntitlement();
    const cleanupBack = initBackHandler();
    // Re-probe liveness when the app returns to the foreground (webview resume)
    // so a connection dropped while backgrounded shows the reconnect banner.
    const onVisibility = () => {
      if (document.visibilityState === "visible" && session.connectedDevice) {
        session.checkLiveness();
      }
    };
    document.addEventListener("visibilitychange", onVisibility);
    return () => {
      cleanupBack();
      document.removeEventListener("visibilitychange", onVisibility);
    };
  });
</script>

{#if router.current === "onboarding"}
  <Onboarding onConnected={handleConnected} />
{:else if router.current === "dashboard"}
  <Dashboard {navigate} onDisconnect={handleDisconnect} />
{:else if router.current === "diagnostics"}
  <Diagnostics {navigate} {back} />
{:else if router.current === "optimize"}
  <Optimize {navigate} />
{:else if router.current === "apps"}
  <Apps {navigate} />
{:else if router.current === "remote"}
  <Remote {navigate} />
{:else if router.current === "more"}
  <More {navigate} onDisconnect={handleDisconnect} />
{:else if router.current === "launcher"}
  <Launcher {navigate} />
{:else if router.current === "tweaks"}
  <Tweaks {navigate} />
{:else if router.current === "snapshots"}
  <Snapshots {navigate} />
{:else if router.current === "devices"}
  <Devices {navigate} onDisconnect={handleDisconnect} />
{:else if router.current === "riskguide"}
  <RiskGuide {navigate} />
{/if}
