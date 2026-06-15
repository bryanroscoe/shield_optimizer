const KEY = "shieldopt.autoUpdate";

export function getAutoUpdate(): boolean {
  if (typeof localStorage === "undefined") return true;
  return localStorage.getItem(KEY) !== "false";
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
