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
  import Files from "./screens/Files.svelte";
  import Backups from "./screens/Backups.svelte";
  import ConnectionBanner from "./components/ConnectionBanner.svelte";

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
  async function handleDisconnect() {
    await session.disconnect();
    router.reset("onboarding");
  }

  // A dropped TV socket is only visible when something talks to it. Probe on
  // foreground resume and on a slow heartbeat so a dead connection turns into
  // a silent reconnect (or the banner) within a minute instead of never.
  const LIVENESS_HEARTBEAT_MS = 45_000;

  onMount(() => {
    session.loadEntitlement();
    const cleanupBack = initBackHandler();
    const detachWatch = session.attachConnectionWatch();
    const probe = () => {
      if (document.visibilityState === "visible" && session.connectedDevice) {
        session.checkLiveness();
      }
    };
    document.addEventListener("visibilitychange", probe);
    const heartbeat = setInterval(probe, LIVENESS_HEARTBEAT_MS);
    return () => {
      cleanupBack();
      detachWatch();
      clearInterval(heartbeat);
      document.removeEventListener("visibilitychange", probe);
    };
  });
</script>

{#if router.current !== "onboarding" && router.current !== "addtv"}
  <ConnectionBanner onSwitch={() => navigate("devices")} />
{/if}

{#if router.current === "onboarding"}
  <Onboarding intent="launch" onConnected={handleConnected} />
{:else if router.current === "addtv"}
  <Onboarding intent="add" onConnected={handleConnected} onCancel={back} />
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
  <Launcher {navigate} {back} />
{:else if router.current === "tweaks"}
  <Tweaks {navigate} {back} />
{:else if router.current === "snapshots"}
  <Snapshots {navigate} {back} />
{:else if router.current === "devices"}
  <Devices {navigate} {back} onDisconnect={handleDisconnect} />
{:else if router.current === "riskguide"}
  <RiskGuide {navigate} {back} />
{:else if router.current === "files"}
  <Files {back} />
{:else if router.current === "backups"}
  <Backups {back} />
{/if}
