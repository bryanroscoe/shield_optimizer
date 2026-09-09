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
