// Theme preference: Light / Dark / System (follow OS).
//
// The actual colors live in CSS variables in +layout.svelte. This drives a
// `data-theme` attribute on <html>, which is always present and always
// concrete — "system" is resolved here rather than left to CSS. That lets the
// stylesheet carry exactly one light block instead of duplicating it between
// `[data-theme="light"]` and a `prefers-color-scheme` media query, which had
// already drifted apart once.
//
// The preference itself (which may be "system") stays in localStorage, so the
// Auto/Light/Dark toggle still reflects what the user actually chose.
// The initial value is also applied pre-paint by an inline script in app.html
// to avoid a flash of the wrong theme on load.

export type ThemePref = "system" | "light" | "dark";
export type ResolvedTheme = "light" | "dark";

const KEY = "shieldopt.theme";
const LIGHT_QUERY = "(prefers-color-scheme: light)";

export function getThemePref(): ThemePref {
  if (typeof localStorage === "undefined") return "system";
  const v = localStorage.getItem(KEY);
  return v === "light" || v === "dark" ? v : "system";
}

/// What the OS is asking for. Defaults to dark when the query is unavailable,
/// matching the app's historical default.
function osTheme(): ResolvedTheme {
  if (typeof window === "undefined" || !window.matchMedia) return "dark";
  return window.matchMedia(LIGHT_QUERY).matches ? "light" : "dark";
}

export function resolveTheme(pref: ThemePref): ResolvedTheme {
  return pref === "system" ? osTheme() : pref;
}

export function applyTheme(pref: ThemePref): void {
  if (typeof document === "undefined") return;
  document.documentElement.setAttribute("data-theme", resolveTheme(pref));
}

export function setThemePref(pref: ThemePref): void {
  if (typeof localStorage !== "undefined") {
    localStorage.setItem(KEY, pref);
  }
  applyTheme(pref);
}

/// Keep "system" live: re-apply when the OS flips appearance while the app is
/// open. Registered once; a no-op while an explicit light/dark is chosen.
export function watchOsTheme(): void {
  if (typeof window === "undefined" || !window.matchMedia) return;
  window.matchMedia(LIGHT_QUERY).addEventListener("change", () => {
    if (getThemePref() === "system") applyTheme("system");
  });
}
