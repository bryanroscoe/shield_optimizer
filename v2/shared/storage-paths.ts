// The TV's user storage: internal storage at /sdcard, plus USB drives and SD cards mounted as
// portable storage at /storage/<fs uuid>. Mirrors crates/core commands::files, which enforces
// the same bound; these only keep buttons off paths the backend would refuse.

export const INTERNAL_STORAGE = "/sdcard";

/// A removable volume's folder name under /storage (crates/core `is_volume_name`): the
/// filesystem UUID Android names a USB drive or SD card by. `emulated` and `self` are the
/// internal storage's own aliases.
export function isVolumeName(name: string): boolean {
  return /^[A-Za-z0-9][A-Za-z0-9-]{0,63}$/.test(name) && name !== "emulated" && name !== "self";
}

/// The user-storage root a path is in: "/sdcard", "/storage/<uuid>" for a USB drive, or null.
/// Mirrors crates/core `user_storage_root`.
export function storageRootOf(path: string): string | null {
  if (path === INTERNAL_STORAGE || path.startsWith(`${INTERNAL_STORAGE}/`)) return INTERNAL_STORAGE;
  if (!path.startsWith("/storage/")) return null;
  const name = path.slice("/storage/".length).split("/")[0];
  return isVolumeName(name) ? `/storage/${name}` : null;
}
