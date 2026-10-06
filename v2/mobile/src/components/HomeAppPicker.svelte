<script lang="ts">
  import type { OtherPackage } from "../lib/types";
  import { filterHomeCandidates } from "../lib/launcherView";

  // Every installed app, for Launcher › Advanced "Set another app as Home".
  // Apps the TV listed as Home handlers are marked; an unmarked app is not
  // known to lack a Home screen, it just wasn't in that list.
  let {
    packages,
    loading,
    error,
    homePackages,
    onPick,
    onRetry,
    onCancel,
  }: {
    packages: OtherPackage[];
    loading: boolean;
    error: string;
    homePackages: Set<string>;
    onPick: (p: OtherPackage) => void;
    onRetry: () => void;
    onCancel: () => void;
  } = $props();

  let search = $state("");
  let showSystem = $state(false);

  const rows = $derived(filterHomeCandidates(packages, search, showSystem));
</script>

<div class="picker">
  <input
    class="picker-search"
    bind:value={search}
    placeholder="Search installed apps…"
    aria-label="Search installed apps by name or package"
  />
  <label class="picker-toggle">
    <input type="checkbox" bind:checked={showSystem} />
    <span>Show system apps</span>
  </label>
  {#if loading}
    <div class="center small">
      <span class="statuspill live"><span class="pdot blink"></span>Reading installed apps…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="ghost picker-retry" onclick={onRetry}>Retry</button>
  {:else if rows.length === 0}
    <p class="lede empty">No matching apps.</p>
  {:else}
    <span class="picker-count">
      {rows.length} shown · {packages.length} installed app{packages.length === 1 ? "" : "s"}
    </span>
    <div class="picker-list">
      {#each rows as p (p.package)}
        <button
          class="picker-row"
          onclick={() => onPick(p)}
          aria-label={`Choose ${p.name ? `${p.name} (${p.package})` : p.package}`}
        >
          <span class="msr picker-icon">{homePackages.has(p.package) ? "home" : "android"}</span>
          <span class="picker-body">
            <span class="picker-name">{p.name ?? p.package}</span>
            <span class="picker-pkg mono">{p.package}</span>
            <span class="picker-flags">
              {homePackages.has(p.package) ? "Home app · " : ""}{p.system ? "System app" : "User app"}{p.enabled
                ? ""
                : " · Disabled"}
            </span>
          </span>
        </button>
      {/each}
    </div>
  {/if}
  <button class="ghost-link" onclick={onCancel}>Cancel</button>
</div>

<style>
  .picker {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .picker-search {
    width: 100%;
    box-sizing: border-box;
    min-height: 46px;
    padding: 0 14px;
    border-radius: 12px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    font-family: var(--sans);
    font-size: 14px;
  }
  .picker-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    color: var(--text-soft);
    min-height: 32px;
  }
  .picker-toggle input {
    width: 18px;
    height: 18px;
    accent-color: var(--accent);
  }
  .picker-count {
    font-family: var(--mono);
    font-size: 11px;
    color: var(--dim);
  }
  .picker-list {
    display: flex;
    flex-direction: column;
    gap: 8px;
    max-height: 360px;
    overflow-y: auto;
  }
  .picker-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 12px;
    border-radius: 13px;
    border: 1px solid var(--line);
    background: var(--surface);
    color: var(--text);
    font-family: var(--sans);
    text-align: left;
    cursor: pointer;
  }
  .picker-row:active {
    background: var(--surface-2);
  }
  .picker-icon {
    font-size: 22px;
    color: var(--text-soft);
    flex: none;
  }
  .picker-body {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .picker-name {
    font-size: 14px;
    font-weight: 600;
  }
  .picker-pkg {
    font-family: var(--mono);
    font-size: 10px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .picker-flags {
    font-size: 11px;
    color: var(--dim);
  }
  .picker-retry {
    min-height: 44px;
    font-size: 14px;
  }
  .center.small {
    display: grid;
    place-items: center;
    padding: 18px 0;
  }
</style>
