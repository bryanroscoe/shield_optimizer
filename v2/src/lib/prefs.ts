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
