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
