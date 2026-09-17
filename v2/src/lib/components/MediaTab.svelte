<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import { api } from "$lib/api";
  import type { MediaCapabilities } from "$lib/types";
  import { formatSupport, matchContentLabel, surroundLabel } from "../../../shared/media";

  let { serial, active = true, resetToken = 0 }: {
    serial: string; active?: boolean; resetToken?: number;
  } = $props();

  let caps = $state<MediaCapabilities | null>(null);
  let loading = $state(false);
  let err = $state<string | null>(null);
  let request = 0;
  let destroyed = false;

  async function load() {
    const id = ++request;
    const target = serial;
    const token = resetToken;
    const current = () => !destroyed && id === request && serial === target && resetToken === token;
    loading = true;
    err = null;
    try {
      const result = await api.mediaReport(target);
      if (current()) caps = result;
    } catch (e) {
      if (current()) err = String(e);
    } finally {
      if (current()) loading = false;
    }
  }

  $effect(() => {
    serial;
    resetToken;
    untrack(() => { ++request; caps = null; err = null; loading = false; });
  });
  $effect(() => {
    serial;
    resetToken;
    if (active) untrack(() => { void load(); });
  });
  onDestroy(() => { destroyed = true; ++request; });

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
    Device-reported codec configuration, display modes, and audio settings.
    This is not a runtime playback or hardware-acceleration test.
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
          </li>
        {/each}
      </ul>
    {/if}

    <h3>Video codec configuration</h3>
    <table class="media-table">
      <thead>
        <tr><th>Format</th><th>Reported configuration</th><th class="mime">MIME</th></tr>
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
      HDR formats reported for the current display chain, not a fixed property
      of the device. Missing information does not establish SDR-only output.
    </p>
    <p class="hdr-list">
      {#if caps.hdr_types.length}
        {#each caps.hdr_types as h (h)}<span class="pill ok">{h}</span>{/each}
      {:else}
        <span class="pill">No HDR information reported</span>
      {/if}
    </p>

    <h3>Display modes</h3>
    <p class="muted small">
      Match Content Frame Rate:
      <strong>
        {caps.match_content_frame_rate
          ? (matchContentLabel[caps.match_content_frame_rate] ?? caps.match_content_frame_rate)
          : "Default/unset"}
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
    border-radius: var(--radius-lg);
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
    border-radius: var(--radius-md);
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
    border-radius: var(--radius-pill);
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
    border-radius: var(--radius-sm);
    padding: 0.6rem 0.8rem;
    line-height: 1.6;
  }
  .audio-formats {
    margin: 0.3rem 0;
  }
</style>
