<script lang="ts">
  import { api } from "$lib/api";
  import type { AppStorage, AppUsage } from "$lib/types";
  import {
    formatBytes,
    hasStorage,
    measuredAt,
    type AppMeasurements,
    type MeasureStatus,
  } from "$lib/app-details";
  import { daysSinceUsed, usageLabel } from "$lib/usage";
  import Icon from "$lib/components/Icon.svelte";

  // The one detail panel every app row opens — App List, "Everything else"
  // and the Optimize wizard all render it through AppRow, so a figure cannot
  // read one way in one table and another way in the next. It is read-only:
  // the row's own verbs are the only per-app actions, and they keep their
  // existing guarded paths. "Re-measure" re-runs the same three reads.
  let {
    package: pkg,
    kindLabel,
    kindClass,
    reason,
    description,
    reviewedAt,
    serial,
    measures,
    memoryMb,
    usage,
    storage,
    onRemeasure,
  }: {
    package: string;
    kindLabel: string;
    kindClass: string;
    reason: string;
    description?: string;
    /// The catalog entry's review date, passed only for a catalog verdict.
    reviewedAt?: string;
    serial?: string;
    measures?: AppMeasurements;
    memoryMb?: number;
    usage?: AppUsage;
    storage?: AppStorage;
    onRemeasure?: () => void;
  } = $props();

  type ApkRead =
    | { status: "idle" }
    | { status: "loading" }
    | { status: "ready"; at: number; value: AppStorage }
    | { status: "unavailable"; at: number; reason: string };

  /// The fallback for a package diskstats has no row for: its APK files only.
  let apk = $state<ApkRead>({ status: "idle" });
  let apkToken = 0;

  let storageSettled = $derived(
    measures?.storage.status === "ready" || measures?.storage.status === "unavailable",
  );
  let needsApkFallback = $derived(!!serial && storageSettled && !hasStorage(storage));

  $effect(() => {
    if (!needsApkFallback || !serial) {
      apk = { status: "idle" };
      return;
    }
    // Keyed on the batch read it is falling back from: a re-measure starts a
    // fresh fallback, and a reply to an older one is dropped.
    void measures?.storage;
    const token = ++apkToken;
    const target = serial;
    apk = { status: "loading" };
    api.appApkSize(target, pkg).then(
      (value) => {
        if (token !== apkToken) return;
        apk = hasStorage(value)
          ? { status: "ready", at: Date.now(), value }
          : { status: "unavailable", at: Date.now(), reason: "no APK size reported" };
      },
      (e) => {
        if (token !== apkToken) return;
        apk = { status: "unavailable", at: Date.now(), reason: String(e) };
      },
    );
  });

  let busy = $derived(
    !!measures &&
      (measures.memory.status === "loading" ||
        measures.usage.status === "loading" ||
        measures.storage.status === "loading"),
  );

  function pending(m: MeasureStatus): string | null {
    if (m.status === "idle") return "Not read yet";
    if (m.status === "loading") return "Reading…";
    return null;
  }

  function sized(bytes: number | null): string {
    return formatBytes(bytes) ?? "unavailable";
  }

  let memoryValue = $derived.by(() => {
    const m = measures?.memory;
    if (!m) return null;
    const p = pending(m);
    if (p) return { value: p, scope: "PSS from dumpsys meminfo" };
    if (m.status === "unavailable") {
      return { value: "Unavailable", scope: `dumpsys meminfo failed at ${measuredAt(m.at)} — ${m.reason}`, warn: true };
    }
    if (m.status !== "ready") return null;
    if (memoryMb !== undefined && memoryMb > 0) {
      return {
        value: `${memoryMb.toFixed(memoryMb >= 100 ? 0 : 1)} MB PSS`,
        scope: `Proportional set size, summed over this package's processes · dumpsys meminfo at ${measuredAt(m.at)} · RAM in use then, not a promise of what stopping it frees`,
      };
    }
    return {
      value: "Not running",
      scope: `No process for this package in dumpsys meminfo at ${measuredAt(m.at)}`,
    };
  });

  let storageValue = $derived.by(() => {
    const m = measures?.storage;
    if (!m) return null;
    if (m.status === "idle" || m.status === "loading") {
      return { value: pending(m) ?? "", scope: "Installed size from dumpsys diskstats" };
    }
    if (m.status === "ready" && hasStorage(storage)) {
      const cache = formatBytes(storage.cache_bytes);
      return {
        value: `App ${sized(storage.app_bytes)} · Data ${sized(storage.data_bytes)}${cache ? ` (incl. ${cache} cache)` : " · cache unavailable"}`,
        scope: `Disk space, not RAM · dumpsys diskstats read at ${measuredAt(m.at)} · Android caches these sizes and refreshes them on its own schedule, so they can be older than the read`,
      };
    }
    const why =
      m.status === "unavailable"
        ? `dumpsys diskstats failed at ${measuredAt(m.at)} — ${m.reason}`
        : `No row for this package in dumpsys diskstats at ${measuredAt(m.at)}`;
    if (apk.status === "loading") return { value: "Reading APK size…", scope: why };
    if (apk.status === "ready") {
      return {
        value: `APK files ${sized(apk.value.app_bytes)} · data and cache unavailable`,
        scope: `${why}. Fallback: pm path + stat at ${measuredAt(apk.at)} — APK files only, not compiled code, data or cache`,
      };
    }
    if (apk.status === "unavailable") {
      return { value: "Unavailable", scope: `${why}. APK size fallback failed — ${apk.reason}`, warn: true };
    }
    return { value: "Unavailable", scope: why, warn: true };
  });

  let usageValue = $derived.by(() => {
    const m = measures?.usage;
    if (!m) return null;
    const p = pending(m);
    if (p) return { value: p, scope: "Last foreground use from dumpsys usagestats" };
    if (m.status === "unavailable") {
      return { value: "Unavailable", scope: `dumpsys usagestats failed at ${measuredAt(m.at)} — ${m.reason}`, warn: true };
    }
    if (m.status !== "ready") return null;
    const history = "Android keeps roughly a year of history and clears it on a factory reset";
    if (!usage || daysSinceUsed(usage) === null) {
      return {
        value: "No recorded use",
        scope: `Nothing in dumpsys usagestats at ${measuredAt(m.at)} · ${history}, so this is not proof it was never opened`,
      };
    }
    const launches = usage.launch_count === 1 ? "1 launch" : `${usage.launch_count} launches`;
    return {
      value: `${usageLabel(usage).replace(/^./, (c) => c.toUpperCase())} · ${launches}`,
      scope: `${usage.last_used} · dumpsys usagestats at ${measuredAt(m.at)} · ${history}`,
    };
  });
</script>

<div class="safety-detail">
  <span class={`safety-detail-kind safety-${kindClass}`}>{kindLabel}</span>
  <p class="safety-detail-reason">{reason}</p>
  <!-- Only when the reason does not already contain it: a catalog verdict
       appends the app's own description to its sentence. -->
  {#if description && !reason.includes(description.trim())}
    <p class="muted small safety-detail-desc">{description}</p>
  {/if}
  <p class="muted small safety-detail-source mono">
    {pkg}{#if reviewedAt}<span class="safety-detail-reviewed">{` · Reviewed ${reviewedAt}`}</span>{/if}
  </p>

  {#if measures}
    <dl class="measures" aria-label="Measurements">
      {#each [
        { key: "ram", label: "RAM", m: memoryValue },
        { key: "storage", label: "Storage", m: storageValue },
        { key: "usage", label: "Last used", m: usageValue },
      ] as row (row.key)}
        {#if row.m}
          <div class="measure" data-measure={row.key}>
            <dt>{row.label}</dt>
            <dd>
              <span class="measure-value" class:warn={"warn" in row.m && row.m.warn}>{row.m.value}</span>
              <span class="measure-scope">{row.m.scope}</span>
            </dd>
          </div>
        {/if}
      {/each}
    </dl>
    {#if onRemeasure}
      <button class="remeasure" onclick={() => onRemeasure?.()} disabled={busy}>
        <Icon name="refresh" size={14} /> {busy ? "Measuring…" : "Re-measure"}
      </button>
    {/if}
  {/if}
</div>

<style>
  .safety-detail {
    margin: 0 0 0.5rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
    border-left: 3px solid var(--border);
    border-radius: 0 4px 4px 0;
  }
  .safety-detail-kind {
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
  }
  .safety-detail-reason {
    margin: 0.25rem 0 0.35rem;
    line-height: 1.45;
  }
  .safety-detail-desc {
    margin: 0 0 0.3rem;
  }
  .safety-detail-source {
    margin: 0;
  }
  .safety-unavailable,
  .safety-unknown {
    color: var(--fg-muted);
  }
  .safety-protected {
    color: var(--danger);
  }
  .safety-caution {
    color: var(--warn);
  }
  .safety-safe {
    color: var(--ok);
  }
  .measures {
    display: grid;
    grid-template-columns: max-content 1fr;
    gap: 0.35rem 0.9rem;
    margin: 0.6rem 0 0.4rem;
    padding-top: 0.5rem;
    border-top: 1px solid var(--border);
  }
  .measure {
    display: contents;
  }
  dt {
    font-size: 0.72rem;
    letter-spacing: 0.06em;
    text-transform: uppercase;
    color: var(--fg-muted);
    padding-top: 0.1rem;
  }
  dd {
    margin: 0;
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .measure-value {
    font-family: var(--mono);
    font-size: 0.82rem;
  }
  .measure-value.warn {
    color: var(--warn);
  }
  .measure-scope {
    font-size: 0.75rem;
    color: var(--fg-muted);
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .remeasure {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
    display: inline-flex;
    align-items: center;
    gap: 0.3rem;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
</style>
