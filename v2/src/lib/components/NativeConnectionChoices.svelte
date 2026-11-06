<script lang="ts">
  import { onMount } from "svelte";
  import type { NativeAdvertisement, NativeDiscoveryReport } from "$lib/types";
  import { buildNativeDiscoveryRows, discoveryAddressUsable, discoveryEndpointUsable, type NativeDiscoveryRow } from "$lib/native-discovery-rows";

  let { report, receivedAt, busy, onChoose }: {
    report: NativeDiscoveryReport;
    receivedAt: number;
    busy: boolean;
    onChoose: (address: NativeAdvertisement) => void;
  } = $props();
  let now = $state(performance.now());
  let selected = $state<Record<string, string>>({});
  const rows = $derived(buildNativeDiscoveryRows(report.advertisements.filter(row => row.service_kind !== "tls_pairing")));
  const protocol = { legacy_adb: "Network debugging", tls_connect: "Wireless debugging", tls_pairing: "Pairing" };
  const endpoint = (address: NativeAdvertisement) => address.connect_address ?? `${address.address} · port ${address.port}`;
  const usable = (address: NativeAdvertisement, time = now) => discoveryAddressUsable(address, time - receivedAt, report.observation_max_age_ms);
  function choice(row: NativeDiscoveryRow): NativeAdvertisement {
    return row.addresses.find(address => endpoint(address) === selected[row.key])
      ?? row.addresses.find(address => usable(address)) ?? row.addresses[0];
  }
  function connect(row: NativeDiscoveryRow) {
    const address = choice(row);
    if (!busy && usable(address, performance.now())) onChoose(address);
  }
  onMount(() => {
    const timer = setInterval(() => { now = performance.now(); }, 250);
    return () => clearInterval(timer);
  });
</script>

{#if rows.length}
  <section class="connection-choices" aria-label="Connection addresses">
    <h3>Connection addresses</h3>
    <p class="muted small">Choose an address from the same advertised service. Names come from network announcements; connecting checks the device details.</p>
    <ul>
      {#each rows as row (row.key)}
        {@const address = choice(row)}
        {@const available = usable(address)}
        <li data-connection-service={row.addresses[0].instance}>
          <div class="connection-info">
            <strong>{row.name} · {protocol[address.service_kind]}{#if address.interface_name} · {address.interface_name}{/if}</strong>
            <span class="muted mono small">{row.addresses[0].instance}</span>
            <label>
              <select aria-label={`Connection address for ${row.addresses[0].instance} on ${address.interface_name ?? 'network'}`} value={endpoint(address)} disabled={busy} onchange={event => { selected[row.key] = event.currentTarget.value; }}>
                {#each row.addresses as alternative}
                  <option value={endpoint(alternative)} disabled={!discoveryEndpointUsable(alternative)}>{endpoint(alternative)} ({alternative.address.includes(":") ? "IPv6" : "IPv4"})</option>
                {/each}
              </select>
            </label>
            {#if !available}<span class="muted small">{!discoveryEndpointUsable(address) ? "This address cannot be used on this interface." : address.cached ? "Not seen in this scan — use Scan LAN to refresh." : "Expired — use Scan LAN to refresh these addresses."}</span>{/if}
          </div>
          <button class="row-action" disabled={busy || !available} onclick={() => connect(row)} aria-label={`Connect using ${endpoint(address)}`}>Connect</button>
        </li>
      {/each}
    </ul>
  </section>
{/if}

<style>
  .connection-choices { margin: 0.75rem 0; }
  h3 { margin: 0; font-size: 0.95rem; }
  ul { list-style: none; padding: 0; margin: 0.5rem 0; }
  li { display: flex; align-items: center; gap: 0.75rem; padding: 0.65rem 0; border-bottom: 1px solid var(--border); }
  .connection-info { display: grid; gap: 0.35rem; flex: 1; min-width: 0; overflow-wrap: anywhere; }
  select { width: 100%; max-width: 100%; min-width: 0; font-family: var(--mono); font-size: 0.8rem; }
  button { flex: none; }
</style>
