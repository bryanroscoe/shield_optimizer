const KEY = "shieldopt.autoUpdate";

// Opt-in: off unless the user explicitly enables it. The build is unsigned, so
// silently downloading + installing a new version on launch (possibly mid-task)
// is surprising — the update is still surfaced via the badge / "Update now".
export function getAutoUpdate(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(KEY) === "true";
}

export function setAutoUpdate(enabled: boolean): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(KEY, String(enabled));
  }
}

const REMOTE_COMPAT_KEY = "shieldopt.remoteForceShell";

/// When true, the Remote tab skips the fast scrcpy channel and uses the slow
/// `input` transport — the escape hatch for devices where the channel misbehaves.
export function getRemoteForceShell(): boolean {
  if (typeof localStorage === "undefined") return false;
  return localStorage.getItem(REMOTE_COMPAT_KEY) === "true";
}

export function setRemoteForceShell(enabled: boolean): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(REMOTE_COMPAT_KEY, String(enabled));
  }
}

const SHELL_BOOKMARKS_KEY = "shieldopt.shellBookmarks";

export interface ShellBookmark {
  label: string;
  command: string;
}

/// Bookmarks are local-only and shared across devices on purpose: the useful
/// ones ("list disabled packages", "dump the launcher") are about Android, not
/// about one TV, so scoping them per-serial would just make the user retype
/// them for every device they connect.
export function getShellBookmarks(): ShellBookmark[] {
  if (typeof localStorage === "undefined") return [];
  try {
    const raw = JSON.parse(localStorage.getItem(SHELL_BOOKMARKS_KEY) ?? "[]");
    if (!Array.isArray(raw)) return [];
    // Hand-edited or older localStorage payloads reach this unchecked, so each
    // row is validated rather than trusted into the UI.
    return raw.filter(
      (b): b is ShellBookmark =>
        !!b && typeof b.label === "string" && typeof b.command === "string",
    );
  } catch {
    return [];
  }
}

export function setShellBookmarks(bookmarks: ShellBookmark[]): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(SHELL_BOOKMARKS_KEY, JSON.stringify(bookmarks));
  }
}

const LAST_SEEN_VERSION_KEY = "shieldopt.lastSeenVersion";

/// The pre-2.3.1 key, written on every launch whether or not any notes were
/// shown. Read only to tell an existing install from a first run; see
/// `decideArrival` in `notes-seen.ts`.
export function getLegacyLastSeenVersion(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(LAST_SEEN_VERSION_KEY);
  } catch {
    return null;
  }
}

const NOTES_SEEN_VERSION_KEY = "shieldopt.notesSeenVersion";

/// The last version whose "what's new" pop-up the user actually saw. Written
/// only after it has been shown, so a launch that could not show it leaves the
/// pop-up owed rather than spent.
export function getNotesSeenVersion(): string | null {
  if (typeof localStorage === "undefined") return null;
  try {
    return localStorage.getItem(NOTES_SEEN_VERSION_KEY);
  } catch {
    return null;
  }
}

export function setNotesSeenVersion(version: string): void {
  if (typeof localStorage === "undefined") return;
  try {
    localStorage.setItem(NOTES_SEEN_VERSION_KEY, version);
  } catch {
    /* a blocked store only means the pop-up may show again */
  }
}

const KEEP_KEY = "shieldopt.keptPackages";

/// Packages the user has explicitly decided to keep, per device.
///
/// Keyed by hardware id (`ro.serialno`), never by address: two TVs can swap
/// IPs, and a decision about one must not silently apply to the other. A
/// device that cannot report an id gets no keep list rather than a shared one.
///
/// This is an opinion, not device state, which is why it lives here and not in
/// a snapshot — a snapshot restores what the TV was, and "I decided to keep
/// Plex" is not something the TV ever knew.
type KeepMap = Record<string, string[]>;

function readKeepMap(): KeepMap {
  if (typeof localStorage === "undefined") return {};
  try {
    const raw = localStorage.getItem(KEEP_KEY);
    if (!raw) return {};
    const parsed: unknown = JSON.parse(raw);
    if (typeof parsed !== "object" || parsed === null || Array.isArray(parsed)) return {};
    const out: KeepMap = {};
    for (const [id, pkgs] of Object.entries(parsed as Record<string, unknown>)) {
      if (Array.isArray(pkgs)) out[id] = pkgs.filter((p): p is string => typeof p === "string");
    }
    return out;
  } catch {
    return {};
  }
}

/// Placeholder serials identify nothing, so they are never a storage key.
export function idKey(hardwareId: string | null | undefined): string | null {
  const id = hardwareId?.trim() ?? "";
  return id && id.toLowerCase() !== "unknown" ? id : null;
}

export function getKeptPackages(raw: string | null | undefined): Set<string> {
  const hardwareId = idKey(raw);
  if (!hardwareId) return new Set();
  return new Set(readKeepMap()[hardwareId] ?? []);
}

export function setPackageKept(
  raw: string | null | undefined,
  pkg: string,
  kept: boolean,
): Set<string> {
  const hardwareId = idKey(raw);
  const current = getKeptPackages(hardwareId);
  if (!hardwareId) return current;
  if (kept) current.add(pkg);
  else current.delete(pkg);
  const map = readKeepMap();
  if (current.size === 0) delete map[hardwareId];
  else map[hardwareId] = [...current].sort();
  try {
    localStorage.setItem(KEEP_KEY, JSON.stringify(map));
  } catch {
    /* storage unavailable — the decision just will not survive a restart */
  }
  return current;
}

const SHELL_ACK_KEY = "shieldOptimizer.expertShellAcknowledged";

function readAckSet(): Set<string> {
  try {
    const raw = localStorage.getItem(SHELL_ACK_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed)
      ? new Set(parsed.filter((v): v is string => typeof v === "string"))
      : new Set();
  } catch {
    return new Set();
  }
}

/// The key this TV's shell consent is filed under: its hardware id, the same
/// rule the keep decisions follow. Consenting to arbitrary shell on the
/// living-room Shield must not silently consent for whatever else later
/// answers on that IP, so a TV with no readable `ro.serialno` gets no key at
/// all. Its tick lives only in the device page's state and is asked again
/// next time.
function shellAckKey(hardwareId: string | null | undefined): string | null {
  return idKey(hardwareId);
}

/// Whether expert shell has been acknowledged for this TV before.
export function getShellAcknowledged(hardwareId: string | null | undefined): boolean {
  const key = shellAckKey(hardwareId);
  if (!key) return false;
  return readAckSet().has(key);
}

export function setShellAcknowledged(
  hardwareId: string | null | undefined,
  acknowledged: boolean,
): void {
  const key = shellAckKey(hardwareId);
  if (!key) return;
  const set = readAckSet();
  if (acknowledged) set.add(key);
  else set.delete(key);
  try {
    localStorage.setItem(SHELL_ACK_KEY, JSON.stringify([...set].sort()));
  } catch {
    /* storage unavailable — the box just has to be ticked again next time */
  }
}

const OPEN_NON_TV_KEY = "shieldopt.openNonTv";

/// Devices that reported they are not an Android TV, but that the user chose
/// to open anyway (#120).
///
/// Hardware id only, with no address fallback: the choice is only offered to a
/// device that answered the TV question, so it always had an id to answer
/// with. Whatever answers on the same IP later has to earn its own.
export function getOpenNonTvIds(): Set<string> {
  try {
    const raw = localStorage.getItem(OPEN_NON_TV_KEY);
    if (!raw) return new Set();
    const parsed = JSON.parse(raw);
    return Array.isArray(parsed)
      ? new Set(parsed.filter((v): v is string => typeof v === "string"))
      : new Set();
  } catch {
    return new Set();
  }
}

export function setOpenNonTv(hardwareId: string | null | undefined, open: boolean): Set<string> {
  const set = getOpenNonTvIds();
  const key = idKey(hardwareId);
  if (!key) return set;
  if (open) set.add(key);
  else set.delete(key);
  try {
    localStorage.setItem(OPEN_NON_TV_KEY, JSON.stringify([...set].sort()));
  } catch {
    /* storage unavailable — the choice lasts until the app restarts */
  }
  return set;
}
