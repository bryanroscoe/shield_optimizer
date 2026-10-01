import type { Discovery, SavedDevice } from "./types";
import {
  normalizeHardwareId,
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
  source: "discovery" | "saved";
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
}

type HostGroup = {
  host: string;
  names: Set<string>;
  instanceNames: Set<string>;
  connectPorts: Set<number>;
  pairingPorts: Set<number>;
  legacyConnectPorts: Set<number>;
};

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
function advertisedMatch(
  instanceNames: Set<string>,
  hardwareId: string | undefined,
): "exact" | "suffixed" | null {
  const wanted = normalizeHardwareId(hardwareId)?.toLowerCase();
  if (!wanted) return null;
  let match: "suffixed" | null = null;
  for (const instance of instanceNames) {
    const serial = advertisedSerial(instance)?.toLowerCase();
    if (!serial) continue;
    if (serial === wanted) return "exact";
    if (
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
  const hosts = new Map<string, HostGroup>();
  for (const discovery of discoveries) {
    const host = discovery.host.trim();
    if (!host || !validPort(discovery.port)) continue;
    const group = hosts.get(host) ?? {
      host,
      names: new Set<string>(),
      instanceNames: new Set<string>(),
      connectPorts: new Set<number>(),
      pairingPorts: new Set<number>(),
      legacyConnectPorts: new Set<number>(),
    };
    const name = discovery.name?.trim();
    if (name) group.instanceNames.add(name);
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
  // row further down -- that is what produced duplicate reconnect entries.
  const verified = new Set<SavedDevice>();
  for (const group of hosts.values()) {
    const connectPorts = [...group.connectPorts].sort((a, b) => a - b);
    const isLive =
      live.connected &&
      live.host === group.host &&
      connectPorts.includes(live.connectPort);
    const savedMatch =
      savedDevices.find(
        (saved) => advertisedMatch(group.instanceNames, saved.hardwareId) === "exact",
      ) ??
      savedDevices.find(
        (saved) => advertisedMatch(group.instanceNames, saved.hardwareId) === "suffixed",
      );
    if (savedMatch) verified.add(savedMatch);
    rows.push({
      key: `discovery:${group.host}`,
      source: "discovery",
      host: group.host,
      // A serial match is verified identity, so the name this TV was saved
      // under is the honest label -- more useful than a generic fallback when
      // several TVs advertise nothing but their adb id.
      name: savedMatch?.name ?? displayName(group.names),
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
  const seen = new Set<string>();
  for (const saved of savedDevices) {
    const identity = savedDeviceKey(saved);
    if (seen.has(identity)) continue;
    seen.add(identity);
    // Already named on a discovery row via its advertised serial.
    if (verified.has(saved)) continue;
    const discovered = hosts.get(saved.host);
    if (discovered?.connectPorts.has(saved.connectPort)) continue;
    const atLiveEndpoint =
      live.connected &&
      live.host === saved.host &&
      live.connectPort === saved.connectPort;
    rows.push({
      key: `saved:${identity}`,
      source: "saved",
      host: saved.host,
      name: saved.name,
      connectPorts: [saved.connectPort],
      pairingPorts: [],
      legacyConnectPorts: [],
      status: atLiveEndpoint
        ? savedDeviceMatchesConnection(saved, live.host, live.connectPort, live.hardwareId)
          ? "connected"
          // Something answers at this saved address, but nothing shows it is
          // this TV, so the row claims the address and not the connection.
          : "saved-address"
        // The address answered the scan on some other port. That is not the
        // same claim as "offline", and it is the common case after a TV
        // reboot rotates the wireless-debugging port.
        : (discovered?.connectPorts.size ?? 0) > 0
          ? "saved-other-port"
          : "saved-missing",
      savedTarget: saved,
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
