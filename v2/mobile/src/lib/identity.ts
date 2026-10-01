import type { SavedDevice } from "./types";

/// The one rule for turning a reported or stored hardware id into identity,
/// matching desktop's `idKey` in `v2/src/lib/prefs.ts`: trim, and treat empty
/// or the "unknown" placeholder as no id at all. The placeholder is shared by
/// every TV that could not read its serial, so letting it through would make
/// unrelated TVs the same TV.
export function normalizeHardwareId(
  value: string | null | undefined,
): string | undefined {
  const id = typeof value === "string" ? value.trim() : "";
  return id && id.toLowerCase() !== "unknown" ? id : undefined;
}

/// Whether a saved row is the TV live on this connection: the same verified
/// hardware id, or, when the live TV reports none, an id-less row at the exact
/// endpoint. An identified row is never matched on its address alone.
export function savedDeviceMatchesConnection(
  device: SavedDevice,
  host: string,
  connectPort: number,
  hardwareId?: string | null,
): boolean {
  const connectedHardwareId = normalizeHardwareId(hardwareId);
  const savedHardwareId = normalizeHardwareId(device.hardwareId);
  if (connectedHardwareId) return savedHardwareId === connectedHardwareId;
  return (
    savedHardwareId === undefined &&
    device.host === host &&
    device.connectPort === connectPort
  );
}

/// Whether a saved row can be told apart as *the specific TV* live on this
/// connection, not just a row that matches the bare address. A verified
/// hardware id is always unambiguous -- ids are unique by construction. An
/// id-less match is only unambiguous when it is the single id-less row saved
/// at that exact endpoint; when another id-less TV shares it, the address
/// alone cannot say which of them answered, so neither counts as the live
/// row here.
export function savedDeviceIsLiveConnection(
  device: SavedDevice,
  allSaved: SavedDevice[],
  host: string,
  connectPort: number,
  hardwareId?: string | null,
): boolean {
  if (!savedDeviceMatchesConnection(device, host, connectPort, hardwareId)) return false;
  if (normalizeHardwareId(hardwareId)) return true;
  const idlessAtEndpoint = allSaved.filter(
    (other) =>
      normalizeHardwareId(other.hardwareId) === undefined &&
      other.host === host &&
      other.connectPort === connectPort,
  ).length;
  return idlessAtEndpoint <= 1;
}

export function savedDeviceKey(device: SavedDevice): string {
  const hardwareId = normalizeHardwareId(device.hardwareId);
  if (hardwareId) return `hardware:${hardwareId}`;
  // An id-less row is keyed by its own stable local id once it has one
  // (assigned by savedDevices.ts the first time it is persisted), so two
  // id-less TVs that share an endpoint stay distinct rows. The bare endpoint
  // is only a fallback for a row that has not gone through that migration.
  return device.localId
    ? `local:${device.localId}`
    : `idless:${device.host}:${device.connectPort}`;
}
