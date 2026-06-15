<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import type {
    TweaksState,
    SettingNamespace,
    DisplayScalePreset,
    CurrentDisplayScaling,
  } from "$lib/types";

  let { serial }: { serial: string } = $props();

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

  async function loadTweaks() {
    tweaksLoading = true;
    tweaksErr = null;
    try {
      const [t, s, states, perm] = await Promise.all([
        api.getTweaks(serial),
        api.getDisplayScaling(serial).catch(() => null),
        api.packageStates(serial, [NETFLIX_HOOKS_PKG]).catch(() => null),
        api.appPermissionState(serial, ASSISTANT_PKG, ASSISTANT_PERM).catch(() => null),
      ]);
      tweaks = t;
      currentDisplayScaling = s;
      netflixHooksState = states ? (states[NETFLIX_HOOKS_PKG] ?? null) : null;
      assistantState = perm ?? null;
    } catch (e) {
      tweaksErr = String(e);
    } finally {
      tweaksLoading = false;
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

  // Human-readable "current value" for each tweak — raw setting values like
  // "2" or "400" aren't self-explanatory.
  function hdmiLabel(v: string | null): string {
    return v === "1" ? "On" : v === "0" ? "Off" : "Unset";
  }
  function matchContentLabel(v: string | null): string {
    return v === "0" ? "Never" : v === "1" ? "Seamless only" : v === "2" ? "Always" : "Unset (default)";
  }
  function bgLimitLabel(v: string | null): string {
    if (!v) return "Standard";
    return v === "0" ? "None" : `At most ${v}`;
  }
  function longPressLabel(v: string | null): string {
    return v ? `${v} ms` : "Unset (default 400 ms)";
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
    tweaksActionBusy = busyId;
    tweaksActionMessage = "";
    try {
      const r = await api.writeSetting(serial, namespace, key, value);
      tweaksActionMessage = `${key} → ${value || "(default)"}: ${r.message.trim()}`;
      await loadTweaks();
    } catch (e) {
      tweaksActionMessage = `${key}: ${e}`;
    } finally {
      tweaksActionBusy = null;
    }
  }

  // Animation triple is one logical control — write all three keys in one go.
  async function setAnimationScale(scale: string) {
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
      : "device defaults";
    if (!confirm(`Apply display scaling: ${label}? The screen will reflow.`)) return;
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
    <h2>System Tweaks</h2>
    <button onclick={loadTweaks} disabled={tweaksLoading}>
      {tweaksLoading ? "Loading…" : "Refresh"}
    </button>
  </div>
  <p class="muted small">
    Flip device behaviors from v1's Display/Input Tuning menu. Most run
    <code>settings put</code> (empty value resets to default).
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
        Controls Nvidia's system hooks (<code>{NETFLIX_HOOKS_PKG}</code>) which
        remap the Xbox controller's Guide button to Home. Disabling fixes Guide
        button conflicts in Steam Link and Moonlight. <strong>Note:</strong> the
        Shield remote's dedicated Netflix button is hardwired at the firmware level
        and cannot be disabled via ADB — use a button remapper app for that.
        Reversible any time.
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
        Turning this off revokes the microphone permission from Google's search
        app (<code>{ASSISTANT_PKG}</code>), so the remote's dedicated
        Assistant/mic button stops listening. The button may still open the
        assistant UI briefly, but it won't be able to hear you.
        <strong>Trade-off:</strong> this also disables voice search in the
        Play Store. Reversible any time.
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

    <h3>HDMI-CEC</h3>
    <p class="muted small">
      Master switch plus three sub-toggles. Disabling the master typically also
      turns off the sub-controls.
    </p>
    <div class="tweak-grid">
      {#each [
        { key: "hdmi_control_enabled", label: "Master (control on/off)", value: tweaks.hdmi_control_enabled },
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
      Lets apps switch refresh rate to match video content (24/25/30/60 Hz). Seamless
      only avoids visible black flashes during the switch.
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

    <h3>Background Process Limit</h3>
    <p class="muted small">
      Caps how many apps stay alive in the background — frees RAM and can make the
      Shield feel snappier (2 is a good balance). <strong>Heads up:</strong> Android
      resets this to Standard on every reboot (a platform limitation, not a bug), so
      you'll need to re-apply it after a restart.
    </p>
    <div class="tweak-row">
      <div>
        <div class="current">Current: <strong>{bgLimitLabel(tweaks.background_process_limit)}</strong></div>
        <div class="muted small mono">global.background_process_limit = {tweaks.background_process_limit ?? "(Standard)"}</div>
      </div>
      <div class="row-actions">
        <button
          class="small-action"
          class:active={!tweaks.background_process_limit}
          disabled={tweaksActionBusy === "background_process_limit"}
          onclick={() => writeTweak("global", "background_process_limit", "", "background_process_limit")}
        >Standard</button>
        {#each [
          { v: "0", label: "None" },
          { v: "1", label: "≤ 1" },
          { v: "2", label: "≤ 2" },
          { v: "3", label: "≤ 3" },
          { v: "4", label: "≤ 4" },
        ] as opt (opt.v)}
          <button
            class="small-action"
            class:active={tweaks.background_process_limit === opt.v}
            disabled={tweaksActionBusy === "background_process_limit"}
            onclick={() => writeTweak("global", "background_process_limit", opt.v, "background_process_limit")}
          >{opt.label}</button>
        {/each}
      </div>
    </div>

    <h3>Long Press Timeout</h3>
    <p class="muted small">
      How long the remote OK button has to be held to register a long-press. Default
      is 400ms; 300ms feels snappier.
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

    <h3>UI Animations</h3>
    <p class="muted small">
      Sets all three animation scales (window / transition / animator) at once.
      0.5× is a noticeable speedup; 0× disables them entirely.
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
      Forces a specific resolution + density via <code>wm size</code> + <code>wm density</code>.
      Mostly for Shield TV — useful for testing 1080p mode on a 4K device.
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
        <span class="scale-title">{displayScaleBusy === "uhd_4k" ? "Applying…" : "Shield 4K"}</span>
        <span class="muted small">3839×2160, density 640</span>
      </button>
      <button
        class="scale-option"
        disabled={displayScaleBusy !== null}
        onclick={() => applyDisplayScaling("fhd_1080p")}
      >
        <span class="scale-title">{displayScaleBusy === "fhd_1080p" ? "Applying…" : "Shield 1080p"}</span>
        <span class="muted small">1920×1080, density 320</span>
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
  {/if}
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button, input) live in the layout and are inherited. */
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
  .card-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 1rem;
  }
  .small {
    font-size: 0.82rem;
  }
  .mono {
    font-family: ui-monospace, monospace;
  }
  .error {
    background: var(--danger-surface);
    color: var(--danger-text);
    padding: 0.7rem 1rem;
    border-radius: 6px;
    font-family: ui-monospace, monospace;
    font-size: 0.85rem;
  }
  .row-actions {
    display: flex;
    gap: 0.4rem;
    align-items: center;
    flex-wrap: wrap;
  }
  .small-action {
    padding: 0.2rem 0.6rem;
    font-size: 0.78rem;
  }
  .small-action.active {
    background: var(--accent-strong);
    color: #fff;
    border-color: var(--accent);
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
    border-radius: 4px;
    word-break: break-word;
  }
  code {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    padding: 0.1rem 0.4rem;
    border-radius: 4px;
    font-family: ui-monospace, monospace;
    font-size: 0.85em;
  }

  /* Tweaks-tab–specific styles. */
  .tweak-grid {
    display: flex;
    flex-direction: column;
    gap: 0.4rem;
    margin: 0.4rem 0 0.8rem;
  }
  .tweak-row {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 1rem;
    padding: 0.5rem 0;
    border-bottom: 1px solid var(--bg-button);
  }
  .current-scaling {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.5rem 0.7rem;
    margin: 0.4rem 0 0.6rem;
    line-height: 1.4;
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
    border-radius: 6px;
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
