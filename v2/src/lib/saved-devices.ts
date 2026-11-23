// Saved devices (#411): network TVs the app remembers across restarts, so it
// can dial them on launch instead of waiting for a scan that a sleeping box
// never answers.
//
// Identity is the verified hardware id (`ro.serialno`) and nothing else, the
// repo-wide rule. A record is only ever created for a device that reported
// one, a saved row is Online only while a device reporting that same id is
// connected, and a different id answering at the saved address is a
// different device: it never inherits the saved name. A device that could not
// be queried (unauthorized, so no id) claims nothing.
//
// Unsaving sticks. Mobile re-saves a TV on its next connection; here an
// unsaved id is remembered and auto-save skips it until the user saves it
// again with the row's bookmark.
//
// Stored locally like the other desktop preferences. No keys, PINs or other
// secrets: an address, a name, what the device reported about itself, and
// its MAC address for Wake-on-LAN. Only Connect on a saved row wakes a TV;
// the launch and restart redials never do.

import { idKey } from "./hardware-id";

export interface SavedDevice {
  hardwareId: string;
  /// The last address it was connected at, `host:port`.
  address: string;
  name: string;
  model?: string;
  manufacturer?: string;
  deviceType: "shield" | "google_tv" | "unknown";
  /// Lowercase `aa:bb:cc:dd:ee:ff`, read while it was connected.
  mac?: string;
  /// ISO time it was last seen connected.
  lastConnected: string;
}

export interface SavedStore {
  devices: SavedDevice[];
  /// Hardware ids the user unsaved, which auto-save leaves alone.
  unsaved: string[];
}

/// What the device list reports about one connected device.
export interface LiveDevice {
  serial: string;
  name: string;
  model?: string;
  status: string;
  connection: string;
  device_type?: string;
  properties?: {
    serial_number?: string;
    manufacturer?: string;
  } | null;
}

export type SavedStatus =
  | "online"
  /// Connect sent a Wake-on-LAN packet; the dial follows.
  | "waking"
  | "connecting"
  | "no_answer"
  /// Another verified id answers at the saved address.
  | "different_device"
  /// Something answers at the saved address but can't say who it is.
  | "unverified"
  | "not_connected";

export type DialState = "waking" | "connecting" | "no_answer";

export const SAVED_DEVICES_KEY = "shieldopt.savedDevices.v1";
export const MAX_SAVED = 32;
export const MAX_UNSAVED = 64;
/// How often seeing a device connected refreshes its last-connected time.
const TOUCH_MS = 60_000;

type KeyValue = Pick<Storage, "getItem" | "setItem">;

function defaultStorage(): KeyValue | null {
  try {
    return typeof localStorage === "undefined" ? null : localStorage;
  } catch {
    return null;
  }
}

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/;
const IPV6 = /^\[[0-9a-f:.]+(%[\w.-]+)?\]$/i;

/// `host:port` when `value` is a dialable network address with a real port,
/// else null. A Wireless debugging service name is not an address.
export function normalizeAddress(value: string | null | undefined): string | null {
  const raw = value?.trim() ?? "";
  const cut = raw.lastIndexOf(":");
  if (cut <= 0) return null;
  const host = raw.slice(0, cut);
  const port = Number(raw.slice(cut + 1));
  if (!/^\d+$/.test(raw.slice(cut + 1)) || port < 1 || port > 65535) return null;
  if (!IPV4.test(host) && !IPV6.test(host)) return null;
  return `${host.toLowerCase()}:${port}`;
}

/// The host part of a saved address, without brackets.
export function hostOfAddress(address: string): string {
  const host = address.slice(0, address.lastIndexOf(":"));
  return host.replace(/^\[|\]$/g, "");
}

/// The address a connected network device can be dialled at again.
export function savedAddressOf(d: LiveDevice): string | null {
  return d.connection === "network" ? normalizeAddress(d.serial) : null;
}

function hardwareIdOf(d: LiveDevice): string | null {
  return idKey(d.properties?.serial_number);
}

const DEVICE_TYPES = new Set(["shield", "google_tv", "unknown"]);
const MAC = /^[0-9a-f]{2}(:[0-9a-f]{2}){5}$/;

function text(value: unknown): string | undefined {
  const s = typeof value === "string" ? value.trim() : "";
  return s || undefined;
}

function normalizeRecord(value: unknown): SavedDevice | null {
  if (!value || typeof value !== "object") return null;
  const r = value as Record<string, unknown>;
  const hardwareId = idKey(typeof r.hardwareId === "string" ? r.hardwareId : null);
  const address = normalizeAddress(typeof r.address === "string" ? r.address : null);
  if (!hardwareId || !address) return null;
  const last = new Date(typeof r.lastConnected === "string" ? r.lastConnected : NaN);
  const mac = text(r.mac)?.toLowerCase();
  const model = text(r.model);
  const manufacturer = text(r.manufacturer);
  return {
    hardwareId,
    address,
    name: text(r.name) ?? address,
    ...(model ? { model } : {}),
    ...(manufacturer ? { manufacturer } : {}),
    deviceType: DEVICE_TYPES.has(r.deviceType as string) ? (r.deviceType as SavedDevice["deviceType"]) : "unknown",
    ...(mac && MAC.test(mac) ? { mac } : {}),
    lastConnected: Number.isNaN(last.getTime()) ? new Date(0).toISOString() : last.toISOString(),
  };
}

/// Read the store, dropping anything malformed or without a verified id, and
/// keeping the newest record when one id appears twice.
export function loadSaved(storage: KeyValue | null = defaultStorage()): SavedStore {
  const empty: SavedStore = { devices: [], unsaved: [] };
  if (!storage) return empty;
  try {
    const raw = storage.getItem(SAVED_DEVICES_KEY);
    if (!raw) return empty;
    const parsed = JSON.parse(raw) as Record<string, unknown>;
    if (!parsed || typeof parsed !== "object") return empty;
    const newest = new Map<string, SavedDevice>();
    for (const record of Array.isArray(parsed.devices) ? parsed.devices : []) {
      const d = normalizeRecord(record);
      if (!d) continue;
      const kept = newest.get(d.hardwareId);
      if (!kept || d.lastConnected > kept.lastConnected) newest.set(d.hardwareId, d);
    }
    const unsaved = [...new Set(
      (Array.isArray(parsed.unsaved) ? parsed.unsaved : [])
        .map((id) => idKey(typeof id === "string" ? id : null))
        .filter((id): id is string => !!id && !newest.has(id)),
    )];
    return { devices: [...newest.values()], unsaved: unsaved.slice(-MAX_UNSAVED) };
  } catch {
    return empty;
  }
}

/// Persist the store. A blocked or full store only means the list won't
/// survive a restart, so it never throws.
export function writeSaved(store: SavedStore, storage: KeyValue | null = defaultStorage()): boolean {
  if (!storage) return false;
  const devices = [...store.devices]
    .sort((a, b) => b.lastConnected.localeCompare(a.lastConnected))
    .slice(0, MAX_SAVED);
  try {
    storage.setItem(SAVED_DEVICES_KEY, JSON.stringify({ devices, unsaved: store.unsaved.slice(-MAX_UNSAVED) }));
    return true;
  } catch {
    return false;
  }
}

function recordFor(d: LiveDevice, hardwareId: string, address: string, now: Date, prior?: SavedDevice): SavedDevice {
  const model = text(d.model) ?? prior?.model;
  const manufacturer = text(d.properties?.manufacturer) ?? prior?.manufacturer;
  const deviceType = DEVICE_TYPES.has(d.device_type ?? "")
    ? (d.device_type as SavedDevice["deviceType"])
    : prior?.deviceType ?? "unknown";
  return {
    hardwareId,
    address,
    name: text(d.name) ?? prior?.name ?? address,
    ...(model ? { model } : {}),
    ...(manufacturer ? { manufacturer } : {}),
    deviceType,
    ...(prior?.mac ? { mac: prior.mac } : {}),
    lastConnected: now.toISOString(),
  };
}

function sameRecord(a: SavedDevice, b: SavedDevice): boolean {
  return a.address === b.address && a.name === b.name && a.model === b.model &&
    a.manufacturer === b.manufacturer && a.deviceType === b.deviceType && a.mac === b.mac;
}

/// Auto-save: a connected network device with a verified id and a dialable
/// address is saved, or its saved record refreshed (it may have moved to a
/// new address). Ids the user unsaved are skipped, and so is a device that
/// answered only because the app dialled a saved address it then turned out
/// not to be: `dialledFor` maps each address the app dialled to the saved id
/// it expected there. Returns the same store object when nothing changed.
export function observeConnected(
  store: SavedStore,
  d: LiveDevice,
  now = new Date(),
  dialledFor?: ReadonlyMap<string, string>,
): SavedStore {
  if (d.status !== "device") return store;
  const hardwareId = hardwareIdOf(d);
  const address = savedAddressOf(d);
  if (!hardwareId) return store;
  const prior = store.devices.find((s) => s.hardwareId === hardwareId);
  if (!prior) {
    if (!address || store.unsaved.includes(hardwareId)) return store;
    const expected = dialledFor?.get(address);
    if (expected !== undefined && expected !== hardwareId) return store;
    return { ...store, devices: [...store.devices, recordFor(d, hardwareId, address, now)] };
  }
  // Reached under a service name: still the saved TV, at its last address.
  const next = recordFor(d, hardwareId, address ?? prior.address, now, prior);
  const stale = now.getTime() - new Date(prior.lastConnected).getTime() >= TOUCH_MS;
  if (sameRecord(prior, next) && !stale) return store;
  return { ...store, devices: store.devices.map((s) => (s === prior ? next : s)) };
}

/// The bookmark on an unsaved row. Null when the device can't be saved: no
/// verified id, or no address to dial it at again.
export function saveDevice(store: SavedStore, d: LiveDevice, now = new Date()): SavedStore | null {
  const hardwareId = hardwareIdOf(d);
  if (!hardwareId || d.status !== "device" || !savedAddressOf(d)) return null;
  const cleared = { ...store, unsaved: store.unsaved.filter((id) => id !== hardwareId) };
  return observeConnected(cleared, d, now);
}

/// The bookmark on a saved row: forget the record and keep it unsaved.
export function unsaveDevice(store: SavedStore, hardwareId: string): SavedStore {
  const id = idKey(hardwareId);
  if (!id) return store;
  return {
    devices: store.devices.filter((s) => s.hardwareId !== id),
    unsaved: [...store.unsaved.filter((u) => u !== id), id].slice(-MAX_UNSAVED),
  };
}

/// File a MAC read under the saved record, but only when the read came back
/// from the device that record is: the same verified id.
export function recordMac(
  store: SavedStore,
  hardwareId: string,
  read: { hardware_id: string | null; mac: string | null },
): SavedStore {
  const mac = read.mac?.trim().toLowerCase();
  if (!mac || !MAC.test(mac) || idKey(read.hardware_id) !== hardwareId) return store;
  const prior = store.devices.find((s) => s.hardwareId === hardwareId);
  if (!prior || prior.mac === mac) return store;
  return { ...store, devices: store.devices.map((s) => (s === prior ? { ...s, mac } : s)) };
}

/// Fill in a MAC that another screen wrote to storage for the same id, so a
/// write from this screen never drops it.
export function mergeStoredMacs(store: SavedStore, stored: SavedStore): SavedStore {
  const macs = new Map(stored.devices.flatMap((d) => (d.mac ? [[d.hardwareId, d.mac] as const] : [])));
  if (!store.devices.some((d) => !d.mac && macs.has(d.hardwareId))) return store;
  return {
    ...store,
    devices: store.devices.map((d) => (!d.mac && macs.has(d.hardwareId) ? { ...d, mac: macs.get(d.hardwareId) } : d)),
  };
}

/// Dial state shared by every Devices screen in one app session.
export interface DialSession {
  dial: Record<string, DialState>;
  /// Ids whose connect call hasn't returned yet.
  pending: Record<string, boolean>;
  /// Counts connect calls that succeeded, so whichever screen is open re-reads
  /// the list when one lands, even after the screen that dialled has closed.
  landed: number;
}

/// A MAC a wake packet can be sent for: well formed, unicast and not all
/// zeros (the backend checks again). Null means no wake.
export function wakeMac(mac: string | null | undefined): string | null {
  const m = mac?.trim().toLowerCase() ?? "";
  if (!MAC.test(m) || m === "00:00:00:00:00:00") return null;
  // The multicast bit also covers broadcast.
  return parseInt(m.slice(0, 2), 16) & 1 ? null : m;
}

/// Connect on a saved row that knows its MAC: send a Wake-on-LAN packet,
/// then dial, retrying while a woken TV boots.
export interface WakeFirst {
  wake: (mac: string, host: string) => Promise<unknown>;
  /// The longest the wake send may hold up the first dial.
  wakeMs: number;
  /// The pause between dials.
  retryMs: number;
  /// Replaces the normal wait: a woken box needs time.
  waitMs: number;
}

const pause = (ms: number) => new Promise<void>((resolve) => setTimeout(resolve, ms));

/// Dial a saved address. Resolves when the call returns or after `waitMs`,
/// whichever is first; at the wait the row says "Didn't answer", but the
/// call stays pending (and Connect stays off) until it really returns. A
/// success never reads as "Didn't answer": it clears the row's dial state,
/// and Online then comes from the list.
///
/// With `wakeFirst` and a usable MAC on the record, the row says "Waking…"
/// while the packet goes out, then dials one call at a time until one
/// answers or `wakeFirst.waitMs` runs out. Without either it dials once, as
/// the launch and restart redials always do: they never wake anything.
export function dialSavedAddress(
  session: DialSession,
  saved: Pick<SavedDevice, "hardwareId" | "address" | "mac">,
  connect: (address: string) => Promise<{ ok: boolean } | null>,
  waitMs: number,
  wakeFirst?: WakeFirst,
): Promise<void> {
  const id = saved.hardwareId;
  if (session.pending[id]) return Promise.resolve();
  const mac = wakeFirst ? wakeMac(saved.mac) : null;
  const wait = mac && wakeFirst ? wakeFirst.waitMs : waitMs;
  const deadline = Date.now() + wait;
  session.pending[id] = true;
  session.dial[id] = mac ? "waking" : "connecting";
  const attempt = async (): Promise<{ ok: boolean } | null> => {
    if (!mac || !wakeFirst) return connect(saved.address).catch(() => null);
    let timer: ReturnType<typeof setTimeout> | undefined;
    await Promise.race([
      // A failed or slow send never blocks the dial.
      Promise.resolve().then(() => wakeFirst.wake(mac, hostOfAddress(saved.address))).catch(() => undefined),
      new Promise<void>((resolve) => { timer = setTimeout(resolve, wakeFirst.wakeMs); }),
    ]);
    clearTimeout(timer);
    if (session.dial[id] === "waking") session.dial[id] = "connecting";
    for (;;) {
      const r = await connect(saved.address).catch(() => null);
      if (r?.ok || Date.now() + wakeFirst.retryMs >= deadline) return r;
      await pause(wakeFirst.retryMs);
    }
  };
  const settled = attempt()
    .catch(() => null)
    .then((r) => {
      delete session.pending[id];
      if (r?.ok) {
        delete session.dial[id];
        session.landed += 1;
      } else {
        session.dial[id] = "no_answer";
      }
    });
  let timer: ReturnType<typeof setTimeout> | undefined;
  const waited = new Promise<void>((resolve) => {
    timer = setTimeout(() => {
      const state = session.dial[id];
      if (session.pending[id] && (state === "connecting" || state === "waking")) session.dial[id] = "no_answer";
      resolve();
    }, wait);
  });
  return Promise.race([settled.then(() => clearTimeout(timer)), waited]);
}

/// The connected device a saved record is, by verified id alone.
export function liveDeviceFor<T extends LiveDevice>(saved: SavedDevice, devices: T[]): T | undefined {
  return devices.find((d) => d.status === "device" && hardwareIdOf(d) === saved.hardwareId);
}

function sameEndpoint(serial: string, address: string): boolean {
  return normalizeAddress(serial) === address;
}

/// What a saved row says. Online needs the same verified id; something else
/// answering at the saved address says so (a different id) or claims nothing
/// (no id); otherwise the dial's own state.
export function savedStatus(saved: SavedDevice, devices: LiveDevice[], dial: DialState | undefined): SavedStatus {
  if (liveDeviceFor(saved, devices)) return "online";
  const atAddress = devices.filter((d) => sameEndpoint(d.serial, saved.address));
  if (atAddress.some((d) => { const id = hardwareIdOf(d); return id !== null && id !== saved.hardwareId; })) {
    return "different_device";
  }
  if (dial === "connecting" || dial === "waking") return dial;
  // An offline transport is what adb leaves when a TV sleeps: nothing is
  // answering, so it is no reason to doubt who is there.
  if (atAddress.some((d) => d.status !== "offline" && hardwareIdOf(d) === null)) return "unverified";
  return dial ?? "not_connected";
}

/// Saved records in a stable order, so a refreshed time never reshuffles them.
export function sortedSaved(store: SavedStore): SavedDevice[] {
  return [...store.devices].sort((a, b) => a.name.localeCompare(b.name) || a.hardwareId.localeCompare(b.hardwareId));
}

/// "Last connected 2 h ago" phrasing for the row's metadata; empty when the
/// time is unknown, so the row leaves the phrase out.
export function lastConnectedLabel(iso: string, now = Date.now()): string {
  const then = new Date(iso).getTime();
  if (Number.isNaN(then) || then <= 0) return "";
  const mins = Math.max(0, Math.round((now - then) / 60000));
  if (mins < 1) return "just now";
  if (mins < 60) return `${mins} min ago`;
  const hours = Math.round(mins / 60);
  if (hours < 24) return `${hours} h ago`;
  const days = Math.round(hours / 24);
  return days === 1 ? "yesterday" : `${days} days ago`;
}
