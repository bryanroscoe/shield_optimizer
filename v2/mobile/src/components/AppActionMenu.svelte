<script lang="ts">
  // Long-press menu for one app row — the phone's counterpart to desktop's
  // right-click menu (#129). It only routes: Disable/Enable go through the
  // screen's own gated paths (safety lookup, confirm, fail closed), exactly as
  // the detail sheet's buttons do.
  import type { AppItem, AppMenuItem } from "../lib/appsList";

  let {
    app,
    items,
    onClose,
  }: {
    app: AppItem | null;
    items: AppMenuItem[];
    onClose: () => void;
  } = $props();
</script>

{#if app}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="menu-overlay" onclick={onClose} oncontextmenu={(e) => e.preventDefault()}>
    <!-- svelte-ignore a11y_click_events_have_key_events -->
    <div
      class="menu"
      role="menu"
      tabindex="-1"
      aria-label={`Actions for ${app.name ?? app.package}`}
      onclick={(e) => e.stopPropagation()}
    >
      <div class="menu-head">
        <span class="name" class:mono={!app.name}>{app.name ?? app.package}</span>
        {#if app.name}<span class="mono pkg">{app.package}</span>{/if}
      </div>
      {#each items as item (item.id)}
        <button
          class="menu-item"
          class:danger={item.danger && !item.disabled}
          role="menuitem"
          disabled={item.disabled}
          onclick={() => {
            onClose();
            item.run();
          }}
        >
          <span class="msr">{item.icon}</span>{item.label}
        </button>
      {/each}
    </div>
  </div>
{/if}

<style>
  .menu-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.5);
    z-index: 310;
    display: flex;
    align-items: flex-end;
  }
  .menu {
    width: 100%;
    background: var(--surface-2);
    border-top: 1px solid var(--line);
    border-radius: 22px 22px 0 0;
    padding: 16px 12px calc(env(safe-area-inset-bottom) + 14px);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 2px;
    animation: sheetUp 0.2s ease-out;
    user-select: none;
    -webkit-user-select: none;
  }
  .menu:focus {
    outline: none;
  }
  .menu-head {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 0 10px 10px;
    border-bottom: 1px solid var(--line);
    margin-bottom: 6px;
    min-width: 0;
  }
  .name {
    font-size: 15px;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .pkg {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .menu-item {
    display: flex;
    align-items: center;
    gap: 14px;
    min-height: 48px;
    padding: 0 10px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    color: var(--text);
    font-family: var(--sans);
    font-size: 14px;
    text-align: left;
    cursor: pointer;
  }
  .menu-item:active {
    background: var(--surface);
  }
  .menu-item:disabled {
    opacity: 0.45;
    cursor: not-allowed;
  }
  .menu-item .msr {
    font-size: 20px;
    color: var(--muted);
  }
  .menu-item.danger,
  .menu-item.danger .msr {
    color: var(--danger);
  }
  @keyframes sheetUp {
    from {
      transform: translateY(100%);
    }
    to {
      transform: translateY(0);
    }
  }
</style>
