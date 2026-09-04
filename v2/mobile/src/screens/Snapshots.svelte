<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type { SnapshotApplyPlan, SnapshotFile } from "../lib/types";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";

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
  // serial the plan was computed against is pinned here — switching TVs while
  // the sheet is open must never apply TV A's plan to TV B.
  let pending = $state<{
    snap: SnapshotFile;
    plan: SnapshotApplyPlan;
    serial: string;
  } | null>(null);
  let planExpanded = $state(false);
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

  async function beginApply(snap: SnapshotFile) {
    const serial = session.serial;
    if (busyPath || !serial) return;
    busyPath = snap.path;
    try {
      const plan = await api.previewApply(serial, snap.path);
      if (serial !== session.serial) {
        showToast("The connected TV changed while the plan was loading. Preview again.", "error");
        return;
      }
      planExpanded = false;
      pending = { snap, plan, serial };
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
    } finally {
      busyPath = "";
    }
  }

  async function confirmApply() {
    const p = pending;
    pending = null;
    if (!p) return;
    if (p.serial !== session.serial) {
      showToast(
        "The connected TV changed since this plan was built — nothing was applied. Preview again.",
        "error",
      );
      return;
    }
    busyPath = p.snap.path;
    try {
      const res = await api.applySnapshot(p.serial, p.snap.path);
      showToast(res.summary || "Snapshot applied.", res.packages_failed.length || res.settings_failed.length ? "info" : "success");
      session.invalidateAll();
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else showToast(String(e), "error");
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
      showToast("Snapshot deleted.", "success");
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

  const settingsToWrite = $derived(
    pending ? Object.entries(pending.plan.settings_to_write) : [],
  );
  const nothingToDo = $derived(
    pending != null &&
      pending.plan.packages_to_disable.length === 0 &&
      settingsToWrite.length === 0 &&
      pending.plan.launcher_to_set == null,
  );
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={back} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Snapshots</h3>
    <span style="width:44px"></span>
  </div>

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Loading snapshots…</span>
    </div>
  {:else if locked}
    <div class="locked-card">
      <span class="locked-icon msr">photo_camera_back</span>
      <h2>Snapshots are a Pro feature</h2>
      <p class="locked-desc">
        Record this TV's disabled apps, launcher and tracked settings to a file, then re-apply that
        record later or to another compatible TV. Applying only re-disables and re-writes what was
        recorded — it never re-enables or reinstalls anything.
      </p>
      <button class="primary" onclick={() => (showPaywall = true)}>
        <span class="msr">bolt</span>Unlock Pro
      </button>
      <button class="ghost-link" onclick={() => navigate("more")}>Already have a key? Enter it in More</button>
    </div>
    <div class="spacer"></div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={load}>Retry</button>
    <div class="spacer"></div>
  {:else}
    <!-- Create -->
    <div class="create-card">
      <span class="card-label">Save current setup</span>
      <p class="card-desc">Captures this TV's disabled packages, launcher and tracked settings.</p>
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
        No snapshots yet. Save one above to record which apps are disabled, which launcher is
        active, and the tracked settings.
      </p>
      <div class="spacer"></div>
    {:else}
      <div class="snap-list">
        {#each snapshots as snap (snap.path)}
          {@const busy = busyPath === snap.path}
          <div class="snap-card">
            <div class="snap-head">
              <div class="snap-title-wrap">
                <span class="snap-title">{snap.label ?? snap.device_name}</span>
                <span class="snap-date mono">{fmtDate(snap.saved_at)}</span>
              </div>
              <button class="icon-del" disabled={busyPath !== ""} onclick={() => (deleteTarget = snap)} aria-label="Delete snapshot">
                <span class="msr">delete</span>
              </button>
            </div>
            <div class="snap-meta">
              <span class="chip"><span class="msr">block</span>{snap.disabled_count} disabled</span>
              <span class="chip"><span class="msr">tune</span>{snap.settings_count} settings</span>
              {#if snap.launcher}<span class="chip"><span class="msr">home</span>launcher</span>{/if}
            </div>
            <button class="apply-btn" disabled={busyPath !== ""} onclick={() => beginApply(snap)}>
              {#if busy}<span class="pdot blink"></span>Working…{:else}<span class="msr">settings_backup_restore</span>Preview &amp; apply{/if}
            </button>
          </div>
        {/each}
      </div>
      <div class="spacer"></div>
    {/if}
  {/if}

  {#if pending}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="plan-overlay" onclick={() => (pending = null)}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="plan-card" onclick={(e) => e.stopPropagation()}>
        <span class="plan-icon msr">settings_backup_restore</span>
        <h3>Apply "{pending.snap.label ?? pending.snap.device_name}"?</h3>
        <p class="plan-target">
          to <strong>{session.deviceLabel}</strong>
        </p>

        {#if pending.plan.cross_device_warning}
          <p class="plan-warning" role="alert">
            <span class="msr">warning</span>{pending.plan.cross_device_warning}
          </p>
        {/if}

        {#if nothingToDo}
          <p class="plan-empty">This TV already matches the snapshot — nothing to change.</p>
        {:else}
          <div class="plan-rows">
            <div class="plan-row">
              <span class="msr">block</span>
              <div class="plan-body">
                <span class="plan-line">
                  Disable {pending.plan.packages_to_disable.length} app{pending.plan
                    .packages_to_disable.length === 1
                    ? ""
                    : "s"}
                </span>
                <span class="plan-sub">
                  {pending.plan.packages_already_disabled.length} already disabled ·
                  {pending.plan.packages_not_installed.length} not installed
                </span>
              </div>
              {#if pending.plan.packages_to_disable.length > 0}
                <button class="plan-toggle" onclick={() => (planExpanded = !planExpanded)}>
                  {planExpanded ? "Hide" : "Show"}
                  <span class="msr" class:flip={planExpanded}>expand_more</span>
                </button>
              {/if}
            </div>
            {#if planExpanded}
              <ul class="plan-pkgs mono">
                {#each pending.plan.packages_to_disable as pkg (pkg)}
                  <li>{pkg}</li>
                {/each}
              </ul>
            {/if}

            <div class="plan-row">
              <span class="msr">home</span>
              <div class="plan-body">
                {#if pending.plan.launcher_to_set}
                  <span class="plan-line">Set the recorded launcher</span>
                  <span class="plan-sub mono">{pending.plan.launcher_to_set}</span>
                {:else}
                  <span class="plan-line dim">Launcher unchanged</span>
                {/if}
              </div>
            </div>

            <div class="plan-row">
              <span class="msr">tune</span>
              <div class="plan-body">
                <span class="plan-line" class:dim={settingsToWrite.length === 0}>
                  {settingsToWrite.length === 0
                    ? "No settings to write"
                    : `Write ${settingsToWrite.length} setting${settingsToWrite.length === 1 ? "" : "s"}`}
                </span>
                {#if pending.plan.settings_already_set.length > 0}
                  <span class="plan-sub">
                    {pending.plan.settings_already_set.length} already match
                  </span>
                {/if}
              </div>
            </div>
          </div>
        {/if}

        <p class="plan-note">
          Applying re-disables the recorded packages, sets the recorded launcher and writes the
          recorded settings. It does not re-enable, reinstall or restore anything the snapshot
          didn't record.
        </p>

        <div class="plan-actions">
          <button class="ghost small" onclick={() => (pending = null)}>Cancel</button>
          <button class="primary small" disabled={nothingToDo} onclick={confirmApply}>Apply</button>
        </div>
      </div>
    </div>
  {/if}

  <ConfirmDialog
    open={deleteTarget !== null}
    danger
    icon="delete"
    title="Delete this snapshot?"
    message="The saved file will be removed from this phone. Your TV is not affected."
    confirmLabel="Delete"
    onConfirm={() => deleteTarget && doDelete(deleteTarget)}
    onCancel={() => (deleteTarget = null)}
  />

  <PaywallSheet open={showPaywall} {navigate} onClose={() => (showPaywall = false)} />
  <Toast message={toast} type={toastType} />
</div>

<style>
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
    display: grid;
    place-items: center;
    padding: 20px;
    z-index: 400;
    overflow-y: auto;
  }
  .plan-card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 22px;
    width: 100%;
    max-width: 340px;
    padding: 22px;
    box-shadow: 0 20px 50px rgba(0, 0, 0, 0.7);
    display: flex;
    flex-direction: column;
    gap: 8px;
    box-sizing: border-box;
    max-height: calc(100vh - 40px);
    overflow-y: auto;
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
  .plan-empty {
    margin: 0;
    font-size: 13px;
    color: var(--muted);
    text-align: center;
  }
  .plan-rows {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .plan-row {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 11px 12px;
    border-radius: 12px;
    background: var(--canvas);
    border: 1px solid var(--line);
  }
  .plan-row > .msr {
    font-size: 19px;
    color: var(--muted);
    flex: none;
  }
  .plan-body {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .plan-line {
    font-size: 13px;
    font-weight: 600;
  }
  .plan-line.dim {
    color: var(--muted);
    font-weight: 500;
  }
  .plan-sub {
    font-size: 11px;
    color: var(--muted);
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .plan-toggle {
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
  .plan-pkgs {
    list-style: none;
    margin: 0;
    padding: 10px 12px;
    max-height: 180px;
    overflow-y: auto;
    border-radius: 12px;
    background: var(--canvas);
    border: 1px solid var(--line);
    display: flex;
    flex-direction: column;
    gap: 5px;
    font-family: var(--mono);
    font-size: 11px;
    color: var(--text-soft);
    word-break: break-all;
  }
  .plan-note {
    margin: 4px 0 8px;
    font-size: 11px;
    color: var(--muted);
    line-height: 1.45;
  }
  .plan-actions {
    display: flex;
    gap: 10px;
    width: 100%;
  }
  .plan-actions .ghost,
  .plan-actions .primary {
    flex: 1;
  }
</style>
