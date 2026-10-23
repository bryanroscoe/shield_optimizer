<script lang="ts">
  import { formatSupport, matchContentLabel, type MediaCapabilities } from "../../../shared/media";
  import type { ResourceSample } from "../lib/types";

  let { playback, resources, playbackBusy, resourceBusy, playbackError, resourceError, connected, onReadPlayback, onSampleResources }: {
    playback: MediaCapabilities | null;
    resources: ResourceSample | null;
    playbackBusy: boolean;
    resourceBusy: boolean;
    playbackError: string;
    resourceError: string;
    connected: boolean;
    onReadPlayback: () => void;
    onSampleResources: () => void;
  } = $props();

  // Keep the report's existing unit conversion and precision unchanged.
  const rate = (value: number | null) => value == null ? "Unavailable" : `${(value / 1024).toFixed(1)} KiB/s`;
</script>

<div class="reports">
  <section class="report-card" aria-label="Playback report">
    <header class="report-header">
      <h3>Playback report</h3>
      <button class="report-action" disabled={playbackBusy || resourceBusy || !connected} onclick={onReadPlayback}>{playbackBusy ? "Reading…" : "Read playback report"}</button>
    </header>
    <p class="report-note">Advertised configuration, not a playback test. Actual decoding and passthrough depend on the app, content and connected display or receiver.</p>
    {#if playbackError}<p class="error" role="alert">{playbackError}</p>{/if}
    {#if playback}
      <section class="report-group" aria-label="Video decoders">
        <h4>Video decoders</h4>
        <dl class="report-facts">
          {#each playback.video as format (format.mime)}
            <div class="fact"><dt>{format.label}</dt><dd>{formatSupport(format).label}</dd></div>
          {/each}
        </dl>
      </section>
      <section class="report-group" aria-label="Display capabilities">
        <h4>Display</h4>
        <dl class="report-facts">
          <div class="fact"><dt>Advertised HDR</dt><dd>{playback.hdr_types.length ? playback.hdr_types.join(", ") : "Unknown / not reported"}</dd></div>
          <div class="fact"><dt>Display modes</dt><dd>
            {#if playback.modes.length}
              <ul class="mode-list" aria-label="Advertised display modes">
                {#each playback.modes as mode}
                  <li><span class="mono">{mode.width}×{mode.height} @ {mode.fps} Hz</span>{#if mode.active}<span class="active-mode">(active)</span>{/if}</li>
                {/each}
              </ul>
            {:else}Unknown / not reported{/if}
          </dd></div>
        </dl>
      </section>
      <section class="report-group" aria-label="Audio and frame-rate policy">
        <h4>Audio &amp; frame rate</h4>
        <dl class="report-facts">
          <div class="fact"><dt>Surround policy</dt><dd>{playback.audio.mode === "unset" ? "Default / unset" : playback.audio.mode}</dd></div>
          <div class="fact"><dt>Manual formats</dt><dd>{playback.audio.enabled_formats.join(", ") || "None / unset"}</dd></div>
          <div class="fact"><dt>Frame-rate policy</dt><dd>{playback.match_content_frame_rate == null ? "Default / unset" : (matchContentLabel[playback.match_content_frame_rate] ?? `Unknown (${playback.match_content_frame_rate})`)}</dd></div>
        </dl>
      </section>
      {#if playback.verdicts.length}
        <section class="report-group" aria-label="Playback interpretation">
          <h4>Interpretation</h4>
          <div class="verdict-list">
            {#each playback.verdicts as verdict}
              <article class="verdict" class:warn={verdict.level === "warn"}>
                <h5>{verdict.title}</h5><p>{verdict.detail}</p>
              </article>
            {/each}
          </div>
        </section>
      {/if}
    {/if}
  </section>

  <section class="report-card" aria-label="Resource sample">
    <header class="report-header">
      <h3>CPU &amp; network</h3>
      <button class="report-action" disabled={resourceBusy || playbackBusy || !connected} onclick={onSampleResources}>{resourceBusy ? "Sampling…" : "Sample resources"}</button>
    </header>
    <p class="report-note">One device-side sample per tap. Interfaces are shown separately because VPN traffic can also appear on the physical interface.</p>
    {#if resourceError}<p class="error" role="alert">{resourceError}</p>{/if}
    {#if resources}
      <dl class="sample-summary">
        <div><dt>CPU use</dt><dd class="mono">{resources.cpu_percent == null ? "Unavailable" : `${resources.cpu_percent.toFixed(1)}%`}</dd></div>
        <div><dt>Sample duration</dt><dd class="mono">{resources.interval_ms == null ? "Unavailable" : `${(resources.interval_ms / 1000).toFixed(2)} s`}</dd></div>
      </dl>
      {#if resources.interfaces.length}
        <section class="report-group" aria-label="Network interfaces">
          <h4>Network interfaces</h4>
          <ul class="network-list">
            {#each resources.interfaces as network (network.name)}
              <li class="network-interface" aria-label={`Interface ${network.name}`}>
                <h5 class="mono">{network.name}</h5>
                <dl class="network-rates">
                  <div><dt>Receive</dt><dd class="mono">{rate(network.rx_bytes_per_s)}</dd></div>
                  <div><dt>Send</dt><dd class="mono">{rate(network.tx_bytes_per_s)}</dd></div>
                </dl>
              </li>
            {/each}
          </ul>
        </section>
      {:else}<p class="report-note">No network counters available.</p>{/if}
    {/if}
  </section>
</div>

<style>
  .reports { margin-top: 16px; display: flex; flex-direction: column; gap: 16px; min-width: 0; }
  .report-card { padding: 16px; border: 1px solid var(--line); border-radius: 18px; background: var(--surface); display: flex; flex-direction: column; gap: 16px; min-width: 0; overflow-wrap: anywhere; }
  .report-header { display: flex; flex-direction: column; align-items: stretch; gap: 12px; }
  h3 { margin: 0; font-size: 1.0625rem; line-height: 1.35; color: var(--text); }
  .report-action { min-height: 44px; padding: 10px 14px; border: 1px solid var(--line); border-radius: 12px; background: var(--surface-2); color: var(--accent); font-family: var(--sans); font-size: .875rem; line-height: 1.5; text-align: center; cursor: pointer; overflow-wrap: anywhere; }
  .report-action:disabled { opacity: .55; cursor: default; }
  .report-action:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  .report-note, .error { margin: 0; font-size: .875rem; line-height: 1.55; }
  .report-note { color: var(--muted); }
  .report-group { border-top: 1px solid var(--line); padding-top: 14px; min-width: 0; }
  h4 { margin: 0 0 12px; font-size: .9375rem; line-height: 1.4; color: var(--text-soft); }
  .report-facts, .sample-summary, .network-rates { margin: 0; }
  .report-facts { display: grid; gap: 14px; }
  .fact { min-width: 0; }
  dt { font-size: .8125rem; line-height: 1.4; color: var(--muted); }
  dd { margin: 4px 0 0; font-size: .9375rem; line-height: 1.55; color: var(--text); overflow-wrap: anywhere; min-width: 0; }
  .mode-list, .network-list { padding: 0; margin: 0; list-style: none; }
  .mode-list { display: grid; gap: 8px; }
  .mode-list li { display: flex; flex-wrap: wrap; align-items: baseline; gap: 4px 8px; min-width: 0; }
  .mode-list .mono { min-width: 0; overflow-wrap: anywhere; }
  .active-mode { color: var(--accent); font-size: .8125rem; }
  .verdict-list { display: grid; gap: 10px; }
  .verdict { padding: 12px; border: 1px solid var(--line); border-radius: 12px; min-width: 0; }
  h5 { margin: 0; font-size: .875rem; line-height: 1.45; color: var(--text-soft); overflow-wrap: anywhere; }
  .verdict.warn { border-left: 3px solid var(--amber); }
  .verdict.warn h5 { color: var(--amber); }
  .verdict p { margin: 6px 0 0; font-size: .875rem; line-height: 1.55; color: var(--text-soft); }
  .sample-summary, .network-rates { display: grid; grid-template-columns: repeat(auto-fit, minmax(min(100%, 8rem), 1fr)); gap: 12px; }
  .sample-summary > div { background: var(--surface-2); padding: 12px; border-radius: 12px; min-width: 0; }
  .sample-summary dd { font-size: 1.125rem; }
  .network-list { display: grid; gap: 12px; }
  .network-interface { background: var(--surface-2); padding: 12px; border-radius: 12px; min-width: 0; }
  .network-rates { margin-top: 10px; }
</style>
