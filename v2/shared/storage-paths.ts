// The TV's user storage: internal storage at /sdcard, plus USB drives and SD cards mounted as
// portable storage at /storage/<fs uuid>. Mirrors crates/core commands::files, which enforces
// the same bound; these only keep buttons off paths the backend would refuse.

export const INTERNAL_STORAGE = "/sdcard";

/// A removable volume's folder name under /storage (crates/core `is_volume_name`): the
/// filesystem UUID Android names a USB drive or SD card by, as `1A2B-3C4D` (FAT/exFAT),
/// 16 hex digits (NTFS) or a dashed 8-4-4-4-12 UUID (ext4, f2fs).
export function isVolumeName(name: string): boolean {
  return /^(?:[0-9A-Fa-f]{4}-[0-9A-Fa-f]{4}|[0-9A-Fa-f]{16}|[0-9A-Fa-f]{8}(?:-[0-9A-Fa-f]{4}){3}-[0-9A-Fa-f]{12})$/.test(name);
}

/// The user-storage root a path is in: "/sdcard", "/storage/<uuid>" for a USB drive, or null.
/// Mirrors crates/core `user_storage_root`.
export function storageRootOf(path: string): string | null {
  if (path === INTERNAL_STORAGE || path.startsWith(`${INTERNAL_STORAGE}/`)) return INTERNAL_STORAGE;
  if (!path.startsWith("/storage/")) return null;
  const name = path.slice("/storage/".length).split("/")[0];
  return isVolumeName(name) ? `/storage/${name}` : null;
}
