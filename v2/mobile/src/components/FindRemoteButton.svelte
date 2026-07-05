<script lang="ts">
  // The ONE find-remote control. Rings an Nvidia Shield remote's locator.
  // Only renders for Shield devices (the underlying `am start` is Shield-only
  // and errors on Google TV), uses a consistent `settings_remote` icon, and
  // confirms with a styled in-app dialog before ringing — replacing the six
  // divergent copies that were scattered across the screens.
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import ConfirmDialog from "./ConfirmDialog.svelte";
  import Toast from "./Toast.svelte";

  let confirming = $state(false);
  let busy = $state(false);
  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;

  function showToast(msg: string, type: "success" | "error" | "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3500);
  }

  const isShield = $derived(session.connectedDevice?.device_type === "shield");

  async function ring() {
    confirming = false;
    if (busy || !session.serial) return;
    busy = true;
    try {
      const res = await api.findRemote(session.serial);
      showToast(
        res.ok ? "Ringing your Shield remote — listen for the beep." : res.message || "Couldn't ring the remote.",
        res.ok ? "success" : "error",
      );
    } catch (e) {
      showToast(String(e), "error");
    } finally {
      busy = false;
    }
  }
</script>

{#if isShield}
  <button
    class="iconbtn"
    class:busy
    disabled={busy}
    onclick={() => (confirming = true)}
    aria-label="Find remote"
    title="Find remote"
  >
    <span class="msr">settings_remote</span>
  </button>

  <ConfirmDialog
    open={confirming}
    icon="settings_remote"
    title="Ring the remote?"
    message="Your Shield remote will beep so you can find it. Make sure the TV is on."
    confirmLabel="Ring it"
    cancelLabel="Cancel"
    onConfirm={ring}
    onCancel={() => (confirming = false)}
  />

  <Toast message={toast} type={toastType} />
{/if}

<style>
  .iconbtn.busy {
    opacity: 0.6;
  }
</style>
