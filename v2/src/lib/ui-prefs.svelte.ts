// Theme and auto-update as one shared view, so the header's theme switch and
// Settings show the same choice. Both persist in localStorage (theme.ts,
// prefs.ts); this only keeps every control on screen in step.
import { getThemePref, setThemePref, type ThemePref } from "$lib/theme";
import { getAutoUpdate, setAutoUpdate } from "$lib/prefs";

export const uiPrefs = $state({
  theme: "system" as ThemePref,
  autoUpdate: false,
});

export function loadUiPrefs(): void {
  uiPrefs.theme = getThemePref();
  uiPrefs.autoUpdate = getAutoUpdate();
}

export function chooseTheme(pref: ThemePref): void {
  uiPrefs.theme = pref;
  setThemePref(pref);
}

export function chooseAutoUpdate(enabled: boolean): void {
  uiPrefs.autoUpdate = enabled;
  setAutoUpdate(enabled);
}

export const THEMES: { id: ThemePref; label: string; title: string }[] = [
  { id: "system", label: "Auto", title: "Follow the system appearance" },
  { id: "light", label: "Light", title: "Always light" },
  { id: "dark", label: "Dark", title: "Always dark" },
];
