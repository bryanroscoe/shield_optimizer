<script lang="ts">
  import HelpSheet from "../components/HelpSheet.svelte";
  import { onMount, untrack } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { SnapshotApplyPlan, SnapshotFile } from "../lib/types";
  import {
    keptPackages,
    launcherSkipNote,
    previewRows,
    previewSummary,
    restoreOutcome,
    snapshotName,
    type RestoreOutcomeRow,
  } from "../lib/snapshotPreview";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import ErrorNotice from "../components/ErrorNotice.svelte";
  import { destructiveWarning } from "../../../shared/support";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import { billing } from "../lib/billing.svelte";
  import Toast from "../components/Toast.svelte";
  import { modal } from "../lib/modal";

  let {
    navigate,
    back,
  }: { navigate: (screen: Screen) => void; back: () => void } = $props();

  let loading = $state(true);
  let error = $state("");
  let locked = $state(false);
  let showPaywall = $state(false);
  let snapshots = $state<SnapshotFile[]>([]);

  let label = $state("");
  let saving = $state(false);
  let busyPath = $state("");

  // Apply flow: preview first, confirm with the real plan, then apply. The
  // connection the plan was computed against is pinned here: a silent redial
  // can land on a different id-less TV at the same address, so a new
  // generation invalidates the plan just like a different serial.
  let pending = $state<{
    snap: SnapshotFile;
    plan: SnapshotApplyPlan;
    serial: string;
    generation: number;
  } | null>(null);
  let unchangedOpen = $state(false);
  let deleteTarget = $state<SnapshotFile | null>(null);

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 4200);
  }

  function isLocked(e: unknown): boolean {
    return String(e).includes("LOCKED:");
  }

  async function load() {
    loading = snapshots.length === 0;
    error = "";
    locked = false;
    try {
      snapshots = await api.listSnapshots();
    } catch (e) {
      if (isLocked(e)) locked = true;
      else error = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  // Pro bought from this screen's paywall: reload instead of leaving the lock up.
  $effect(() => {
    if (locked && session.isPro) untrack(() => void load());
  });

  async function save() {
    if (saving || !session.serial) return;
    saving = true;
    try {
      const snap = await api.saveSnapshot(
        session.serial,
        session.deviceLabel,
        label.trim() || undefined,
      );
      showToast(`Saved "${snap.label ?? snap.device_name}".`, "success");
      label = "";
      await load();
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      saving = false;
    }
  }

  function ownsConnection(serial: string, generation: number): boolean {
    return session.isConnected && serial === session.serial && generation === session.generation;
  }

  const CONNECTION_CHANGED =
    "The TV connection changed since this preview was built, so nothing was applied. Preview again.";

  // A reconnect (even to the same address) or a lost connection closes the
  // preview: its plan and "Now" column describe the previous connection.
  $effect(() => {
    void session.serial;
    void session.generation;
    void session.isConnected;
    untrack(() => {
      if (pending && !ownsConnection(pending.serial, pending.generation)) {
        pending = null;
        showToast(CONNECTION_CHANGED, "error");
      }
    });
  });

  async function beginApply(snap: SnapshotFile) {
    const serial = session.serial;
    const generation = session.generation;
    if (busyPath || !serial) return;
    if (!session.isConnected) {
      showToast("No TV connected. Connect a TV, then preview again.", "error");
      return;
    }
    busyPath = snap.path;
    try {
      const plan = await api.previewApply(serial, snap.path);
      if (!ownsConnection(serial, generation)) {
        showToast("The TV connection changed while the plan was loading. Preview again.", "error");
        return;
      }
      pending = { snap, plan, serial, generation };
      unchangedOpen = previewRows(plan, snap.launcher).unchanged.length <= 6;
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      busyPath = "";
    }
  }

  /// A restore that threw, or finished with items that didn't apply.
  let applyFailure = $state<{ title: string; error: unknown } | null>(null);
  /// What the finished restore skipped or kept, each with the backend's reason.
  let notRestored = $state<RestoreOutcomeRow[]>([]);

  async function confirmApply() {
    const p = pending;
    pending = null;
    if (!p) return;
    if (!ownsConnection(p.serial, p.generation)) {
      showToast(CONNECTION_CHANGED, "error");
      return;
    }
    busyPath = p.snap.path;
    applyFailure = null;
    notRestored = [];
    try {
      const res = await api.applySnapshot(p.serial, p.snap.path);
      const outcome = restoreOutcome(res, p.plan);
      // Only the summary names a stock launcher left off with its setup
      // helper (Setup Wraith) still on; that is not a clean success either.
      const followUp = res.summary?.includes("Setup Wraith");
      showToast(
        res.summary || "Snapshot applied.",
        outcome.length || followUp ? "info" : "success",
      );
      notRestored = outcome;
      if (outcome.length > 0) {
        applyFailure = {
          title: `${outcome.length} item${outcome.length === 1 ? "" : "s"} didn't restore`,
          error: res.summary || "Some changes didn't apply.",
        };
      }
      session.invalidateAll();
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else applyFailure = { title: "Couldn't restore the snapshot", error: e };
    } finally {
      busyPath = "";
    }
  }

  async function doDelete(snap: SnapshotFile) {
    deleteTarget = null;
    if (busyPath) return;
    busyPath = snap.path;
    try {
      await api.deleteSnapshot(snap.path);
      showToast(`Deleted snapshot "${snapshotName(snap)}".`, "success");
      await load();
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      busyPath = "";
    }
  }

  function fmtDate(iso: string): string {
    const d = new Date(iso);
    if (Number.isNaN(d.getTime())) return iso;
    return d.toLocaleDateString(undefined, { month: "short", day: "numeric" }) +
      " · " + d.toLocaleTimeString(undefined, { hour: "numeric", minute: "2-digit" });
  }

  const rows = $derived(
    pending ? previewRows(pending.plan, pending.snap.launcher) : { acting: [], unchanged: [] },
  );
  const nothingToDo = $derived(pending != null && rows.acting.length === 0);
  const homeSkip = $derived(pending ? launcherSkipNote(pending.plan) : null);
  const kept = $derived(pending ? keptPackages(pending.plan) : []);

  // As in ConfirmDialog: the plan scrolls inside the card and Cancel/Restore
  // stay pinned, with a divider once anything is cut off.
  let planBody = $state<HTMLElement>();
  let planContent = $state<HTMLElement>();
  let planOverflowing = $state(false);
  $effect(() => {
    if (!planBody || !planContent) return;
    const scroller = planBody;
    const update = () => (planOverflowing = scroller.scrollHeight > scroller.clientHeight + 1);
    const observer = new ResizeObserver(update);
    observer.observe(scroller);
    observer.observe(planContent);
    update();
    return () => observer.disconnect();
  });
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Snapshots <span class="beta-tag">Beta</span></h3>
    <span style="width:44px"></span>
  </div>

  {#if applyFailure}
    <ErrorNotice
      error={applyFailure.error}
      title={applyFailure.title}
      action="Snapshot restore"
      onDismiss={() => {
        applyFailure = null;
        notRestored = [];
      }}
    />
    {#if notRestored.length > 0}
      <ul class="plan-list not-restored" aria-label="Not restored">
        {#each notRestored as row, i (i)}
          <li class="plan-item">
            <span class="plan-key mono">{row.item}</span>
            <span class="plan-why">{row.reason}</span>
          </li>
        {/each}
      </ul>
    {/if}
  {/if}

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading snapshots…</span>
    </div>
  {:else if locked}
    <div class="locked-card">
      <span class="locked-icon msr">photo_camera_back</span>
      <h2>Snapshots are a Pro feature</h2>
      <p class="locked-desc">
        A snapshot records which apps are disabled, which known apps weren't installed, your Home
        app and a set of display, audio, HDMI-CEC and screensaver settings. Restoring it disables
        those apps if they're on and puts the settings back. It never re-enables anything, and it
        never uninstalls or reinstalls apps.
      </p>
      <button class="primary" onclick={() => (showPaywall = true)}>
        <span class="msr">bolt</span>Unlock Pro
      </button>
      {#if billing.licenseKeyEntry}
        <button class="ghost-link" onclick={() => navigate("more")}>Already have a key? Enter it in More</button>
      {/if}
    </div>
    <div class="spacer"></div>
  {:else if error}
    <ErrorNotice {error} title="Couldn't list snapshots" action="List snapshots" />
    <button class="primary" onclick={load}>Retry</button>
    <div class="spacer"></div>
  {:else}
    <p class="intro">Restoring disables the recorded apps and restores settings. It never re-enables anything or reinstalls apps.</p>
    <HelpSheet title="What snapshots include"><p>A snapshot records disabled apps, your Home app and display, audio, HDMI-CEC and screensaver settings. Use it to set this TV up the same way again, for example after a factory reset. Restoring disables every recorded app, with no per-app choice, on whichever TV you restore it to.</p></HelpSheet>

    <!-- Create -->
    <div class="create-card">
      <span class="card-label">Save current setup</span>
      <p class="card-desc">Records this TV's disabled apps, Home app and tracked settings.</p>
      <div class="create-row">
        <input class="label-input" bind:value={label} placeholder="Label (optional)" onkeydown={(e) => e.key === "Enter" && save()} />
        <button class="primary small-inline" disabled={saving} onclick={save}>
          {#if saving}<span class="pdot blink"></span>{:else}<span class="msr">add_circle</span>{/if}Save
        </button>
      </div>
    </div>

    <span class="section-label">Saved snapshots</span>
    {#if snapshots.length === 0}
      <p class="lede empty">
        No snapshots yet. Save one above to record which apps are disabled, your Home app and the
        tracked settings.
      </p>
      <div class="spacer"></div>
    {:else}
      <div class="snap-list">
        {#each snapshots as snap (snap.path)}
          {@const busy = busyPath === snap.path}
          <div class="snap-card">
            <div class="snap-head">
              <div class="snap-title-wrap">
                <span class="snap-title">{snapshotName(snap)}</span>
                <span class="snap-date mono">{fmtDate(snap.saved_at)}</span>
              </div>
              <button
                class="icon-del"
                disabled={busyPath !== ""}
                onclick={() => (deleteTarget = snap)}
                aria-label={`Delete snapshot ${snapshotName(snap)}`}
              >
                <span class="msr">delete</span>
              </button>
            </div>
            <div class="snap-meta">
              <span class="chip"><span class="msr">block</span>{snap.disabled_count} disabled</span>
              <span class="chip"><span class="msr">tune</span>{snap.settings_count} settings</span>
              {#if snap.launcher}<span class="chip"><span class="msr">home</span>launcher</span>{/if}
            </div>
            <button class="apply-btn" disabled={busyPath !== ""} onclick={() => beginApply(snap)}>
              {#if busy}<span class="pdot blink"></span>Working…{:else}<span class="msr">settings_backup_restore</span>Preview restore{/if}
            </button>
          </div>
        {/each}
      </div>
      <div class="spacer"></div>
    {/if}
  {/if}

  {#if pending}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="plan-overlay" use:modal={{ onClose: () => (pending = null) }} onclick={() => (pending = null)}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div
        class="plan-card"
        role="dialog"
        aria-modal="true"
        aria-labelledby="plan-title"
        tabindex="-1"
        onclick={(e) => e.stopPropagation()}
      >
        <div class="plan-body" bind:this={planBody}>
          <div class="plan-content" bind:this={planContent}>
            <span class="plan-icon msr" aria-hidden="true">settings_backup_restore</span>
            <h3 id="plan-title">Restore "{snapshotName(pending.snap)}"?</h3>
            <p class="plan-target">
              to <strong>{session.deviceLabel}</strong>
            </p>

            {#if pending.plan.cross_device_warning}
              <p class="plan-warning" role="alert">
                <span class="msr">warning</span>{pending.plan.cross_device_warning}
              </p>
            {/if}

            <p class="plan-summary mono">{previewSummary(pending.plan)}</p>

            {#if homeSkip}
              <p class="plan-skip" data-testid="home-skip">
                <span class="msr" aria-hidden="true">home</span>
                <span>The Home app won't change: {homeSkip}</span>
              </p>
            {/if}
            {#if kept.length > 0}
              <p class="plan-skip" data-testid="kept-for-home">
                <span class="msr" aria-hidden="true">info</span>
                <span>
                  Kept on so the TV keeps a Home screen:
                  {kept.map((k) => `${k.package} (${k.reason})`).join(", ")}.
                </span>
              </p>
            {/if}

            {#if nothingToDo}
              <p class="plan-empty">
                {homeSkip || kept.length > 0
                  ? "Nothing to change. Everything else already matches the snapshot."
                  : "This TV already matches the snapshot — nothing to change."}
              </p>
            {:else}
              <span class="plan-heading">What happens</span>
              <ul class="plan-list" aria-label="Changes">
                {#each rows.acting as row (`${row.kind}:${row.item}`)}
                  <li class="plan-item acting">
                    <span class="plan-key mono">{row.item}</span>
                    <span class="plan-now"><span class="now-label">Now</span><span class="mono">{row.now}</span></span>
                    <span class="plan-act {row.kind}">{row.result}</span>
                  </li>
                {/each}
              </ul>
            {/if}

            {#if rows.unchanged.length > 0}
              <button class="plan-toggle" onclick={() => (unchangedOpen = !unchangedOpen)} aria-expanded={unchangedOpen}>
                {rows.unchanged.length} already match or are skipped
                <span class="msr" class:flip={unchangedOpen}>expand_more</span>
              </button>
              {#if unchangedOpen}
                <ul class="plan-list" aria-label="No change">
                  {#each rows.unchanged as row, i (i)}
                    <li class="plan-item dim">
                      <span class="plan-key mono">{row.item}</span>
                      <span class="plan-now"><span class="now-label">Now</span><span class="mono">{row.now}</span></span>
                      <span class="plan-reason" class:long={row.result.length > 24}>{row.result}</span>
                    </li>
                  {/each}
                </ul>
              {/if}
            {/if}

            {#if destructiveWarning(session.support)}
              <p class="plan-support-warning" data-testid="support-warning" role="alert">
                <span class="msr" aria-hidden="true">warning</span>{destructiveWarning(session.support)}
              </p>
            {/if}

            <p class="plan-note">
              Restoring disables the recorded apps and writes the recorded settings back, including
              resetting settings the snapshot recorded as never set. It never re-enables anything or
              reinstalls apps. A disabled app can be turned back on with Emergency recovery under More.
            </p>
          </div>
        </div>

        <div class="plan-actions" class:divided={planOverflowing}>
          <button class="ghost small" onclick={() => (pending = null)}>Cancel</button>
          <button class="primary small" disabled={nothingToDo} onclick={confirmApply}>Restore</button>
        </div>
      </div>
    </div>
  {/if}

  <ConfirmDialog
    open={deleteTarget !== null}
    danger
    icon="delete"
    title={deleteTarget ? `Delete snapshot "${snapshotName(deleteTarget)}"?` : "Delete snapshot?"}
    message={deleteTarget
      ? `Saved ${fmtDate(deleteTarget.saved_at)} · ${deleteTarget.filename}. This permanently deletes the saved file from this phone. Deleting a local snapshot does not change the TV.`
      : ""}
    confirmLabel="Delete"
    onConfirm={() => deleteTarget && doDelete(deleteTarget)}
    onCancel={() => (deleteTarget = null)}
  />

  <PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />
  <Toast message={toast} type={toastType} />
</div>

<style>
  .plan-support-warning {
    display: flex;
    gap: 8px;
    align-items: flex-start;
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--danger);
  }
  .plan-support-warning .msr {
    font-size: 18px;
    flex: none;
  }
  .header-left {
    display: flex;
    gap: 8px;
    align-items: center;
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }

  .locked-card {
    display: flex;
    flex-direction: column;
    align-items: center;
    text-align: center;
    gap: 12px;
    padding: 32px 20px;
    border-radius: 22px;
    background: var(--surface);
    border: 1px solid var(--line);
    margin-top: 20px;
  }
  .locked-icon {
    font-size: 44px;
    color: var(--accent);
  }
  .locked-card h2 {
    margin: 0;
    font-size: 20px;
    font-weight: 700;
  }
  .locked-desc {
    margin: 0 0 8px;
    font-size: 13px;
    color: var(--muted);
    line-height: 1.5;
  }
  .locked-card .primary {
    max-width: 260px;
  }

  .create-card {
    padding: 16px;
    border-radius: 18px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 10px;
    margin-bottom: 6px;
  }
  .card-label {
    font-family: var(--mono);
    font-size: 11px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
  }
  .card-desc {
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
    margin: 0;
  }
  .create-row {
    display: flex;
    gap: 8px;
  }
  .label-input {
    flex: 1;
    min-height: 44px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 11px;
    padding: 0 12px;
    color: var(--text);
    font-size: 13px;
  }
  .label-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .small-inline {
    width: auto;
    min-height: 44px;
    padding: 0 18px;
    font-size: 14px;
    border-radius: 11px;
    flex: none;
    gap: 6px;
  }
  .small-inline .msr {
    font-size: 18px;
  }

  .snap-list {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .snap-card {
    padding: 15px;
    border-radius: 16px;
    background: var(--surface);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 12px;
  }
  .snap-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 10px;
  }
  .snap-title-wrap {
    display: flex;
    flex-direction: column;
    gap: 3px;
    min-width: 0;
  }
  .snap-title {
    font-size: 15px;
    font-weight: 700;
  }
  .snap-date {
    font-size: 11px;
    color: var(--muted);
  }
  .icon-del {
    background: transparent;
    border: none;
    color: var(--dim);
    cursor: pointer;
    padding: 2px;
    flex: none;
  }
  .icon-del:active {
    color: var(--danger);
  }
  .icon-del .msr {
    font-size: 20px;
  }
  .snap-meta {
    display: flex;
    gap: 8px;
    flex-wrap: wrap;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    font-size: 11px;
    color: var(--text-soft);
    background: var(--surface-2);
    border: 1px solid var(--line);
    padding: 4px 9px;
    border-radius: 8px;
  }
  .chip .msr {
    font-size: 14px;
    color: var(--muted);
  }
  .apply-btn {
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    min-height: 44px;
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
    border-radius: 12px;
    background: color-mix(in srgb, var(--accent) 10%, transparent);
    color: var(--accent);
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .apply-btn:active {
    background: color-mix(in srgb, var(--accent) 6%, transparent);
  }
  .apply-btn:disabled {
    opacity: 0.6;
    cursor: default;
  }
  .apply-btn .msr {
    font-size: 18px;
  }

  .plan-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.72);
    display: flex;
    align-items: center;
    justify-content: center;
    padding: max(16px, env(safe-area-inset-top)) max(16px, env(safe-area-inset-right))
      max(16px, env(safe-area-inset-bottom)) max(16px, env(safe-area-inset-left));
    box-sizing: border-box;
    overscroll-behavior: contain;
    z-index: 400;
  }
  /* max-height: 100% of the fixed overlay follows Android's dynamic bars,
     where 100vh does not. */
  .plan-card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 22px;
    width: 100%;
    max-width: 340px;
    max-height: 100%;
    overflow: hidden;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.7);
    display: flex;
    flex-direction: column;
    box-sizing: border-box;
  }
  .plan-body {
    flex: 0 1 auto;
    min-height: 0;
    overflow-y: auto;
    overscroll-behavior: contain;
    padding: 22px 22px 8px;
  }
  .plan-content {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .plan-icon {
    font-size: 36px;
    color: var(--accent);
    align-self: center;
  }
  .plan-card h3 {
    margin: 0;
    font-size: 17px;
    font-weight: 700;
    text-align: center;
  }
  .plan-target {
    margin: 0 0 4px;
    font-size: 12px;
    color: var(--muted);
    text-align: center;
  }
  .plan-warning {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 2px 0 6px;
    padding: 10px 12px;
    border: 1px solid color-mix(in srgb, var(--amber) 42%, transparent);
    border-radius: 10px;
    background: color-mix(in srgb, var(--amber) 10%, transparent);
    color: var(--amber);
    font-size: 12px;
    line-height: 1.4;
  }
  .plan-warning .msr {
    flex: none;
    font-size: 17px;
  }
  .plan-skip {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    margin: 0 0 4px;
    padding: 10px 12px;
    border-radius: 10px;
    border: 1px solid var(--line);
    background: var(--canvas);
    color: var(--text-soft);
    font-size: 12px;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .plan-skip .msr {
    flex: none;
    font-size: 17px;
    color: var(--muted);
  }
  .plan-empty {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
    text-align: center;
  }
  .plan-summary {
    margin: 0 0 4px;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.45;
    text-align: center;
  }
  .plan-heading {
    font-family: var(--mono);
    font-size: 10px;
    font-weight: 600;
    letter-spacing: 0.12em;
    color: var(--dim);
    text-transform: uppercase;
    margin-top: 4px;
  }
  .plan-list {
    list-style: none;
    margin: 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .plan-item {
    display: grid;
    grid-template-columns: minmax(0, 1fr) auto;
    gap: 4px 10px;
    padding: 9px 11px;
    border-radius: 11px;
    background: var(--canvas);
    border: 1px solid var(--line);
  }
  .plan-item.acting {
    box-shadow: inset 3px 0 0 var(--accent);
  }
  .plan-item.dim {
    opacity: 0.6;
  }
  .plan-key {
    grid-column: 1 / -1;
    font-size: 11px;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .plan-now {
    display: flex;
    gap: 6px;
    align-items: baseline;
    min-width: 0;
    font-size: 11px;
    color: var(--muted);
    overflow-wrap: anywhere;
  }
  .now-label {
    font-size: 10px;
    text-transform: uppercase;
    letter-spacing: 0.06em;
    color: var(--dim);
    flex: none;
  }
  .plan-act {
    font-size: 11px;
    font-weight: 600;
    color: var(--accent);
    text-align: right;
    overflow-wrap: anywhere;
    max-width: 60vw;
  }
  .plan-act.setting,
  .plan-act.reset {
    color: var(--text-soft);
    font-weight: 500;
  }
  .not-restored {
    margin: -4px 0 12px;
  }
  .plan-why {
    grid-column: 1 / -1;
    font-size: 11px;
    line-height: 1.4;
    color: var(--text-soft);
    overflow-wrap: anywhere;
  }
  .plan-reason {
    font-size: 10px;
    padding: 2px 7px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--text) 7%, transparent);
    color: var(--muted);
    align-self: center;
    text-align: right;
  }
  /* A sentence-long reason (a skipped Home app) gets its own line. */
  .plan-reason.long {
    grid-column: 1 / -1;
    justify-self: start;
    text-align: left;
    line-height: 1.4;
    overflow-wrap: anywhere;
  }
  .beta-tag {
    margin-left: 4px;
    padding: 2px 6px;
    border-radius: 6px;
    background: color-mix(in srgb, var(--amber) 14%, transparent);
    color: var(--amber);
    font-size: 10px;
    font-weight: 700;
    letter-spacing: 0.04em;
    text-transform: uppercase;
    vertical-align: middle;
  }
  .intro {
    margin: 0 0 12px;
    font-size: 12px;
    color: var(--muted);
    line-height: 1.45;
  }
  .plan-toggle {
    align-self: flex-start;
    display: inline-flex;
    align-items: center;
    gap: 3px;
    min-height: 36px;
    padding: 0 8px;
    border: none;
    border-radius: 9px;
    background: transparent;
    color: var(--accent);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    flex: none;
  }
  .plan-toggle .msr {
    font-size: 18px;
    transition: transform 0.15s;
  }
  .plan-toggle .msr.flip {
    transform: rotate(180deg);
  }
  .plan-note {
    margin: 4px 0 0;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.45;
  }
  .plan-actions {
    flex: none;
    display: flex;
    gap: 10px;
    padding: 8px 22px 22px;
    border-top: 1px solid transparent;
  }
  .plan-actions.divided {
    border-top-color: var(--line);
    padding-top: 14px;
  }
  .plan-actions .ghost,
  .plan-actions .primary {
    flex: 1;
  }
</style>
