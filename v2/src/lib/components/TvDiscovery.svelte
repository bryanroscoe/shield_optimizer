<script lang="ts">
  import { onMount } from "svelte";
  import HelpTip from "$lib/components/HelpTip.svelte";
  import type { TvObservation, TvDiscoveryReport } from "$lib/types";
  import { buildTvDiscoveryRows, observationFresh, discoverySetupHost, usableObservationAddresses, selectedObservation, type TvDiscoveryRow } from "$lib/tv-discovery-rows";
  let { report, receivedAt, busy, onSetup }: {
    report: TvDiscoveryReport;
    receivedAt: number;
    busy: boolean;
    onSetup: (observation: TvObservation) => void;
  } = $props();
  let now = $state(performance.now());
  let selected = $state<Record<string, string>>({});
  const rows = $derived(buildTvDiscoveryRows(report.observations));
  $effect(() => { if (report) selected = {}; });
  const protocol = { google_cast: "Google Cast", android_tv_remote: "Android TV Remote" };
  const fresh = (address: TvObservation, time = now) => observationFresh(address, time - receivedAt, report.observation_max_age_ms);
  function choice(row: TvDiscoveryRow, time = now): TvObservation | null {
    return selectedObservation(row, selected[row.key], time - receivedAt, report.observation_max_age_ms);
  }
  onMount(() => {
    const timer = setInterval(() => { now = performance.now(); }, 250);
    return () => clearInterval(timer);
  });
</script>

{#if rows.length}
  <section class="tv-discovery" aria-label="Nearby TV names" data-no-breadcrumb>
    <HelpTip label="Nearby TV names">
      <p>Names advertised on this network. ADB availability and device identity are checked separately. Multiple announcements can belong to one TV.</p>
      <ul class="interface-details">
        {#each rows as row (row.key)}
          {@const address = row.addresses[0]}
          <li>{row.name} · {protocol[address.source]}: {address.interface_name ?? "Interface not reported"}</li>
        {/each}
      </ul>
    </HelpTip>
    <ul>
      {#each rows as row (row.key)}
        {@const chosen = choice(row)}
        {@const address = chosen ?? row.addresses[0]}
        {@const addresses = usableObservationAddresses(row, now - receivedAt, report.observation_max_age_ms)}
        <li data-connection-service={address.instance}>
          <div class="discovery-info">
            <strong>{row.name}</strong>
            <span class="muted small">{protocol[address.source]}</span>
            {#if address.model}<span class="muted small">{address.model}</span>{/if}
            {#if addresses.length > 1}
              <select aria-label={`Address for ${row.name} via ${protocol[address.source]} on ${address.interface_name ?? 'network'}`} value={selected[row.key] ?? ''} disabled={busy} onchange={event => { selected[row.key] = event.currentTarget.value; }}>
                <option value="" disabled>Choose address</option>
                {#each addresses as alternative}
                  <option value={discoverySetupHost(alternative) ?? ''}>{discoverySetupHost(alternative)}</option>
                {/each}
              </select>
            {:else}
              <span class="muted mono small">{address.address}</span>
            {/if}
            <span class="muted small">ADB availability unverified</span>
            {#if !fresh(address)}<span class="muted small">Not current — use Scan LAN to refresh.</span>{/if}
          </div>
          <button disabled={busy || !chosen} onclick={() => { const observation = choice(row, performance.now()); if (!busy && observation) onSetup(observation); }} aria-label={`Enter ADB details for ${row.name}`}>Enter ADB details</button>
        </li>
      {/each}
    </ul>
  </section>
{/if}
<style>
  .tv-discovery { margin: 0.75rem 0; flex-basis: 100%; width: 100%; min-width: 0; }
  ul { list-style: none; padding: 0; margin: 0.5rem 0; }
  li { display: flex; flex-wrap: wrap; align-items: center; gap: 0.75rem; padding: 0.65rem 0; border-bottom: 1px solid var(--border); }
  .interface-details { list-style: disc; padding-left: 1.2rem; }
  .interface-details li { display: list-item; padding: 0; border: 0; }
  .discovery-info { display: grid; gap: 0.35rem; flex: 1; min-width: 0; overflow-wrap: anywhere; }
  select { min-width: 0; max-width: 100%; width: 100%; font-family: var(--mono); }
  button { flex: none; font-size: 0.82rem; padding: 0.3rem 0.8rem; }
</style>
