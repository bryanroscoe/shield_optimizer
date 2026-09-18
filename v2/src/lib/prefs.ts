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

/// Version this machine last had a look at. Used to notice that an update
/// landed since the last launch, which is the only way someone with
/// auto-update on ever finds out what changed.
export function getLastSeenVersion(): string | null {
  if (typeof localStorage === "undefined") return null;
  return localStorage.getItem(LAST_SEEN_VERSION_KEY);
}

export function setLastSeenVersion(version: string): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(LAST_SEEN_VERSION_KEY, version);
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

export function getKeptPackages(hardwareId: string | null | undefined): Set<string> {
  if (!hardwareId) return new Set();
  return new Set(readKeepMap()[hardwareId] ?? []);
}

export function setPackageKept(
  hardwareId: string | null | undefined,
  pkg: string,
  kept: boolean,
): Set<string> {
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

/// Whether expert shell has been acknowledged for this TV before.
///
/// Keyed by hardware id, never by address — the same rule the keep decisions
/// follow. Consenting to arbitrary shell on the living-room Shield must not
/// silently consent for whatever else later answers on that IP. With no
/// hardware id we have not identified the device, so the answer is no and the
/// user ticks the box again.
export function getShellAcknowledged(hardwareId: string | null | undefined): boolean {
  if (!hardwareId) return false;
  return readAckSet().has(hardwareId);
}

export function setShellAcknowledged(
  hardwareId: string | null | undefined,
  acknowledged: boolean,
): void {
  if (!hardwareId) return;
  const set = readAckSet();
  if (acknowledged) set.add(hardwareId);
  else set.delete(hardwareId);
  try {
    localStorage.setItem(SHELL_ACK_KEY, JSON.stringify([...set].sort()));
  } catch {
    /* storage unavailable — the box just has to be ticked again next time */
  }
}
