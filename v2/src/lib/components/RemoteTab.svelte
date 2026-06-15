<script lang="ts">
  import { onDestroy } from "svelte";
  import { api } from "$lib/api";
  import { getRemoteForceShell, setRemoteForceShell } from "$lib/prefs";

  let { serial }: { serial: string } = $props();

  // Escape hatch: force the slow but universal `input` transport, skipping the
  // fast scrcpy channel. Persisted across sessions.
  let forceShell = $state(getRemoteForceShell());

  function toggleForceShell() {
    forceShell = !forceShell;
    setRemoteForceShell(forceShell);
    // Reset the cue so it reflects the next send's actual transport.
    transport = null;
  }

  /// Rolling echo of what live typing has sent (display only).
  let remoteEcho = $state("");
  let remoteMessage = $state("");
  let remoteCaptureFocused = $state(false);
  /// Transport the backend last reported: "channel" = instant scrcpy socket,
  /// "shell" = slow `input` fallback (~700 ms/press), null until the first
  /// send. Drives the status cue, typing batch window, and hold-to-repeat.
  let transport = $state<"channel" | "shell" | null>(null);
  // Keystrokes are sent strictly in order through one promise chain — a
  // backspace must never overtake the characters typed before it.
  let remoteQueue: Promise<void> = Promise.resolve();
  let remoteBuffer = "";
  let remoteFlushTimer: ReturnType<typeof setTimeout> | null = null;

  function noteTransport(t: string) {
    if (t === "channel" || t === "shell") transport = t;
  }

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
      const r = await api.sendText(serial, chunk, forceShell);
      noteTransport(r.transport);
      if (!r.ok) remoteMessage = r.message;
    });
  }

  function sendRemoteKey(key: string) {
    remoteFlushBuffer();
    remoteEnqueue(async () => {
      const r = await api.sendKey(serial, key, forceShell);
      noteTransport(r.transport);
      remoteMessage = r.ok ? "" : r.message;
    });
  }

  // Hold-to-repeat for the D-pad: first repeat after 400 ms, then ~7/s. Only
  // armed on the instant channel — on the slow shell transport the queue
  // would pile up far behind the finger, so a hold is just one press there.
  let repeatTimer: ReturnType<typeof setTimeout> | null = null;
  let repeatInterval: ReturnType<typeof setInterval> | null = null;

  function stopRepeat() {
    if (repeatTimer) { clearTimeout(repeatTimer); repeatTimer = null; }
    if (repeatInterval) { clearInterval(repeatInterval); repeatInterval = null; }
  }

  function pressStart(key: string) {
    sendRemoteKey(key);
    if (transport !== "channel") return;
    stopRepeat();
    repeatTimer = setTimeout(() => {
      repeatInterval = setInterval(() => sendRemoteKey(key), 140);
    }, 400);
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
    if (e.key.length === 1 && !e.isComposing) {
      // The channel injects full UTF-8; the shell fallback re-checks and
      // rejects non-ASCII with a clear message, so don't pre-filter here.
      e.preventDefault();
      remoteBuffer += e.key;
      remoteEcho = (remoteEcho + e.key).slice(-60);
      // Channel sends are ~ms, so flush almost immediately — characters land
      // on the TV as you type. The shell fallback pays ~700 ms per call, so
      // batch rapid typing into one `input text` per pause instead.
      if (remoteFlushTimer) clearTimeout(remoteFlushTimer);
      remoteFlushTimer = setTimeout(remoteFlushBuffer, transport === "shell" ? 250 : 30);
    }
  }

  onDestroy(() => {
    if (remoteFlushTimer) clearTimeout(remoteFlushTimer);
    stopRepeat();
  });
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-remote" aria-labelledby="tab-remote">
  <div class="remote-header">
    <h2>Remote</h2>
    {#if transport}
      <span class="transport" class:live={transport === "channel"}
        title={transport === "channel"
          ? "Low-latency control channel — presses reach the TV in milliseconds, and holding a D-pad button repeats."
          : "Compatibility mode — each press is a slower ADB call (~0.7s)."}>
        {transport === "channel" ? "● instant" : "○ compatible (slower)"}
      </span>
    {/if}
    <label class="compat-toggle" title="Skip the fast channel and use the slower, universal ADB input — use this if the instant channel misbehaves on your device.">
      <input type="checkbox" checked={forceShell} onchange={toggleForceShell} />
      Force compatible mode
    </label>
  </div>
  <div class="remote-layout">
    <div class="remote-typing">
      <h3>Live typing</h3>
      <p class="muted small">
        Click below and type — keystrokes go straight to whatever field has
        focus on the TV, including Backspace and Enter.
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
      <!-- D-pad uses pointerdown/up (not click) so holding a direction
           auto-repeats on the fast channel; pointerleave/cancel stop the
           repeat if the cursor slides off mid-hold. -->
      <div class="dpad">
        <span></span>
        <button onpointerdown={() => pressStart("up")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad up (hold to repeat)">▲</button>
        <span></span>
        <button onpointerdown={() => pressStart("left")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad left (hold to repeat)">◀</button>
        <button class="ok" onclick={() => sendRemoteKey("select")} title="Select / OK">OK</button>
        <button onpointerdown={() => pressStart("right")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad right (hold to repeat)">▶</button>
        <span></span>
        <button onpointerdown={() => pressStart("down")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad down (hold to repeat)">▼</button>
        <span></span>
      </div>
      <div class="remote-row">
        <button onclick={() => sendRemoteKey("back")} title="Back">Back</button>
        <button onclick={() => sendRemoteKey("home")} title="Home">Home</button>
        <button onclick={() => sendRemoteKey("menu")} title="Menu (KEYCODE_MENU)">Menu</button>
      </div>
      <div class="remote-row">
        <button onclick={() => sendRemoteKey("recents")} title="Recent apps / app switcher">Recents</button>
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
  .remote-header {
    display: flex;
    align-items: baseline;
    gap: 0.8rem;
    flex-wrap: wrap;
  }
  .transport {
    font-size: 0.74rem;
    color: var(--fg-muted);
    cursor: default;
  }
  .compat-toggle {
    margin-left: auto;
    display: flex;
    align-items: center;
    gap: 0.35rem;
    font-size: 0.78rem;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .compat-toggle input {
    accent-color: var(--accent);
    cursor: pointer;
  }
  .transport.live {
    color: var(--ok);
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
