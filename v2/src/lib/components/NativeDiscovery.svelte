<script lang="ts">
  import { onMount } from "svelte";
  import type { NativeAdvertisement, NativeDiscoveryReport } from "$lib/types";

  let { report, receivedAt, busy = false, onChoose }: {
    report: NativeDiscoveryReport;
    receivedAt: number;
    busy?: boolean;
    onChoose: (advertisement: NativeAdvertisement) => void;
  } = $props();
  let now = $state(performance.now());
  let container: HTMLElement;
  const labels = { legacy_adb: "Legacy ADB", tls_pairing: "Pairing service", tls_connect: "TLS connect service" };

  function expired(advertisement: NativeAdvertisement, current = now): boolean {
    return advertisement.observed_age_ms + Math.max(0, current - receivedAt) >= report.observation_max_age_ms;
  }

  function choose(advertisement: NativeAdvertisement) {
    if (!busy && !advertisement.cached && advertisement.connect_address && !expired(advertisement, performance.now())) {
      onChoose(advertisement);
    }
  }

  function closeDetails(event: KeyboardEvent) {
    const details = document.activeElement?.closest("details");
    if (event.key !== "Escape" || !details?.open || !container?.contains(details)) return;
    event.stopPropagation();
    details.open = false;
    details.querySelector("summary")?.focus();
  }

  onMount(() => {
    const timer = setInterval(() => { now = performance.now(); }, 250);
    return () => clearInterval(timer);
  });
</script>

<svelte:window onkeydown={closeDetails} />
<section class="native-discovery" aria-labelledby="native-discovery-title" bind:this={container}>
  <h2 id="native-discovery-title">Network advertisements</h2>
  <p class="muted small">Services are unverified. Choose an address, then connect manually.</p>
  {#each report.warnings as warning}
    <p class="warning small">{warning}</p>
  {/each}
  {#if report.advertisements.length === 0}
    <p class="muted small">No ADB advertisements observed. You can still enter the TV's IP and port above.</p>
  {:else}
    <ul>
      {#each report.advertisements as advertisement}
        {@const isExpired = expired(advertisement)}
        <li>
          <div class="advertisement-heading">
            <h3>{labels[advertisement.service_kind]}</h3>
            {#if isExpired}<span class="tag">Expired observation</span>
            {:else if advertisement.cached}<span class="tag">Cached observation</span>{/if}
          </div>
          <code class="endpoint">{advertisement.connect_address ?? `${advertisement.address} · port ${advertisement.port}`}</code>
          {#if advertisement.warning}<p class="warning small">{advertisement.warning}</p>{/if}
          {#if (isExpired || advertisement.cached) && !advertisement.warning}
            <p class="muted small">Scan again or enter the address shown on the TV.</p>
          {/if}
          <div class="advertisement-actions">
            <button
              disabled={busy || isExpired || advertisement.cached || !advertisement.connect_address}
              onclick={() => choose(advertisement)}
              aria-label={`${advertisement.service_kind === "tls_pairing" ? "Use pairing address" : "Use address"} ${advertisement.connect_address ?? advertisement.address}`}
            >{advertisement.service_kind === "tls_pairing" ? "Use pairing address" : "Use address"}</button>
            <details>
              <summary>Advertisement details</summary>
              <dl>
                <dt>Service instance</dt><dd>{advertisement.instance}</dd>
                <dt>Observed</dt><dd>{Math.floor((advertisement.observed_age_ms + Math.max(0, now - receivedAt)) / 1000)} s ago</dd>
                <dt>Interface</dt><dd>{advertisement.interface_name ?? "Not reported"}{advertisement.interface_index !== null ? ` (index ${advertisement.interface_index})` : ""}</dd>
                {#if advertisement.scope_id !== null}<dt>IPv6 scope</dt><dd>{advertisement.scope_id}</dd>{/if}
              </dl>
            </details>
          </div>
        </li>
      {/each}
    </ul>
  {/if}
  <details class="discovery-help">
    <summary>Discovery help</summary>
    <p class="muted small">These are service observations, not verified device identities or connection status. The local observation age limit is not a DNS freshness guarantee. Some TVs do not advertise; Add by IP and Pair PIN remain available. Pairing and connect services use separate ports.</p>
  </details>
</section>

<style>
  .native-discovery { margin: 1rem 0 1.5rem; padding: 1rem; border: 1px solid var(--border); border-radius: var(--radius-sm); }
  h2 { font-size: 1rem; margin: 0 0 0.5rem; }
  h3 { font-size: 0.9rem; margin: 0; }
  ul { list-style: none; padding: 0; margin: 0.75rem 0; display: grid; gap: 0.75rem; }
  li { padding: 0.75rem; background: var(--bg-surface-2); border: 1px solid var(--border); border-radius: var(--radius-sm); min-width: 0; }
  .advertisement-heading, .advertisement-actions { display: flex; gap: 0.75rem; align-items: baseline; flex-wrap: wrap; }
  .endpoint { display: block; margin: 0.5rem 0; overflow-wrap: anywhere; }
  .advertisement-actions { margin-top: 0.5rem; }
  details { font-size: 0.82rem; max-width: 100%; }
  summary { cursor: pointer; width: fit-content; }
  summary:focus-visible { outline: 2px solid var(--accent); outline-offset: 3px; }
  dl { display: grid; grid-template-columns: auto minmax(0, 1fr); gap: 0.25rem 0.75rem; }
  dt { color: var(--fg-secondary); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .discovery-help { margin-top: 0.75rem; }
  p { margin: 0.5rem 0; }
</style>
