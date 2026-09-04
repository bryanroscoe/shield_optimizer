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
    WriteResult,
  } from "../lib/types";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import Toast from "../components/Toast.svelte";

  let {
    navigate,
    back,
  }: { navigate: (screen: Screen) => void; back: () => void } = $props();

  let loading = $state(true);
  let noDevice = $state("");
  let tweaks = $state<TweaksState | null>(null);
  let dns = $state<PrivateDnsState | null>(null);
  let scaling = $state<CurrentDisplayScaling | null>(null);
  // Per-card read errors — one failed read must not blank the whole screen.
  let tweaksError = $state("");
  let dnsError = $state("");
  let scalingError = $state("");
  let busy = $state("");
  let showPaywall = $state(false);
  let dnsHost = $state("");
  let dnsEditing = $state(false);
  let scaleConfirm = $state<DisplayScalePreset | null>(null);
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
      noDevice = "No TV connected.";
      loading = false;
      return;
    }
    loading = tweaks === null && dns === null && scaling === null;
    noDevice = "";
    const [t, d, s] = await Promise.allSettled([
      api.getTweaks(serial),
      api.getPrivateDns(serial),
      api.getDisplayScaling(serial),
    ]);
    if (generation !== loadGeneration || serial !== session.serial) return;
    if (t.status === "fulfilled") {
      tweaks = t.value;
      tweaksError = "";
    } else {
      tweaksError = String(t.reason);
    }
    if (d.status === "fulfilled") {
      dns = d.value;
      dnsError = "";
      if (d.value.hostname) dnsHost = d.value.hostname;
    } else {
      dnsError = String(d.reason);
    }
    if (s.status === "fulfilled") {
      scaling = s.value;
      scalingError = "";
    } else {
      scalingError = String(s.reason);
    }
    loading = false;
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

  /// Write several keys in one action, stopping at the first failure so the
  /// toast reports the real error rather than the last successful write.
  function writeAll(
    namespace: "global" | "secure",
    keys: string[],
    value: string,
  ): (targetSerial: string) => Promise<WriteResult> {
    return async (targetSerial: string) => {
      let last: WriteResult = { ok: true, message: "ok" };
      for (const key of keys) {
        last = await api.writeSetting(targetSerial, namespace, key, value);
        if (!last.ok) break;
      }
      return last;
    };
  }

  // ---- Derived current states (null = unset on the device; rendered as such) ----
  const CEC_KEYS = [
    "hdmi_control_enabled",
    "hdmi_control_auto_wakeup_enabled",
    "hdmi_control_auto_device_off_enabled",
    "hdmi_system_audio_control_enabled",
  ];
  const cecRows = $derived([
    {
      key: "hdmi_control_enabled",
      title: "HDMI-CEC control",
      desc: "One remote for TV + soundbar",
      value: tweaks?.hdmi_control_enabled ?? null,
    },
    {
      key: "hdmi_control_auto_wakeup_enabled",
      title: "Auto-wake the TV",
      desc: "TV switches on with this device",
      value: tweaks?.hdmi_control_auto_wakeup_enabled ?? null,
    },
    {
      key: "hdmi_control_auto_device_off_enabled",
      title: "Auto-off with the TV",
      desc: "This device sleeps when the TV does",
      value: tweaks?.hdmi_control_auto_device_off_enabled ?? null,
    },
    {
      key: "hdmi_system_audio_control_enabled",
      title: "System audio control",
      desc: "Volume goes to the receiver",
      value: tweaks?.hdmi_system_audio_control_enabled ?? null,
    },
  ]);

  const frameRate = $derived(tweaks?.match_content_frame_rate ?? null);
  const animScale = $derived(
    tweaks?.window_animation_scale != null ? parseFloat(tweaks.window_animation_scale) : null,
  );
  const animMixed = $derived(
    tweaks != null &&
      !(
        tweaks.window_animation_scale === tweaks.transition_animation_scale &&
        tweaks.window_animation_scale === tweaks.animator_duration_scale
      ),
  );
  const longPress = $derived(
    tweaks?.long_press_timeout != null ? parseInt(tweaks.long_press_timeout, 10) : null,
  );
  const bgLimit = $derived(tweaks?.background_process_limit ?? null);
  const dnsMode = $derived(dns?.mode ?? null);

  function wmValue(block: string | undefined, kind: "Physical" | "Override"): string | null {
    if (!block) return null;
    for (const line of block.split("\n")) {
      const m = line.trim().match(/^(Physical|Override)\s+\w+:\s*(.+)$/i);
      if (m && m[1].toLowerCase() === kind.toLowerCase()) return m[2].trim();
    }
    return null;
  }
  const physicalSize = $derived(wmValue(scaling?.size, "Physical"));
  const overrideSize = $derived(wmValue(scaling?.size, "Override"));
  const physicalDensity = $derived(wmValue(scaling?.density, "Physical"));
  const overrideDensity = $derived(wmValue(scaling?.density, "Override"));
  // The override is what the TV is actually rendering at; show it first and
  // never present the physical panel size as the current setting.
  const scaleLine = $derived.by(() => {
    if (overrideSize) {
      const density = overrideDensity ?? physicalDensity;
      return `${overrideSize}${density ? ` @ ${density}` : ""} · panel ${physicalSize ?? "—"}`;
    }
    if (physicalSize) {
      return `${physicalSize}${physicalDensity ? ` @ ${physicalDensity}` : ""} · device default`;
    }
    return "—";
  });

  const framePresets: { label: string; value: string }[] = [
    { label: "Never", value: "0" },
    { label: "Seamless", value: "1" },
    { label: "Always", value: "2" },
  ];
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

  function triLabel(v: string | null): string {
    return v === "1" ? "On" : v === "0" ? "Off" : "Unset";
  }
  function frameLabel(v: string | null): string {
    return v === "0"
      ? "Never"
      : v === "1"
        ? "Seamless only"
        : v === "2"
          ? "Always"
          : "Unset (device default)";
  }
  function animLabel(): string {
    if (tweaks == null) return "—";
    if (animMixed) {
      return `Mixed: ${tweaks.window_animation_scale ?? "unset"} / ${
        tweaks.transition_animation_scale ?? "unset"
      } / ${tweaks.animator_duration_scale ?? "unset"}`;
    }
    const w = tweaks.window_animation_scale;
    return w == null ? "Unset (device default)" : w === "0" ? "Off" : `${w}×`;
  }

  function toggleCec(row: { key: string; title: string; value: string | null }) {
    const next = row.value === "1" ? "0" : "1";
    apply(
      `cec-${row.key}`,
      (target) => api.writeSetting(target, "global", row.key, next),
      `${row.title} turned ${next === "1" ? "on" : "off"}.`,
    );
  }
  function resetCec() {
    apply("cec-reset", writeAll("global", CEC_KEYS, ""), "HDMI-CEC reset to device defaults.");
  }
  function setFrameRate(value: string) {
    apply(
      `fr-${value}`,
      (target) => api.writeSetting(target, "secure", "match_content_frame_rate", value),
      `Frame-rate matching set to ${frameLabel(value).toLowerCase()}.`,
    );
  }
  function resetFrameRate() {
    apply(
      "fr-reset",
      writeAll("secure", ["match_content_frame_rate"], ""),
      "Frame-rate matching reset to the device default.",
    );
  }
  const ANIM_KEYS = [
    "window_animation_scale",
    "transition_animation_scale",
    "animator_duration_scale",
  ];
  function setAnim(value: string) {
    // Animation speed is three scales in lockstep — write all so the UI is
    // consistent (window / transition / animator).
    apply(`anim-${value}`, writeAll("global", ANIM_KEYS, value), "Animation speed updated.");
  }
  function resetAnim() {
    apply(
      "anim-reset",
      writeAll("global", ANIM_KEYS, ""),
      "Animation speed reset to the device default.",
    );
  }
  function setLongPress(ms: number) {
    apply(
      `lp-${ms}`,
      (target) => api.writeSetting(target, "secure", "long_press_timeout", String(ms)),
      `Long-press timeout set to ${ms}ms.`,
    );
  }
  function resetLongPress() {
    apply(
      "lp-reset",
      writeAll("secure", ["long_press_timeout"], ""),
      "Long-press timeout reset to the device default.",
    );
  }
  function setBgLimit(value: string) {
    apply(
      `bg-${value}`,
      (target) => api.writeSetting(target, "global", "background_process_limit", value),
      "Background limit updated. Android resets it on the next reboot.",
    );
  }
  function setScaling(preset: DisplayScalePreset) {
    apply(
      `scale-${preset}`,
      (target) => api.setDisplayScaling(target, preset),
      "Display scaling applied.",
    );
  }
  function confirmScaling() {
    const preset = scaleConfirm;
    scaleConfirm = null;
    if (preset) setScaling(preset);
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
    if (animScale == null || animMixed) return false;
    return Math.abs(animScale - parseFloat(value)) < 0.001;
  }

  const scaleConfirmMessage = $derived(
    scaleConfirm === "reset"
      ? "Clears the size and density override so the TV goes back to its own defaults. The screen will flicker while it re-lays out."
      : scaleConfirm === "uhd_4k"
        ? "Sets the UI to 3839x2160 at density 640. The screen will flicker and some apps re-layout; use Reset if anything looks wrong."
        : "Sets the UI to 1920x1080 at density 320. The screen will flicker and some apps re-layout; use Reset if anything looks wrong.",
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
  {:else if noDevice}
    <p class="error">{noDevice}</p>
    <button class="primary" onclick={load}>Retry</button>
    <div class="spacer"></div>
  {:else}
    <div class="tweaks-content">
      <!-- Display & sound -->
      <span class="section-label nomargin">Display &amp; sound</span>
      <div class="tweak-group">
        {#if tweaksError}
          <div class="card-error">
            <p class="error small">{tweaksError}</p>
            <button class="l-btn" onclick={load}>Retry</button>
          </div>
        {:else}
          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon">settings_input_hdmi</span>
              <div class="t-info">
                <span class="t-title">HDMI-CEC</span>
                <span class="t-desc">Four device settings, read straight from the TV</span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "cec-reset"}
                disabled={busy !== ""}
                onclick={resetCec}
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <div class="subrows">
              {#each cecRows as row (row.key)}
                <div class="subrow">
                  <div class="t-info">
                    <span class="t-subtitle">{row.title}</span>
                    <span class="t-desc">{row.desc} · <span class="state">{triLabel(row.value)}</span></span>
                  </div>
                  <button
                    class="switch"
                    class:on={row.value === "1"}
                    class:unset={row.value == null}
                    class:busy={busy === `cec-${row.key}`}
                    disabled={busy !== ""}
                    onclick={() => toggleCec(row)}
                    aria-label={`${row.title}: ${triLabel(row.value)}`}
                  >
                    <span class="knob"></span>
                  </button>
                </div>
              {/each}
            </div>
          </div>

          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon">30fps_select</span>
              <div class="t-info">
                <span class="t-title">Match content frame rate</span>
                <span class="t-desc">{frameLabel(frameRate)}</span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "fr-reset"}
                disabled={busy !== ""}
                onclick={resetFrameRate}
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <div class="segmented">
              {#each framePresets as p (p.value)}
                <button
                  class="seg"
                  class:active={frameRate === p.value}
                  class:busy={busy === `fr-${p.value}`}
                  disabled={busy !== ""}
                  onclick={() => setFrameRate(p.value)}>{p.label}</button
                >
              {/each}
            </div>
          </div>
        {/if}

        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon">aspect_ratio</span>
            <div class="t-info">
              <span class="t-title">Display scaling</span>
              <span class="t-desc mono">{scalingError ? "Couldn't read the current size" : scaleLine}</span>
            </div>
          </div>
          <div class="segmented">
            {#each scalePresets as p (p.value)}
              <button
                class="seg"
                class:busy={busy === `scale-${p.value}`}
                disabled={busy !== ""}
                onclick={() => (scaleConfirm = p.value)}>{p.label}</button
              >
            {/each}
          </div>
        </div>
      </div>

      <!-- Speed & input -->
      <span class="section-label">Speed &amp; input</span>
      <div class="tweak-group">
        {#if tweaksError}
          <div class="card-error">
            <p class="error small">{tweaksError}</p>
            <button class="l-btn" onclick={load}>Retry</button>
          </div>
        {:else}
          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon">animation</span>
              <div class="t-info">
                <span class="t-title">Animation speed</span>
                <span class="t-desc">{animLabel()}</span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "anim-reset"}
                disabled={busy !== ""}
                onclick={resetAnim}
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <div class="segmented">
              {#each animPresets as p (p.value)}
                <button
                  class="seg"
                  class:active={animActive(p.value)}
                  class:busy={busy === `anim-${p.value}`}
                  disabled={busy !== ""}
                  onclick={() => setAnim(p.value)}>{p.label}</button
                >
              {/each}
            </div>
          </div>
          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon">touch_app</span>
              <div class="t-info">
                <span class="t-title">Long-press timeout</span>
                <span class="t-desc">{longPress != null ? `${longPress}ms` : "Unset (device default)"}</span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "lp-reset"}
                disabled={busy !== ""}
                onclick={resetLongPress}
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <div class="segmented">
              {#each longPressPresets as ms (ms)}
                <button
                  class="seg"
                  class:active={longPress === ms}
                  class:busy={busy === `lp-${ms}`}
                  disabled={busy !== ""}
                  onclick={() => setLongPress(ms)}>{ms}</button
                >
              {/each}
            </div>
          </div>
          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon">memory</span>
              <div class="t-info">
                <span class="t-title">Background process limit</span>
                <span class="t-desc">
                  {bgLimit == null ? "Standard" : bgLimit === "0" ? "None" : `At most ${bgLimit}`} ·
                  Android clears this on every reboot
                </span>
              </div>
            </div>
            <div class="segmented">
              {#each bgPresets as p (p.label)}
                <button
                  class="seg"
                  class:active={(p.value === "" && bgLimit == null) || bgLimit === p.value}
                  class:busy={busy === `bg-${p.value}`}
                  disabled={busy !== ""}
                  onclick={() => setBgLimit(p.value)}>{p.label}</button
                >
              {/each}
            </div>
          </div>
        {/if}
      </div>

      <!-- Network -->
      <span class="section-label">Network</span>
      <div class="tweak-group">
        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon">dns</span>
            <div class="t-info">
              <span class="t-title">Private DNS</span>
              <span class="t-desc">
                {#if dnsError}
                  Couldn't read the current mode
                {:else}
                  DNS-over-TLS · {dnsMode === "off"
                    ? "Off"
                    : dnsMode === "opportunistic"
                      ? "Automatic"
                      : dnsMode === "hostname"
                        ? dns?.hostname
                          ? `Custom (${dns.hostname})`
                          : "Custom"
                        : "Unset"}
                {/if}
              </span>
            </div>
          </div>
          <div class="segmented">
            <button class="seg" class:active={dnsMode === "off"} class:busy={busy === "dns-off"} disabled={busy !== ""} onclick={() => setDns("off")}>Off</button>
            <button class="seg" class:active={dnsMode === "opportunistic"} class:busy={busy === "dns-opportunistic"} disabled={busy !== ""} onclick={() => setDns("opportunistic")}>Auto</button>
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

      {#if scalingError}
        <p class="error small">{scalingError}</p>
      {/if}
      {#if dnsError}
        <p class="error small">{dnsError}</p>
      {/if}
    </div>
    <div class="spacer"></div>
  {/if}

  <ConfirmDialog
    open={scaleConfirm !== null}
    icon="aspect_ratio"
    title={scaleConfirm === "reset" ? "Reset display scaling?" : "Change display scaling?"}
    message={scaleConfirmMessage}
    confirmLabel={scaleConfirm === "reset" ? "Reset" : "Apply"}
    onConfirm={confirmScaling}
    onCancel={() => (scaleConfirm = null)}
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
  .t-subtitle {
    font-size: 13px;
    font-weight: 600;
  }
  .t-desc {
    font-size: 11px;
    color: var(--muted);
    line-height: 1.4;
  }
  .t-desc .state {
    color: var(--text-soft);
    font-weight: 600;
  }

  .subrows {
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding-left: 34px;
  }
  .subrow {
    display: flex;
    align-items: center;
    gap: 12px;
    padding: 8px 0;
    border-top: 1px solid var(--line);
  }
  .subrow:first-child {
    border-top: none;
  }

  .card-error {
    display: flex;
    align-items: center;
    gap: 10px;
    padding: 14px 15px;
  }
  .error.small {
    font-size: 12px;
    margin: 0;
    flex: 1;
  }
  .l-btn {
    min-height: 38px;
    padding: 0 14px;
    border: 1px solid rgba(255, 255, 255, 0.12);
    border-radius: 11px;
    background: var(--surface-2);
    color: var(--text);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    flex: none;
  }

  .reset-btn {
    display: inline-flex;
    align-items: center;
    gap: 5px;
    min-height: 40px;
    padding: 0 12px;
    border: 1px solid var(--line);
    border-radius: 11px;
    background: var(--canvas);
    color: var(--muted);
    font-family: var(--sans);
    font-size: 11px;
    font-weight: 600;
    cursor: pointer;
    flex: none;
  }
  .reset-btn .msr {
    font-size: 15px;
  }
  .reset-btn:disabled {
    cursor: default;
  }
  .reset-btn.busy {
    opacity: 0.6;
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
  .switch.unset {
    background: color-mix(in srgb, var(--amber) 20%, #2a2e36);
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
  .switch.unset .knob {
    transform: translateX(9px);
    background: var(--amber);
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
