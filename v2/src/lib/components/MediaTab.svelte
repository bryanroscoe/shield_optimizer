<script lang="ts">
  import { onMount } from "svelte";
  import { api } from "$lib/api";
  import type { DeviceType, MediaCapabilities, VideoFormat } from "$lib/types";

  let { serial, deviceType }: { serial: string; deviceType: DeviceType } = $props();

  let caps = $state<MediaCapabilities | null>(null);
  let loading = $state(false);
  let err = $state<string | null>(null);

  async function load() {
    loading = true;
    err = null;
    try {
      caps = await api.mediaReport(serial, deviceType);
    } catch (e) {
      err = String(e);
    } finally {
      loading = false;
    }
  }

  onMount(load);

  /// Three states, not two: "not advertised" is genuinely different from
  /// "software only", and collapsing them would hide the AV1 answer.
  function formatSupport(v: VideoFormat): { label: string; cls: string } {
    if (v.hardware) return { label: "Hardware", cls: "ok" };
    if (v.software) return { label: "Software only", cls: "warn" };
    return { label: "Not supported", cls: "bad" };
  }

  const matchContentLabel: Record<string, string> = {
    "0": "Never",
    "1": "Seamless only",
    "2": "Always",
  };

  const surroundLabel: Record<string, string> = {
    auto: "Auto",
    never: "Never",
    always: "Always",
    manual: "Manual",
    unset: "Auto (unset)",
  };

  // A mode is only worth calling out when it can carry 24p film — the whole
  // reason the full mode list is fetched instead of just the active one.
  const isFilmRate = (fps: number) => fps >= 23.9 && fps <= 24.1;
</script>

<div class="card" role="tabpanel" tabindex={0} id="tabpanel-media" aria-labelledby="tab-media">
  <div class="card-header">
    <h2>Playback</h2>
    <button onclick={load} disabled={loading}>
      {loading ? "Reading…" : "Refresh"}
    </button>
  </div>
  <p class="muted small">
    What this device can actually decode and output, read from the device itself —
    its codec list, display modes, and audio passthrough settings.
  </p>

  {#if err}
    <p class="error">{err}</p>
  {:else if !caps}
    <p class="muted">{loading ? "Reading playback capabilities…" : "No data."}</p>
  {:else}
    {#if caps.verdicts.length}
      <h3>Verdict</h3>
      <ul class="verdicts">
        {#each caps.verdicts as v (v.title)}
          <li class="verdict {v.level}">
            <div class="verdict-title">{v.title}</div>
            <div class="verdict-detail">{v.detail}</div>
            {#if v.note}
              <div class="verdict-note">
                <span class="note-tag">This device</span>
                {v.note}
              </div>
            {/if}
          </li>
        {/each}
      </ul>
    {/if}

    <h3>Video decoding</h3>
    <table class="media-table">
      <thead>
        <tr><th>Format</th><th>Decoding</th><th class="mime">MIME</th></tr>
      </thead>
      <tbody>
        {#each caps.video as v (v.mime)}
          {@const support = formatSupport(v)}
          <tr>
            <td>{v.label}</td>
            <td><span class="pill {support.cls}">{support.label}</span></td>
            <td class="muted small mono mime">{v.mime}</td>
          </tr>
        {/each}
      </tbody>
    </table>

    <h3>HDR formats</h3>
    <p class="muted small">
      What the current display chain accepts — this tracks the TV or receiver
      that is connected right now, not a fixed property of the device.
    </p>
    <p class="hdr-list">
      {#if caps.hdr_types.length}
        {#each caps.hdr_types as h (h)}<span class="pill ok">{h}</span>{/each}
      {:else}
        <span class="pill bad">SDR only</span>
      {/if}
    </p>

    <h3>Display modes</h3>
    <p class="muted small">
      Match Content Frame Rate:
      <strong>
        {caps.match_content_frame_rate
          ? (matchContentLabel[caps.match_content_frame_rate] ?? caps.match_content_frame_rate)
          : "Never (unset)"}
      </strong>
      — change it on the Tweaks tab.
    </p>
    {#if caps.modes.length}
      <table class="media-table">
        <thead>
          <tr><th>Resolution</th><th>Refresh</th><th>Notes</th></tr>
        </thead>
        <tbody>
          {#each caps.modes as m (`${m.width}x${m.height}@${m.fps}`)}
            <tr class:film={isFilmRate(m.fps)}>
              <td class="mono">{m.width}×{m.height}</td>
              <td class="mono">{m.fps.toFixed(3)} Hz</td>
              <td>
                {#if m.active}<span class="pill ok">Active</span>{/if}
                {#if isFilmRate(m.fps)}<span class="pill film-pill">24p film</span>{/if}
              </td>
            </tr>
          {/each}
        </tbody>
      </table>
    {:else}
      <p class="muted small">The device reported no display modes.</p>
    {/if}

    <h3>Audio passthrough</h3>
    <div class="audio-box">
      <div>
        Mode: <strong>{surroundLabel[caps.audio.mode] ?? caps.audio.mode}</strong>
      </div>
      {#if caps.audio.enabled_formats.length}
        <div class="audio-formats">
          {#each caps.audio.enabled_formats as f (f)}<span class="pill ok">{f}</span>{/each}
        </div>
      {/if}
      <div class="muted small mono">
        global.encoded_surround_output_enabled_formats = {caps.audio.raw_formats ?? "(unset)"}
      </div>
    </div>
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
    margin: 1.2rem 0 0.4rem;
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

  /* Media-tab–specific styles. */
  .verdicts {
    list-style: none;
    margin: 0.4rem 0 0;
    padding: 0;
    display: flex;
    flex-direction: column;
    gap: 0.5rem;
  }
  .verdict {
    border: 1px solid var(--border);
    border-left-width: 3px;
    border-radius: 6px;
    padding: 0.6rem 0.8rem;
    background: var(--bg-inset);
  }
  .verdict.good {
    border-left-color: var(--ok);
  }
  .verdict.warn {
    border-left-color: var(--warn);
  }
  .verdict.info {
    border-left-color: var(--accent);
  }
  .verdict-title {
    font-weight: 500;
    margin-bottom: 0.15rem;
  }
  .verdict-detail {
    font-size: 0.85rem;
    color: var(--fg-secondary);
    line-height: 1.45;
  }
  /* Curated knowledge is set apart from the derived detail on purpose — the
     user should be able to tell what the device said from what we know. */
  .verdict-note {
    margin-top: 0.45rem;
    padding-top: 0.45rem;
    border-top: 1px dashed var(--border);
    font-size: 0.85rem;
    color: var(--fg-secondary);
    line-height: 1.45;
  }
  .note-tag {
    display: inline-block;
    margin-right: 0.35rem;
    padding: 0.05rem 0.35rem;
    border-radius: 3px;
    background: var(--bg-button);
    border: 1px solid var(--border);
    font-size: 0.7rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--fg-secondary);
  }
  .media-table {
    width: 100%;
    border-collapse: collapse;
    margin: 0.3rem 0 0.2rem;
  }
  .media-table th {
    text-align: left;
    font-weight: 500;
    font-size: 0.78rem;
    text-transform: uppercase;
    letter-spacing: 0.03em;
    color: var(--fg-secondary);
    padding: 0.3rem 0.5rem 0.3rem 0;
    border-bottom: 1px solid var(--border);
  }
  .media-table td {
    padding: 0.4rem 0.5rem 0.4rem 0;
    border-bottom: 1px solid var(--bg-button);
    font-size: 0.9rem;
  }
  .media-table tr.film td {
    background: var(--bg-inset);
  }
  .mime {
    width: 40%;
  }
  .pill {
    display: inline-block;
    padding: 0.1rem 0.45rem;
    border-radius: 999px;
    font-size: 0.75rem;
    border: 1px solid var(--border);
    background: var(--bg-button);
    margin-right: 0.3rem;
  }
  .pill.ok {
    border-color: var(--ok);
    color: var(--ok);
  }
  .pill.warn {
    border-color: var(--warn);
    color: var(--warn);
  }
  .pill.bad {
    border-color: var(--danger-text);
    color: var(--danger-text);
  }
  .pill.film-pill {
    border-color: var(--accent);
    color: var(--accent);
  }
  .hdr-list {
    margin: 0.3rem 0 0;
  }
  .audio-box {
    background: var(--bg-inset);
    border: 1px solid var(--border);
    border-radius: 4px;
    padding: 0.6rem 0.8rem;
    line-height: 1.6;
  }
  .audio-formats {
    margin: 0.3rem 0;
  }
</style>
