// The app's right-click menu (#129).
//
// Every webview ships its own default menu — WebKit on macOS, WebView2 on
// Windows, WebKitGTK on Linux — and on an app screen it offers "Open Link in
// New Window", "Download Linked File" and the like, which mean nothing here
// and can do odd things with in-app routes. Release builds suppress it
// everywhere except where it is genuinely useful (text fields, the Shell
// output, selected text), and the few places where a right-click has an
// obvious meaning get a small app menu instead.
//
// One open menu at a time, held here; `ContextMenu.svelte` renders it.

export interface MenuItem {
  label: string;
  /// May return a short confirmation ("Copied IP address") to flash after the
  /// menu closes.
  run: () => void | string | null | Promise<void | string | null>;
  disabled?: boolean;
  danger?: boolean;
}

export interface OpenMenu {
  x: number;
  y: number;
  items: MenuItem[];
  /// Where focus goes back to when the menu closes.
  returnFocus: HTMLElement | null;
}

type Listener = (menu: OpenMenu | null) => void;

let current: OpenMenu | null = null;
const listeners = new Set<Listener>();

function emit() {
  for (const l of listeners) l(current);
}

export function subscribeContextMenu(listener: Listener): () => void {
  listeners.add(listener);
  listener(current);
  return () => listeners.delete(listener);
}

export function closeContextMenu() {
  if (!current) return;
  current = null;
  emit();
}

/// Elements whose own menu is worth keeping: anything you can type in, raw
/// output blocks (`pre`: adb error details, the Shell output), and any element
/// that opts in with `data-native-contextmenu`.
const NATIVE_SELECTOR =
  'pre, input, textarea, select, [contenteditable=""], [contenteditable="true"], [contenteditable="plaintext-only"], [data-native-contextmenu]';

function elementOf(target: EventTarget | null): Element | null {
  if (target instanceof Element) return target;
  if (target instanceof Node) return target.parentElement;
  return null;
}

export function isNativeMenuTarget(target: EventTarget | null): boolean {
  return !!elementOf(target)?.closest(NATIVE_SELECTOR);
}

/// The text the user has selected, if any. A right-click over a selection is
/// almost always "copy this", so the app menu leads with Copy.
export function selectedText(): string {
  if (typeof window === "undefined") return "";
  return window.getSelection()?.toString() ?? "";
}

function copySelectionItem(text: string): MenuItem {
  return {
    label: "Copy",
    run: async () => {
      await navigator.clipboard.writeText(text);
      return "Copied";
    },
  };
}

/// Open the app menu for a `contextmenu` event. Returns false, and leaves the
/// event alone, when the native menu should show instead.
///
/// In dev builds Shift+right-click falls through to the webview's own menu, so
/// Inspect Element is still one click away on the rows that carry a menu.
export function openContextMenu(e: MouseEvent, items: MenuItem[]): boolean {
  // Already claimed by a menu nested inside this element.
  if (e.defaultPrevented) return false;
  if (isNativeMenuTarget(e.target)) return false;
  if (!import.meta.env.PROD && e.shiftKey) return false;
  const text = selectedText();
  const all = text ? [copySelectionItem(text), ...items] : items;
  if (all.length === 0) return false;
  e.preventDefault();

  const origin = elementOf(e.currentTarget) ?? elementOf(e.target);
  let x = e.clientX;
  let y = e.clientY;
  // The Menu key and Shift+F10 fire `contextmenu` with no pointer position
  // (0,0, or -1 for the button on WebView2). Anchor it to the element instead.
  if ((x === 0 && y === 0) || e.button === -1) {
    const r = origin?.getBoundingClientRect();
    if (r) {
      x = r.left + Math.min(24, r.width / 2);
      y = r.top + Math.min(r.height, 32);
    }
  }
  const active = document.activeElement;
  current = {
    x,
    y,
    items: all,
    returnFocus:
      active instanceof HTMLElement && active !== document.body
        ? active
        : origin instanceof HTMLElement
          ? origin
          : null,
  };
  emit();
  return true;
}

/// Decide what one `contextmenu` event that no app menu claimed should do.
/// Exported for the test harness and the dev toggle; the listener below is the
/// only caller in the app.
export function shouldSuppressDefault(e: MouseEvent, suppress: boolean): boolean {
  if (!suppress || e.defaultPrevented) return false;
  if (isNativeMenuTarget(e.target)) return false;
  // Selected text anywhere else gets the app menu with Copy, never the
  // webview's link-and-share menu.
  if (selectedText()) return false;
  return true;
}

const DEV_SUPPRESS_KEY = "shieldopt.dev.suppressContextMenu";

/// Release builds suppress the default menu; dev builds keep it so Inspect
/// Element works. Setting `shieldopt.dev.suppressContextMenu` to "1" in a dev
/// build previews the release behaviour (and is what the test drives). A
/// production build never reads that key.
function suppressionWanted(): boolean {
  if (import.meta.env.PROD) return true;
  try {
    return localStorage.getItem(DEV_SUPPRESS_KEY) === "1";
  } catch {
    return false;
  }
}

/// Install the app-wide listener. Bubble phase on `document`, so a row's own
/// `oncontextmenu` (which opens the app menu and calls `preventDefault`) has
/// already run. Returns the uninstaller.
export function installContextMenuGuard(): () => void {
  const suppress = suppressionWanted();
  const onContextMenu = (e: MouseEvent) => {
    if (e.defaultPrevented) return;
    if (suppress && !isNativeMenuTarget(e.target)) {
      const text = selectedText();
      if (text) {
        openContextMenu(e, []);
        return;
      }
    }
    if (shouldSuppressDefault(e, suppress)) {
      e.preventDefault();
      closeContextMenu();
    }
  };
  document.addEventListener("contextmenu", onContextMenu);
  return () => document.removeEventListener("contextmenu", onContextMenu);
}
