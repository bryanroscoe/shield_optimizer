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

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  let loading = $state(true);
  let error = $state("");
  let locked = $state(false);
  let showPaywall = $state(false);
  let snapshots = $state<SnapshotFile[]>([]);

  let label = $state("");
  let saving = $state(false);
  let busyPath = $state("");

  // Apply flow: preview first, confirm with the real plan, then apply.
  let pending = $state<{ snap: SnapshotFile; plan: SnapshotApplyPlan } | null>(null);
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
    if (busyPath || !session.serial) return;
    busyPath = snap.path;
    try {
      const plan = await api.previewApply(session.serial, snap.path);
      pending = { snap, plan };
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
    if (!p || !session.serial) return;
    busyPath = p.snap.path;
    try {
      const res = await api.applySnapshot(session.serial, p.snap.path);
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

  const planSummary = $derived.by(() => {
    const p = pending?.plan;
    if (!p) return "";
    const parts: string[] = [];
    if (p.packages_to_disable.length) parts.push(`disable ${p.packages_to_disable.length} app${p.packages_to_disable.length === 1 ? "" : "s"}`);
    const settings = Object.keys(p.settings_to_write).length;
    if (settings) parts.push(`write ${settings} setting${settings === 1 ? "" : "s"}`);
    if (p.launcher_to_set) parts.push("set the saved launcher");
    if (parts.length === 0) return "This TV already matches the snapshot — nothing to change.";
    return `This will ${parts.join(", ")}. Every change is reversible.`;
  });
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={() => navigate("more")} aria-label="Back">
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
        Capture this TV's disabled apps, launcher and tweaks to a file, then reapply those recorded
        changes later or to another compatible TV.
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
      <p class="lede empty">No snapshots yet. Save one above to enable one-tap rollback.</p>
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

  <ConfirmDialog
    open={pending !== null}
    icon="settings_backup_restore"
    title={`Apply "${pending?.snap.label ?? pending?.snap.device_name ?? "snapshot"}"?`}
    warning={pending?.plan.cross_device_warning ?? ""}
    message={planSummary}
    confirmLabel="Apply"
    onConfirm={confirmApply}
    onCancel={() => (pending = null)}
  />

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
</style>
