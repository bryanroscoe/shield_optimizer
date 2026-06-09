<script lang="ts">
  import { onDestroy } from "svelte";
  import { api } from "$lib/api";

  let { serial }: { serial: string } = $props();

  /// Rolling echo of what live typing has sent (display only).
  let remoteEcho = $state("");
  let remoteMessage = $state("");
  let remoteCaptureFocused = $state(false);
  // Keystrokes are sent strictly in order through one promise chain — a
  // backspace must never overtake the characters typed before it.
  let remoteQueue: Promise<void> = Promise.resolve();
  let remoteBuffer = "";
  let remoteFlushTimer: ReturnType<typeof setTimeout> | null = null;

  function remoteEnqueue(work: () => Promise<void>) {
    remoteQueue = remoteQueue.then(work).catch((e) => {
      remoteMessage = String(e);
    });
  }

  function remoteFlushBuffer() {
    if (remoteFlushTimer) {
      clearTimeout(remoteFlushTimer);
      remoteFlushTimer = null;
    }
    if (!remoteBuffer) return;
    const chunk = remoteBuffer;
    remoteBuffer = "";
    remoteEnqueue(async () => {
      const r = await api.sendText(serial, chunk);
      if (!r.ok) remoteMessage = r.message;
    });
  }

  function sendRemoteKey(key: string) {
    remoteFlushBuffer();
    remoteEnqueue(async () => {
      const r = await api.sendKey(serial, key);
      remoteMessage = r.ok ? "" : r.message;
    });
  }

  function remoteKeydown(e: KeyboardEvent) {
    if (e.metaKey || e.ctrlKey || e.altKey) return;
    if (e.key === "Backspace") {
      e.preventDefault();
      remoteEcho = remoteEcho.slice(0, -1);
      sendRemoteKey("delete");
      return;
    }
    if (e.key === "Enter") {
      e.preventDefault();
      remoteEcho = "";
      sendRemoteKey("enter");
      return;
    }
    if (e.key.length === 1 && e.key >= " " && e.key <= "~") {
      e.preventDefault();
      remoteBuffer += e.key;
      remoteEcho = (remoteEcho + e.key).slice(-60);
      // Batch rapid typing into one `input text` per pause — each adb call
      // is a ~100-300 ms round-trip, so per-character would lag behind fast
      // typists forever.
      if (remoteFlushTimer) clearTimeout(remoteFlushTimer);
      remoteFlushTimer = setTimeout(remoteFlushBuffer, 250);
    }
  }

  onDestroy(() => {
    if (remoteFlushTimer) clearTimeout(remoteFlushTimer);
  });
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-remote" aria-labelledby="tab-remote">
  <h2>Remote</h2>
  <div class="remote-layout">
    <div class="remote-typing">
      <h3>Live typing</h3>
      <p class="muted small">
        Click below and type — keystrokes go straight to whatever field has
        focus on the TV, including Backspace and Enter. Each press is an ADB
        round-trip, so it feels like typing over SSH.
      </p>
      <div
        class="type-capture"
        class:focused={remoteCaptureFocused}
        tabindex="0"
        role="textbox"
        aria-label="Live typing capture — keystrokes are sent to the TV"
        onkeydown={remoteKeydown}
        onfocus={() => (remoteCaptureFocused = true)}
        onblur={() => (remoteCaptureFocused = false)}
      >
        {#if remoteEcho}
          <span class="mono">{remoteEcho}</span><span class="caret">▏</span>
        {:else if remoteCaptureFocused}
          <span class="muted">Type now — sending to the TV…</span><span class="caret">▏</span>
        {:else}
          <span class="muted">Click here, then type</span>
        {/if}
      </div>
      {#if remoteMessage}
        <p class="warn-text small mono">{remoteMessage}</p>
      {/if}
    </div>
    <div class="remote-pad">
      <h3>Buttons</h3>
      <div class="dpad">
        <span></span>
        <button onclick={() => sendRemoteKey("up")} title="D-pad up">▲</button>
        <span></span>
        <button onclick={() => sendRemoteKey("left")} title="D-pad left">◀</button>
        <button class="ok" onclick={() => sendRemoteKey("select")} title="Select / OK">OK</button>
        <button onclick={() => sendRemoteKey("right")} title="D-pad right">▶</button>
        <span></span>
        <button onclick={() => sendRemoteKey("down")} title="D-pad down">▼</button>
        <span></span>
      </div>
      <div class="remote-row">
        <button onclick={() => sendRemoteKey("back")} title="Back">Back</button>
        <button onclick={() => sendRemoteKey("home")} title="Home">Home</button>
      </div>
      <div class="remote-row">
        <button onclick={() => sendRemoteKey("rewind")} title="Rewind">◀◀</button>
        <button onclick={() => sendRemoteKey("play_pause")} title="Play / Pause">▶❙❙</button>
        <button onclick={() => sendRemoteKey("fast_forward")} title="Fast forward">▶▶</button>
      </div>
      <div class="remote-row">
        <button onclick={() => sendRemoteKey("volume_down")} title="Volume down">Vol −</button>
        <button onclick={() => sendRemoteKey("mute")} title="Mute">Mute</button>
        <button onclick={() => sendRemoteKey("volume_up")} title="Volume up">Vol +</button>
      </div>
      <div class="remote-row">
        <button onclick={() => sendRemoteKey("wakeup")} title="Wake the screen (KEYCODE_WAKEUP)">Wake</button>
        <button onclick={() => sendRemoteKey("power")} title="Power toggle (sleep / wake)">Power</button>
      </div>
    </div>
  </div>
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button) live in the layout and are inherited. */
  .card {
    background: var(--bg-surface);
    border: 1px solid var(--border);
    border-radius: 8px;
    padding: 1.2rem;
  }
  .card h2 {
    margin: 0 0 0.8rem;
    font-size: 1.1rem;
  }
  .card h3 {
    margin: 1rem 0 0.4rem;
    font-size: 1rem;
    color: var(--fg-secondary);
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .warn-text {
    color: var(--warn);
  }

  /* Remote-specific styles. */
  .remote-layout {
    display: flex;
    gap: 2.5rem;
    flex-wrap: wrap;
    align-items: flex-start;
  }
  .remote-typing { flex: 1; min-width: 280px; max-width: 480px; }
  .type-capture {
    min-height: 3.2rem;
    padding: 0.8rem;
    border: 1px dashed var(--border);
    border-radius: 6px;
    cursor: text;
    background: var(--bg-inset);
  }
  .type-capture.focused {
    border-style: solid;
    border-color: var(--accent);
  }
  .type-capture .caret {
    color: var(--accent);
    animation: caret-blink 1s steps(1) infinite;
  }
  @keyframes caret-blink { 50% { opacity: 0; } }
  .remote-pad { display: flex; flex-direction: column; gap: 0.6rem; }
  .dpad {
    display: grid;
    grid-template-columns: repeat(3, 3.2rem);
    grid-auto-rows: 3.2rem;
    gap: 0.4rem;
    justify-items: stretch;
  }
  .dpad button { font-size: 1rem; }
  .dpad .ok { font-weight: 700; }
  .remote-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.4rem;
    max-width: 10.4rem;
  }
  .remote-row button { padding: 0.45rem 0.3rem; white-space: nowrap; }
</style>
