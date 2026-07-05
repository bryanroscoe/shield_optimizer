<script lang="ts">
  import { onDestroy } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import BottomTabs from "../components/BottomTabs.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  const FORCE_SHELL_KEY = "atv.remote.forceShell";
  function loadForceShell(): boolean {
    try {
      return localStorage.getItem(FORCE_SHELL_KEY) === "1";
    } catch {
      return false;
    }
  }

  // Transport / latency come from the backend's real SendTextResult — no
  // fabricated "12 ms". null until the first press.
  let transport = $state<"channel" | "shell" | null>(null);
  let latency = $state<number | null>(null);
  let forceShell = $state(loadForceShell());
  let remoteMessage = $state("");

  let textToSend = $state("");
  let showKeyboardModal = $state(false);
  let sending = $state(false);

  // Presses are serialized through one promise chain so ordering is preserved.
  let queue: Promise<void> = Promise.resolve();
  function enqueue(work: () => Promise<void>) {
    queue = queue.then(work).catch((e) => {
      remoteMessage = String(e);
    });
  }

  function noteResult(t: "channel" | "shell" | "none", ms: number) {
    if (t === "channel" || t === "shell") {
      transport = t;
      latency = ms;
    }
  }

  function sendKey(key: string) {
    enqueue(async () => {
      const start = performance.now();
      const r = await api.sendKey(session.serial, key, forceShell);
      noteResult(r.transport, Math.round(performance.now() - start));
      remoteMessage = r.ok ? "" : r.message;
    });
  }

  // Hold-to-repeat for the D-pad — only armed on the fast channel, where a
  // hold won't pile the queue up far behind the finger.
  let repeatTimer: ReturnType<typeof setTimeout> | null = null;
  let repeatInterval: ReturnType<typeof setInterval> | null = null;
  function stopRepeat() {
    if (repeatTimer) { clearTimeout(repeatTimer); repeatTimer = null; }
    if (repeatInterval) { clearInterval(repeatInterval); repeatInterval = null; }
  }
  function pressStart(key: string) {
    sendKey(key);
    if (transport !== "channel") return;
    stopRepeat();
    repeatTimer = setTimeout(() => {
      repeatInterval = setInterval(() => sendKey(key), 140);
    }, 400);
  }

  function toggleForceShell() {
    forceShell = !forceShell;
    try {
      localStorage.setItem(FORCE_SHELL_KEY, forceShell ? "1" : "0");
    } catch {
      // non-fatal
    }
    transport = null;
    latency = null;
  }

  async function handleSendText() {
    if (!textToSend || sending) return;
    sending = true;
    try {
      const start = performance.now();
      const r = await api.sendText(session.serial, textToSend, forceShell);
      noteResult(r.transport, Math.round(performance.now() - start));
      if (r.ok) {
        textToSend = "";
        showKeyboardModal = false;
      } else {
        remoteMessage = r.message;
      }
    } catch (e) {
      remoteMessage = String(e);
    } finally {
      sending = false;
    }
  }

  function openSettings() {
    enqueue(async () => {
      const r = await api.openSettings(session.serial);
      remoteMessage = r.ok ? "" : r.message;
    });
  }

  onDestroy(stopRepeat);
</script>

<div class="screen">
  <div class="topline">
    <div class="header-info">
      <h3 class="header-title">Remote</h3>
      <span class="device-subtitle">{session.deviceLabel}</span>
    </div>
    <div class="header-actions">
      <FindRemoteButton />
      {#if transport}
        <span class="mono latency-badge" class:compat={transport === "shell"}>
          <span class="l-dot"></span>{transport === "channel" ? `${latency ?? "—"} ms` : "compat"}
        </span>
      {/if}
    </div>
  </div>

  <div class="remote-layout">
    <div class="nav-row">
      <button class="remote-btn" onclick={() => sendKey("back")}>
        <span class="msr">arrow_back</span>Back
      </button>
      <button class="remote-btn" onclick={() => sendKey("home")}>
        <span class="msr">home</span>Home
      </button>
      <button class="remote-btn" onclick={() => sendKey("recents")} aria-label="Recents">
        <span class="msr">menu</span>
      </button>
    </div>

    <div class="dpad-container">
      <div class="dpad-ring">
        <button class="dpad-dir up" onpointerdown={() => pressStart("up")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} aria-label="Up">
          <span class="msr">keyboard_arrow_up</span>
        </button>
        <button class="dpad-dir down" onpointerdown={() => pressStart("down")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} aria-label="Down">
          <span class="msr">keyboard_arrow_down</span>
        </button>
        <button class="dpad-dir left" onpointerdown={() => pressStart("left")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} aria-label="Left">
          <span class="msr">keyboard_arrow_left</span>
        </button>
        <button class="dpad-dir right" onpointerdown={() => pressStart("right")} onpointerup={stopRepeat} onpointerleave={stopRepeat} onpointercancel={stopRepeat} aria-label="Right">
          <span class="msr">keyboard_arrow_right</span>
        </button>
        <button class="dpad-center" onclick={() => sendKey("select")}>OK</button>
      </div>
    </div>
    <span class="helper-text">Tap direction to move · hold to repeat · center to select</span>

    <div class="media-row">
      <button class="media-btn" onclick={() => sendKey("rewind")} aria-label="Rewind">
        <span class="msr">fast_rewind</span>
      </button>
      <button class="media-btn play-btn" onclick={() => sendKey("play_pause")} aria-label="Play/Pause">
        <span class="msr fill">play_arrow</span>
      </button>
      <button class="media-btn" onclick={() => sendKey("fast_forward")} aria-label="Fast forward">
        <span class="msr">fast_forward</span>
      </button>
    </div>

    <div class="secondary-controls">
      <button class="control-btn" onclick={() => sendKey("volume_up")} aria-label="Volume Up">
        <span class="msr">volume_up</span>
      </button>
      <button class="control-btn" onclick={() => sendKey("mute")} aria-label="Mute">
        <span class="msr">volume_off</span>
      </button>
      <button class="control-btn" onclick={() => (showKeyboardModal = true)} aria-label="Keyboard Input">
        <span class="msr">keyboard</span>
      </button>
      <button class="control-btn" onclick={openSettings} aria-label="Settings">
        <span class="msr">settings</span>
      </button>
      <button class="control-btn danger" onclick={() => sendKey("power")} aria-label="Power">
        <span class="msr">power_settings_new</span>
      </button>
    </div>

    <label class="compat-toggle">
      <input type="checkbox" checked={forceShell} onchange={toggleForceShell} />
      Force compatible mode (slower, universal)
    </label>

    {#if remoteMessage}
      <p class="remote-message mono">{remoteMessage}</p>
    {/if}
  </div>

  {#if showKeyboardModal}
    <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
    <div class="modal-overlay" onclick={() => (showKeyboardModal = false)}>
      <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
      <div class="modal-card" onclick={(e) => e.stopPropagation()}>
        <!-- svelte-ignore a11y_click_events_have_key_events, a11y_no_static_element_interactions -->
        <span class="modal-close msr" onclick={() => (showKeyboardModal = false)}>close</span>
        <h3>Send Text to TV</h3>
        <p class="modal-lede">Type text to send to the focused field on the TV.</p>
        <input
          type="text"
          bind:value={textToSend}
          placeholder="Enter text..."
          class="modal-input"
          onkeydown={(e) => e.key === "Enter" && handleSendText()}
        />
        <div class="modal-actions">
          <button class="primary small" disabled={!textToSend || sending} onclick={handleSendText}>
            {sending ? "Sending..." : "Send Text"}
          </button>
          <button class="ghost small" onclick={() => (showKeyboardModal = false)}>Cancel</button>
        </div>
      </div>
    </div>
  {/if}

  <div class="spacer"></div>
  <BottomTabs active="remote" {navigate} />
</div>

<style>
  .header-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
  }
  .header-title {
    margin: 0;
    font-size: 22px;
    font-weight: 700;
    letter-spacing: -0.01em;
  }
  .device-subtitle {
    font-size: 12px;
    color: var(--muted);
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .latency-badge {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    font-family: var(--mono);
    font-size: 12px;
    font-weight: 600;
    color: var(--teal);
    background: color-mix(in srgb, var(--teal) 10%, transparent);
    border: 1px solid color-mix(in srgb, var(--teal) 22%, transparent);
    padding: 6px 11px;
    border-radius: 999px;
  }
  .latency-badge.compat {
    color: var(--muted);
    background: color-mix(in srgb, var(--text) 6%, transparent);
    border-color: var(--line);
  }
  .l-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: currentColor;
    box-shadow: 0 0 8px currentColor;
  }

  .remote-layout {
    display: flex;
    flex-direction: column;
    align-items: center;
    width: 100%;
    margin-top: 10px;
  }

  .nav-row {
    display: flex;
    gap: 10px;
    width: 100%;
    margin-bottom: 18px;
  }
  .remote-btn {
    flex: 1;
    height: 48px;
    border: 1px solid var(--line);
    border-radius: 14px;
    background: var(--surface);
    color: var(--text-soft);
    display: flex;
    align-items: center;
    justify-content: center;
    gap: 7px;
    font-family: var(--sans);
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
  }
  .remote-btn:active {
    background: var(--surface-2);
  }
  .remote-btn .msr {
    font-size: 20px;
  }

  .dpad-container {
    display: grid;
    place-items: center;
    padding: 12px 0 6px;
  }
  .dpad-ring {
    position: relative;
    width: 262px;
    height: 262px;
    border-radius: 50%;
    background: radial-gradient(circle at 50% 40%, #1b1f26, #121418);
    border: 1px solid var(--line);
    box-shadow: inset 0 2px 30px rgba(0, 0, 0, 0.5);
    display: grid;
    place-items: center;
  }
  .dpad-dir {
    position: absolute;
    background: transparent;
    border: none;
    cursor: pointer;
    padding: 10px;
    color: var(--muted);
    display: grid;
    place-items: center;
    touch-action: none;
  }
  .dpad-dir:active {
    color: var(--accent);
  }
  .dpad-dir .msr {
    font-size: 30px;
  }
  .dpad-dir.up { top: 12px; left: 50%; transform: translateX(-50%); }
  .dpad-dir.down { bottom: 12px; left: 50%; transform: translateX(-50%); }
  .dpad-dir.left { left: 12px; top: 50%; transform: translateY(-50%); }
  .dpad-dir.right { right: 12px; top: 50%; transform: translateY(-50%); }

  .dpad-center {
    width: 104px;
    height: 104px;
    border-radius: 50%;
    background: linear-gradient(160deg, #20252d, #171a1f);
    border: 1px solid color-mix(in srgb, var(--accent) 28%, transparent);
    color: var(--accent);
    font-family: var(--sans);
    font-size: 16px;
    font-weight: 700;
    letter-spacing: 0.06em;
    cursor: pointer;
    box-shadow: 0 0 30px color-mix(in srgb, var(--accent) 10%, transparent);
  }
  .dpad-center:active {
    background: #20252d;
  }

  .helper-text {
    text-align: center;
    font-size: 11px;
    color: var(--dim);
    margin-top: 6px;
  }

  .media-row {
    display: flex;
    gap: 10px;
    margin-top: 18px;
    width: 100%;
  }
  .media-btn {
    flex: 1;
    height: 54px;
    border: 1px solid var(--line);
    border-radius: 15px;
    background: var(--surface);
    color: var(--text-soft);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .media-btn:active {
    background: var(--surface-2);
  }
  .media-btn.play-btn {
    flex: 1.3;
    color: var(--text);
  }
  .media-btn .msr {
    font-size: 26px;
  }
  .media-btn.play-btn .msr {
    font-size: 28px;
  }

  .secondary-controls {
    display: flex;
    gap: 10px;
    margin-top: 10px;
    width: 100%;
  }
  .control-btn {
    flex: 1;
    height: 50px;
    border: 1px solid var(--line);
    border-radius: 15px;
    background: var(--surface);
    color: var(--text-soft);
    display: grid;
    place-items: center;
    cursor: pointer;
  }
  .control-btn:active {
    background: var(--surface-2);
  }
  .control-btn.danger {
    border-color: rgba(251, 107, 95, 0.25);
    background: rgba(251, 107, 95, 0.08);
    color: var(--danger);
  }
  .control-btn .msr {
    font-size: 22px;
  }

  .compat-toggle {
    display: flex;
    align-items: center;
    gap: 8px;
    margin-top: 16px;
    font-size: 12px;
    color: var(--muted);
    cursor: pointer;
  }
  .compat-toggle input {
    accent-color: var(--accent);
    width: 16px;
    height: 16px;
  }
  .remote-message {
    margin-top: 12px;
    font-size: 12px;
    color: var(--danger);
    text-align: center;
  }

  .modal-overlay {
    position: fixed;
    inset: 0;
    background: rgba(0, 0, 0, 0.7);
    display: grid;
    place-items: center;
    padding: 24px;
    z-index: 200;
  }
  .modal-card {
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 20px;
    width: 100%;
    max-width: 320px;
    padding: 24px;
    position: relative;
    box-shadow: 0 15px 40px rgba(0, 0, 0, 0.6);
    box-sizing: border-box;
  }
  .modal-close {
    position: absolute;
    top: 16px;
    right: 16px;
    font-size: 20px;
    color: var(--muted);
    cursor: pointer;
  }
  .modal-card h3 {
    margin: 0 0 8px;
    font-size: 18px;
    font-weight: 700;
  }
  .modal-lede {
    margin: 0 0 16px;
    font-size: 13px;
    color: var(--muted);
    line-height: 1.4;
  }
  .modal-input {
    width: 100%;
    min-height: 48px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 0 12px;
    color: var(--text);
    box-sizing: border-box;
    margin-bottom: 20px;
    font-family: var(--sans);
  }
  .modal-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .modal-actions {
    display: flex;
    gap: 10px;
  }
</style>
