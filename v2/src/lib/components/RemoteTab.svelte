<script lang="ts">
  import { onDestroy } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
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

  /// Send pasted text as whole lines rather than synthesised keystrokes.
  ///
  /// One `input text` per line is a single round trip instead of one per
  /// character, which matters most for exactly what this was asked for: long
  /// URLs, usernames and passwords that are miserable to type on a remote
  /// (GitHub #91).
  ///
  /// Newlines become explicit Enter presses, in order, because `input text`
  /// cannot carry them — sending them literally would silently drop them and
  /// paste something different from what was on the clipboard.
  function sendPastedText(raw: string) {
    const text = raw.replace(/\r\n?/g, "\n");
    if (!text) return;
    remoteFlushBuffer();
    const lines = text.split("\n");
    lines.forEach((line, i) => {
      if (line) {
        remoteEnqueue(async () => {
          const r = await api.sendText(serial, line, forceShell);
          noteTransport(r.transport);
          if (!r.ok) remoteMessage = r.message;
        });
      }
      if (i < lines.length - 1) sendRemoteKey("enter");
    });
    // Echo shows a newline as a return glyph so a multi-line paste is legible.
    remoteEcho = (remoteEcho + text.replace(/\n/g, "\u23ce")).slice(-60);
  }

  /// The capture is a non-editable div, but a focused one still receives paste
  /// with `clipboardData` populated — verified in both Chromium and WebKit,
  /// the latter being what Tauri uses on macOS. No clipboard permission and no
  /// plugin needed for this path.
  function remotePaste(event: ClipboardEvent) {
    event.preventDefault();
    const text = event.clipboardData?.getData("text") ?? "";
    if (!text) return;
    remoteMessage = "";
    sendPastedText(text);
  }

  /// The button exists because a dashed box that silently accepts Cmd+V is not
  /// something anyone would guess at. Reading the clipboard directly can be
  /// refused, so say what to do instead rather than failing silently.
  async function pasteFromClipboard() {
    remoteMessage = "";
    try {
      const text = await navigator.clipboard.readText();
      if (!text) {
        remoteMessage = "Clipboard is empty.";
        return;
      }
      sendPastedText(text);
    } catch {
      remoteMessage =
        "Couldn't read the clipboard. Click the typing box, then press \u2318V / Ctrl+V.";
    }
  }

  // Settings opens via an intent (am start), not a keycode — the Shield's gear
  // button is an intent launch, and KEYCODE_SETTINGS/MENU no-op when injected.
  function openSettings() {
    remoteFlushBuffer();
    remoteEnqueue(async () => {
      const r = await api.openSettings(serial);
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
    <h2><Icon name="settings_remote" size={17} /> Remote</h2>
    {#if transport}
      <span class="transport" class:live={transport === "channel"}
        title={transport === "channel"
          ? "Low-latency control channel — presses reach the TV in milliseconds, and holding a D-pad button repeats."
          : "Compatibility mode — each press is a slower ADB call (~0.7s)."}>
        {transport === "channel" ? "● instant" : "○ compatible (slower)"}
      </span>
    {/if}
  </div>
  <div class="remote-layout">
    <div class="remote-typing">
      <div class="typing-header">
        <h3>Live typing</h3>
        <button class="small-action primary" onclick={pasteFromClipboard} title="Send the clipboard to the TV">
          <Icon name="content_paste" size={14} /> Paste
        </button>
      </div>
      <p class="muted small">
        Click below and type — keystrokes go straight to whatever field has
        focus on the TV, including Backspace and Enter. You can paste too
        (⌘V / Ctrl+V), which is easier for a long URL or password.
      </p>
      <div
        class="type-capture"
        class:focused={remoteCaptureFocused}
        tabindex="0"
        role="textbox"
        aria-label="Live typing capture — keystrokes and pasted text are sent to the TV"
        onkeydown={remoteKeydown}
        onpaste={remotePaste}
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
      <!-- Row order follows mobile board 6.1: nav trio, then the disc as the
           anchor, then transport, volume, and the system trio last. -->
      <div class="remote-row nav-row">
        <button onclick={() => sendRemoteKey("back")} title="Back"><Icon name="arrow_back" size={15} /> Back</button>
        <button onclick={() => sendRemoteKey("home")} title="Home"><Icon name="home" size={15} /> Home</button>
        <button onclick={() => sendRemoteKey("recents")} title="Recent apps / app switcher"><Icon name="apps" size={15} /> Recents</button>
      </div>
      <!-- D-pad uses pointerdown/up (not click) so holding a direction
           auto-repeats on the fast channel; pointerleave/cancel stop the
           repeat if the cursor slides off mid-hold. -->
      <div class="dpad">
        <span></span>
        <button onpointerdown={() => pressStart("up")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad up (hold to repeat)" aria-label="D-pad up"><Icon name="keyboard_arrow_up" size={22} /></button>
        <span></span>
        <button onpointerdown={() => pressStart("left")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad left (hold to repeat)" aria-label="D-pad left"><Icon name="keyboard_arrow_left" size={22} /></button>
        <button class="ok" onclick={() => sendRemoteKey("select")} title="Select / OK">OK</button>
        <button onpointerdown={() => pressStart("right")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad right (hold to repeat)" aria-label="D-pad right"><Icon name="keyboard_arrow_right" size={22} /></button>
        <span></span>
        <button onpointerdown={() => pressStart("down")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} title="D-pad down (hold to repeat)" aria-label="D-pad down"><Icon name="keyboard_arrow_down" size={22} /></button>
        <span></span>
      </div>
      <!-- What the status pill's tooltip used to hide. It changes with the
           transport, and it is the one thing you need to know before you hold
           a direction down. -->
      {#if transport}
        <p class="pad-caption">
          {transport === "channel"
            ? "Hold a direction to repeat"
            : "Each press takes about 0.7s"}
        </p>
      {/if}
      <div class="remote-keys">
      <div class="remote-row transport-row">
        <button onclick={() => sendRemoteKey("rewind")} title="Rewind" data-tip="Rewind" aria-label="Rewind"><Icon name="fast_rewind" size={18} /></button>
        <button onclick={() => sendRemoteKey("play_pause")} title="Play / Pause" data-tip="Play / Pause" aria-label="Play or pause"><Icon name="play_pause" size={20} /></button>
        <button onclick={() => sendRemoteKey("fast_forward")} title="Fast forward" data-tip="Fast forward" aria-label="Fast forward"><Icon name="fast_forward" size={18} /></button>
      </div>
      <div class="remote-row">
        <button onclick={() => sendRemoteKey("volume_down")} title="Volume down" data-tip="Volume down" aria-label="Volume down"><Icon name="volume_down" size={18} /></button>
        <button onclick={() => sendRemoteKey("mute")} title="Mute" data-tip="Mute" aria-label="Mute"><Icon name="volume_off" size={18} /></button>
        <button onclick={() => sendRemoteKey("volume_up")} title="Volume up" data-tip="Volume up" aria-label="Volume up"><Icon name="volume_up" size={18} /></button>
      </div>
      <div class="remote-row">
        <button onclick={openSettings} title="Open Settings (the Shield remote's gear button)"><Icon name="settings" size={15} /> Settings</button>
        <button onclick={() => sendRemoteKey("wakeup")} title="Wake the screen (KEYCODE_WAKEUP)">Wake</button>
        <button class="power" onclick={() => sendRemoteKey("power")} title="Power toggle (sleep / wake)"><Icon name="power_settings_new" size={15} /> Power</button>
      </div>
      </div>
      <!-- A setting about how this remote talks to the TV, so it sits with the
           remote and with the caption it changes — not in the card header,
           where it looked like a page-level control. -->
      <label class="compat-toggle" title="Skip the fast channel and use the slower, universal ADB input — use this if the instant channel misbehaves on your device.">
        <input type="checkbox" checked={forceShell} onchange={toggleForceShell} />
        Force compatible mode
      </label>
    </div>
  </div>
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button) live in the layout and are inherited. */
  .remote-header {
    display: flex;
    align-items: baseline;
    gap: 0.8rem;
    flex-wrap: wrap;
  }
  .transport {
    margin-left: auto;
    font-size: 0.74rem;
    color: var(--fg-muted);
    cursor: default;
  }
  .compat-toggle {
    display: flex;
    align-items: center;
    gap: 0.35rem;
    margin-top: 0.2rem;
    font-size: 0.78rem;
    color: var(--fg-muted);
    cursor: pointer;
  }
  .compat-toggle input {
    accent-color: var(--accent-strong);
    cursor: pointer;
  }
  .transport.live {
    color: var(--ok);
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
  .warn-text {
    color: var(--warn);
  }

  /* Remote-specific styles. */
  /* Bounded and centred rather than stretched. Nothing on this screen scales
     with the viewport — a capture box that echoes about sixty characters has
     no business being 1200px wide — so letting the row fill a wide card left
     the remote in a narrow rail beside several hundred pixels of nothing. */
  .remote-layout {
    display: flex;
    gap: 3rem;
    flex-wrap: wrap;
    align-items: flex-start;
    justify-content: center;
    max-width: 900px;
    margin-inline: auto;
  }
  /* The pad reads first and the typing box second: the remote is the thing
     this screen is, and the left edge is where a reader starts. DOM order
     stays typing-then-pad so Tab still reaches the capture box before fifteen
     buttons and the paste path is untouched. */
  .remote-typing {
    order: 2;
  }
  .remote-pad {
    order: 1;
  }
  @media (max-width: 760px) {
    /* Stacked, the order that matters is reading order again. */
    .remote-typing,
    .remote-pad {
      order: 0;
    }
  }
  .remote-typing {
    /* ~60ch of mono: the echo's own length. */
    flex: 0 1 480px;
    max-width: 480px;
    min-width: 280px;
  }
  .typing-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.5rem;
  }
  .typing-header h3 {
    margin: 0;
  }
  /* Same shape as the small actions in the Files and Install APK tabs; Svelte
     scopes styles per component, so it is redeclared rather than inherited. */
  .small-action {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
  }
  .type-capture {
    /* Five lines, so a pasted URL or a multi-line paste has somewhere to go
       and the column reads as a surface rather than a single input. */
    min-height: 9rem;
    overflow-wrap: anywhere;
    padding: 0.8rem;
    border: 1px dashed var(--border);
    border-radius: var(--radius-md);
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
  /* The width mobile board 6.1 gives the remote. Fixed, because a remote is a
     physical object — it should not get wider just because the window did. */
  .remote-pad {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 0.9rem;
    flex: 0 0 340px;
    max-width: 340px;
  }
  .pad-caption {
    margin: -0.2rem 0 0;
    font-size: 0.76rem;
    color: var(--fg-muted);
    text-align: center;
  }
  /* A D-pad should look like one control, not four loose rectangles. The
     ring is a single disc; the four directions are transparent wedges laid
     over it in a 3x3 grid, with OK as a raised centre. */
  .dpad {
    position: relative;
    display: grid;
    grid-template-columns: repeat(3, 5rem);
    grid-auto-rows: 5rem;
    justify-items: stretch;
    border-radius: 50%;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.35rem;
    width: max-content;
  }
  .dpad button {
    background: none;
    border: none;
    border-radius: 50%;
    color: var(--fg-secondary);
    padding: 0;
  }
  .dpad button:hover {
    background: var(--bg-button-hover);
    color: var(--fg-primary);
  }
  .dpad button:active {
    background: var(--accent-surface);
    color: var(--accent);
  }
  .dpad .ok {
    background: var(--bg-button);
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
    color: var(--accent);
    font-weight: 700;
    font-size: 1.05rem;
  }
  .dpad .ok:hover {
    background: var(--accent-strong);
    border-color: var(--accent);
    color: var(--accent-ink);
  }
  /* One grid for every key row, so the columns line up down the stack
     instead of each row sizing itself. */
  .remote-keys {
    display: grid;
    gap: 0.4rem;
    width: 100%;
  }
  .remote-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 0.4rem;
    width: 100%;
  }
  /* Play is the one you reach for, so it gets the extra width — as on 6.1. */
  .remote-row.transport-row {
    grid-template-columns: 1fr 1.3fr 1fr;
  }
  .remote-row.nav-row {
    margin-bottom: 0.2rem;
  }
  /* Destructive, so it carries the danger edge rather than a plain one. */
  .remote-row button.power {
    border-color: color-mix(in srgb, var(--danger) 30%, transparent);
    color: var(--danger);
  }
  .remote-row button.power:hover {
    background: color-mix(in srgb, var(--danger) 14%, transparent);
  }
  .dpad button,
  .remote-row button {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 0.35rem;
  }
  .remote-row button {
    padding: 0.6rem 0.3rem;
    white-space: nowrap;
    border-radius: var(--radius-lg);
  }
</style>
