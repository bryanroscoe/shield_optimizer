<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type {
    CurrentDisplayScaling,
    DisplayScalePreset,
    PrivateDnsState,
    TweaksState,
  } from "../lib/types";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";

  let { navigate }: { navigate: (screen: Screen) => void } = $props();

  let loading = $state(true);
  let error = $state("");
  let tweaks = $state<TweaksState | null>(null);
  let dns = $state<PrivateDnsState | null>(null);
  let scaling = $state<CurrentDisplayScaling | null>(null);
  let busy = $state("");
  let showPaywall = $state(false);
  let dnsHost = $state("");
  let dnsEditing = $state(false);
  let loadGeneration = 0;

  let toast = $state("");
  let toastType = $state<"success" | "error" | "info">("info");
  let toastTimer: ReturnType<typeof setTimeout> | undefined;
  function showToast(msg: string, type: "success" | "error" | "info" = "info") {
    toast = msg;
    toastType = type;
    clearTimeout(toastTimer);
    toastTimer = setTimeout(() => (toast = ""), 3800);
  }

  function isLocked(e: unknown): boolean {
    return String(e).includes("LOCKED:");
  }

  async function load() {
    const serial = session.serial;
    const generation = ++loadGeneration;
    if (!serial) {
      error = "No TV connected.";
      loading = false;
      return;
    }
    loading = tweaks === null;
    error = "";
    try {
      const [t, d, s] = await Promise.all([
        api.getTweaks(serial),
        api.getPrivateDns(serial),
        api.getDisplayScaling(serial),
      ]);
      if (generation !== loadGeneration || serial !== session.serial) return;
      tweaks = t;
      dns = d;
      scaling = s;
      if (d.hostname) dnsHost = d.hostname;
    } catch (e) {
      if (generation !== loadGeneration || serial !== session.serial) return;
      error = String(e);
    } finally {
      if (generation === loadGeneration && serial === session.serial) loading = false;
    }
  }

  onMount(load);

  // Wrap a Pro write: route LOCKED to the paywall, reload on success/revert,
  // never fabricate the new value (we re-read the device instead).
  async function apply(
    key: string,
    fn: (targetSerial: string) => Promise<{ ok: boolean; message: string; reverted?: boolean }>,
    successMsg: string,
  ) {
    if (busy) return;
    const targetSerial = session.serial;
    if (!targetSerial) return;
    busy = key;
    try {
      const r = await fn(targetSerial);
      if (r.ok) {
        showToast(successMsg, "success");
        await load();
      } else {
        showToast(r.message || "Change failed.", "error");
        // A multi-write operation may have changed earlier values before a
        // later write failed. Re-read instead of leaving optimistic/stale UI.
        await load();
      }
    } catch (e) {
      if (isLocked(e)) showPaywall = true;
      else {
        showToast(String(e), "error");
        await load();
      }
    } finally {
      busy = "";
    }
  }

  // ---- Derived current states (null = unknown; we render honestly) ----
  const hdmiOn = $derived(tweaks?.hdmi_control_enabled === "1");
  const frameRateOn = $derived(
    tweaks?.match_content_frame_rate != null && tweaks.match_content_frame_rate !== "0",
  );
  const animScale = $derived(
    tweaks?.window_animation_scale != null ? parseFloat(tweaks.window_animation_scale) : null,
  );
  const longPress = $derived(
    tweaks?.long_press_timeout != null ? parseInt(tweaks.long_press_timeout, 10) : null,
  );
  const bgLimit = $derived(tweaks?.background_process_limit ?? null);
  const dnsMode = $derived(dns?.mode ?? null);

  const scaleSize = $derived(
    scaling?.size ? scaling.size.replace(/Physical size:\s*/i, "").split("\n")[0].trim() : "—",
  );

  const animPresets: { label: string; value: string }[] = [
    { label: "Off", value: "0" },
    { label: "1×", value: "1" },
    { label: "0.5×", value: "0.5" },
  ];
  const longPressPresets = [300, 400, 500, 750];
  const bgPresets: { label: string; value: string }[] = [
    { label: "Standard", value: "" },
    { label: "≤4", value: "4" },
    { label: "≤2", value: "2" },
    { label: "None", value: "0" },
  ];
  const scalePresets: { label: string; value: DisplayScalePreset }[] = [
    { label: "4K", value: "uhd_4k" },
    { label: "1080p", value: "fhd_1080p" },
    { label: "Reset", value: "reset" },
  ];

  function toggleHdmi() {
    apply("hdmi", (target) => api.writeSetting(target, "global", "hdmi_control_enabled", hdmiOn ? "0" : "1"),
      hdmiOn ? "HDMI-CEC turned off." : "HDMI-CEC turned on.");
  }
  function toggleFrameRate() {
    apply("framerate", (target) => api.writeSetting(target, "secure", "match_content_frame_rate", frameRateOn ? "0" : "2"),
      frameRateOn ? "Frame-rate matching off." : "Frame-rate matching on.");
  }
  function setAnim(value: string) {
    // Animation speed is three scales in lockstep — write all so the UI is
    // consistent (window / transition / animator).
    apply(`anim-${value}`, async (target) => {
      let last = { ok: true, message: "ok" };
      for (const k of ["window_animation_scale", "transition_animation_scale", "animator_duration_scale"]) {
        last = await api.writeSetting(target, "global", k, value);
        if (!last.ok) break;
      }
      return last;
    }, "Animation speed updated.");
  }
  function setLongPress(ms: number) {
    apply(`lp-${ms}`, (target) => api.writeSetting(target, "secure", "long_press_timeout", String(ms)),
      `Long-press timeout set to ${ms}ms.`);
  }
  function setBgLimit(value: string) {
    apply(`bg-${value}`, (target) => api.writeSetting(target, "global", "background_process_limit", value),
      "Background limit updated. Note: Android resets this on reboot.");
  }
  function setScaling(preset: DisplayScalePreset) {
    apply(`scale-${preset}`, (target) => api.setDisplayScaling(target, preset),
      "Display scaling applied.");
  }
  function setDns(mode: string) {
    if (mode === "hostname") {
      dnsEditing = true;
      return;
    }
    dnsEditing = false;
    apply(`dns-${mode}`, (target) => api.setPrivateDns(target, mode),
      mode === "off" ? "Private DNS off." : "Private DNS set to automatic.");
  }
  function applyDnsHost() {
    const h = dnsHost.trim();
    if (!h) return;
    apply("dns-host", (target) => api.setPrivateDns(target, "hostname", h),
      "Private DNS hostname applied.");
  }

  function animActive(value: string): boolean {
    if (animScale == null) return false;
    return Math.abs(animScale - parseFloat(value)) < 0.001;
  }
</script>

<div class="screen">
  <div class="topline">
    <div class="header-left">
      <button class="iconbtn" onclick={() => navigate("more")} aria-label="Back">
        <span class="msr">arrow_back</span>
      </button>
      <FindRemoteButton />
    </div>
    <h3 class="header-title">Tweaks</h3>
    <span style="width:44px"></span>
  </div>

  {#if !session.isPro}
    <div class="pro-banner">
      <span class="msr">lock</span>
      <span class="pro-banner-text">Reading settings is free. Changing them is a Pro feature.</span>
      <button class="pro-banner-btn" onclick={() => (showPaywall = true)}>Unlock</button>
    </div>
  {/if}

  {#if loading}
    <div class="center">
      <span class="statuspill live"><span class="pdot blink"></span>Reading settings…</span>
    </div>
  {:else if error}
    <p class="error">{error}</p>
    <button class="primary" onclick={load}>Retry</button>
    <div class="spacer"></div>
  {:else}
    <div class="tweaks-content">
      <!-- Display & sound -->
      <span class="section-label nomargin">Display &amp; sound</span>
      <div class="tweak-group">
        <div class="tweak-row">
          <span class="msr t-icon">settings_input_hdmi</span>
          <div class="t-info">
            <span class="t-title">HDMI-CEC control</span>
            <span class="t-desc">One remote for TV + soundbar</span>
          </div>
          <button class="switch" class:on={hdmiOn} class:busy={busy === "hdmi"} disabled={busy !== ""} onclick={toggleHdmi} aria-label="Toggle HDMI-CEC">
            <span class="knob"></span>
          </button>
        </div>
        <div class="tweak-row">
          <span class="msr t-icon">30fps_select</span>
          <div class="t-info">
            <span class="t-title">Match content frame rate</span>
            <span class="t-desc">Auto-switch 24/50/60 Hz</span>
          </div>
          <button class="switch" class:on={frameRateOn} disabled={busy !== ""} onclick={toggleFrameRate} aria-label="Toggle frame-rate matching">
            <span class="knob"></span>
          </button>
        </div>
        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon">aspect_ratio</span>
            <div class="t-info">
              <span class="t-title">Display scaling</span>
              <span class="t-desc mono">{scaleSize}</span>
            </div>
          </div>
          <div class="segmented">
            {#each scalePresets as p (p.value)}
              <button class="seg" class:busy={busy === `scale-${p.value}`} disabled={busy !== ""} onclick={() => setScaling(p.value)}>{p.label}</button>
            {/each}
          </div>
        </div>
      </div>

      <!-- Speed & input -->
      <span class="section-label">Speed &amp; input</span>
      <div class="tweak-group">
        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon">animation</span>
            <div class="t-info">
              <span class="t-title">Animation speed</span>
              <span class="t-desc">Snappier UI transitions</span>
            </div>
          </div>
          <div class="segmented">
            {#each animPresets as p (p.value)}
              <button class="seg" class:active={animActive(p.value)} disabled={busy !== ""} onclick={() => setAnim(p.value)}>{p.label}</button>
            {/each}
          </div>
        </div>
        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon">touch_app</span>
            <div class="t-info">
              <span class="t-title">Long-press timeout</span>
              <span class="t-desc">{longPress != null ? `${longPress}ms` : "Remote hold delay"}</span>
            </div>
          </div>
          <div class="segmented">
            {#each longPressPresets as ms (ms)}
              <button class="seg" class:active={longPress === ms} disabled={busy !== ""} onclick={() => setLongPress(ms)}>{ms}</button>
            {/each}
          </div>
        </div>
        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon">memory</span>
            <div class="t-info">
              <span class="t-title">Background process limit</span>
              <span class="t-desc">Frees RAM · resets on reboot</span>
            </div>
          </div>
          <div class="segmented">
            {#each bgPresets as p (p.label)}
              <button class="seg" class:active={(p.value === "" && bgLimit == null) || bgLimit === p.value} disabled={busy !== ""} onclick={() => setBgLimit(p.value)}>{p.label}</button>
            {/each}
          </div>
        </div>
      </div>

      <!-- Network -->
      <span class="section-label">Network</span>
      <div class="tweak-group">
        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon">dns</span>
            <div class="t-info">
              <span class="t-title">Private DNS</span>
              <span class="t-desc">DNS-over-TLS{dnsMode === "hostname" && dns?.hostname ? ` · ${dns.hostname}` : ""}</span>
            </div>
          </div>
          <div class="segmented">
            <button class="seg" class:active={dnsMode === "off"} disabled={busy !== ""} onclick={() => setDns("off")}>Off</button>
            <button class="seg" class:active={dnsMode === "opportunistic"} disabled={busy !== ""} onclick={() => setDns("opportunistic")}>Auto</button>
            <button class="seg" class:active={dnsMode === "hostname" || dnsEditing} disabled={busy !== ""} onclick={() => setDns("hostname")}>Custom</button>
          </div>
          {#if dnsEditing}
            <div class="dns-input-row">
              <input class="dns-input mono" bind:value={dnsHost} placeholder="dns.adguard.com" onkeydown={(e) => e.key === "Enter" && applyDnsHost()} />
              <button class="primary small-inline" disabled={!dnsHost.trim() || busy !== ""} onclick={applyDnsHost}>Apply</button>
            </div>
          {/if}
        </div>
      </div>
    </div>
    <div class="spacer"></div>
  {/if}

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

  .pro-banner {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 11px 13px;
    border-radius: 13px;
    background: color-mix(in srgb, var(--accent) 8%, transparent);
    border: 1px solid color-mix(in srgb, var(--accent) 22%, transparent);
    margin-bottom: 14px;
  }
  .pro-banner .msr {
    font-size: 18px;
    color: var(--accent);
    flex: none;
  }
  .pro-banner-text {
    flex: 1;
    font-size: 12px;
    color: var(--text-soft);
    line-height: 1.4;
  }
  .pro-banner-btn {
    flex: none;
    background: var(--accent);
    color: var(--accent-ink);
    border: none;
    border-radius: 9px;
    padding: 7px 12px;
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 700;
    cursor: pointer;
  }

  .tweaks-content {
    display: flex;
    flex-direction: column;
    gap: 6px;
  }
  .section-label {
    margin: 16px 0 8px;
  }
  .section-label.nomargin {
    margin: 0 0 8px;
  }
  .tweak-group {
    display: flex;
    flex-direction: column;
    background: var(--surface);
    border: 1px solid var(--line);
    border-radius: 16px;
    overflow: hidden;
  }
  .tweak-row {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 14px 15px;
    border-bottom: 1px solid var(--line);
  }
  .tweak-row:last-child {
    border-bottom: none;
  }
  .tweak-row.column {
    flex-direction: column;
    align-items: stretch;
    gap: 11px;
  }
  .t-row-head {
    display: flex;
    align-items: center;
    gap: 12px;
  }
  .t-icon {
    font-size: 22px;
    color: var(--text-soft);
    flex: none;
  }
  .t-info {
    flex: 1;
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .t-title {
    font-size: 14px;
    font-weight: 600;
  }
  .t-desc {
    font-size: 11px;
    color: var(--muted);
  }

  .switch {
    position: relative;
    width: 44px;
    height: 26px;
    border-radius: 999px;
    background: #2a2e36;
    border: none;
    flex: none;
    cursor: pointer;
    padding: 0;
    transition: background-color 0.2s;
  }
  .switch.on {
    background: var(--accent);
  }
  .switch.busy {
    opacity: 0.6;
  }
  .switch:disabled {
    cursor: default;
  }
  .knob {
    position: absolute;
    top: 3px;
    left: 3px;
    width: 20px;
    height: 20px;
    border-radius: 50%;
    background: #6b727c;
    transition: transform 0.2s, background-color 0.2s;
  }
  .switch.on .knob {
    transform: translateX(18px);
    background: var(--accent-ink);
  }

  .segmented {
    display: flex;
    padding: 3px;
    border-radius: 11px;
    background: var(--canvas);
    gap: 3px;
  }
  .seg {
    flex: 1;
    text-align: center;
    padding: 8px 4px;
    border-radius: 9px;
    border: none;
    background: transparent;
    color: var(--muted);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }
  .seg.active {
    background: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .seg.busy {
    opacity: 0.6;
  }
  .seg:disabled {
    cursor: default;
  }

  .dns-input-row {
    display: flex;
    gap: 8px;
  }
  .dns-input {
    flex: 1;
    min-height: 42px;
    background: var(--canvas);
    border: 1px solid var(--line);
    border-radius: 10px;
    padding: 0 12px;
    color: var(--text);
    font-size: 13px;
  }
  .dns-input:focus {
    outline: 2px solid var(--accent);
    border-color: transparent;
  }
  .small-inline {
    width: auto;
    min-height: 42px;
    padding: 0 16px;
    font-size: 13px;
    border-radius: 10px;
    flex: none;
  }
</style>
