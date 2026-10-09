<script lang="ts" module>
  // The screensaver that was set the first time Tweaks read each TV in this
  // app session, keyed by serial. Module-level so leaving and re-opening the
  // tab doesn't recapture a value this screen itself wrote, which would make
  // "Restore previous" restore nothing.
  const screensaverOriginals = new Map<
    string,
    { components: string | null; enabled: string | null }
  >();
</script>

<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api } from "../lib/api";
  import { session } from "../lib/session.svelte";
  import type { Screen } from "../lib/router.svelte";
  import type {
    CurrentDisplayScaling,
    DisplayScalePreset,
    PermissionState,
    PrivateDnsState,
    TweaksState,
    WriteResult,
  } from "../lib/types";
  import ConfirmDialog from "../components/ConfirmDialog.svelte";
  import FindRemoteButton from "../components/FindRemoteButton.svelte";
  import PaywallSheet from "../components/PaywallSheet.svelte";
  import BottomTabs from "../components/BottomTabs.svelte";
  import Toast from "../components/Toast.svelte";

  let { navigate }: { navigate: (screen: Screen) => void; back?: () => void } = $props();

  type HooksState = "enabled" | "disabled" | "missing";

  const HOOKS_PKG = "com.nvidia.shieldtech.hooks";
  const ASSISTANT_PKG = "com.google.android.katniss";
  const ASSISTANT_PERM = "android.permission.RECORD_AUDIO";
  const BASIC_DAYDREAM = "com.android.dreams.basic/com.android.dreams.basic.BasicDream";

  let loading = $state(true);
  let noDevice = $state("");
  let tweaks = $state<TweaksState | null>(null);
  let dns = $state<PrivateDnsState | null>(null);
  let scaling = $state<CurrentDisplayScaling | null>(null);
  let hooks = $state<HooksState | null>(null);
  let assistant = $state<PermissionState | null>(null);
  // Per-card read errors — one failed read must not blank the whole screen.
  let tweaksError = $state("");
  let dnsError = $state("");
  let scalingError = $state("");
  let hooksError = $state("");
  let assistantError = $state("");
  let screensaverOriginal = $state<{ components: string | null; enabled: string | null } | null>(
    null,
  );
  let busy = $state("");
  let showPaywall = $state(false);
  let dnsHost = $state("");
  let dnsEditing = $state(false);
  let scaleConfirm = $state<DisplayScalePreset | null>(null);
  let loadGeneration = 0;
  let destroyed = false;

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
    const connection = session.generation;
    const generation = ++loadGeneration;
    if (!serial || !session.isConnected) {
      noDevice = "No live TV connection.";
      loading = false;
      return;
    }
    loading = tweaks === null && dns === null && scaling === null;
    noDevice = "";
    const [t, d, s, h, a] = await Promise.allSettled([
      api.getTweaks(serial),
      api.getPrivateDns(serial),
      api.getDisplayScaling(serial),
      api.packageStates(serial, [HOOKS_PKG]),
      api.appPermissionState(serial, ASSISTANT_PKG, ASSISTANT_PERM),
    ]);
    if (
      destroyed || generation !== loadGeneration || serial !== session.serial ||
      connection !== session.generation || !session.isConnected
    ) return;
    if (t.status === "fulfilled") {
      const value = t.value;
      tweaks = value;
      tweaksError = "";
      if (!screensaverOriginals.has(serial)) {
        screensaverOriginals.set(serial, {
          components: value.screensaver_components ?? null,
          enabled: value.screensaver_enabled ?? null,
        });
      }
      screensaverOriginal = screensaverOriginals.get(serial) ?? null;
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
    // A reply that doesn't name the package is unreadable, not "missing":
    // only an explicit missing hides the row.
    const hookValue = h.status === "fulfilled" ? h.value?.[HOOKS_PKG] : undefined;
    if (hookValue === "enabled" || hookValue === "disabled" || hookValue === "missing") {
      hooks = hookValue;
      hooksError = "";
    } else {
      hooks = null;
      hooksError = h.status === "rejected" ? String(h.reason) : "The TV didn't report this package.";
    }
    const micValue = a.status === "fulfilled" ? a.value : undefined;
    if (micValue === "granted" || micValue === "revoked" || micValue === "missing") {
      assistant = micValue;
      assistantError = "";
    } else {
      assistant = null;
      assistantError = a.status === "rejected"
        ? String(a.reason)
        : "The TV didn't report the microphone permission.";
    }
    loading = false;
  }

  $effect(() => {
    void session.serial;
    void session.generation;
    void session.liveness;
    tweaks = null;
    dns = null;
    scaling = null;
    hooks = null;
    assistant = null;
    screensaverOriginal = null;
    tweaksError = "";
    dnsError = "";
    scalingError = "";
    hooksError = "";
    assistantError = "";
    busy = "";
    toast = "";
    showPaywall = false;
    scaleConfirm = null;
    dnsEditing = false;
    clearTimeout(toastTimer);
    untrack(() => void load());
  });

  onDestroy(() => {
    destroyed = true;
    ++loadGeneration;
    clearTimeout(toastTimer);
  });

  // Wrap a Pro write: route LOCKED to the paywall, reload on success/revert,
  // never fabricate the new value (we re-read the device instead).
  async function apply(
    key: string,
    fn: (targetSerial: string) => Promise<{ ok: boolean; message: string; reverted?: boolean }>,
    successMsg: string,
  ) {
    if (busy) return;
    const targetSerial = session.serial;
    if (!targetSerial || !session.isConnected) return;
    const connection = session.generation;
    const current = () =>
      !destroyed && targetSerial === session.serial &&
      connection === session.generation && session.isConnected;
    busy = key;
    try {
      const r = await fn(targetSerial);
      if (!current()) return;
      session.invalidateHealth();
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
      if (!current()) return;
      if (isLocked(e)) showPaywall = true;
      else {
        session.invalidateHealth();
        showToast(String(e), "error");
        await load();
      }
    } finally {
      if (current()) busy = "";
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

  // ---- Remote & buttons ----
  function setHooks(enabled: boolean) {
    // Turning the hooks back on is free (`enable_package`); turning them off
    // is a curated disable and goes through the backend's Pro + safety gate.
    apply(
      enabled ? "hooks-on" : "hooks-off",
      (target) => enabled ? api.enablePackage(target, HOOKS_PKG) : api.disablePackage(target, HOOKS_PKG),
      enabled ? "Nvidia System Hooks turned on." : "Nvidia System Hooks turned off.",
    );
  }
  function setAssistant(grant: boolean) {
    apply(
      grant ? "mic-on" : "mic-off",
      (target) => api.setAppPermission(target, ASSISTANT_PKG, ASSISTANT_PERM, grant),
      grant ? "Assistant button microphone turned on." : "Assistant button microphone turned off.",
    );
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
  const surroundMode = $derived(tweaks?.encoded_surround_output ?? null);
  const audioFormats = $derived(
    (tweaks?.encoded_surround_output_enabled_formats ?? "")
      .split(",")
      .map((value) => value.trim())
      .filter(Boolean),
  );
  const surroundModes = [
    { value: "0", label: "Auto" },
    { value: "1", label: "Never" },
    { value: "2", label: "Always" },
    { value: "3", label: "Manual" },
  ];
  const audioChoices = [
    { value: "5", label: "Dolby Digital (AC-3)" },
    { value: "6", label: "Dolby Digital Plus (E-AC-3)" },
    { value: "18", label: "E-AC-3 JOC (Atmos)" },
    { value: "14", label: "Dolby TrueHD" },
    { value: "19", label: "Dolby MAT" },
    { value: "7", label: "DTS" },
    { value: "8", label: "DTS-HD" },
  ];
  function audioLabel(value: string): string {
    return audioChoices.find((choice) => choice.value === value)?.label
      ?? ({ "26": "MPEG-H LC L4", "27": "DTS UHD P1" }[value])
      ?? `Encoding ${value}`;
  }
  function setSurroundMode(value: string) {
    apply(
      value === "" ? "audio-reset" : `audio-${value}`,
      (target) => api.writeSetting(target, "global", "encoded_surround_output", value),
      value === "" ? "Audio passthrough reset to the device default." : "Surround policy updated.",
    );
  }
  function toggleAudioFormat(value: string) {
    const next = audioFormats.includes(value)
      ? audioFormats.filter((format) => format !== value)
      : [...audioFormats, value];
    apply(
      "audio-formats",
      (target) => api.writeSetting(target, "global", "encoded_surround_output_enabled_formats", next.join(",")),
      "Manual audio formats updated.",
    );
  }
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
  const cachedLimit = $derived(tweaks?.cached_process_limit ?? null);
  const dnsMode = $derived(dns?.mode ?? null);

  const ssComponent = $derived(tweaks?.screensaver_components ?? null);
  const ssEnabled = $derived(tweaks?.screensaver_enabled ?? null);
  const ssIsBasic = $derived(ssEnabled !== "0" && ssComponent === BASIC_DAYDREAM);
  const ssIsOff = $derived(ssEnabled === "0");
  const ssAtOriginal = $derived(
    screensaverOriginal != null &&
      (screensaverOriginal.components ?? null) === ssComponent &&
      (screensaverOriginal.enabled ?? null) === ssEnabled,
  );

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
    { label: "0.5×", value: "0.5" },
    { label: "1×", value: "1" },
  ];
  const longPressPresets = [300, 400, 500, 750];
  const scalePresets: { label: string; value: DisplayScalePreset }[] = [
    { label: "4K", value: "uhd_4k" },
    { label: "1080p", value: "fhd_1080p" },
    { label: "720p", value: "hd_720p" },
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
  function screensaverLabel(component: string | null, enabled: string | null): string {
    if (enabled === "0") return "Off";
    if (!component) return "None set";
    if (component === BASIC_DAYDREAM) return "Basic Daydream";
    return component;
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
  function removeBgLimit() {
    apply(
      "bg-remove",
      (target) => api.writeSetting(target, "global", "background_process_limit", ""),
      "Old setting removed.",
    );
  }

  // Screensaver is two settings acting as one control: screensaver_enabled
  // gates whether Daydream runs at all, and screensaver_components picks which
  // one. Clearing the component alone can leave a vendor default running, so
  // Off turns the flag off too. A refused flag write stops before the
  // component changes, so Daydream is never left enabled with no component.
  function writeScreensaver(key: string, component: string, enabled: string, successMsg: string) {
    apply(
      key,
      async (target) => {
        const flag = await api.writeSetting(target, "secure", "screensaver_enabled", enabled);
        if (!flag.ok) {
          return {
            ok: false,
            message: `The TV refused the screensaver switch (${flag.message.trim() || "no reason given"}). The screensaver was left unchanged.`,
          };
        }
        return api.writeSetting(target, "secure", "screensaver_components", component);
      },
      successMsg,
    );
  }
  function setBasicDaydream() {
    writeScreensaver("ss-basic", BASIC_DAYDREAM, "1", "Screensaver set to Basic Daydream.");
  }
  function screensaverOff() {
    writeScreensaver("ss-off", "", "0", "Screensaver turned off.");
  }
  function restoreScreensaver() {
    const original = screensaverOriginal;
    if (!original) return;
    writeScreensaver(
      "ss-restore",
      original.components ?? "",
      original.enabled ?? "",
      "Previous screensaver restored.",
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
        : scaleConfirm === "hd_720p"
          ? "Sets the UI to 1280x720 at density 213. Lightest on the TV, but menus look soft. Use Reset if anything looks wrong."
          : "Sets the UI to 1920x1080 at density 320. The screen will flicker and some apps re-layout; use Reset if anything looks wrong.",
  );

  const showHooks = $derived(
    hooks === "enabled" || hooks === "disabled" ||
      (hooksError !== "" && session.connectedDevice?.device_type === "shield"),
  );
  const showAssistant = $derived(
    assistant === "granted" || assistant === "revoked" || assistantError !== "",
  );
</script>

{#snippet readError(message: string)}
  <div class="card-error">
    <p class="error small">{message}</p>
    <button class="l-btn" disabled={busy !== ""} onclick={load}>Retry</button>
  </div>
{/snippet}

<div class="screen">
  <div class="topline">
    <div class="header-info">
      <h3 class="header-title">Tweaks</h3>
      <span class="device-subtitle">{session.deviceLabel}</span>
    </div>
    <div class="header-actions">
      <FindRemoteButton />
      <button
        class="iconbtn"
        aria-label="Re-read settings"
        disabled={busy !== "" || loading}
        onclick={load}
      >
        <span class="msr">refresh</span>
      </button>
    </div>
  </div>

  <p class="intro">
    Settings Android hides or buries, read straight from the TV. Reset puts a setting back to the
    TV's default.
  </p>

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
      <span class="section-label nomargin">Remote &amp; buttons</span>
      <div class="tweak-group">
        {#if showHooks}
          <div class="tweak-row column" data-tweak="hooks">
            <div class="t-row-head">
              <span class="msr t-icon" aria-hidden="true">stadia_controller</span>
              <div class="t-info">
                <span class="t-title">Nvidia System Hooks</span>
                <span class="t-desc">
                  {#if hooksError}
                    Couldn't read the current state
                  {:else}
                    Current: <span class="state">{hooks === "disabled" ? "Hooks off" : "Hooks on"}</span>
                  {/if}
                </span>
              </div>
            </div>
            <p class="t-note">
              Turns the Xbox controller's Guide button into Home. Turning the hooks off can fix
              Guide button conflicts in Steam Link and Moonlight. This doesn't change the Shield
              remote's Netflix button, which is handled in firmware; use a button remapper app for
              that. Turning the hooks back on is always free.
            </p>
            {#if hooksError}
              {@render readError(hooksError)}
            {:else}
              <div class="segmented" role="group" aria-label="Nvidia System Hooks">
                <button
                  class="seg"
                  class:active={hooks === "enabled"}
                  class:busy={busy === "hooks-on"}
                  aria-pressed={hooks === "enabled"}
                  disabled={busy !== ""}
                  onclick={() => setHooks(true)}>On</button
                >
                <button
                  class="seg"
                  class:active={hooks === "disabled"}
                  class:busy={busy === "hooks-off"}
                  aria-pressed={hooks === "disabled"}
                  disabled={busy !== ""}
                  onclick={() => setHooks(false)}>Off</button
                >
              </div>
            {/if}
          </div>
        {/if}

        {#if showAssistant}
          <div class="tweak-row column" data-tweak="assistant">
            <div class="t-row-head">
              <span class="msr t-icon" aria-hidden="true">settings_remote</span>
              <div class="t-info">
                <span class="t-title">Remote Assistant Button</span>
                <span class="t-desc">
                  {#if assistantError}
                    Couldn't read the microphone permission
                  {:else}
                    Current: <span class="state">{assistant === "revoked" ? "Mic off" : "Mic on"}</span>
                  {/if}
                </span>
              </div>
            </div>
            <p class="t-note">
              Off removes the microphone permission from Google's search app, so the remote's
              Assistant button can't hear you. The button may still open the assistant for a
              moment. Trade-off: voice search in the Play Store stops working too.
            </p>
            {#if assistantError}
              {@render readError(assistantError)}
            {:else}
              <div class="segmented" role="group" aria-label="Remote Assistant Button">
                <button
                  class="seg"
                  class:active={assistant === "granted"}
                  class:busy={busy === "mic-on"}
                  aria-pressed={assistant === "granted"}
                  disabled={busy !== ""}
                  onclick={() => setAssistant(true)}>On</button
                >
                <button
                  class="seg"
                  class:active={assistant === "revoked"}
                  class:busy={busy === "mic-off"}
                  aria-pressed={assistant === "revoked"}
                  disabled={busy !== ""}
                  onclick={() => setAssistant(false)}>Off</button
                >
              </div>
            {/if}
          </div>
        {/if}

        {#if tweaksError}
          {@render readError(tweaksError)}
        {:else}
          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon" aria-hidden="true">touch_app</span>
              <div class="t-info">
                <span class="t-title">Long-press timeout</span>
                <span class="t-desc">
                  Current: <span class="state">{longPress != null ? `${longPress} ms` : "Unset (default 400 ms)"}</span>
                </span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "lp-reset"}
                disabled={busy !== ""}
                onclick={resetLongPress}
                aria-label="Reset long-press timeout"
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <p class="t-note">
              How long you hold OK before it counts as a long press. Shorter reacts sooner but
              makes accidental long presses more likely.
            </p>
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
        {/if}
      </div>

      <span class="section-label">Picture &amp; sound</span>
      <div class="tweak-group">
        {#if tweaksError}
          {@render readError(tweaksError)}
        {:else}
          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon" aria-hidden="true">settings_input_hdmi</span>
              <div class="t-info">
                <span class="t-title">HDMI-CEC</span>
                <span class="t-desc">Lets the TV and this box control each other over HDMI</span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "cec-reset"}
                disabled={busy !== ""}
                onclick={resetCec}
                aria-label="Reset HDMI-CEC"
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
              <span class="msr t-icon" aria-hidden="true">sync_alt</span>
              <div class="t-info">
                <span class="t-title">Match content frame rate</span>
                <span class="t-desc">Current: <span class="state">{frameLabel(frameRate)}</span></span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "fr-reset"}
                disabled={busy !== ""}
                onclick={resetFrameRate}
                aria-label="Reset frame-rate matching"
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <p class="t-note">
              Lets apps switch the TV's refresh rate to match the video, such as 24 Hz for films.
              Seamless only switches without a black screen. The app and TV both have to support it.
            </p>
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

          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon" aria-hidden="true">audiotrack</span>
              <div class="t-info">
                <span class="t-title">Audio passthrough</span>
                <span class="t-desc">Current: <span class="state">{surroundModes.find((mode) => mode.value === surroundMode)?.label ?? (surroundMode == null ? "Default / unset" : `Unknown (${surroundMode})`)}</span></span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "audio-reset"}
                disabled={busy !== ""}
                onclick={() => setSurroundMode("")}
                aria-label="Reset audio policy"
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <p class="t-note">
              Auto uses what the connected equipment advertises. Manual overrides can cause silence
              on unsupported equipment. Receiver and app support determine actual playback.
            </p>
            <div class="segmented">
              {#each surroundModes as mode (mode.value)}
                <button class="seg" class:active={surroundMode === mode.value} class:busy={busy === `audio-${mode.value}`} disabled={busy !== ""} onclick={() => setSurroundMode(mode.value)} aria-label={`Surround ${mode.label}`}>{mode.label}</button>
              {/each}
            </div>
            {#if surroundMode === "3"}
              <p class="t-desc">Enable only formats supported by your receiver. Existing encodings not listed below are preserved.</p>
              {#each audioChoices as format (format.value)}
                <label class="subrow">
                  <span class="t-subtitle">{format.label}</span>
                  <input type="checkbox" checked={audioFormats.includes(format.value)} disabled={busy !== ""} onchange={() => toggleAudioFormat(format.value)} />
                </label>
              {/each}
              <span class="t-desc">Configured: {audioFormats.length ? audioFormats.map(audioLabel).join(", ") : "None / unset"}</span>
            {/if}
          </div>
        {/if}

        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon" aria-hidden="true">aspect_ratio</span>
            <div class="t-info">
              <span class="t-title">Display scaling</span>
              <span class="t-desc mono">{scalingError ? "Couldn't read the current size" : scaleLine}</span>
            </div>
          </div>
          <p class="t-note">
            The resolution Android draws its menus at. The TV's output resolution doesn't change and
            video still decodes at full resolution. Lower is lighter on the TV but looks softer.
            Restarting the TV does not undo this; use Reset.
          </p>
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
          {#if scalingError}
            <p class="error small">{scalingError}</p>
          {/if}
        </div>
      </div>

      <span class="section-label">Interface</span>
      <div class="tweak-group">
        {#if tweaksError}
          {@render readError(tweaksError)}
        {:else}
          <div class="tweak-row column" data-tweak="screensaver">
            <div class="t-row-head">
              <span class="msr t-icon" aria-hidden="true">smart_display</span>
              <div class="t-info">
                <span class="t-title">Screensaver</span>
                <span class="t-desc">Current: <span class="state wrap">{screensaverLabel(ssComponent, ssEnabled)}</span></span>
              </div>
            </div>
            <p class="t-note">
              Which screensaver (Daydream) runs when the TV is idle. Basic Daydream is the plain one
              built into Android.
            </p>
            <div class="segmented">
              <button
                class="seg"
                class:active={ssIsBasic}
                class:busy={busy === "ss-basic"}
                aria-pressed={ssIsBasic}
                disabled={busy !== ""}
                onclick={setBasicDaydream}>Basic Daydream</button
              >
              <button
                class="seg"
                class:active={ssIsOff}
                class:busy={busy === "ss-off"}
                aria-pressed={ssIsOff}
                disabled={busy !== ""}
                onclick={screensaverOff}>Off</button
              >
            </div>
            {#if screensaverOriginal}
              <button
                class="restore-btn"
                class:busy={busy === "ss-restore"}
                disabled={busy !== "" || ssAtOriginal}
                onclick={restoreScreensaver}
              >
                <span class="msr">history</span>
                <span class="restore-copy">
                  <span>Restore previous</span>
                  <span class="mono restore-value">{screensaverLabel(screensaverOriginal.components, screensaverOriginal.enabled)}</span>
                </span>
              </button>
              <span class="t-desc">
                Previous is what was set when Tweaks first read this TV, including a vendor
                screensaver this app can't name.
              </span>
            {/if}
          </div>

          <div class="tweak-row column">
            <div class="t-row-head">
              <span class="msr t-icon" aria-hidden="true">animation</span>
              <div class="t-info">
                <span class="t-title">Animation speed</span>
                <span class="t-desc">Current: <span class="state">{animLabel()}</span></span>
              </div>
              <button
                class="reset-btn"
                class:busy={busy === "anim-reset"}
                disabled={busy !== ""}
                onclick={resetAnim}
                aria-label="Reset animation speed"
              >
                <span class="msr">restart_alt</span>Reset
              </button>
            </div>
            <p class="t-note">Sets Android's three animation speeds together. 0.5× makes menus feel quicker.</p>
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
              <span class="msr t-icon" aria-hidden="true">memory</span>
              <div class="t-info">
                <span class="t-title">Background process limit</span>
                <span class="t-desc">
                  Current: <span class="state">{cachedLimit != null ? `Up to ${cachedLimit} cached apps` : "Not reported by this TV"}</span>
                </span>
              </div>
              {#if bgLimit != null}
                <button
                  class="reset-btn"
                  class:busy={busy === "bg-remove"}
                  disabled={busy !== ""}
                  onclick={removeBgLimit}
                >
                  <span class="msr">delete</span>Remove old setting
                </button>
              {/if}
            </div>
            <p class="t-note">
              How many recent apps Android keeps in memory. This app can't change it; use Background
              process limit in the TV's Developer options.
            </p>
            {#if bgLimit != null}
              <p class="t-desc mono">global.background_process_limit = {bgLimit} (ignored by Android)</p>
            {/if}
          </div>
        {/if}
      </div>

      <span class="section-label">Network</span>
      <div class="tweak-group">
        <div class="tweak-row column">
          <div class="t-row-head">
            <span class="msr t-icon" aria-hidden="true">dns</span>
            <div class="t-info">
              <span class="t-title">Private DNS</span>
              <span class="t-desc">
                {#if dnsError}
                  Couldn't read the current mode
                {:else}
                  Current: <span class="state">{dnsMode === "off"
                    ? "Off"
                    : dnsMode === "opportunistic"
                      ? "Automatic"
                      : dnsMode === "hostname"
                        ? dns?.hostname
                          ? `Custom (${dns.hostname})`
                          : "Custom"
                        : "Unset"}</span>
                {/if}
              </span>
            </div>
          </div>
          <p class="t-note">
            Encrypts the TV's DNS lookups. Custom sends them to a server you choose, such as AdGuard
            or NextDNS. If a test lookup through it fails, the TV is switched back to Automatic so it
            stays online.
          </p>
          <div class="segmented">
            <button class="seg" class:active={dnsMode === "off"} class:busy={busy === "dns-off"} disabled={busy !== ""} onclick={() => setDns("off")}>Off</button>
            <button class="seg" class:active={dnsMode === "opportunistic"} class:busy={busy === "dns-opportunistic"} disabled={busy !== ""} onclick={() => setDns("opportunistic")}>Auto</button>
            <button class="seg" class:active={dnsMode === "hostname" || dnsEditing} disabled={busy !== ""} onclick={() => setDns("hostname")}>Custom</button>
          </div>
          {#if dnsEditing}
            <div class="dns-input-row">
              <input class="dns-input mono" bind:value={dnsHost} placeholder="dns.adguard.com" aria-label="Private DNS hostname" onkeydown={(e) => e.key === "Enter" && applyDnsHost()} />
              <button class="primary small-inline" disabled={!dnsHost.trim() || busy !== ""} onclick={applyDnsHost}>Apply</button>
            </div>
          {/if}
          {#if dnsError}
            <p class="error small">{dnsError}</p>
          {/if}
        </div>
      </div>
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

  <BottomTabs active="tweaks" {navigate} />
</div>

<style>
  .header-info {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
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
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .header-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex: none;
  }
  .header-actions .iconbtn:disabled {
    opacity: 0.5;
  }
  .topline {
    margin-bottom: 14px;
  }
  .intro {
    margin: 0 0 14px;
    font-size: 13px;
    line-height: 1.45;
    color: var(--muted);
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
  .t-desc .state.wrap {
    overflow-wrap: anywhere;
  }
  .t-note {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--muted);
  }

  .restore-btn {
    display: flex;
    align-items: center;
    gap: 10px;
    min-height: 44px;
    padding: 8px 12px;
    border: 1px solid var(--line);
    border-radius: 11px;
    background: var(--canvas);
    color: var(--text-soft);
    font-family: var(--sans);
    font-size: 12px;
    font-weight: 600;
    text-align: left;
    cursor: pointer;
  }
  .restore-btn .msr {
    font-size: 18px;
    flex: none;
  }
  .restore-btn:disabled {
    opacity: 0.55;
    cursor: default;
  }
  .restore-btn.busy {
    opacity: 0.6;
  }
  .restore-copy {
    display: flex;
    flex-direction: column;
    gap: 2px;
    min-width: 0;
  }
  .restore-value {
    font-size: 10px;
    font-weight: 400;
    color: var(--muted);
    overflow-wrap: anywhere;
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
