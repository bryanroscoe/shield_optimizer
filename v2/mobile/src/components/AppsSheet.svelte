<script lang="ts">
  import { modal } from "../lib/modal";
  import type { LaunchableApp } from "../lib/types";
  import { appLabel, filterApps, MAX_PINS } from "../../../shared/remote-shortcuts";

  /// The Remote's Apps sheet: every app the TV can open, by name, with Open and
  /// Pin. Reading the list and launching belong to the Remote screen.
  let {
    apps,
    loading,
    error,
    pins,
    message,
    onOpen,
    onPin,
    onRefresh,
    onClose,
  }: {
    apps: LaunchableApp[] | null;
    loading: boolean;
    error: string;
    pins: string[];
    message: string;
    onOpen: (pkg: string) => void;
    onPin: (pkg: string) => void;
    onRefresh: () => void;
    onClose: () => void;
  } = $props();

  const id = $props.id();
  let query = $state("");
  let shown = $derived(apps ? filterApps(apps, query) : []);
</script>

<!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
<div class="apps-overlay" use:modal={{ onClose }} onclick={onClose}>
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div
    class="apps-sheet"
    role="dialog"
    aria-modal="true"
    aria-labelledby="{id}-title"
    tabindex="-1"
    onclick={(event) => event.stopPropagation()}
  >
    <div class="apps-head">
      <h3 id="{id}-title">Apps on this TV</h3>
      <span class="pin-count">{pins.length} of {MAX_PINS} pinned</span>
    </div>
    <input class="apps-search" type="search" placeholder="Search apps" aria-label="Search apps" bind:value={query} />
    <div class="apps-scroll">
      {#if loading && !apps}
        <p class="apps-note" role="status">Reading the TV's apps…</p>
      {:else if error}
        <p class="apps-error" role="alert">Couldn't list the TV's apps. {error}</p>
        <button class="ghost apps-retry" onclick={onRefresh}>Try again</button>
      {:else if apps && apps.length === 0}
        <p class="apps-note">The TV didn't list any apps it can open.</p>
      {:else if apps}
        {#if shown.length === 0}
          <p class="apps-note">No app matches "{query.trim()}".</p>
        {/if}
        <ul class="apps-list">
          {#each shown as app (app.package)}
            {@const pinned = pins.includes(app.package)}
            <li class="app-item" data-app={app.package}>
              <button class="app-open" onclick={() => onOpen(app.package)} aria-label={`Open ${appLabel(app)}`}>
                <span class="app-name">{appLabel(app)}</span>
                {#if app.name}<span class="mono app-pkg">{app.package}</span>{/if}
              </button>
              <button
                class="app-pin"
                class:pinned
                onclick={() => onPin(app.package)}
                aria-pressed={pinned}
                aria-label={`${pinned ? "Unpin" : "Pin"} ${appLabel(app)}`}
                disabled={!pinned && pins.length >= MAX_PINS}
              >{pinned ? "Unpin" : "Pin"}</button>
            </li>
          {/each}
        </ul>
      {/if}
    </div>
    <p class="apps-message" role="status">{message}</p>
    <button type="button" class="ghost apps-done" onclick={onClose}>Done</button>
  </div>
</div>

<style>
  .apps-overlay {
    position: fixed;
    inset: 0;
    z-index: 500;
    display: flex;
    align-items: flex-end;
    background: rgba(0, 0, 0, 0.6);
    overscroll-behavior: contain;
  }
  .apps-sheet {
    box-sizing: border-box;
    width: 100%;
    max-width: var(--content-max);
    max-height: 85vh;
    margin-inline: auto;
    display: flex;
    flex-direction: column;
    gap: 10px;
    overflow: hidden;
    padding: 20px max(20px, env(safe-area-inset-right)) calc(env(safe-area-inset-bottom) + 18px)
      max(20px, env(safe-area-inset-left));
    background: var(--surface-2);
    border-top: 1px solid var(--line);
    border-radius: 22px 22px 0 0;
  }
  .apps-sheet:focus {
    outline: none;
  }
  .apps-head {
    display: flex;
    flex-wrap: wrap;
    align-items: baseline;
    justify-content: space-between;
    gap: 4px 12px;
  }
  h3 {
    margin: 0;
    font-size: 1.0625rem;
    font-weight: 700;
    color: var(--text);
  }
  .pin-count,
  .apps-note,
  .apps-message {
    color: var(--text-soft);
    font-size: 0.8125rem;
    margin: 0;
  }
  .apps-message:empty {
    display: none;
  }
  .apps-error {
    color: var(--danger);
    font-size: 0.8125rem;
    margin: 0;
    overflow-wrap: anywhere;
  }
  .apps-search {
    box-sizing: border-box;
    width: 100%;
    min-height: 44px;
  }
  .apps-scroll {
    flex: 0 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior-y: contain;
  }
  .apps-list {
    list-style: none;
    margin: 0;
    padding: 0;
  }
  .app-item {
    display: flex;
    align-items: center;
    gap: 8px;
    border-bottom: 1px solid var(--line);
  }
  .app-open {
    flex: 1 1 auto;
    min-width: 0;
    min-height: 52px;
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    justify-content: center;
    gap: 2px;
    padding: 6px 4px;
    border: 0;
    background: transparent;
    color: var(--text);
    text-align: left;
    font-family: var(--sans);
    cursor: pointer;
  }
  .app-name {
    font-size: 0.9375rem;
    font-weight: 600;
    overflow-wrap: anywhere;
  }
  .app-pkg {
    font-size: 0.6875rem;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .app-pin {
    flex: none;
    min-height: 44px;
    min-width: 64px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: 12px;
    background: transparent;
    color: var(--text);
    font-family: var(--sans);
    font-size: 0.8125rem;
    font-weight: 600;
    cursor: pointer;
  }
  .app-pin.pinned {
    border-color: var(--accent);
    color: var(--accent);
  }
  .app-pin:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .app-open:focus-visible,
  .app-pin:focus-visible {
    outline: 2px solid var(--accent);
    outline-offset: 2px;
    border-radius: 8px;
  }
  .apps-retry,
  .apps-done {
    flex: none;
    width: 100%;
    min-height: 48px;
  }
</style>
