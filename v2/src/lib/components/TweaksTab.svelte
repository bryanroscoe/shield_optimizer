<script lang="ts">
  import { onMount, onDestroy } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
  import { api } from "$lib/api";
  import type {
    TweaksState,
    SettingNamespace,
    DisplayScalePreset,
    CurrentDisplayScaling,
    PrivateDnsState,
  } from "$lib/types";

  let { serial, onSettingsChanged }: { serial: string; onSettingsChanged?: () => void } = $props();
  let alive = true;
  onDestroy(() => { alive = false; });

  let tweaks = $state<TweaksState | null>(null);
  let tweaksLoading = $state(false);
  let tweaksErr = $state<string | null>(null);
  let tweaksActionBusy = $state<string | null>(null);
  let tweaksActionMessage = $state<string>("");
  let displayScaleBusy = $state<DisplayScalePreset | null>(null);
  let displayScaleMessage = $state<string>("");
  let currentDisplayScaling = $state<CurrentDisplayScaling | null>(null);

  // Disabling Nvidia's system hooks package. This controls Xbox controller
  // button remapping (Guide → Home). On the 2019+ remote the dedicated
  // Netflix button is handled at the firmware/keylayout level and cannot be
  // disabled via ADB — a button remapper app is needed for that.
  const NETFLIX_HOOKS_PKG = "com.nvidia.shieldtech.hooks";
  let netflixHooksState = $state<"enabled" | "disabled" | "missing" | null>(null);
  let netflixBusy = $state(false);

  // Revoking RECORD_AUDIO from Google's search app disables the remote's
  // Assistant/mic button (#77). "missing" => the app/permission isn't here.
  const ASSISTANT_PKG = "com.google.android.katniss";
  const ASSISTANT_PERM = "android.permission.RECORD_AUDIO";
  let assistantState = $state<"granted" | "revoked" | "missing" | null>(null);
  let assistantBusy = $state(false);

  // Private DNS (DNS-over-TLS) — off / opportunistic (automatic) / hostname.
  let privateDns = $state<PrivateDnsState | null>(null);
  let dnsHostInput = $state("");
  let dnsBusy = $state(false);
  let dnsMessage = $state("");

  // Screensaver (Daydream). AOSP ships Basic Daydream on every Android TV
  // build; a vendor's own ambient mode (Ambient Mode, Glance, …) is whatever
  // was already there when the tab first loaded, so "Restore previous" can
  // always put it back without the UI having to guess its component name.
  const BASIC_DAYDREAM = "com.android.dreams.basic/com.android.dreams.basic.BasicDream";
  let screensaverOriginal = $state<string | null>(null);
  let screensaverEnabledOriginal = $state<string | null>(null);
  // Which serial `screensaverOriginal` was captured for — not reactive state,
  // just bookkeeping so a device switch (same component instance, new
  // `serial` prop) re-captures instead of reusing the previous TV's value.
  let screensaverLoadedFor: string | null = null;

  async function loadTweaks() {
    const target = serial;
    tweaksLoading = true;
    tweaksErr = null;
    try {
      const [t, s, states, perm, dns] = await Promise.all([
        api.getTweaks(serial),
        api.getDisplayScaling(serial).catch(() => null),
        api.packageStates(serial, [NETFLIX_HOOKS_PKG]).catch(() => null),
        api.appPermissionState(serial, ASSISTANT_PKG, ASSISTANT_PERM).catch(() => null),
        api.getPrivateDns(serial).catch(() => null),
      ]);
      if (!alive || serial !== target) return;
      tweaks = t;
      currentDisplayScaling = s;
      netflixHooksState = states ? (states[NETFLIX_HOOKS_PKG] ?? null) : null;
      assistantState = perm ?? null;
      privateDns = dns;
      dnsHostInput = dns?.hostname ?? "";
      // Capture the screensaver's value only once per device — every later
      // call for the same serial is a post-write refresh, and capturing
      // again would overwrite the thing "Restore previous" exists to bring
      // back.
      if (screensaverLoadedFor !== target) {
        screensaverOriginal = t.screensaver_components;
        screensaverEnabledOriginal = t.screensaver_enabled;
        screensaverLoadedFor = target;
      }
    } catch (e) {
      if (!alive || serial !== target) return;
      tweaksErr = String(e);
    } finally {
      if (alive && serial === target) tweaksLoading = false;
    }
  }

  // On = enable the hooks (Xbox Guide → Home works); Off = disable them.
  async function setNetflixButton(enabled: boolean) {
    netflixBusy = true;
    tweaksActionMessage = "";
    try {
      const r = enabled
        ? await api.enablePackage(serial, NETFLIX_HOOKS_PKG)
        : await api.disablePackage(serial, NETFLIX_HOOKS_PKG);
      tweaksActionMessage = `System hooks ${enabled ? "on" : "off"}: ${r.message.trim()}`;
      const states = await api.packageStates(serial, [NETFLIX_HOOKS_PKG]);
      netflixHooksState = states[NETFLIX_HOOKS_PKG] ?? netflixHooksState;
    } catch (e) {
      tweaksActionMessage = `System hooks: ${e}`;
    } finally {
      netflixBusy = false;
    }
  }

  // On = grant RECORD_AUDIO (Assistant button works); Off = revoke it.
  async function setAssistantButton(enabled: boolean) {
    assistantBusy = true;
    tweaksActionMessage = "";
    try {
      const r = await api.setAppPermission(serial, ASSISTANT_PKG, ASSISTANT_PERM, enabled);
      tweaksActionMessage = `Assistant button ${enabled ? "on" : "off"}: ${r.message.trim()}`;
      assistantState = await api.appPermissionState(serial, ASSISTANT_PKG, ASSISTANT_PERM);
    } catch (e) {
      tweaksActionMessage = `Assistant button: ${e}`;
    } finally {
      assistantBusy = false;
    }
  }

  // Apply a Private DNS mode. For a custom hostname the backend validates it,
  // probes resolution, and reverts to automatic if the host is dead — so a bad
  // entry can't strand the device offline.
  async function applyPrivateDns(mode: "off" | "opportunistic" | "hostname") {
    if (mode === "hostname" && !dnsHostInput.trim()) {
      dnsMessage = "Enter a hostname first (e.g. dns.adguard.com).";
      return;
    }
    dnsBusy = true;
    dnsMessage = mode === "hostname" ? "Applying and running a test lookup…" : "";
    try {
      const r = await api.setPrivateDns(serial, mode, mode === "hostname" ? dnsHostInput.trim() : null);
      dnsMessage = r.message;
      privateDns = await api.getPrivateDns(serial);
      dnsHostInput = privateDns?.hostname ?? dnsHostInput;
    } catch (e) {
      dnsMessage = `Private DNS: ${e}`;
    } finally {
      dnsBusy = false;
    }
  }

  function dnsModeLabel(mode: string | null, hostname: string | null): string {
    if (mode === "off") return "Off";
    if (mode === "opportunistic") return "Automatic";
    if (mode === "hostname") return hostname ? `Custom (${hostname})` : "Custom";
    return "Unset";
  }

  // Human-readable "current value" for each tweak — raw setting values like
  // "2" or "400" aren't self-explanatory.
  function hdmiLabel(v: string | null): string {
    return v === "1" ? "On" : v === "0" ? "Off" : "Unset";
  }
  function matchContentLabel(v: string | null): string {
    return v === "0" ? "Never" : v === "1" ? "Seamless only" : v === "2" ? "Always" : "Unset (default)";
  }
  function surroundLabel(v: string | null): string {
    return v === "0"
      ? "Auto"
      : v === "1"
        ? "Never"
        : v === "2"
          ? "Always"
          : v === "3"
            ? "Manual"
            : "Unset (Auto)";
  }

  /// `AudioFormat.ENCODING_*` values that can appear in a passthrough
  /// allow-list, in the order a receiver owner thinks about them: lossy
  /// first, then the lossless formats that are the reason to touch this.
  const SURROUND_FORMATS: { code: string; label: string }[] = [
    { code: "5", label: "Dolby Digital" },
    { code: "6", label: "Dolby Digital Plus" },
    { code: "18", label: "Atmos over DD+" },
    { code: "14", label: "Dolby TrueHD" },
    { code: "19", label: "Dolby MAT" },
    { code: "7", label: "DTS" },
    { code: "8", label: "DTS-HD" },
  ];

  function surroundFormatOn(raw: string | null, code: string): boolean {
    if (!raw) return false;
    return raw.split(",").map((c) => c.trim()).includes(code);
  }

  /// Toggling a format rewrites the whole comma-separated list. Order is
  /// normalised to SURROUND_FORMATS so the value stays stable regardless of
  /// which checkbox the user clicked first.
  async function toggleSurroundFormat(raw: string | null, code: string) {
    const current = new Set(
      (raw ?? "")
        .split(",")
        .map((c) => c.trim())
        .filter(Boolean),
    );
    if (current.has(code)) current.delete(code);
    else current.add(code);
    const ordered = SURROUND_FORMATS.filter((f) => current.has(f.code)).map((f) => f.code);
    // Codes the picker does not model must survive a toggle rather than being
    // silently dropped from the device's list.
    const unknown = [...current].filter((c) => !SURROUND_FORMATS.some((f) => f.code === c));
    await writeTweak(
      "global",
      "encoded_surround_output_enabled_formats",
      [...ordered, ...unknown].join(","),
      "encoded_surround_output_enabled_formats",
    );
  }

  function longPressLabel(v: string | null): string {
    return v ? `${v} ms` : "Unset (default 400 ms)";
  }
  function screensaverLabel(v: string | null, enabled: string | null = "1"): string {
    if (enabled === "0") return "Off";
    if (!v) return "None";
    if (v === BASIC_DAYDREAM) return "Basic Daydream";
    return v;
  }
  function animationsLabel(t: TweaksState): string {
    const w = t.window_animation_scale;
    const same = w === t.transition_animation_scale && w === t.animator_duration_scale;
    if (!same) return `mixed (${w ?? "?"} / ${t.transition_animation_scale ?? "?"} / ${t.animator_duration_scale ?? "?"})`;
    return w === "0" ? "Off" : w === "0.5" ? "Fast (0.5×)" : w === "1" ? "Default (1×)" : w ? `${w}×` : "Unset (default)";
  }

  // Write a setting, then refresh the on-screen state for that key by
  // re-pulling all tweaks. Cheap (one batched shell call).
  async function writeTweak(
    namespace: SettingNamespace,
    key: string,
    value: string,
    busyId: string,
  ) {
    if (tweaksActionBusy !== null) return;
    const target = serial;
    tweaksActionBusy = busyId;
    tweaksActionMessage = "";
    try {
      const r = await api.writeSetting(target, namespace, key, value);
      if (!alive || serial !== target) return;
      tweaksActionMessage = `${key} → ${value || "(default)"}: ${r.message.trim()}`;
    } catch (e) {
      if (!alive || serial !== target) return;
      tweaksActionMessage = `${key}: ${e}`;
    } finally {
      if (alive && serial === target) {
        onSettingsChanged?.();
        await loadTweaks();
        tweaksActionBusy = null;
      }
    }
  }

  // Screensaver is two settings acting as one control: screensaver_enabled
  // gates whether Daydream runs at all, and screensaver_components picks
  // which one. Clearing the component alone can leave a framework/vendor
  // default Daydream running while `enabled` stays on, so "None" has to turn
  // that off too — not just blank the component.
  async function writeScreensaver(component: string, enabled: string) {
    if (tweaksActionBusy !== null) return;
    const target = serial;
    tweaksActionBusy = "screensaver_components";
    tweaksActionMessage = "";
    try {
      const enabledResult = await api.writeSetting(target, "secure", "screensaver_enabled", enabled);
      if (!alive || serial !== target) return;
      // A rejected flag write resolves, it does not throw. Changing the
      // component anyway could leave Daydream enabled with no component —
      // the state that lets a vendor fallback screensaver run under "None".
      if (!enabledResult.ok) {
        tweaksActionMessage =
          `screensaver_enabled → ${enabled || "(default)"} was refused: ` +
          `${enabledResult.message.trim()}. The screensaver was left unchanged.`;
        return;
      }
      const componentResult = await api.writeSetting(target, "secure", "screensaver_components", component);
      if (!alive || serial !== target) return;
      tweaksActionMessage =
        `screensaver_enabled → ${enabled || "(default)"}: ${enabledResult.message.trim()}; ` +
        `screensaver_components → ${component || "(default)"}: ${componentResult.message.trim()}`;
    } catch (e) {
      if (!alive || serial !== target) return;
      tweaksActionMessage = `screensaver: ${e}`;
    } finally {
      if (alive && serial === target) {
        onSettingsChanged?.();
        await loadTweaks();
        tweaksActionBusy = null;
      }
    }
  }

  // Animation triple is one logical control — write all three keys in one go.
  async function setAnimationScale(scale: string) {
    if (tweaksActionBusy !== null) return;
    tweaksActionBusy = "animations";
    tweaksActionMessage = "";
    try {
      const keys = ["window_animation_scale", "transition_animation_scale", "animator_duration_scale"];
      const results = await Promise.all(
        keys.map((k) => api.writeSetting(serial, "global", k, scale)),
      );
      const failed = results.filter((r) => !r.ok);
      tweaksActionMessage =
        failed.length === 0
          ? `Animations → ${scale || "default"}`
          : `Animations partially failed (${failed.length}/3): ${failed.map((r) => r.message).join("; ")}`;
      await loadTweaks();
    } catch (e) {
      tweaksActionMessage = `Animations: ${e}`;
    } finally {
      tweaksActionBusy = null;
    }
  }

  async function applyDisplayScaling(preset: DisplayScalePreset) {
    const label = preset === "uhd_4k" ? "4K (3839x2160, density 640)"
      : preset === "fhd_1080p" ? "1080p (1920x1080, density 320)"
      : preset === "hd_720p" ? "720p (1280x720, density 213)"
      : "device defaults";
    if (!confirm(`Change display scaling to ${label}? The screen will redraw.`)) return;
    displayScaleBusy = preset;
    displayScaleMessage = "";
    try {
      const r = await api.setDisplayScaling(serial, preset);
      displayScaleMessage = r.message.trim() || (r.ok ? "ok" : "no output");
      // Refresh the displayed current values.
      currentDisplayScaling = await api.getDisplayScaling(serial).catch(() => currentDisplayScaling);
    } catch (e) {
      displayScaleMessage = String(e);
    } finally {
      displayScaleBusy = null;
    }
  }

  onMount(loadTweaks);
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-tweaks" aria-labelledby="tab-tweaks">
  <div class="card-header">
    <h2><Icon name="tune" size={20} /> System Tweaks</h2>
    <button onclick={loadTweaks} disabled={tweaksLoading}>
      {tweaksLoading ? "Loading…" : "Refresh"}
    </button>
  </div>
  <p class="muted small">
    Change TV settings that Android hides or buries. Reset puts a setting back
    to the TV's default.
  </p>
  {#if tweaksErr}
    <div class="error">{tweaksErr}</div>
  {:else if !tweaks}
    <div class="muted">{tweaksLoading ? "Querying…" : "—"}</div>
  {:else}
    {#if tweaksActionMessage}
      <p class="muted small mono action-message">{tweaksActionMessage}</p>
    {/if}

    {#if netflixHooksState && netflixHooksState !== "missing"}
      <h3>Nvidia System Hooks</h3>
      <p class="muted small">
        Controls Nvidia's system hooks (<code>{NETFLIX_HOOKS_PKG}</code>), which
        turn the Xbox controller's Guide button into Home. Turning them off can
        fix Guide button conflicts in Steam Link and Moonlight.
        <strong>Note:</strong> this does not change the Netflix button on the
        Shield remote. That button is handled in firmware and can't be changed
        over ADB, so use a button remapper app for it. You can turn this back on
        at any time.
      </p>
      <div class="tweak-row">
        <div>
          <div class="current">Current: <strong>{netflixHooksState === "disabled" ? "hooks disabled" : "hooks active"}</strong></div>
          <div class="muted small mono">{NETFLIX_HOOKS_PKG} = {netflixHooksState}</div>
        </div>
        <div class="row-actions">
          <button
            class="small-action"
            class:active={netflixHooksState === "enabled"}
            disabled={netflixBusy}
            onclick={() => setNetflixButton(true)}
          >On</button>
          <button
            class="small-action"
            class:active={netflixHooksState === "disabled"}
            disabled={netflixBusy}
            onclick={() => setNetflixButton(false)}
          >Off</button>
        </div>
      </div>
    {/if}

    {#if assistantState && assistantState !== "missing"}
      <h3>Remote Assistant Button</h3>
      <p class="muted small">
        Turning this off removes the microphone permission from Google's search
        app (<code>{ASSISTANT_PKG}</code>), so the remote's Assistant button
        can't hear you. The button may still open the assistant for a moment.
        <strong>Trade-off:</strong> this also turns off voice search in the Play
        Store. You can turn it back on at any time.
      </p>
      <div class="tweak-row">
        <div>
          <div class="current">Current: <strong>{assistantState === "revoked" ? "mic revoked" : "mic active"}</strong></div>
          <div class="muted small mono">{ASSISTANT_PKG} mic = {assistantState}</div>
        </div>
        <div class="row-actions">
          <button
            class="small-action"
            class:active={assistantState === "granted"}
            disabled={assistantBusy}
            onclick={() => setAssistantButton(true)}
          >On</button>
          <button
            class="small-action"
            class:active={assistantState === "revoked"}
            disabled={assistantBusy}
            onclick={() => setAssistantButton(false)}
          >Off</button>
        </div>
      </div>
    {/if}

    {#if privateDns}
      <h3>Private DNS (DNS-over-TLS)</h3>
      <p class="muted small">
        Encrypts the TV's DNS lookups. <strong>Automatic</strong> uses encrypted
        DNS when the network offers it. <strong>Custom</strong> sends lookups to a
        server you choose, such as AdGuard, NextDNS or Cloudflare. If a test
        lookup through a custom server fails, the app switches back to Automatic
        so the TV stays online.
      </p>
      <div class="tweak-row">
        <div>
          <div class="current">Current: <strong>{dnsModeLabel(privateDns.mode, privateDns.hostname)}</strong></div>
          <div class="muted small mono">private_dns_mode = {privateDns.mode ?? "unset"}</div>
        </div>
        <div class="row-actions">
          <button
            class="small-action"
            class:active={privateDns.mode === "off"}
            disabled={dnsBusy}
            onclick={() => applyPrivateDns("off")}
          >Off</button>
          <button
            class="small-action"
            class:active={privateDns.mode === "opportunistic"}
            disabled={dnsBusy}
            onclick={() => applyPrivateDns("opportunistic")}
          >Automatic</button>
        </div>
      </div>
      <div class="dns-custom">
        <input
          type="text"
          placeholder="dns.adguard.com"
          bind:value={dnsHostInput}
          disabled={dnsBusy}
          onkeydown={(e) => e.key === "Enter" && applyPrivateDns("hostname")}
        />
        <button
          class="small-action"
          class:active={privateDns.mode === "hostname"}
          disabled={dnsBusy}
          onclick={() => applyPrivateDns("hostname")}
        >Use custom</button>
      </div>
      {#if dnsMessage}
        <p class="muted small mono action-message">{dnsMessage}</p>
      {/if}
    {/if}

    <h3>HDMI-CEC</h3>
    <p class="muted small">
      Lets the TV and this box control each other over HDMI. Turning off the
      main switch usually turns off the other three as well.
    </p>
    <div class="tweak-grid">
      {#each [
        { key: "hdmi_control_enabled", label: "Main switch", value: tweaks.hdmi_control_enabled },
        { key: "hdmi_control_auto_wakeup_enabled", label: "Auto wake on TV power", value: tweaks.hdmi_control_auto_wakeup_enabled },
        { key: "hdmi_control_auto_device_off_enabled", label: "Auto sleep when TV off", value: tweaks.hdmi_control_auto_device_off_enabled },
        { key: "hdmi_system_audio_control_enabled", label: "System audio control", value: tweaks.hdmi_system_audio_control_enabled },
      ] as row (row.key)}
        <div class="tweak-row">
          <div>
            <div>{row.label}</div>
            <div class="current">Current: <strong>{hdmiLabel(row.value)}</strong></div>
            <div class="muted small mono">global.{row.key} = {row.value ?? "(unset)"}</div>
          </div>
          <div class="row-actions">
            <button
              class="small-action"
              class:active={row.value === "1"}
              disabled={tweaksActionBusy === row.key}
              onclick={() => writeTweak("global", row.key, "1", row.key)}
            >On</button>
            <button
              class="small-action"
              class:active={row.value === "0"}
              disabled={tweaksActionBusy === row.key}
              onclick={() => writeTweak("global", row.key, "0", row.key)}
            >Off</button>
            <button
              class="small-action"
              disabled={tweaksActionBusy === row.key}
              onclick={() => writeTweak("global", row.key, "", row.key)}
            >Reset</button>
          </div>
        </div>
      {/each}
    </div>

    <h3>Match Content Frame Rate</h3>
    <p class="muted small">
      Lets apps change the TV's refresh rate to match the video, such as 24 Hz
      for films. Seamless only switches when the TV can do it without a black
      screen. The app and the TV both have to support it.
    </p>
    <div class="tweak-row">
      <div>
        <div class="current">Current: <strong>{matchContentLabel(tweaks.match_content_frame_rate)}</strong></div>
        <div class="muted small mono">secure.match_content_frame_rate = {tweaks.match_content_frame_rate ?? "(unset)"}</div>
      </div>
      <div class="row-actions">
        {#each [
          { v: "0", label: "Never" },
          { v: "1", label: "Seamless only" },
          { v: "2", label: "Always" },
        ] as opt (opt.v)}
          <button
            class="small-action"
            class:active={tweaks.match_content_frame_rate === opt.v}
            disabled={tweaksActionBusy === "match_content_frame_rate"}
            onclick={() => writeTweak("secure", "match_content_frame_rate", opt.v, "match_content_frame_rate")}
          >{opt.label}</button>
        {/each}
        <button
          class="small-action"
          disabled={tweaksActionBusy === "match_content_frame_rate"}
          onclick={() => writeTweak("secure", "match_content_frame_rate", "", "match_content_frame_rate")}
        >Reset</button>
      </div>
    </div>

    <h3>Audio Passthrough</h3>
    <p class="muted small">
      Controls Android's encoded surround format policy. Auto uses the connected
      equipment's advertised formats. Manual overrides can cause silence on
      unsupported equipment. Playback also depends on the app and the audio path.
    </p>
    <div class="tweak-row">
      <div>
        <div class="current">Current: <strong>{surroundLabel(tweaks.encoded_surround_output)}</strong></div>
        <div class="muted small mono">
          global.encoded_surround_output = {tweaks.encoded_surround_output ?? "(unset)"}
        </div>
      </div>
      <div class="row-actions">
        {#each [
          { v: "0", label: "Auto" },
          { v: "1", label: "Never" },
          { v: "2", label: "Always" },
          { v: "3", label: "Manual" },
        ] as opt (opt.v)}
          <button
            class="small-action"
            class:active={tweaks.encoded_surround_output === opt.v}
            disabled={tweaksActionBusy !== null || tweaksLoading}
            onclick={() => writeTweak("global", "encoded_surround_output", opt.v, "encoded_surround_output")}
          >{opt.label}</button>
        {/each}
        <button
          class="small-action"
          disabled={tweaksActionBusy !== null || tweaksLoading}
          onclick={() => writeTweak("global", "encoded_surround_output", "", "encoded_surround_output")}
        >Reset</button>
      </div>
    </div>
    {#if tweaks.encoded_surround_output === "3"}
      <div class="surround-formats">
        <p class="muted small">
          Formats allowed by Android's Manual policy. This does not guarantee
          passthrough or determine how an app handles other formats.
        </p>
        <div class="row-actions">
          {#each SURROUND_FORMATS as f (f.code)}
            <button
              class="small-action"
              class:active={surroundFormatOn(tweaks.encoded_surround_output_enabled_formats, f.code)}
              disabled={tweaksActionBusy !== null || tweaksLoading}
              onclick={() =>
                toggleSurroundFormat(tweaks?.encoded_surround_output_enabled_formats ?? null, f.code)}
            >{f.label}</button>
          {/each}
        </div>
        <div class="muted small mono">
          global.encoded_surround_output_enabled_formats =
          {tweaks.encoded_surround_output_enabled_formats ?? "(empty)"}
        </div>
      </div>
    {/if}

    <h3>Background Process Limit</h3>
    <p class="muted small">
      Android keeps recently used apps in memory so they open faster. This shows
      how many it will keep. This app can't change that number. Earlier versions
      wrote a setting for it, but a Shield TV ignored that setting and kept its
      limit at 32.
    </p>
    <p class="muted small">
      To set a limit, open Developer options on the TV and use Background process
      limit. In standard Android, that choice is cleared when the TV restarts.
    </p>
    <div class="tweak-row">
      <div>
        <div class="current">
          Current limit:
          <strong>
            {tweaks.cached_process_limit !== null
              ? `Up to ${tweaks.cached_process_limit} cached apps`
              : "Not reported by this TV"}
          </strong>
        </div>
        <div class="muted small mono">CUR_MAX_CACHED_PROCESSES = {tweaks.cached_process_limit ?? "(not reported)"}</div>
        {#if tweaks.background_process_limit !== null}
          <div class="muted small mono">global.background_process_limit = {tweaks.background_process_limit} (ignored by Android)</div>
        {/if}
      </div>
      {#if tweaks.background_process_limit !== null}
        <div class="row-actions">
          <button
            class="small-action"
            disabled={tweaksActionBusy === "background_process_limit"}
            onclick={() => writeTweak("global", "background_process_limit", "", "background_process_limit")}
          >Remove old setting</button>
        </div>
      {/if}
    </div>

    <h3>Long Press Timeout</h3>
    <p class="muted small">
      How long you hold OK on the remote before it counts as a long press. The
      default is 400 ms. 300 ms reacts sooner but makes accidental long presses
      more likely.
    </p>
    <div class="tweak-row">
      <div>
        <div class="current">Current: <strong>{longPressLabel(tweaks.long_press_timeout)}</strong></div>
        <div class="muted small mono">secure.long_press_timeout = {tweaks.long_press_timeout ?? "(unset)"}</div>
      </div>
      <div class="row-actions">
        {#each ["300", "400", "500"] as v (v)}
          <button
            class="small-action"
            class:active={tweaks.long_press_timeout === v}
            disabled={tweaksActionBusy === "long_press_timeout"}
            onclick={() => writeTweak("secure", "long_press_timeout", v, "long_press_timeout")}
          >{v} ms</button>
        {/each}
        <button
          class="small-action"
          disabled={tweaksActionBusy === "long_press_timeout"}
          onclick={() => writeTweak("secure", "long_press_timeout", "", "long_press_timeout")}
        >Reset</button>
      </div>
    </div>

    <h3>Screensaver</h3>
    <p class="muted small">
      Which screensaver (Daydream) runs when the TV is idle. Basic Daydream is
      the plain screensaver built into Android. Restore previous puts back the
      screensaver that was set when this tab opened, including a vendor one this
      app can't name.
    </p>
    <div class="tweak-row">
      <div>
        <div class="current">Current: <strong>{screensaverLabel(tweaks.screensaver_components, tweaks.screensaver_enabled)}</strong></div>
        <div class="muted small mono">secure.screensaver_components = {tweaks.screensaver_components ?? "(unset)"}, secure.screensaver_enabled = {tweaks.screensaver_enabled ?? "(unset)"}</div>
      </div>
      <div class="row-actions">
        <button
          class="small-action"
          class:active={tweaks.screensaver_enabled !== "0" && tweaks.screensaver_components === BASIC_DAYDREAM}
          disabled={tweaksActionBusy === "screensaver_components"}
          onclick={() => writeScreensaver(BASIC_DAYDREAM, "1")}
        >Basic Daydream</button>
        <button
          class="small-action"
          disabled={tweaksActionBusy === "screensaver_components"}
          onclick={() => writeScreensaver(screensaverOriginal ?? "", screensaverEnabledOriginal ?? "1")}
        >Restore previous ({screensaverLabel(screensaverOriginal, screensaverEnabledOriginal)})</button>
        <button
          class="small-action"
          class:active={tweaks.screensaver_enabled === "0"}
          disabled={tweaksActionBusy === "screensaver_components"}
          onclick={() => writeScreensaver("", "0")}
        >None</button>
      </div>
    </div>

    <h3>UI Animations</h3>
    <p class="muted small">
      Sets Android's three animation speeds together. 0.5× makes menus feel
      quicker. Off removes the animations.
    </p>
    <div class="tweak-row">
      <div>
        <div class="current">Current: <strong>{animationsLabel(tweaks)}</strong></div>
        <div class="muted small mono">
          window = {tweaks.window_animation_scale ?? "(unset)"} /
          transition = {tweaks.transition_animation_scale ?? "(unset)"} /
          animator = {tweaks.animator_duration_scale ?? "(unset)"}
        </div>
      </div>
      <div class="row-actions">
        {#each [
          { v: "0", label: "Off" },
          { v: "0.5", label: "Fast (0.5×)" },
          { v: "1", label: "Default (1×)" },
        ] as opt (opt.v)}
          <button
            class="small-action"
            class:active={tweaks.window_animation_scale === opt.v && tweaks.transition_animation_scale === opt.v && tweaks.animator_duration_scale === opt.v}
            disabled={tweaksActionBusy === "animations"}
            onclick={() => setAnimationScale(opt.v)}
          >{opt.label}</button>
        {/each}
        <button
          class="small-action"
          disabled={tweaksActionBusy === "animations"}
          onclick={() => setAnimationScale("")}
        >Reset</button>
      </div>
    </div>

    <h3>Display Scaling</h3>
    <p class="muted small">
      Changes the resolution Android draws its menus at, using
      <code>wm size</code> and <code>wm density</code>. The TV still gets its
      usual output resolution, and the menus are scaled up to fit.
    </p>
    <p class="muted small">
      <strong>Why lower it:</strong> at 4K, Android draws four times as many
      pixels as at 1080p. Drawing menus at 1080p puts less load on the graphics
      chip and memory, but text and icons look softer. Video players decode at
      the video's own resolution, so playback should look the same.
      <strong>Why raise it:</strong> to put it back. Density sets how large
      things are drawn. Each preset pairs a resolution with a density that keeps
      things about the same size.
    </p>
    {#if currentDisplayScaling}
      <div class="current-scaling muted small mono">
        {currentDisplayScaling.size || "Size: unknown"}
        <br />
        {currentDisplayScaling.density || "Density: unknown"}
      </div>
    {/if}
    <div class="scale-options">
      <button
        class="scale-option"
        disabled={displayScaleBusy !== null}
        onclick={() => applyDisplayScaling("uhd_4k")}
      >
        <span class="scale-title">{displayScaleBusy === "uhd_4k" ? "Applying…" : "4K"}</span>
        <span class="muted small">3839×2160, density 640. Shield rejects 3840.</span>
      </button>
      <button
        class="scale-option"
        disabled={displayScaleBusy !== null}
        onclick={() => applyDisplayScaling("fhd_1080p")}
      >
        <span class="scale-title">{displayScaleBusy === "fhd_1080p" ? "Applying…" : "1080p"}</span>
        <span class="muted small">1920×1080, density 320. A quarter of 4K's pixels.</span>
      </button>
      <button
        class="scale-option"
        disabled={displayScaleBusy !== null}
        onclick={() => applyDisplayScaling("hd_720p")}
      >
        <span class="scale-title">{displayScaleBusy === "hd_720p" ? "Applying…" : "720p"}</span>
        <span class="muted small">1280×720, density 213. Lightest, but menus look soft.</span>
      </button>
      <button
        class="scale-option"
        disabled={displayScaleBusy !== null}
        onclick={() => applyDisplayScaling("reset")}
      >
        <span class="scale-title">{displayScaleBusy === "reset" ? "Resetting…" : "Reset"}</span>
        <span class="muted small">Restore device defaults</span>
      </button>
    </div>
    {#if displayScaleMessage}
      <p class="muted small mono action-message">{displayScaleMessage}</p>
    {/if}
    <div class="callout callout-warn scale-note">
      <Icon name="warning" size={16} />
      <span>
        A size or density the TV doesn't handle well can make menus unreadable
        or push the launcher off screen. Restarting the TV does not undo it. Use
        Reset here, or run <code>wm size reset</code> in the Shell tab.
      </span>
    </div>
  {/if}
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button, input) live in the layout and are inherited. */
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: var(--mono);
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: var(--radius-md);
    font-family: var(--mono);
    font-size: 0.85rem;
  }
  .row-actions {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
  }
  /* A two-choice setting reads as a segmented control: one recessed trough,
     the chosen segment filled. Kept as two explicit buttons rather than a
     single switch — for a system setting, saying which state you want is
     better than flipping an unlabelled toggle. */
  .tweak-row .row-actions {
    display: inline-flex;
    gap: 2px;
    padding: 3px;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .tweak-row .row-actions .small-action {
    border: 1px solid transparent;
    background: none;
    border-radius: calc(var(--radius-md) - 3px);
    color: var(--fg-muted);
    min-width: 3rem;
  }
  .tweak-row .row-actions .small-action:hover:not(.active):not(:disabled) {
    background: var(--bg-button-hover);
    color: var(--fg-primary);
  }
  /* This must out-specify `.tweak-row .row-actions .small-action` above, which
     is (0,3,0) and blanks the background. A bare `.small-action.active` is
     only (0,2,0), so it lost — and every segmented control in Tweaks rendered
     with neither side selected, making live settings look inert. */
  .tweak-row .row-actions .small-action.active {
    background: var(--accent-strong);
    border-color: var(--accent);
    color: var(--accent-ink);
    font-weight: 600;
  }
  .tweak-row .row-actions .small-action.active:disabled {
    opacity: 1;
  }
  .small-action {
    padding: 0.25rem 0.7rem;
    font-size: 0.78rem;
  }
  .small-action.active {
    background: var(--accent-strong);
    color: var(--accent-ink);
    border-color: var(--accent);
  }
  .dns-custom {
    display: flex;
    gap: 0.5rem;
    align-items: center;
    margin-top: 0.5rem;
    flex-wrap: wrap;
  }
  .dns-custom input {
    flex: 1;
    min-width: 200px;
    max-width: 320px;
  }
  .current {
    font-size: 0.85rem;
    color: var(--fg-secondary);
  }
  .current strong {
    color: var(--fg-primary);
  }
  .action-message {
    margin-top: 0.4rem;
    padding: 0.4rem 0.6rem;
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    word-break: break-word;
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: var(--radius-sm);
    font-family: var(--mono);
    font-size: 0.85em;
  }

  /* Tweaks-tab–specific styles. */
  .tweak-grid {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin: 0.4rem 0 0.8rem;
  }
  /* Each control is its own object rather than a hairline-separated line in
     a long scroll — the board's settings rows. Eleven sections on one page
     need the grouping more than they need the density. */
  .tweak-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.7rem 0.9rem;
    background: var(--bg-surface-2);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
  }
  .tweak-row + .tweak-row {
    margin-top: 0.4rem;
  }
  .surround-formats {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0.5rem 0.7rem;
    margin: 0.4rem 0 0.8rem;
    line-height: 1.5;
  }
  .current-scaling {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: var(--radius-sm);
    padding: 0.5rem 0.7rem;
    margin: 0.4rem 0 0.6rem;
    line-height: 1.4;
  }
  .scale-note {
    margin-top: 0.8rem;
  }
  .scale-options {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(180px, 1fr));
    gap: 0.5rem;
    margin: 0.4rem 0;
  }
  .scale-option {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    text-align: left;
    padding: 0.6rem 0.8rem;
    gap: 0.2rem;
    background: var(--bg-button);
    border: 1px solid var(--border);
    border-radius: var(--radius-md);
    cursor: pointer;
  }
  .scale-option:hover:not(:disabled) {
    background: var(--border);
  }
  .scale-option .scale-title {
    font-weight: 500;
    font-size: 0.92rem;
  }
</style>
