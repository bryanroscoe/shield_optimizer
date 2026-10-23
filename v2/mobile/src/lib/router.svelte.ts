// A small screen-stack router replacing the old single `currentScreen` string.
// Navigation pushes/replaces/pops a stack; the Android system/gesture back
// button is intercepted via the webview history (popstate) so it pops the
// stack instead of exiting the app on the first press.

import { session } from "./session.svelte";

export type Screen =
  | "onboarding"
  // Onboarding pushed from Devices to add a TV; never auto-dials.
  | "addtv"
  // Bottom tabs.
  | "dashboard"
  | "apps"
  | "tweaks"
  | "remote"
  | "more"
  // Detail screens reached from Dashboard / More (not bottom tabs). Open any
  // of them with `router.navigate(screen)` (or a screen's `navigate` prop),
  // which pushes it so Back returns to where the user came from.
  | "diagnostics"
  | "optimize"
  | "launcher"
  | "snapshots"
  | "devices"
  | "riskguide"
  | "files"
  | "backups";

/// Screens that live behind the bottom tab bar (top-level). Everything else
/// (Optimize, Diagnostics, Launcher, …) is a detail screen pushed on top.
export const TAB_SCREENS: readonly Screen[] = ["dashboard", "apps", "tweaks", "remote", "more"];
const TABS = new Set<Screen>(TAB_SCREENS);

export function isTabScreen(screen: Screen): boolean {
  return TABS.has(screen);
}

// The Android back gesture reaches us as `popstate` only while the webview
// has a history entry to pop. We keep exactly ONE spare entry while the stack
// is deeper than the root, and none at the root, so a root press falls
// through to the OS immediately instead of eating one dead press per prior
// navigation.
let hasSpare = false;
let ignorePops = 0;

function syncHistory(depth: number): void {
  const wantSpare = depth > 1;
  try {
    if (wantSpare && !hasSpare) {
      history.pushState({ d: depth }, "");
      hasSpare = true;
    } else if (!wantSpare && hasSpare) {
      hasSpare = false;
      ignorePops += 1;
      history.back();
    }
  } catch {
    // history unavailable (SSR/tests) — in-app back still works
  }
}

class Router {
  stack = $state<Screen[]>(["onboarding"]);

  get current(): Screen {
    return this.stack[this.stack.length - 1];
  }

  /// Replace the whole stack — used when entering/leaving the connected
  /// session (onboarding <-> dashboard).
  reset(screen: Screen): void {
    if (session.applyInProgress) return;
    this.stack = [screen];
    syncHistory(this.stack.length);
  }

  /// Tab-aware navigation. Tabs re-root the stack under the dashboard so the
  /// Home tab always lands on the dashboard and back from any tab returns
  /// there; detail screens push onto the stack.
  navigate(screen: Screen): void {
    if (session.applyInProgress) return;
    if (screen === this.current) return;
    if (TABS.has(screen)) {
      this.stack =
        screen === "dashboard" ? ["dashboard"] : ["dashboard", screen];
    } else {
      this.stack = [...this.stack, screen];
    }
    syncHistory(this.stack.length);
  }

  push(screen: Screen): void {
    if (session.applyInProgress) return;
    this.stack = [...this.stack, screen];
    syncHistory(this.stack.length);
  }

  replace(screen: Screen): void {
    if (session.applyInProgress) return;
    this.stack = [...this.stack.slice(0, -1), screen];
  }

  /// Pop one level. Returns false when already at the root (nothing to pop).
  back(): boolean {
    if (session.applyInProgress) {
      syncHistory(this.stack.length);
      return true;
    }
    if (this.stack.length > 1) {
      this.stack = this.stack.slice(0, -1);
      syncHistory(this.stack.length);
      return true;
    }
    return false;
  }
}

export const router = new Router();

/// Wire the hardware/gesture back button. A user press consumes the spare
/// history entry and fires `popstate`; we pop our stack and `syncHistory`
/// re-seeds the spare only while we are still below the root.
export function initBackHandler(): () => void {
  const onPop = () => {
    if (ignorePops > 0) {
      ignorePops -= 1;
      return;
    }
    hasSpare = false;
    router.back();
  };
  window.addEventListener("popstate", onPop);
  syncHistory(router.stack.length);
  return () => window.removeEventListener("popstate", onPop);
}
