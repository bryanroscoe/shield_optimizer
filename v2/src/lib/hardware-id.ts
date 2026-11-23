/// Placeholder serials identify nothing, so they are never a storage key.
/// The same rule as mobile's `normalizeHardwareId` and core's
/// `verified_hardware_id`: trim, and treat empty or the "unknown" placeholder
/// as no id at all.
export function idKey(hardwareId: string | null | undefined): string | null {
  const id = hardwareId?.trim() ?? "";
  return id && id.toLowerCase() !== "unknown" ? id : null;
}
