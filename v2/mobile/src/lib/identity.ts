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

export function savedDeviceKey(device: SavedDevice): string {
  const hardwareId = normalizeHardwareId(device.hardwareId);
  return hardwareId
    ? `hardware:${hardwareId}`
    : `idless:${device.host}:${device.connectPort}`;
}
