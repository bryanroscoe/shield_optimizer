// Saved-TV persistence for the design's §1.0 Reconnect flow. Paired TVs are
// remembered in localStorage (host/port/name/type + last-used time) so the app
// can offer a one-tap "Reconnect to <name>" on launch instead of always
// re-scanning. The RSA pairing key is persisted Kotlin-side, so reconnecting a
// previously-authorized TV is silent — this is only app-side bookkeeping.
//
// This never holds secrets: no keys, no PINs. Just enough to re-open the ADB
// socket to a TV the phone already trusts.

import type { Device, SavedDevice } from "./types";
import { deviceLabelOf } from "./types";
import {
  normalizeHardwareId,
  savedDeviceIsLiveConnection,
  savedDeviceKey,
  savedDeviceMatchesConnection,
} from "./identity";

export { savedDeviceIsLiveConnection, savedDeviceKey, savedDeviceMatchesConnection };

const KEY = "atv.savedDevices.v1";
const AUTO_KEY = "atv.autoConnect.v1";
const MAX = 16;
const EPOCH = new Date(0).toISOString();

/// Not a security identifier -- just random enough that two rows saved in the
/// same process tick never collide.
function randomLocalId(): string {
  return `${Date.now().toString(36)}${Math.random().toString(36).slice(2, 10)}`;
}

function normalizeSavedDevice(value: unknown): SavedDevice | null {
  if (!value || typeof value !== "object") return null;
  const d = value as Record<string, unknown>;
  if (
    typeof d.host !== "string" ||
    d.host.trim() === "" ||
    typeof d.connectPort !== "number" ||
    !Number.isInteger(d.connectPort) ||
    d.connectPort < 1 ||
    d.connectPort > 65535
  ) {
    return null;
  }
  const deviceType =
    d.deviceType === "shield" ||
    d.deviceType === "google_tv" ||
    d.deviceType === "unknown"
      ? d.deviceType
      : d.deviceType === "googletv"
        ? "google_tv"
        : "unknown";
  const parsedLastUsed =
    typeof d.lastUsed === "string" ? new Date(d.lastUsed) : new Date(NaN);
  const hardwareId = normalizeHardwareId(
    typeof d.hardwareId === "string" ? d.hardwareId : undefined,
  );
  // Every id-less row needs a stable key of its own (see identity.ts); carry
  // an existing one over, or mint one now if this row has never had one (an
  // older record, or one whose only id was a placeholder that just migrated
  // away above).
  const localId = hardwareId
    ? undefined
    : typeof d.localId === "string" && d.localId.trim() !== ""
      ? d.localId.trim()
      : randomLocalId();
  return {
    host: d.host.trim(),
    connectPort: d.connectPort,
    ...(hardwareId ? { hardwareId } : {}),
    ...(localId ? { localId } : {}),
    name:
      typeof d.name === "string" && d.name.trim() !== ""
        ? d.name.trim()
        : d.host.trim(),
    deviceType,
    lastUsed: Number.isNaN(parsedLastUsed.getTime())
      ? EPOCH
      : parsedLastUsed.toISOString(),
  };
}

/// Whether a raw stored record will come out of `normalizeSavedDevice` as
/// id-less and without an existing local key -- i.e. it is about to be
/// assigned a fresh one, which must be persisted so it stays stable.
function rawNeedsLocalId(raw: unknown): boolean {
  if (!raw || typeof raw !== "object") return false;
  const d = raw as Record<string, unknown>;
  const hardwareId = normalizeHardwareId(
    typeof d.hardwareId === "string" ? d.hardwareId : undefined,
  );
  if (hardwareId) return false;
  return typeof d.localId !== "string" || d.localId.trim() === "";
}

function read(): SavedDevice[] {
  try {
    const raw = localStorage.getItem(KEY);
    if (!raw) return [];
    const parsed = JSON.parse(raw);
    if (!Array.isArray(parsed)) return [];
    // Normalize older or partially-corrupt records in memory. Invalid dates
    // sort to the end instead of throwing during reconnect-screen startup.
    const normalized = parsed
      .map(normalizeSavedDevice)
      .filter((d): d is SavedDevice => d !== null);
    // A stored placeholder id normalizes to id-less, which can leave two rows
    // with one identity. Keep the newest so every key is unique (the saved-TV
    // lists key on it) and Forget removes exactly one TV, and persist the
    // result so the placeholder is migrated rather than re-normalized forever.
    const newest = new Map<string, SavedDevice>();
    for (const device of normalized) {
      const key = savedDeviceKey(device);
      const kept = newest.get(key);
      if (!kept || device.lastUsed > kept.lastUsed) newest.set(key, device);
    }
    const unique = normalized.filter((d) => newest.get(savedDeviceKey(d)) === d);
    const unnormalizedId = parsed.some((d) => {
      if (!d || typeof d !== "object" || !("hardwareId" in d)) return false;
      const stored = d.hardwareId;
      return normalizeHardwareId(typeof stored === "string" ? stored : undefined) !== stored;
    });
    const needsLocalIdMigration = parsed.some(rawNeedsLocalId);
    if (unnormalizedId || needsLocalIdMigration || unique.length !== normalized.length) {
      write(unique);
    }
    return unique;
  } catch {
    return [];
  }
}

function write(list: SavedDevice[]): void {
  try {
    const newestFirst = [...list].sort((a, b) =>
      b.lastUsed.localeCompare(a.lastUsed),
    );
    localStorage.setItem(KEY, JSON.stringify(newestFirst.slice(0, MAX)));
  } catch {
    // Storage unavailable/full — reconnect simply won't be offered next launch.
  }
}

/// All saved TVs, most-recently-used first.
export function listSavedDevices(): SavedDevice[] {
  return read().sort((a, b) => b.lastUsed.localeCompare(a.lastUsed));
}

/// The single most-recently-used TV, or null. Drives the §1.0 landing screen.
export function lastSavedDevice(): SavedDevice | null {
  return listSavedDevices()[0] ?? null;
}

function hardwareIdOf(device: Device | null): string | undefined {
  return normalizeHardwareId(device?.properties?.serial_number);
}

export function savedHostHasMultipleIdentities(
  devices: SavedDevice[],
  host: string,
): boolean {
  return new Set(
    devices
      .filter((device) => device.host === host)
      .map(savedDeviceKey),
  ).size > 1;
}


export function shouldAutoDialSavedDevices(
  devices: SavedDevice[],
  enabled: boolean,
): boolean {
  return enabled && devices.length === 1;
}

/// Does a saved row describe the TV we just connected to? Hardware ids are
/// strong identity. Without them, only an unchanged endpoint is trustworthy.
function sameTv(
  row: SavedDevice,
  host: string,
  connectPort: number,
  hardwareId: string | undefined,
): boolean {
  const liveId = normalizeHardwareId(hardwareId);
  const savedId = normalizeHardwareId(row.hardwareId);
  if (liveId || savedId) return liveId !== undefined && savedId === liveId;
  return row.host === host && row.connectPort === connectPort;
}

/// Record (or refresh) a successful connection. The hardware serial is the
/// durable identity when the TV reports one (ports rotate and DHCP can hand a
/// TV's old IP to another device); the host is the fallback.
export function rememberDevice(
  host: string,
  connectPort: number,
  device: Device | null,
): void {
  const current = read();
  const hardwareId = hardwareIdOf(device);
  const matches = current.filter((d) => sameTv(d, host, connectPort, hardwareId));
  // An id-less connection can match more than one saved row only when several
  // id-less TVs have shared this exact endpoint over time. Which one just
  // answered is not knowable from the endpoint alone, so nothing is written:
  // claiming one would be a guess, and saving a fresh row on every repeat
  // reconnect would eventually evict a genuine saved TV once MAX is reached.
  if (matches.length > 1) return;
  const existing = matches[0];
  const combinedHardwareId = hardwareId ?? existing?.hardwareId;
  const reportedFriendlyName = device?.properties?.friendly_name?.trim();
  const name = reportedFriendlyName || existing?.name || deviceLabelOf(device);
  const list = current.filter((d) => d !== existing);
  list.unshift({
    host,
    connectPort,
    name,
    deviceType: device?.device_type ?? existing?.deviceType ?? "unknown",
    ...(combinedHardwareId ? { hardwareId: combinedHardwareId } : {}),
    ...(combinedHardwareId ? {} : { localId: existing?.localId ?? randomLocalId() }),
    lastUsed: new Date().toISOString(),
  });
  write(list);
}

export function forgetDevice(host: string, connectPort: number): void {
  const current = listSavedDevices();
  const target = current.find(
    (d) => d.host === host && d.connectPort === connectPort,
  );
  if (!target) return;
  forgetSavedDevice(target);
}

export function forgetSavedDevice(device: SavedDevice): void {
  const identity = savedDeviceKey(device);
  write(listSavedDevices().filter((saved) => savedDeviceKey(saved) !== identity));
}

/// Whether the app may dial the single saved TV on launch without being
/// asked. Set by an explicit successful connect, cleared by an explicit
/// disconnect, and persisted so a process kill can't revive a connection the
/// user deliberately ended.
export function autoConnectEnabled(): boolean {
  try {
    return localStorage.getItem(AUTO_KEY) !== "0";
  } catch {
    return true;
  }
}

export function setAutoConnect(enabled: boolean): void {
  try {
    localStorage.setItem(AUTO_KEY, enabled ? "1" : "0");
  } catch {
    // Storage unavailable — worst case we ask instead of auto-dialing.
  }
}

/// Cached friendly name for the live TV, under the same identity rule as
/// everywhere else: its verified hardware id or, when it reports none, an
/// id-less row at the exact endpoint. An identified row never lends its name
/// to a TV that reports no id, so a reused IP can never show another TV's name.
export function cachedDeviceName(
  host: string,
  connectPort: number,
  hardwareId?: string,
): string | null {
  if (!host) return null;
  const rows = listSavedDevices().filter((d) =>
    savedDeviceMatchesConnection(d, host, connectPort, hardwareId),
  );
  return rows.length === 1 ? rows[0].name : null;
}

/// Compact "last used" phrasing for the reconnect card (e.g. "2h ago").
export function lastUsedLabel(iso: string): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then)) return "";
  const mins = Math.max(0, Math.round((Date.now() - then) / 60000));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins}m ago`;
  const hrs = Math.round(mins / 60);
  if (hrs < 24) return `${hrs}h ago`;
  const days = Math.round(hrs / 24);
  return `${days}d ago`;
}
