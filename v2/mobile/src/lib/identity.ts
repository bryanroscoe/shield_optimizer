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

export function savedDeviceKey(device: SavedDevice): string {
  const hardwareId = normalizeHardwareId(device.hardwareId);
  return hardwareId
    ? `hardware:${hardwareId}`
    : `idless:${device.host}:${device.connectPort}`;
}
