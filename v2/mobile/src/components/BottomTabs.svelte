<script lang="ts">
  import type { Screen } from "../lib/router.svelte";
  import { session } from "../lib/session.svelte";

  // Tabs map directly to router screens. The Home tab routes to "dashboard";
  // the router has no "home" screen. `active` is the current screen so
  // highlighting is correct.
  let { active, navigate }: { active: Screen; navigate: (tab: Screen) => void } =
    $props();

  const tabs: { id: Screen; icon: string; label: string }[] = [
    { id: "dashboard", icon: "grid_view", label: "Home" },
    { id: "apps", icon: "apps", label: "Apps" },
    { id: "tweaks", icon: "tune", label: "Tweaks" },
    { id: "remote", icon: "stadia_controller", label: "Remote" },
    { id: "more", icon: "more_horiz", label: "More" },
  ];
</script>

<nav class="bottom-tabs" aria-label="Main">
  {#each tabs as tab (tab.id)}
    <button
      class="tab-btn"
      class:active={active === tab.id}
      class:locked={session.applyInProgress}
      disabled={session.applyInProgress}
      aria-disabled={session.applyInProgress}
      aria-current={active === tab.id ? "page" : undefined}
      onclick={() => navigate(tab.id)}
    >
      <span class="msr" class:fill={active === tab.id} aria-hidden="true">{tab.icon}</span>
      <span class="tab-label">{tab.label}</span>
    </button>
  {/each}
</nav>

<style>
  /* Five equal columns that always fit: at 150% text a phone is ~275 CSS px
     wide. A label that still runs out of room ellipsizes; its full text stays
     the button's accessible name. */
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
    padding: 11px max(8px, env(safe-area-inset-right), calc((100% - var(--content-max)) / 2))
      calc(env(safe-area-inset-bottom) + 8px)
      max(8px, env(safe-area-inset-left), calc((100% - var(--content-max)) / 2));
    box-sizing: border-box;
    z-index: 10;
  }

  .tab-btn {
    flex: 1 1 0;
    min-width: 0;
    background: transparent;
    border: none;
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    color: var(--dim);
    cursor: pointer;
    font-family: var(--sans);
    font-size: 10px;
    font-weight: 500;
    padding: 4px 2px;
    box-sizing: border-box;
  }

  .tab-btn.active {
    color: var(--accent);
    font-weight: 600;
  }

  .tab-btn .msr {
    font-size: 24px;
  }

  .tab-label {
    max-width: 100%;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    line-height: 1.25;
  }

  /* Locked while an apply loop is running so the loop can't be orphaned. */
  .tab-btn.locked {
    opacity: 0.4;
    cursor: not-allowed;
  }
</style>
