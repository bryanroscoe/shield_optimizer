<script lang="ts">
  // Bottom-sheet detail for one package (design §8.1). Safety tier + reason come
  // ONLY from the core `safety_info` command via src/lib/safety.ts — never an
  // inline classifier. Memory / usage / storage are passed in from the parent's
  // lazily-loaded maps, each with when it was read. Destructive actions are
  // emitted as callbacks so the parent owns the optimistic-update + Pro-gating
  // logic. Desktop's AppDetailPanel is the reference for the figures.
  import { onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import { reasonOf, tierOf, type SafetyStatus } from "../lib/safety";
  import { safetySourceLabel } from "../../../shared/safety";
  import { canOfferUninstall, recommendation } from "../lib/recommendation";
  import { formatBytes, hasStorage, measuredAt, type AppMeasurements, type MeasureStatus } from "../lib/app-details";
  import type { AppItem } from "../lib/appsList";
  import type { AppStorage, AppUsage, Safety } from "../lib/types";

  let {
    app,
    memoryMb,
    usage,
    storage,
    measures,
    onRemeasure,
    busy,
    uninstalled = false,
    canReport = false,
    onClose,
    onToggle,
    onForceStop,
    onUninstall,
    onPlayStore,
    onReinstall,
    onReport,
    onVerdict,
  }: {
    app: AppItem | null;
    memoryMb: number | null;
    usage: AppUsage | null;
    storage: AppStorage | null;
    measures: AppMeasurements;
    onRemeasure?: () => void;
    busy: boolean;
    /// True once the package has been removed for the TV's current user, so
    /// the only meaningful actions left are Reinstall / Play Store.
    uninstalled?: boolean;
    canReport?: boolean;
    onClose: () => void;
    onToggle: (app: AppItem) => void;
    onForceStop: (app: AppItem) => void;
    onUninstall: (app: AppItem) => void;
    onPlayStore: (app: AppItem) => void;
    onReinstall?: (app: AppItem) => void;
    onReport?: (app: AppItem) => void;
    /// The verdict this sheet resolved, so a report opened from here carries
    /// the same one the user is looking at.
    onVerdict?: (pkg: string, verdict: Safety | null) => void;
  } = $props();

  let safety = $state<Safety | null>(null);
  let safetyLoading = $state(false);
  let safetyError = $state(false);
  let safetyRequest = 0;
  let safetyRetry = $state(0);
  let tierOpen = $state(false);
  let sourcesOpen = $state(false);

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
    sourcesOpen = false;
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
        if (!current()) return;
        safety = result;
        onVerdict?.(pkg, result);
      })
      .catch(() => {
        if (!current()) return;
        safetyError = true;
        onVerdict?.(pkg, null);
      })
      .finally(() => {
        if (current()) safetyLoading = false;
      });
  });

  type ApkRead =
    | { status: "idle" }
    | { status: "loading" }
    | { status: "ready"; at: number; value: AppStorage }
    | { status: "unavailable"; at: number; reason: string };

  /// The fallback for a package diskstats has no row for: its APK files only.
  let apk = $state<ApkRead>({ status: "idle" });
  let apkToken = 0;
  const storageSettled = $derived(
    measures.storage.status === "ready" || measures.storage.status === "unavailable",
  );
  const needsApkFallback = $derived(
    !!app && !uninstalled && app.state !== "missing" && storageSettled && !hasStorage(storage ?? undefined),
  );

  $effect(() => {
    const pkg = app?.package ?? "";
    const device = session.connectedDevice;
    const generation = session.generation;
    // Keyed on the batch read it falls back from: a re-measure starts a fresh
    // fallback, and a reply to an older one is dropped.
    void measures.storage;
    const token = ++apkToken;
    if (!needsApkFallback || !pkg || !device || !session.isConnected) {
      apk = { status: "idle" };
      return;
    }
    const serial = device.serial;
    const current = () =>
      token === apkToken && app?.package === pkg && session.serial === serial && session.generation === generation;
    apk = { status: "loading" };
    api.appApkSize(serial, pkg).then(
      (value) => {
        if (!current()) return;
        apk = hasStorage(value)
          ? { status: "ready", at: Date.now(), value }
          : { status: "unavailable", at: Date.now(), reason: "no APK size reported" };
      },
      (e) => {
        if (current()) apk = { status: "unavailable", at: Date.now(), reason: String(e) };
      },
    );
  });

  onDestroy(() => {
    ++safetyRequest;
    ++apkToken;
  });

  const tier = $derived(tierOf(safety));
  const reason = $derived(reasonOf(safety));
  const blocked = $derived(safety?.kind === "never_disable");
  // Fail closed: no verdict yet means no destructive action.
  const safetyUnavailable = $derived(safetyLoading || safetyError || safety === null);
  const stateKnown = $derived(app?.state === "enabled" || app?.state === "disabled");
  const isEnabled = $derived(app?.state === "enabled");
  const disableBlocked = $derived(isEnabled && !uninstalled && (safetyUnavailable || blocked));
  const uninstallBlocked = $derived(safetyUnavailable || blocked || !stateKnown);
  const missing = $derived(app?.state === "missing");
  const offerUninstall = $derived(!!app && (!app.entry || canOfferUninstall(app.entry)));
  const offerPlayStore = $derived(!!app && (!app.entry || app.entry.play_store));

  const safetyStatus = $derived<SafetyStatus>(
    safety
      ? { status: "ready", verdict: safety }
      : safetyLoading
        ? { status: "checking" }
        : { status: "unavailable", reason: "lookup failed" },
  );
  const rec = $derived(app?.entry && !uninstalled ? recommendation(app.entry, app.state, safetyStatus) : null);
  const showDescription = $derived(
    !!app?.description && !(reason && reason.includes(app.description.trim())),
  );
  const sources = $derived(app?.entry?.sources ?? []);

  const measuring = $derived(
    measures.memory.status === "loading" ||
      measures.usage.status === "loading" ||
      measures.storage.status === "loading",
  );

  function pending(m: MeasureStatus): string | null {
    if (m.status === "idle") return "Not read yet";
    if (m.status === "loading") return "Reading…";
    return null;
  }

  function sized(bytes: number | null): string {
    return formatBytes(bytes) ?? "unavailable";
  }

  type Figure = { value: string; scope: string; warn?: boolean };

  const memoryFigure = $derived.by((): Figure => {
    const m = measures.memory;
    const p = pending(m);
    if (p) return { value: p, scope: "RAM in use, from dumpsys meminfo" };
    if (m.status === "unavailable") {
      return { value: "Unavailable", scope: `dumpsys meminfo failed at ${measuredAt(m.at)}`, warn: true };
    }
    if (m.status !== "ready") return { value: "", scope: "" };
    if (memoryMb != null && memoryMb > 0) {
      return {
        value: `${memoryMb.toFixed(memoryMb >= 100 ? 0 : 1)} MB`,
        scope: `RAM in use at ${measuredAt(m.at)}, not a promise of what stopping it frees`,
      };
    }
    return { value: "Not running", scope: `No process for this app at ${measuredAt(m.at)}` };
  });

  const storageFigure = $derived.by((): Figure => {
    const m = measures.storage;
    if (m.status === "idle" || m.status === "loading") {
      return { value: pending(m) ?? "", scope: "Disk space, from dumpsys diskstats" };
    }
    if (m.status === "ready" && hasStorage(storage ?? undefined) && storage) {
      const cache = formatBytes(storage.cache_bytes);
      return {
        value: `App ${sized(storage.app_bytes)} · Data ${sized(storage.data_bytes)}${cache ? ` (incl. ${cache} cache)` : ""}`,
        scope: `Disk space, not RAM · measured at ${measuredAt(m.at)} · Android refreshes these sizes on its own schedule, so they can be older than that`,
      };
    }
    const why =
      m.status === "unavailable"
        ? `dumpsys diskstats failed at ${measuredAt(m.at)}`
        : `No storage row for this app at ${measuredAt(m.at)}`;
    if (apk.status === "loading") return { value: "Reading APK size…", scope: why };
    if (apk.status === "ready") {
      return {
        value: `APK ${sized(apk.value.app_bytes)} · data and cache unavailable`,
        scope: `${why}. APK files only, measured at ${measuredAt(apk.at)}`,
      };
    }
    if (apk.status === "unavailable") {
      return { value: "Unavailable", scope: `${why}. The APK size couldn't be read either.`, warn: true };
    }
    return { value: "Unavailable", scope: why, warn: true };
  });

  const usageFigure = $derived.by((): Figure => {
    const m = measures.usage;
    const p = pending(m);
    if (p) return { value: p, scope: "Last opened, from dumpsys usagestats" };
    if (m.status === "unavailable") {
      return { value: "Unavailable", scope: `dumpsys usagestats failed at ${measuredAt(m.at)}`, warn: true };
    }
    if (m.status !== "ready") return { value: "", scope: "" };
    const history = "Android keeps about a year of history and clears it on a factory reset";
    if (!usage || !usage.last_used) {
      return { value: "No recorded use", scope: `As of ${measuredAt(m.at)} · ${history}, so this is not proof it was never opened` };
    }
    const launches = usage.launch_count === 1 ? "1 launch" : `${usage.launch_count} launches`;
    return { value: `${usage.last_used} · ${launches}`, scope: `Measured at ${measuredAt(m.at)} · ${history}` };
  });

  function fmtLabel(a: AppItem): string {
    return a.name || a.package;
  }

  function iconFor(a: AppItem): string {
    if (a.system) return "system_update";
    const n = (a.name ?? "").toLowerCase();
    return n.includes("video") || n.includes("tv") ? "smart_display" : "apps";
  }

  function stateLabel(a: AppItem): { text: string; off: boolean } {
    if (uninstalled) return { text: "Uninstalled", off: true };
    if (a.state === "enabled") return { text: "Enabled", off: false };
    if (a.state === "disabled") return { text: "Disabled", off: true };
    if (a.state === "missing") return { text: "Not installed", off: true };
    return { text: "State unavailable", off: true };
  }

  function requestToggle(a: AppItem) {
    if (!stateKnown) return;
    if (isEnabled && (safetyUnavailable || blocked)) return;
    onToggle(a);
  }

  function requestUninstall(a: AppItem) {
    if (uninstallBlocked || !offerUninstall) return;
    onUninstall(a);
  }
</script>

{#if app}
  {@const st = stateLabel(app)}
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
        <button class="close" onclick={onClose} aria-label="Close"><span class="msr">close</span></button>
      </div>

      <div class="tags">
        <span class="state-tag" class:off={st.off}>{st.text}</span>
        {#if safetyLoading}
          <span class="tier-tag loading">Checking safety…</span>
        {:else if safetyError}
          <span class="tier-tag unavailable">Safety unavailable</span>
        {:else if tier}
          <span class="tier-tag {tier.cls}">{tier.label}</span>
        {/if}
        {#if app.entry}
          <span class="meta-tag recognised">Recognised</span>
        {/if}
      </div>

      {#if showDescription}
        <p class="description">{app.description}</p>
      {/if}

      {#if tier}
        <div class="reason-block">
          <span class="reason-label">If you remove it</span>
          <span class="reason-text">{reason}</span>
          {#if rec}
            <span class="rec-line" data-rec={rec.label}>
              Our suggestion: <strong>{rec.label}</strong>
            </span>
          {/if}
          <button class="tier-toggle" onclick={() => (tierOpen = !tierOpen)} aria-expanded={tierOpen}>
            What does “{tier.label}” mean?
            <span class="msr" class:open={tierOpen}>expand_more</span>
          </button>
          {#if tierOpen}
            <span class="tier-explainer">{tier.description}</span>
          {/if}
        </div>
      {/if}

      <div class="reversible">
        <span class="msr">restore</span>
        <span>
          Disable is reversible with Enable. Uninstall removes the app for this user — Reinstall
          brings back an APK still on the TV; anything else needs the Play Store.
        </span>
      </div>

      {#if uninstalled || missing}
        <div class="sheet-actions">
          <button class="act-btn wide" disabled={busy || !onReinstall} onclick={() => onReinstall?.(app)}>
            <span class="msr">download</span>Reinstall
          </button>
          {#if offerPlayStore}
            <button class="act-btn wide" disabled={busy} onclick={() => onPlayStore(app)}>
              <span class="msr">shop</span>Play Store
            </button>
          {/if}
        </div>
      {:else}
        <div class="sheet-actions">
          <button class="act-btn" disabled={busy || !stateKnown} onclick={() => onForceStop(app)}>
            <span class="msr">stop_circle</span>Force stop
          </button>
          {#if offerPlayStore}
            <button class="act-btn" disabled={busy} onclick={() => onPlayStore(app)}>
              <span class="msr">shop</span>Play Store
            </button>
          {/if}
        </div>
        <div class="sheet-actions">
          <button
            class="act-btn wide"
            class:danger={isEnabled && !disableBlocked}
            disabled={busy || !stateKnown || disableBlocked}
            onclick={() => requestToggle(app)}
          >
            {app.state === "disabled" ? "Enable" : "Disable"}
          </button>
          {#if offerUninstall}
            <button class="act-btn wide danger" disabled={busy || uninstallBlocked} onclick={() => requestUninstall(app)}>
              Uninstall<span class="pro-badge">PRO</span>
            </button>
          {/if}
        </div>
      {/if}

      {#if !stateKnown && !missing && !uninstalled}
        <p class="blocked-note">This app's state couldn't be read. Refresh the list before changing it.</p>
      {:else if blocked}
        <p class="blocked-note">This package is protected — it can't be disabled or uninstalled from here.</p>
      {:else if safetyError}
        <p class="blocked-note">
          Safety unavailable. Disable and uninstall stay locked.
          <button class="retry-safety" onclick={() => ++safetyRetry}>Retry safety check</button>
        </p>
      {:else if app.entry && !offerUninstall && !missing && !uninstalled}
        <p class="fine-note">Not offered for uninstall: the Play Store can't give this one back. Disable is reversible.</p>
      {/if}
      {#if safety || app.entry?.reviewed_at || sources.length > 0}
        <div class="provenance">
          {#if safety}
            <span>{safetySourceLabel(safety.source)}</span>
          {/if}
          {#if app.entry?.reviewed_at}
            <span>Reviewed {app.entry.reviewed_at}</span>
          {/if}
          {#if sources.length > 0}
            <button class="tier-toggle" onclick={() => (sourcesOpen = !sourcesOpen)} aria-expanded={sourcesOpen}>
              {sources.length === 1 ? "1 source" : `${sources.length} sources`}
              <span class="msr" class:open={sourcesOpen}>expand_more</span>
            </button>
          {/if}
        </div>
        {#if sourcesOpen}
          <ul class="sources">
            {#each sources as source, i (i)}
              <li class="mono">{source}</li>
            {/each}
          </ul>
        {/if}
      {/if}

      {#if !uninstalled && !missing}
        <dl class="measures" aria-label="Measurements">
          {#each [
            { key: "ram", label: "RAM", f: memoryFigure },
            { key: "storage", label: "Storage", f: storageFigure },
            { key: "usage", label: "Last used", f: usageFigure },
          ] as row (row.key)}
            <div class="measure" data-measure={row.key}>
              <dt>{row.label}</dt>
              <dd>
                <span class="mono measure-value" class:warn={row.f.warn}>{row.f.value}</span>
                {#if row.f.scope}<span class="measure-scope">{row.f.scope}</span>{/if}
              </dd>
            </div>
          {/each}
        </dl>
      {/if}

      <div class="tools">
        {#if onRemeasure && !uninstalled && !missing}
          <button class="link-btn" onclick={() => onRemeasure?.()} disabled={measuring}>
            <span class="msr">refresh</span>{measuring ? "Measuring…" : "Re-measure"}
          </button>
        {/if}
        {#if canReport && onReport}
          <button class="link-btn" onclick={() => onReport?.(app)}>
            <span class="msr">warning</span>Report this app
          </button>
        {/if}
      </div>

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
    max-height: 90vh;
    overflow-y: auto;
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
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--muted);
    cursor: pointer;
  }
  .close .msr {
    font-size: 20px;
  }
  .description {
    margin: 0;
    font-size: 13px;
    color: var(--text-soft);
    line-height: 1.45;
  }
  .rec-line {
    display: inline-flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 4px;
    margin-top: 4px;
    font-size: 12px;
    color: var(--text-soft);
  }
  .provenance {
    display: flex;
    flex-wrap: wrap;
    align-items: center;
    gap: 4px 12px;
    font-size: 11px;
    color: var(--muted);
  }
  .sources {
    margin: -6px 0 0;
    padding-left: 18px;
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .measures {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 8px 14px;
    margin: 0;
    padding: 12px 14px;
    border-radius: 13px;
    background: var(--surface);
    border: 1px solid var(--line);
  }
  .measure {
    display: contents;
  }
  .measures dt {
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--muted);
    padding-top: 2px;
  }
  .measures dd {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .measure-value {
    font-size: 12px;
    color: var(--text);
    overflow-wrap: anywhere;
  }
  .measure-value.warn {
    color: var(--amber);
  }
  .measure-scope {
    font-size: 10px;
    color: var(--muted);
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .tools {
    display: flex;
    flex-wrap: wrap;
    gap: 6px 16px;
  }
  .tools:empty {
    display: none;
  }
  .link-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 32px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--accent);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .link-btn:disabled {
    opacity: 0.5;
  }
  .link-btn .msr {
    font-size: 16px;
  }
  .fine-note {
    margin: 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
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
    white-space: nowrap;
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
  .tier-tag.safe {
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 14%, transparent);
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
  .meta-tag.recognised {
    font-size: 10px;
    font-weight: 700;
    padding: 4px 9px;
    border-radius: 7px;
    color: var(--text-soft);
    background: color-mix(in srgb, var(--text) 6%, transparent);
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
