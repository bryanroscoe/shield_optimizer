import type { Discovery, SavedDevice } from "./types";

export type DiscoveryRowStatus =
  | "connected"
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
}

type HostGroup = {
  host: string;
  names: Set<string>;
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

function endpointKey(host: string, port: number): string {
  return `${host}:${port}`;
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
      connectPorts: new Set<number>(),
      pairingPorts: new Set<number>(),
      legacyConnectPorts: new Set<number>(),
    };
    const name = discovery.name?.trim();
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
  for (const group of hosts.values()) {
    const connectPorts = [...group.connectPorts].sort((a, b) => a - b);
    const isLive =
      live.connected &&
      live.host === group.host &&
      connectPorts.includes(live.connectPort);
    rows.push({
      key: `discovery:${group.host}`,
      source: "discovery",
      host: group.host,
      name: displayName(group.names),
      connectPorts,
      pairingPorts: [...group.pairingPorts].sort((a, b) => a - b),
      legacyConnectPorts: [...group.legacyConnectPorts].sort((a, b) => a - b),
      status: isLive
        ? "connected"
        : savedDevices.some((saved) => saved.host === group.host)
          ? "saved-address"
          : "found",
    });
  }

  const savedEndpoints = new Map<string, SavedDevice[]>();
  for (const saved of savedDevices) {
    const key = endpointKey(saved.host, saved.connectPort);
    const matches = savedEndpoints.get(key) ?? [];
    matches.push(saved);
    savedEndpoints.set(key, matches);
  }
  for (const [endpoint, matches] of savedEndpoints) {
    const first = matches[0];
    const discovered = hosts.get(first.host);
    if (discovered?.connectPorts.has(first.connectPort)) continue;
    const savedTarget = matches.length === 1
      ? first
      : {
          host: first.host,
          connectPort: first.connectPort,
          name: "Saved TV",
          deviceType: "unknown" as const,
          lastUsed: matches.reduce(
            (latest, saved) => saved.lastUsed > latest ? saved.lastUsed : latest,
            matches[0].lastUsed,
          ),
        };
    rows.push({
      key: `saved:${endpoint}`,
      source: "saved",
      host: first.host,
      name: savedTarget.name,
      connectPorts: [first.connectPort],
      pairingPorts: [],
      legacyConnectPorts: [],
      status:
        live.connected &&
        live.host === first.host &&
        live.connectPort === first.connectPort
          ? "connected"
          // The address answered the scan on some other port. That is not the
          // same claim as "offline", and it is the common case after a TV
          // reboot rotates the wireless-debugging port.
          : (discovered?.connectPorts.size ?? 0) > 0
            ? "saved-other-port"
            : "saved-missing",
      savedTarget,
    });
  }

  return rows;
}
