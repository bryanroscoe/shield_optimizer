<script lang="ts">
  import { onMount } from 'svelte';
  import Icon from './Icon.svelte';
  import type { NativeAdvertisement, NativeDiscoveryReport } from '$lib/types';
  import { buildNativeDiscoveryRows, discoveryAddressUsable, type NativeDiscoveryRow } from '$lib/native-discovery-rows';

  let { report, receivedAt, busy = false, scanning = false, connectingAddress = null, onChoose }: {
    report: NativeDiscoveryReport | null;
    receivedAt: number;
    busy?: boolean;
    scanning?: boolean;
    connectingAddress?: string | null;
    onChoose: (advertisement: NativeAdvertisement) => void;
  } = $props();
  let now = $state(performance.now());
  let selected = $state<Record<string, string>>({});
  let container: HTMLElement;
  const rows = $derived(buildNativeDiscoveryRows(report?.advertisements ?? []));
  const protocol = { legacy_adb: 'ADB over TCP', tls_pairing: 'Wireless debugging pairing', tls_connect: 'Wireless debugging connection' };
  function endpoint(row: NativeAdvertisement): string { return row.connect_address ?? `${row.address} · port ${row.port}`; }
  function usable(row: NativeAdvertisement, time = now): boolean {
    return discoveryAddressUsable(row, time - receivedAt, report?.observation_max_age_ms ?? 0);
  }
  function choice(row: NativeDiscoveryRow): NativeAdvertisement {
    return row.addresses.find(a => endpoint(a) === selected[row.key]) ?? row.addresses.find(a => usable(a)) ?? row.addresses[0];
  }
  function choose(row: NativeDiscoveryRow) {
    const address = choice(row);
    if (!busy && usable(address, performance.now())) onChoose(address);
  }
  function closeDetails(event: KeyboardEvent) {
    const details = document.activeElement?.closest('details');
    if (event.key !== 'Escape' || !details?.open || !container?.contains(details)) return;
    event.stopPropagation(); details.open = false; details.querySelector('summary')?.focus();
  }
  onMount(() => {
    const timer = setInterval(() => { now = performance.now(); }, 250);
    return () => clearInterval(timer);
  });
</script>

<svelte:window onkeydown={closeDetails} />
<section class="native-discovery" aria-labelledby="native-discovery-title" bind:this={container}>
  <div class="discovery-topline">
    <h2 id="native-discovery-title">Find your TV</h2>
    <span class="status-pill" class:scanning={scanning} role="status">{scanning ? 'Scanning…' : 'Scan finished'}</span>
  </div>
  {#if scanning}<p class="muted small" role="status">Looking for nearby devices. Some TVs take a few seconds to respond.</p>{/if}
  {#each report?.warnings ?? [] as warning}<p class="warning small">{warning}</p>{/each}
  {#if !scanning && rows.length === 0}
    <div class="discovery-empty"><Icon name="tv" size={30} /><h3>No devices answered this scan</h3><p class="muted small">Check debugging is on, then scan again or use Add by IP.</p></div>
  {/if}
  <ul class="nearby-devices">
    {#each rows as row (row.key)}
      {@const address = choice(row)}
      {@const available = usable(address)}
      <li class="nearby-device">
        <div class="device-top">
          <span class="device-icon"><Icon name="cast" size={24} /></span>
          <div class="device-body"><h3>{row.name}</h3><span class="device-address mono">{endpoint(address)}</span>
            {#if !available}<span class="device-note">{address.cached ? 'Not seen again — scan to refresh' : address.connect_address ? 'Scan to refresh this address' : 'Address unavailable'}</span>{/if}
          </div>
          <button class="primary device-connect" disabled={busy || !available} onclick={() => choose(row)} aria-label={`${address.service_kind === 'tls_pairing' ? 'Pair with code' : 'Connect'} ${row.name} at ${endpoint(address)}`}>{connectingAddress === address.connect_address ? 'Connecting…' : address.service_kind === 'tls_pairing' ? 'Pair with code' : 'Connect'}</button>
        </div>
        <details>
          <summary>Details</summary>
          <p class="muted small">Advertised names are unverified. Connecting checks the device details.</p>
          {#if row.addresses.length > 1}
            <label>Connection address<select aria-label={`Connection address for ${row.name}`} value={endpoint(address)} onchange={event => { selected[row.key] = event.currentTarget.value; }}>
              {#each row.addresses as alternative}<option value={endpoint(alternative)} disabled={!usable(alternative)}>{endpoint(alternative)}</option>{/each}
            </select></label>
          {/if}
          <dl>
            <dt>Connection type</dt><dd>{protocol[address.service_kind]}</dd>
            <dt>Advertised service</dt><dd>{address.instance}</dd>
            <dt>Interface</dt><dd>{address.interface_name ?? 'Not reported'}{address.interface_index !== null ? ` (${address.interface_index})` : ''}</dd>
            {#if address.scope_id !== null}<dt>IPv6 scope</dt><dd>{address.scope_id}</dd>{/if}
          </dl>
          {#if address.warning}<p class="warning small">{address.warning}</p>{/if}
        </details>
      </li>
    {/each}
  </ul>
  <details class="discovery-help"><summary>Can't find your TV?</summary><p class="muted small">Some TVs do not announce themselves. Use the address shown in the TV's debugging settings. Pair with code uses the pairing dialog; connecting uses the main debugging screen's port. Names and alternate addresses here are network announcements, not verified TV identities.</p></details>
</section>

<style>
  .native-discovery { margin: 1.25rem 0; }
  .discovery-topline { display:flex; align-items:center; justify-content:space-between; gap:1rem; }
  h2 { font-size:1.15rem; margin:0; } h3 { font-size:1rem; margin:0; }
  .status-pill { padding:.25rem .65rem; border:1px solid var(--border); border-radius:999px; font-size:.78rem; color:var(--fg-secondary); }
  .status-pill.scanning { color:var(--accent); }
  .nearby-devices { list-style:none; padding:0; margin:1rem 0; display:grid; grid-template-columns:repeat(auto-fit,minmax(min(100%,25rem),1fr)); gap:.75rem; }
  .nearby-device { min-width:0; padding:1rem; border:1px solid var(--border); background:var(--bg-surface); border-radius:var(--radius-md); }
  .device-top { display:flex; align-items:center; gap:.85rem; }
  .device-icon { display:grid; place-items:center; flex:none; width:42px; height:42px; border-radius:var(--radius-md); background:var(--accent-surface); color:var(--accent); }
  .device-body { flex:1; min-width:0; } .device-body h3 { overflow-wrap:anywhere; }
  .device-address,.device-note { display:block; margin-top:.3rem; font-size:.78rem; color:var(--fg-secondary); overflow-wrap:anywhere; }
  .device-note { color:var(--fg-muted); } .device-connect { flex:none; }
  details { margin-top:.8rem; font-size:.8rem; min-width:0; } summary { cursor:pointer; width:fit-content; }
  summary:focus-visible { outline:2px solid var(--accent); outline-offset:3px; }
  dl { display:grid; grid-template-columns:auto minmax(0,1fr); gap:.3rem .7rem; } dt { color:var(--fg-secondary); } dd { margin:0; overflow-wrap:anywhere; }
  select { display:block; max-width:100%; margin-top:.4rem; } p { margin:.6rem 0; } .discovery-empty { text-align:center; padding:1.5rem; color:var(--fg-secondary); } .discovery-empty h3 { margin-top:.6rem; }
  @media(max-width:600px) { .nearby-device { padding:.8rem; } .device-top { flex-wrap:wrap; } .device-connect { margin-left:auto; } }
</style>
