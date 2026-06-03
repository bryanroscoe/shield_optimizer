// Theme preference: Light / Dark / System (follow OS).
//
// The actual colors live in CSS variables in +layout.svelte. This just drives
// a `data-theme` attribute on <html>:
//   - "system" → no attribute; CSS falls back to `prefers-color-scheme`
//   - "light" / "dark" → explicit override regardless of OS
// The initial value is also applied pre-paint by an inline script in app.html
// to avoid a flash of the wrong theme on load.

export type ThemePref = "system" | "light" | "dark";

const KEY = "shieldopt.theme";

export function getThemePref(): ThemePref {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(KEY);
  return v === "light" || v === "dark" ? v : "system";
}

export function applyTheme(pref: ThemePref): void {
  if (typeof document === "undefined") return;
  const root = document.documentElement;
  if (pref === "system") {
    root.removeAttribute("data-theme");
  } else {
    root.setAttribute("data-theme", pref);
  }
}

export function setThemePref(pref: ThemePref): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(KEY, pref);
  }
  applyTheme(pref);
}
