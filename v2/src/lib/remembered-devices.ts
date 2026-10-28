import type { Device } from './types';

export interface RememberedDevice {
  hardwareId: string;
  name: string;
  model: string;
  lastSeen: string;
  alias?: string;
}

type StorageAccess = Pick<Storage, 'getItem' | 'setItem'>;
export const REMEMBERED_DEVICES_KEY = 'atv.desktop.rememberedDevices.v1';
export const MAX_REMEMBERED_DEVICES = 64;
const MAX_STORED_LENGTH = 128 * 1024;

function hardwareId(value: unknown): string | undefined {
  if (typeof value !== 'string') return undefined;
  const id = value.trim();
  return id && id.length <= 256 && id.toLowerCase() !== 'unknown' && !/[\u0000-\u001f\u007f]/.test(id)
    && !Array.from(id).some(character => { const point = character.codePointAt(0)!; return point >= 0xd800 && point <= 0xdfff; })
    ? id : undefined;
}

function label(value: unknown, maximum = 128): string {
  return typeof value === 'string'
    ? value.replace(/[\u0000-\u001f\u007f]/g, '').trim().slice(0, maximum)
    : '';
}

function normalizeRecord(value: unknown): RememberedDevice | undefined {
  if (!value || typeof value !== 'object' || Array.isArray(value)) return undefined;
  const row = value as Record<string, unknown>;
  const id = hardwareId(row.hardwareId);
  const date = typeof row.lastSeen === 'string' ? Date.parse(row.lastSeen) : NaN;
  if (!id || !Number.isFinite(date)) return undefined;
  const alias = label(row.alias, 64);
  return {
    hardwareId: id,
    name: label(row.name) || 'Android device',
    model: label(row.model) || 'Unknown model',
    lastSeen: new Date(date).toISOString(),
    ...(alias ? { alias } : {}),
  };
}

function normalizeRecords(values: readonly unknown[]): RememberedDevice[] {
  const byId = new Map<string, RememberedDevice>();
  for (const value of values) {
    const row = normalizeRecord(value);
    if (!row) continue;
    const previous = byId.get(row.hardwareId);
    if (!previous || row.lastSeen > previous.lastSeen) byId.set(row.hardwareId, row);
  }
  return [...byId.values()]
    .sort((a, b) => b.lastSeen.localeCompare(a.lastSeen) || a.hardwareId.localeCompare(b.hardwareId))
    .slice(0, MAX_REMEMBERED_DEVICES);
}

export function deserializeRememberedDevices(raw: string | null): RememberedDevice[] {
  if (!raw || raw.length > MAX_STORED_LENGTH) return [];
  try {
    const parsed = JSON.parse(raw);
    if (!parsed || parsed.schemaVersion !== 1 || !Array.isArray(parsed.devices)) return [];
    return normalizeRecords(parsed.devices);
  } catch {
    return [];
  }
}

export function serializeRememberedDevices(records: readonly RememberedDevice[]): string {
  return JSON.stringify({ schemaVersion: 1, devices: normalizeRecords(records) });
}

export function verifiedDeviceHardwareId(device: Device): string | undefined {
  return device.status === 'device' ? hardwareId(device.properties?.serial_number) : undefined;
}

export function updateRememberedDevices(
  records: readonly RememberedDevice[],
  devices: readonly Device[],
  now = Date.now(),
): RememberedDevice[] {
  const remembered = normalizeRecords(records);
  const timestamp = new Date(now);
  if (!Number.isFinite(timestamp.getTime())) return remembered;
  const byId = new Map(remembered.map(row => [row.hardwareId, row]));
  for (const device of devices) {
    const id = verifiedDeviceHardwareId(device);
    if (!id) continue;
    const previous = byId.get(id);
    byId.set(id, {
      hardwareId: id,
      name: label(device.properties?.friendly_name) || previous?.name || label(device.name) || 'Android device',
      model: label(device.model) || label(device.properties?.model) || previous?.model || 'Unknown model',
      lastSeen: timestamp.toISOString(),
      ...(previous?.alias ? { alias: previous.alias } : {}),
    });
  }
  return normalizeRecords([...byId.values()]);
}

export function renameRememberedDevice(
  records: readonly RememberedDevice[],
  id: string,
  alias: string,
): RememberedDevice[] {
  const key = hardwareId(id);
  const name = label(alias, 64);
  return normalizeRecords(records).map(row => {
    if (!key || row.hardwareId !== key) return row;
    const renamed = { ...row };
    if (name) renamed.alias = name;
    else delete renamed.alias;
    return renamed;
  });
}

export function forgetRememberedDevice(records: readonly RememberedDevice[], id: string): RememberedDevice[] {
  const key = hardwareId(id);
  return normalizeRecords(records).filter(row => row.hardwareId !== key);
}

export function rememberedDeviceLabel(record: RememberedDevice): string {
  return record.alias || record.name;
}

export function readRememberedDevices(storage?: StorageAccess): RememberedDevice[] {
  try {
    const target = storage ?? (typeof window === 'undefined' ? undefined : window.localStorage);
    return target ? deserializeRememberedDevices(target.getItem(REMEMBERED_DEVICES_KEY)) : [];
  } catch {
    return [];
  }
}

export function writeRememberedDevices(records: readonly RememberedDevice[], storage?: StorageAccess): boolean {
  try {
    const target = storage ?? (typeof window === 'undefined' ? undefined : window.localStorage);
    if (!target) return false;
    target.setItem(REMEMBERED_DEVICES_KEY, serializeRememberedDevices(records));
    return true;
  } catch {
    return false;
  }
}
