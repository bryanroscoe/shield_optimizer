<script lang="ts">
  import { onDestroy, untrack } from "svelte";
  import Icon from "$lib/components/Icon.svelte";
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
    <div class="header-title">
      <h2><Icon name="play_circle" size={17} /> Playback</h2>
      <p class="muted small mono header-sub">decoders &amp; formats reported by the device</p>
    </div>
    <button onclick={load} disabled={loading}>
      <Icon name="refresh" size={15} /> {loading ? "Reading…" : "Re-probe"}
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

    <div class="media-layout">
    <div class="media-main">
    <p class="rail-label">Video codecs</p>
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

    <p class="rail-label">Display modes</p>
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

    </div>

    <aside class="media-rail">
      <p class="rail-label">HDR formats</p>
      <div class="rail-list">
        {#if caps.hdr_types.length}
          {#each caps.hdr_types as h (h)}
            <div class="rail-item"><span>{h}</span><span class="pill ok">Reported</span></div>
          {/each}
        {:else}
          <div class="rail-item muted"><span>No HDR information reported</span></div>
        {/if}
      </div>
      <p class="muted small rail-note">
        Reported for the current display chain, not a fixed property of the device.
        Missing information does not establish SDR-only output.
      </p>

      <p class="rail-label">Audio formats</p>
      <div class="rail-list">
        <div class="rail-item">
          <span>Passthrough mode</span>
          <span class="pill">{surroundLabel[caps.audio.mode] ?? caps.audio.mode}</span>
        </div>
        {#each caps.audio.enabled_formats as f (f)}
          <div class="rail-item"><span>{f}</span><span class="pill ok">Enabled</span></div>
        {/each}
      </div>
      <p class="muted small mono rail-note">
        global.encoded_surround_output_enabled_formats = {caps.audio.raw_formats ?? "(unset)"}
      </p>

      <!-- Two verdict vocabularies meet on this screen and they mean different
           things. Saying so once here is cheaper than a reader concluding that
           a SOFTWARE decoder is a package that is unsafe to remove. -->
      <div class="callout media-note">
        <Icon name="info" size={16} />
        <span>
          These verdicts describe the decode path only. They are unrelated to the
          four package safety verdicts used on the App List and Optimize.
        </span>
      </div>
    </aside>
    </div>
  {/if}
</div>

<style>
  /* Shared scoped utilities duplicated from the page; global rules
     (.muted, button, input) live in the layout and are inherited. */
  .small {
    font-size: 0.82rem;
  }
  .header-title {
    display: flex;
    flex-direction: column;
    gap: 0.1rem;
    min-width: 0;
  }
  .header-title h2 {
    margin: 0;
  }
  .header-sub {
    margin: 0;
  }
  /* Codecs and modes on the left, the format lists on the right — board 11.3.
     Stacked, the audio section was four scrolls below the codec table it is
     usually read against. */
  .media-layout {
    display: grid;
    grid-template-columns: minmax(0, 1fr) minmax(0, 24rem);
    gap: 1.5rem;
    align-items: start;
    margin-top: 1rem;
  }
  .media-main,
  .media-rail {
    min-width: 0;
  }
  .rail-label {
    margin: 1.2rem 0 0.5rem;
    font-family: var(--mono);
    font-size: 0.72rem;
    text-transform: uppercase;
    letter-spacing: 0.08em;
    color: var(--fg-muted);
  }
  .media-main > .rail-label:first-child,
  .media-rail > .rail-label:first-child {
    margin-top: 0;
  }
  .rail-list {
    border: 1px solid var(--border);
    border-radius: var(--radius-lg);
    overflow: hidden;
  }
  .rail-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 0.75rem;
    padding: 0.6rem 0.8rem;
    background: var(--bg-surface-2);
    border-bottom: 1px solid var(--border);
    font-size: 0.88rem;
  }
  .rail-item:last-child {
    border-bottom: none;
  }
  .rail-note {
    margin: 0.5rem 0 0;
    overflow-wrap: anywhere;
  }
  .media-note {
    margin-top: 1.2rem;
  }
  @media (max-width: 1100px) {
    .media-layout {
      grid-template-columns: minmax(0, 1fr);
    }
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
    border-bottom: 1px solid var(--border);
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
