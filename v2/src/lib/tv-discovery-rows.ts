import type { TvObservation } from './types';

export interface TvDiscoveryRow { key: string; name: string; addresses: TvObservation[] }
const suffixes = { google_cast: '._googlecast._tcp.local.', android_tv_remote: '._androidtvremote2._tcp.local.' };
export function observedName(row: TvObservation): string {
  const name = row.friendly_name?.trim();
  return name && name.length <= 160 && !/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/u.test(name) ? name : 'Nearby device';
}
export function buildTvDiscoveryRows(observations: TvObservation[]): TvDiscoveryRow[] {
  const groups = new Map<string, TvDiscoveryRow>();
  observations.forEach((row, index) => {
    const scoped = row.interface_index !== null && row.interface_index > 0;
    const fullInstance = row.instance.toLowerCase().endsWith(suffixes[row.source]);
    // One service/interface observation, never a physical or saved-TV identity.
    const key = scoped && fullInstance
      ? JSON.stringify([row.source, row.instance.toLowerCase(), row.interface_index, row.interface_name])
      : JSON.stringify(['separate-observation', index, row.source, row.instance, row.address, row.scope_id]);
    const group = groups.get(key) ?? { key, name: observedName(row), addresses: [] };
    group.addresses.push(row);
    groups.set(key, group);
  });
  return [...groups.values()].map(group => ({ ...group, addresses: group.addresses.sort((a, b) => Number(a.address.includes(':')) - Number(b.address.includes(':')) || a.address.localeCompare(b.address)) }));
}
export function observationFresh(row: TvObservation, age: number, maxAge: number): boolean {
  return !row.cached && row.observed_age_ms + Math.max(0, age) < maxAge;
}
export function discoverySetupHost(row: TvObservation): string | null {
  return row.warning ? null : row.setup_host;
}

export function usableObservationAddresses(row: TvDiscoveryRow, age: number, maxAge: number): TvObservation[] {
  const hosts = new Map<string, TvObservation>();
  for (const observation of row.addresses) {
    const host = discoverySetupHost(observation);
    if (host && observationFresh(observation, age, maxAge)) hosts.set(host, observation);
  }
  return [...hosts.values()];
}
export function selectedObservation(row: TvDiscoveryRow, selectedHost: string | undefined, age: number, maxAge: number): TvObservation | null {
  const choices = usableObservationAddresses(row, age, maxAge);
  if (choices.length === 1) return choices[0];
  return choices.find(observation => discoverySetupHost(observation) === selectedHost) ?? null;
}
