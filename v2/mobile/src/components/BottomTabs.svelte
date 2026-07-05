<script lang="ts">
  import type { Screen } from "../lib/router.svelte";

  // Tabs map directly to router screens. The Home tab routes to "dashboard"
  // (the old id was "home", which the router had no screen for — a blank
  // screen bug). `active` is the current screen so highlighting is correct.
  let { active, navigate }: { active: Screen; navigate: (tab: Screen) => void } =
    $props();

  const tabs: { id: Screen; icon: string; label: string }[] = [
    { id: "dashboard", icon: "grid_view", label: "Home" },
    { id: "optimize", icon: "auto_fix_high", label: "Optimize" },
    { id: "apps", icon: "apps", label: "Apps" },
    { id: "remote", icon: "stadia_controller", label: "Remote" },
    { id: "more", icon: "more_horiz", label: "More" },
  ];
</script>

<div class="bottom-tabs">
  {#each tabs as tab (tab.id)}
    <button
      class="tab-btn"
      class:active={active === tab.id}
      onclick={() => navigate(tab.id)}
    >
      <span class="msr" class:fill={active === tab.id}>{tab.icon}</span>
      <span class="tab-label">{tab.label}</span>
    </button>
  {/each}
</div>

<style>
  .bottom-tabs {
    position: fixed;
    bottom: 0;
    left: 0;
    right: 0;
    background: rgba(11, 13, 16, 0.95);
    backdrop-filter: blur(20px);
    -webkit-backdrop-filter: blur(20px);
    border-top: 1px solid rgba(255, 255, 255, 0.07);
    display: flex;
    justify-content: space-around;
    padding: 11px 8px calc(env(safe-area-inset-bottom) + 8px);
    box-sizing: border-box;
    z-index: 10;
  }

  .tab-btn {
    background: transparent;
    border: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    color: #6b727c;
    cursor: pointer;
    font-family: var(--sans);
    font-size: 10px;
    font-weight: 500;
    padding: 4px 12px;
    min-width: 60px;
    box-sizing: border-box;
  }

  .tab-btn.active {
    color: var(--accent);
    font-weight: 600;
  }

  .tab-btn .msr {
    font-size: 24px;
  }
</style>
