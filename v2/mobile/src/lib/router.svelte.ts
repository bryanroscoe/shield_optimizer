// A small screen-stack router replacing the old single `currentScreen` string.
// Navigation pushes/replaces/pops a stack; the Android system/gesture back
// button is intercepted via the webview history (popstate) so it pops the
// stack instead of exiting the app on the first press.

export type Screen =
  | "onboarding"
  | "dashboard"
  | "diagnostics"
  | "optimize"
  | "apps"
  | "remote"
  | "more"
  // Detail screens reached from More / Dashboard (not bottom tabs).
  | "launcher"
  | "tweaks"
  | "snapshots"
  | "devices"
  | "riskguide"
  | "files"
  | "backups";

/// Screens that live behind the bottom tab bar (top-level). Everything else
/// (e.g. diagnostics) is a detail screen reached from the dashboard.
const TABS = new Set<Screen>(["dashboard", "optimize", "apps", "remote", "more"]);

function pushHistory(depth: number): void {
  try {
    history.pushState({ d: depth }, "");
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
    this.stack = [screen];
  }

  /// Tab-aware navigation. Tabs re-root the stack under the dashboard so the
  /// Home tab always lands on the dashboard and back from any tab returns
  /// there; detail screens push onto the stack.
  navigate(screen: Screen): void {
    if (screen === this.current) return;
    if (TABS.has(screen)) {
      const next: Screen[] =
        screen === "dashboard" ? ["dashboard"] : ["dashboard", screen];
      const deeper = next.length > this.stack.length;
      this.stack = next;
      if (deeper) pushHistory(this.stack.length);
    } else {
      this.stack = [...this.stack, screen];
      pushHistory(this.stack.length);
    }
  }

  push(screen: Screen): void {
    this.stack = [...this.stack, screen];
    pushHistory(this.stack.length);
  }

  replace(screen: Screen): void {
    this.stack = [...this.stack.slice(0, -1), screen];
  }

  /// Pop one level. Returns false when already at the root (nothing to pop).
  back(): boolean {
    if (this.stack.length > 1) {
      this.stack = this.stack.slice(0, -1);
      return true;
    }
    return false;
  }
}

export const router = new Router();

/// Wire the hardware/gesture back button. The Android back press pops the
/// webview history and fires `popstate`; we consume it to pop our own stack
/// and re-seed an entry so the next press is caught too. At the root there's
/// nothing to pop, so the press falls through to the OS (backgrounds the app).
export function initBackHandler(): () => void {
  const onPop = () => {
    if (router.back()) {
      pushHistory(router.stack.length);
    }
  };
  window.addEventListener("popstate", onPop);
  pushHistory(router.stack.length);
  return () => window.removeEventListener("popstate", onPop);
}
