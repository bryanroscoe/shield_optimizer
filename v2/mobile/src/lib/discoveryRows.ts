import type { Discovery, SavedDevice } from "./types";
import {
  normalizeHardwareId,
  savedDeviceIsLiveConnection,
  savedDeviceKey,
  savedDeviceMatchesConnection,
} from "./identity";

export type DiscoveryRowStatus =
  | "connected"
  | "saved-verified"
  | "saved-address"
  | "found"
  | "saved-other-port"
  | "saved-missing";

export interface DiscoveryRow {
  key: string;
  source: "discovery" | "saved" | "advertisement";
  advertisedName?: boolean;
  advertisementAmbiguous?: boolean;
  host: string;
  name: string;
  connectPorts: number[];
  pairingPorts: number[];
  legacyConnectPorts: number[];
  status: DiscoveryRowStatus;
  savedTarget?: SavedDevice;
}

export interface LiveEndpoint {
  connected: boolean;
  host: string;
  connectPort: number;
  /// The live TV's reported hardware id, if any. A saved row only claims the
  /// live connection when this identity agrees with it.
  hardwareId?: string | null;
  name?: string | null;
}

/// An advertised instance name plus the mDNS service type it arrived on.
/// The service type gates suffix matching in `advertisedMatch` below.
type AdvertisedInstance = { name: string; service: string };

type HostGroup = {
  host: string;
  names: Set<string>;
  instances: AdvertisedInstance[];
  connectPorts: Set<number>;
  pairingPorts: Set<number>;
  legacyConnectPorts: Set<number>;
  networks: Set<string>;
};

function serviceType(service: string): string {
  return service.trim().toLowerCase().replace(/^\.+|\.+$/g, "").replace(/\.local$/, "");
}
const ADB_TYPES = new Set(["_adb._tcp", "_adb-tls-connect._tcp", "_adb-tls-pairing._tcp"]);
const METADATA_TYPES = new Set(["_googlecast._tcp", "_androidtvremote2._tcp"]);
function safeName(value: string | null | undefined): string | null {
  const name = value?.trim();
  return name && name.length <= 128 && !/[\u0000-\u001f\u007f-\u009f\u202a-\u202e\u2066-\u2069]/.test(name) ? name : null;
}

function validPort(port: number): boolean {
  return Number.isInteger(port) && port >= 1 && port <= 65535;
}

function displayName(names: Set<string>): string {
  return names.size === 1 ? [...names][0] : "Android TV";
}

/// adbd advertises its mDNS service as `adb-<serial>` or `adb-<serial>-<suffix>`,
/// so the hardware serial is broadcast in the clear. Returns the part after the
/// `adb-` prefix, which is the serial plus whatever suffix the daemon appended.
function advertisedSerial(instanceName: string): string | null {
  const trimmed = instanceName.trim();
  if (!trimmed.toLowerCase().startsWith("adb-")) return null;
  const rest = trimmed.slice(4);
  return rest.length > 0 ? rest : null;
}

/// adbd's own suffix is six random alphanumerics. Bounding it is what stops a
/// saved id of `shield` from matching a stranger advertising `adb-shield-a`.
const ADBD_SUFFIX = /^[a-z0-9]{6}$/;

/// How this address advertises a saved TV's hardware id: as the whole
/// advertised serial, with adbd's random suffix, or not at all. Matching the
/// broadcast serial is real identity evidence, so a scan can recognize a TV it
/// has connected to before without opening a connection -- and without falling
/// back to the address, which DHCP can hand to a different device.
///
/// The random suffix is only something adbd's TLS services append (RFC 6763
/// instance-name disambiguation on `_adb-tls-connect`/`_adb-tls-pairing`).
/// Legacy `_adb._tcp` adverts carry the bare serial with no suffix, so a
/// legacy serial that happens to start with a saved id plus six more
/// characters is a different TV, not the same one with a suffix -- suffix
/// matching is only trusted on the TLS services that actually produce it.
function advertisedMatch(
  instances: AdvertisedInstance[],
  hardwareId: string | undefined,
): "exact" | "suffixed" | null {
  const wanted = normalizeHardwareId(hardwareId)?.toLowerCase();
  if (!wanted) return null;
  let match: "suffixed" | null = null;
  for (const { name, service } of instances) {
    if (!ADB_TYPES.has(serviceType(service))) continue;
    const serial = advertisedSerial(name)?.toLowerCase();
    if (!serial) continue;
    if (serial === wanted) return "exact";
    if (
      service.includes("tls") &&
      serial.startsWith(`${wanted}-`) &&
      ADBD_SUFFIX.test(serial.slice(wanted.length + 1))
    ) {
      match = "suffixed";
    }
  }
  return match;
}

export function buildDiscoveryRows(
  discoveries: Discovery[],
  savedDevices: SavedDevice[],
  live: LiveEndpoint,
): DiscoveryRow[] {
  // mDNS entries can outlive the TV that published them (a one-shot scan only
  // drops a service whose goodbye arrives while it runs), so the connected
  // TV's own reported id outranks any advert: it names the live row, and an
  // advert for that same TV at any other host is stale. Only that advert is
  // dropped, never the other devices answering from the same address.
  const liveId = live.connected ? normalizeHardwareId(live.hardwareId) : undefined;
  const metadata = new Map<string, Discovery[]>();
  for (const item of discoveries) {
    const type = serviceType(item.service);
    const host = item.host.trim();
    const unresolved = type === "_androidtvremote2._tcp" && !host && item.port === 0;
    if (!METADATA_TYPES.has(type) || (!unresolved && (!host || !validPort(item.port))) || !safeName(item.friendly_name)) continue;
    const key = unresolved ? `unresolved:${item.name}` : host;
    metadata.set(key, [...(metadata.get(key) ?? []), item]);
  }
  const hosts = new Map<string, HostGroup>();
  for (const discovery of discoveries) {
    if (!ADB_TYPES.has(serviceType(discovery.service))) continue;
    const host = discovery.host.trim();
    if (!host || !validPort(discovery.port)) continue;
    if (
      liveId &&
      host !== live.host &&
      advertisedMatch(
        [{ name: discovery.name ?? "", service: discovery.service }],
        liveId,
      )
    ) {
      continue;
    }
    const group = hosts.get(host) ?? {
      host,
      names: new Set<string>(),
      instances: [],
      connectPorts: new Set<number>(),
      pairingPorts: new Set<number>(),
      legacyConnectPorts: new Set<number>(),
      networks: new Set<string>(),
    };
    group.networks.add(discovery.network?.trim() ?? "");
    const name = safeName(discovery.name);
    if (name) group.instances.push({ name, service: discovery.service });
    if (name && !name.startsWith("adb-")) group.names.add(name);
    if (discovery.service.includes("pairing")) {
      group.pairingPorts.add(discovery.port);
    } else {
      group.connectPorts.add(discovery.port);
      if (!discovery.service.includes("tls")) {
        group.legacyConnectPorts.add(discovery.port);
      }
    }
    hosts.set(host, group);
  }

  const rows: DiscoveryRow[] = [];
  // Saved TVs this scan identified by their advertised serial. They are named
  // on their discovery row, so they must not also appear as a saved-endpoint
  // row further down, which would list the same TV twice.
  const verified = new Set<SavedDevice>();
  for (const group of hosts.values()) {
    const connectPorts = [...group.connectPorts].sort((a, b) => a - b);
    const isLive =
      live.connected &&
      live.host === group.host &&
      connectPorts.includes(live.connectPort);
    let savedMatch =
      savedDevices.find(
        (saved) => advertisedMatch(group.instances, saved.hardwareId) === "exact",
      ) ??
      savedDevices.find(
        (saved) => advertisedMatch(group.instances, saved.hardwareId) === "suffixed",
      );
    // On the live row only the live TV's own identity counts. When it reports
    // no id, an advert there may be stale, so the row is named only after an
    // id-less saved TV at this exact endpoint -- and only when that endpoint
    // has exactly one such TV saved, never a guess between several.
    if (isLive) {
      savedMatch = savedDevices.find((saved) =>
        savedDeviceIsLiveConnection(saved, savedDevices, live.host, live.connectPort, live.hardwareId),
      );
    }
    if (savedMatch) verified.add(savedMatch);
    const observations = metadata.get(group.host) ?? [];
    const observedNames = new Set(observations.map((item) => safeName(item.friendly_name)!));
    const networks = new Set(observations.map((item) => item.network?.trim() ?? ""));
    const instances = new Set(group.instances.map((item) => item.name));
    // A live scan's one network/address mapping may supply a display hint only.
    // Saved identity and actions remain determined exclusively by ADB evidence.
    const observedName = observedNames.size === 1 && networks.size === 1 &&
      group.networks.size === 1 && Boolean([...group.networks][0]) && networks.has([...group.networks][0]) &&
      instances.size <= 1 ? [...observedNames][0] : null;
    if (observedName) metadata.delete(group.host);
    rows.push({
      key: `discovery:${group.host}`,
      source: "discovery",
      host: group.host,
      // A serial match is verified identity, so the name this TV was saved
      // under is the honest label -- more useful than a generic fallback when
      // several TVs advertise nothing but their adb id.
      name: savedMatch?.name ?? (isLive ? safeName(live.name) : null) ?? (group.names.size ? displayName(group.names) : observedName ?? "Android TV"),
      ...(observedName && !savedMatch && !(isLive && safeName(live.name)) && !group.names.size ? { advertisedName: true } : {}),
      connectPorts,
      pairingPorts: [...group.pairingPorts].sort((a, b) => a - b),
      legacyConnectPorts: [...group.legacyConnectPorts].sort((a, b) => a - b),
      status: isLive
        ? "connected"
        : savedMatch
          ? "saved-verified"
          : savedDevices.some((saved) => saved.host === group.host)
            ? "saved-address"
            : "found",
      ...(savedMatch ? { savedTarget: savedMatch } : {}),
    });
  }

  // One row per stored TV, never per address: two saved TVs that once shared
  // an address are still two TVs, and each row carries the real stored entry
  // so reconnect and Forget act on something that exists.
  const identitiesAt = new Map<string, Set<string>>();
  for (const saved of savedDevices) {
    const endpoint = `${saved.host}:${saved.connectPort}`;
    const identities = identitiesAt.get(endpoint) ?? new Set<string>();
    identities.add(savedDeviceKey(saved));
    identitiesAt.set(endpoint, identities);
  }
  const seen = new Set<string>();
  for (const saved of savedDevices) {
    const identity = savedDeviceKey(saved);
    if (seen.has(identity)) continue;
    seen.add(identity);
    // Already named on a discovery row via its advertised serial.
    if (verified.has(saved)) continue;
    const discovered = hosts.get(saved.host);
    const answered = discovered?.connectPorts.has(saved.connectPort) ?? false;
    // The discovery row stands in for a lone saved TV at an endpoint that
    // answered. When several saved TVs share it, the scan cannot say which one
    // answered, so each keeps its own row rather than vanishing into one.
    const shared = (identitiesAt.get(`${saved.host}:${saved.connectPort}`)?.size ?? 0) > 1;
    const atLiveEndpoint =
      live.connected &&
      live.host === saved.host &&
      live.connectPort === saved.connectPort;
    // The live row never stands in for a saved TV the live identity does not
    // match, so that TV keeps its own row instead of vanishing.
    const liveRowIsOther =
      atLiveEndpoint &&
      !savedDeviceMatchesConnection(saved, live.host, live.connectPort, live.hardwareId);
    if (answered && !shared && !liveRowIsOther) continue;
    rows.push({
      key: `saved:${identity}`,
      source: "saved",
      host: saved.host,
      name: saved.name,
      connectPorts: [saved.connectPort],
      pairingPorts: [],
      legacyConnectPorts: [],
      status: atLiveEndpoint
        ? savedDeviceIsLiveConnection(saved, savedDevices, live.host, live.connectPort, live.hardwareId)
          ? "connected"
          // Something answers at this saved address, but nothing shows it is
          // this TV, so the row claims the address and not the connection.
          : "saved-address"
        : answered
          ? "saved-address"
        // The address answered the scan on some other port. That is not the
        // same claim as "offline", and it is the common case after a TV
        // reboot rotates the wireless-debugging port.
        : (discovered?.connectPorts.size ?? 0) > 0
          ? "saved-other-port"
          : "saved-missing",
      savedTarget: saved,
    });
  }

  for (const [observationKey, observations] of metadata) {
    const host = observations[0].host.trim();
    const names = new Set(observations.map((item) => safeName(item.friendly_name)!));
    const networks = new Set(observations.map((item) => item.network?.trim() ?? ""));
    const ambiguous = names.size !== 1 || networks.size > 1;
    // A Cast/Remote port is never an ADB port. These rows cannot connect or
    // stand in for a saved TV, even when a name or address looks familiar.
    rows.push({
      key: `advertisement:${observationKey}`,
      source: "advertisement",
      host,
      name: !ambiguous ? [...names][0] : "Nearby device",
      advertisementAmbiguous: ambiguous,
      advertisedName: true,
      connectPorts: [], pairingPorts: [], legacyConnectPorts: [], status: "found",
    });
  }

  // mDNS answers arrive in whatever order the network produced, so without an
  // explicit order the list reshuffles between scans. Rank by how directly the
  // row is usable, then by recency, then by address so it is fully determinate.
  const statusRank: Record<DiscoveryRowStatus, number> = {
    connected: 0,
    "saved-verified": 1,
    "saved-address": 2,
    "saved-other-port": 3,
    // A TV that answered this scan outranks one that did not, even a saved one.
    found: 4,
    "saved-missing": 5,
  };
  rows.sort((a, b) => {
    const byStatus = statusRank[a.status] - statusRank[b.status];
    if (byStatus !== 0) return byStatus;
    const aUsed = a.savedTarget?.lastUsed ?? "";
    const bUsed = b.savedTarget?.lastUsed ?? "";
    if (aUsed !== bUsed) return aUsed < bUsed ? 1 : -1;
    // Numeric-aware so 192.168.42.71 sorts before 192.168.42.196.
    return a.host.localeCompare(b.host, undefined, { numeric: true });
  });

  return rows;
}
