<script lang="ts">
  // Bottom-sheet detail for one package (design §8.1). Safety tier + reason come
  // ONLY from the core `safety_info` command via src/lib/safety.ts — never an
  // inline classifier. Memory / last-used are passed in from the parent's
  // lazily-loaded maps. Destructive actions are emitted as callbacks so the
  // parent owns the optimistic-update + Pro-gating logic.
  import { onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { reasonOf, tierOf } from "../lib/safety";
  import type { AppUsage, OtherPackage, Safety } from "../lib/types";

  let {
    app,
    memoryMb,
    usage,
    busy,
    uninstalled = false,
    onClose,
    onToggle,
    onForceStop,
    onUninstall,
    onPlayStore,
    onReinstall,
  }: {
    app: OtherPackage | null;
    memoryMb: number | null;
    usage: AppUsage | null;
    busy: boolean;
    /// True once the package has been removed for the TV's current user, so
    /// the only meaningful actions left are Reinstall / Play Store.
    uninstalled?: boolean;
    onClose: () => void;
    onToggle: (app: OtherPackage) => void;
    onForceStop: (app: OtherPackage) => void;
    onUninstall: (app: OtherPackage) => void;
    onPlayStore: (app: OtherPackage) => void;
    onReinstall?: (app: OtherPackage) => void;
  } = $props();

  let safety = $state<Safety | null>(null);
  let safetyLoading = $state(false);
  let safetyError = $state(false);
  let safetyRequest = 0;
  let safetyRetry = $state(0);
  let tierOpen = $state(false);

  // Reload safety whenever the selected package changes.
  $effect(() => {
    const pkg = app?.package ?? "";
    const device = session.connectedDevice;
    const liveness = session.liveness;
    const generation = session.generation;
    void safetyRetry;
    const request = ++safetyRequest;
    safety = null;
    safetyError = false;
    tierOpen = false;
    safetyLoading = pkg !== "" && device !== null && liveness === "live";
    if (!pkg) return;
    if (!device || liveness !== "live") {
      safetyError = true;
      return;
    }
    const serial = device.serial;
    const current = () =>
      request === safetyRequest &&
      app?.package === pkg &&
      session.serial === serial &&
      session.generation === generation &&
      session.isConnected;
    api
      .safetyInfo(pkg)
      .then((result) => {
        if (current()) safety = result;
      })
      .catch(() => {
        if (current()) safetyError = true;
      })
      .finally(() => {
        if (current()) safetyLoading = false;
      });
  });

  onDestroy(() => ++safetyRequest);

  const tier = $derived(tierOf(safety));
  const reason = $derived(reasonOf(safety));
  const blocked = $derived(safety?.kind === "never_disable");
  // Fail closed: no verdict yet means no destructive action.
  const safetyUnavailable = $derived(safetyLoading || safetyError || safety === null);
  const disableBlocked = $derived(
    app?.enabled && !uninstalled && (safetyUnavailable || blocked),
  );
  const uninstallBlocked = $derived(safetyUnavailable || blocked);

  function fmtLabel(a: OtherPackage): string {
    return a.name || a.package;
  }

  function iconFor(a: OtherPackage): string {
    if (a.system) return "system_update";
    const n = (a.name ?? "").toLowerCase();
    return n.includes("video") || n.includes("tv") ? "smart_display" : "apps";
  }

  function requestToggle(a: OtherPackage) {
    if (a.enabled && (safetyUnavailable || blocked)) return;
    onToggle(a);
  }

  function requestUninstall(a: OtherPackage) {
    if (safetyUnavailable || blocked) return;
    onUninstall(a);
  }
</script>

{#if app}
  <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
  <div class="sheet-overlay" onclick={onClose}>
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="sheet" onclick={(e) => e.stopPropagation()}>
      <div class="sheet-head">
        <div class="avatar"><span class="msr">{iconFor(app)}</span></div>
        <div class="head-body">
          <span class="app-name">{fmtLabel(app)}</span>
          <span class="mono app-pkg">{app.package}</span>
        </div>
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="close msr" onclick={onClose}>close</span>
      </div>

      <div class="tags">
        {#if uninstalled}
          <span class="state-tag off">Uninstalled</span>
        {:else}
          <span class="state-tag" class:off={!app.enabled}>{app.enabled ? "Enabled" : "Disabled"}</span>
        {/if}
        {#if safetyLoading}
          <span class="tier-tag loading">Checking safety…</span>
        {:else if safetyError}
          <span class="tier-tag unavailable">Safety unavailable</span>
        {:else if tier}
          <span class="tier-tag {tier.cls}">{tier.label}</span>
        {/if}
        {#if memoryMb != null && memoryMb > 0}
          <span class="mono meta-tag"><span class="msr">memory</span>{Math.round(memoryMb)} MB</span>
        {/if}
      </div>

      {#if tier}
        <div class="reason-block">
          <span class="reason-label">If you remove it</span>
          <span class="reason-text">{reason}</span>
          <button class="tier-toggle" onclick={() => (tierOpen = !tierOpen)} aria-expanded={tierOpen}>
            What does “{tier.label}” mean?
            <span class="msr" class:open={tierOpen}>expand_more</span>
          </button>
          {#if tierOpen}
            <span class="tier-explainer">{tier.description}</span>
          {/if}
        </div>
      {/if}

      {#if usage}
        <div class="usage-row">
          <span class="msr">history</span>
          <span class="usage-text">
            {usage.last_used ? `Last opened ${usage.last_used}` : "Never opened"}
            {usage.launch_count > 0 ? ` · ${usage.launch_count} launches` : ""}
          </span>
        </div>
      {/if}

      <div class="reversible">
        <span class="msr">restore</span>
        <span>
          Disable is reversible with Enable. Uninstall removes the app for this user — Reinstall
          brings back an APK still on the TV; anything else needs the Play Store.
        </span>
      </div>

      {#if uninstalled}
        <div class="sheet-actions">
          <button class="act-btn wide" disabled={busy || !onReinstall} onclick={() => onReinstall?.(app)}>
            <span class="msr">download</span>Reinstall
          </button>
          <button class="act-btn wide" disabled={busy} onclick={() => onPlayStore(app)}>
            <span class="msr">shop</span>Play Store
          </button>
        </div>
      {:else}
        <div class="sheet-actions">
          <button class="act-btn" disabled={busy} onclick={() => onForceStop(app)}>
            <span class="msr">stop_circle</span>Force stop
          </button>
          <button class="act-btn" disabled={busy} onclick={() => onPlayStore(app)}>
            <span class="msr">shop</span>Play Store
          </button>
        </div>
        <div class="sheet-actions">
          <button
            class="act-btn wide"
            class:danger={app.enabled && !disableBlocked}
            disabled={busy || disableBlocked}
            onclick={() => requestToggle(app)}
          >
            {app.enabled ? "Disable" : "Enable"}
          </button>
          <button class="act-btn wide danger" disabled={busy || uninstallBlocked} onclick={() => requestUninstall(app)}>
            Uninstall<span class="pro-badge">PRO</span>
          </button>
        </div>
      {/if}

      {#if blocked}
        <p class="blocked-note">This package is protected — it can't be disabled or uninstalled from here.</p>
      {:else if safetyError}
        <p class="blocked-note">
          Safety unavailable. Disable and uninstall stay locked.
          <button class="retry-safety" onclick={() => ++safetyRetry}>Retry safety check</button>
        </p>
      {/if}
    </div>
  </div>
{/if}

<style>
  .sheet-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.6);
    z-index: 300;
    display: flex;
    align-items: flex-end;
  }
  .sheet {
    width: 100%;
    background: var(--surface-2);
    border-top: 1px solid var(--line);
    border-radius: 22px 22px 0 0;
    padding: 20px 24px calc(env(safe-area-inset-bottom) + 20px);
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 14px;
    animation: sheetUp 0.25s ease-out;
  }
  .sheet-head {
    display: flex;
    align-items: center;
    gap: 12px;
    position: relative;
  }
  .avatar {
    width: 46px;
    height: 46px;
    border-radius: 13px;
    background: var(--surface);
    display: grid;
    place-items: center;
    flex: none;
  }
  .avatar .msr {
    font-size: 24px;
    color: var(--text-soft);
  }
  .head-body {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
    padding-right: 24px;
  }
  .app-name {
    font-size: 16px;
    font-weight: 700;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .app-pkg {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .close {
    position: absolute;
    top: 0;
    right: 0;
    font-size: 20px;
    color: var(--muted);
    cursor: pointer;
  }

  .tags {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
    align-items: center;
  }
  .state-tag {
    font-size: 10px;
    font-weight: 700;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
    padding: 4px 9px;
    border-radius: 7px;
  }
  .state-tag.off {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
  }
  .tier-tag {
    font-size: 10px;
    font-weight: 700;
    padding: 4px 9px;
    border-radius: 7px;
    text-transform: uppercase;
    letter-spacing: 0.03em;
  }
  .tier-tag.loading {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
    text-transform: none;
  }
  .tier-tag.unavailable {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .tier-tag.unknown {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 7%, transparent);
  }
  .tier-tag.caution {
    color: var(--amber);
    background: color-mix(in srgb, var(--amber) 14%, transparent);
  }
  .tier-tag.blocked {
    color: var(--danger);
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .meta-tag {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-soft);
  }
  .meta-tag .msr {
    font-size: 14px;
    color: var(--muted);
  }

  .reason-block {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding: 13px 14px;
    border-radius: 13px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .reason-label {
    font-size: 11px;
    font-weight: 600;
    color: var(--muted);
    text-transform: uppercase;
    letter-spacing: 0.06em;
  }
  .reason-text {
    font-size: 13px;
    color: var(--text-soft);
    line-height: 1.45;
  }
  .tier-toggle {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    align-self: flex-start;
    margin-top: 4px;
    padding: 0;
    background: transparent;
    border: none;
    color: var(--accent);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .tier-toggle .msr {
    font-size: 16px;
    transition: transform 0.15s ease;
  }
  .tier-toggle .msr.open {
    transform: rotate(180deg);
  }
  .tier-explainer {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
  }

  .usage-row {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 12px;
    color: var(--muted);
  }
  .usage-row .msr {
    font-size: 16px;
  }

  .reversible {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }
  .reversible .msr {
    font-size: 16px;
    color: var(--teal);
    flex: none;
  }

  .sheet-actions {
    display: flex;
    gap: 9px;
  }
  .act-btn {
    flex: 1;
    height: 46px;
    border: 1px solid rgba(255, 255, 255, 0.1);
    border-radius: 13px;
    background: var(--surface);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 6px;
  }
  .act-btn:active {
    background: var(--surface-2);
  }
  .act-btn:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }
  .act-btn .msr {
    font-size: 18px;
  }
  .act-btn.danger {
    border-color: rgba(251, 107, 95, 0.3);
    background: rgba(251, 107, 95, 0.1);
    color: var(--danger);
  }
  .pro-badge {
    font-size: 9px;
    font-weight: 700;
    background: var(--accent);
    color: var(--accent-ink);
    padding: 2px 5px;
    border-radius: 5px;
  }
  .blocked-note {
    margin: 0;
    font-size: 11px;
    color: var(--danger);
    line-height: 1.4;
  }
  .retry-safety {
    margin-left: 6px;
    padding: 0;
    border: 0;
    background: transparent;
    color: inherit;
    font: inherit;
    font-weight: 700;
    text-decoration: underline;
    cursor: pointer;
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
