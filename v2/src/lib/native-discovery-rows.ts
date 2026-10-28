import type { NativeAdvertisement } from './types';

export interface NativeDiscoveryRow {
  key: string;
  name: string;
  addresses: NativeAdvertisement[];
}
const suffixes = {
  legacy_adb: '._adb._tcp.local.',
  tls_pairing: '._adb-tls-pairing._tcp.local.',
  tls_connect: '._adb-tls-connect._tcp.local.',
};

export function advertisedName(row: NativeAdvertisement): string {
  const suffix = suffixes[row.service_kind];
  const prefix = row.instance.toLowerCase().endsWith(suffix)
    ? row.instance.slice(0, -suffix.length).replace(/\\032/g, ' ').trim()
    : '';
  if (!prefix || /^adb[-_]/i.test(prefix) || /^[a-f\d-]{12,}$/i.test(prefix)) return 'Nearby device';
  return prefix.replace(/[\u0000-\u001f\u007f]/g, '').slice(0, 64) || 'Nearby device';
}

export function buildNativeDiscoveryRows(advertisements: NativeAdvertisement[]): NativeDiscoveryRow[] {
  const groups = new Map<string, NativeDiscoveryRow>();
  advertisements.forEach((row, index) => {
    const fullInstance = row.instance.toLowerCase().endsWith(suffixes[row.service_kind]);
    const scoped = row.interface_index !== null && row.interface_index > 0;
    // A service on one interface may have several addresses. This is never a physical-TV identity.
    const key = fullInstance && scoped
      ? JSON.stringify([row.service_kind, row.instance.toLowerCase(), row.port, row.interface_index, row.interface_name])
      : JSON.stringify(['separate-observation', index, row.service_kind, row.instance, row.address, row.port, row.scope_id]);
    const group = groups.get(key) ?? { key, name: advertisedName(row), addresses: [] };
    group.addresses.push(row);
    groups.set(key, group);
  });
  return [...groups.values()].map(group => ({ ...group, addresses: group.addresses.sort((a, b) => Number(a.address.includes(':')) - Number(b.address.includes(':')) || a.address.localeCompare(b.address)) }));
}

export function discoveryAddressUsable(row: NativeAdvertisement, age: number, maxAge: number): boolean {
  if (row.cached || row.observed_age_ms + Math.max(0, age) >= maxAge) return false;
  return discoveryEndpointUsable(row);
}

export function discoveryEndpointUsable(row: NativeAdvertisement): boolean {
  if (!row.connect_address) return false;
  if (/^fe[89ab][\da-f]:/i.test(row.address)) {
    return Number.isInteger(row.scope_id) && (row.scope_id ?? 0) > 0 && row.connect_address.includes(`%${row.scope_id}]`);
  }
  return true;
}

export function refreshedDiscoveryChoice(selected: NativeAdvertisement, report: import('./types').NativeDiscoveryReport): NativeAdvertisement | undefined {
  return report.advertisements.find(row =>
    row.service_kind === selected.service_kind && row.instance.toLowerCase() === selected.instance.toLowerCase()
    && row.connect_address === selected.connect_address && row.port === selected.port
    && row.interface_index === selected.interface_index && row.interface_name === selected.interface_name
    && row.scope_id === selected.scope_id && discoveryAddressUsable(row, 0, report.observation_max_age_ms));
}

export function excludeConnectedAdvertisements(advertisements: NativeAdvertisement[], readyEndpoints: string[]): NativeAdvertisement[] {
  const ready = new Set(readyEndpoints);
  return buildNativeDiscoveryRows(advertisements).filter(group => !group.addresses.some(ad => ad.service_kind !== 'tls_pairing' && ad.connect_address !== null && ready.has(ad.connect_address))).flatMap(group => group.addresses);
}
