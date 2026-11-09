// A small screen-stack router.
// Navigation pushes/replaces/pops a stack. The Android system/gesture back
// button pops the stack (or runs a screen's own step back) while there is
// somewhere to go, and is left to Android at the root.

import { untrack } from "svelte";
import { isTauri, type PluginListener } from "@tauri-apps/api/core";
import { onBackButtonPress } from "@tauri-apps/api/app";
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
  | "catalog"
  | "files"
  | "logs"
  | "backups"
  | "sideload";

/// Screens that live behind the bottom tab bar (top-level). Everything else
/// (Optimize, Diagnostics, Launcher, …) is a detail screen pushed on top.
export const TAB_SCREENS: readonly Screen[] = ["dashboard", "apps", "tweaks", "remote", "more"];
const TABS = new Set<Screen>(TAB_SCREENS);

export function isTabScreen(screen: Screen): boolean {
  return TABS.has(screen);
}

// In the app, Back arrives as the core app plugin's native `back-button`
// event. It is not done with webview history: an entry re-pushed from the
// `popstate` handler has no user activation, and Chromium's history
// manipulation intervention marks such entries skippable, so the next Back
// jumped past it and past the page and Android backgrounded the app.
// A browser (the tests, the demo) has no native event and keeps the history
// fallback below.
let native = isTauri();
/// `initBackHandler` is live; until then nothing is registered.
let started = false;
let nativeListener: PluginListener | null = null;
/// A register or unregister call is in flight; it re-syncs when it lands.
let nativeBusy = false;

// History fallback: the back gesture reaches us as `popstate` only while the
// webview has a history entry to pop. We keep exactly ONE spare entry while
// there is something to go back to, and none at the root, so a root press
// falls through to the OS immediately instead of eating one dead press per
// prior navigation.
let hasSpare = false;
let ignorePops = 0;
/// A screen's own step back (onboarding's pairing and approval wait). While
/// set, Back runs it instead of popping, even at the root.
let localBack: (() => void) | null = null;
/// The stack depth last synced, kept outside the reactive stack: a screen's
/// teardown reads state as it was before the change that unmounted it.
let syncedDepth = 1;

/// Whether Back has anything to do in the app: pop a level, run a screen's
/// own step back, or be swallowed while an apply is running.
function wantsBack(): boolean {
  return syncedDepth > 1 || localBack !== null || session.applyInProgress;
}

function syncBack(depth: number): void {
  syncedDepth = depth;
  if (native) syncNative();
  else syncHistory();
}

function syncNative(): void {
  if (nativeBusy) return;
  // At the root the listener is removed, not left to ignore presses: with no
  // listener AppPlugin hands Back to Android, which moves the task to the
  // background and keeps its state. (The `exit` command would finish the
  // activity and lose it.) A press that lands while the removal is in flight
  // still reaches stepBack, which finds nothing to do.
  const want = started && wantsBack();
  if (want && !nativeListener) {
    nativeBusy = true;
    onBackButtonPress(stepBack).then(
      (listener) => {
        nativeListener = listener;
        nativeBusy = false;
        syncNative();
      },
      () => {
        nativeBusy = false;
        native = false;
        syncHistory();
      },
    );
  } else if (!want && nativeListener) {
    const listener = nativeListener;
    nativeBusy = true;
    listener.unregister().then(
      () => {
        if (nativeListener === listener) nativeListener = null;
        nativeBusy = false;
        syncNative();
      },
      // Still registered: the next sync retries rather than looping here.
      () => {
        nativeBusy = false;
      },
    );
  }
}

function syncHistory(): void {
  const wantSpare = wantsBack();
  try {
    if (wantSpare && !hasSpare) {
      // A removal still in flight would pop this new entry (or step past the
      // page) instead of the old one; the pop handler re-syncs once it lands.
      if (ignorePops > 0) return;
      history.pushState({ d: syncedDepth }, "");
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

/// One Back press: a screen's own step back first, else pop the stack (which
/// swallows the press while an apply is running and does nothing at the root).
function stepBack(): void {
  const handler = localBack;
  if (handler && !session.applyInProgress) {
    handler();
    syncBack(router.stack.length);
    return;
  }
  router.back();
}

class Router {
  stack = $state<Screen[]>(["onboarding"]);
  catalogPackage = $state("");

  get current(): Screen {
    return this.stack[this.stack.length - 1];
  }

  /// Replace the whole stack — used when entering/leaving the connected
  /// session (onboarding <-> dashboard).
  reset(screen: Screen): void {
    if (session.applyInProgress) return;
    this.stack = [screen];
    syncBack(this.stack.length);
  }

  /// Tab-aware navigation. Tabs re-root the stack under the dashboard so the
  /// Home tab always lands on the dashboard and back from any tab returns
  /// there; detail screens push onto the stack.
  navigate(screen: Screen, packageQuery = ""): void {
    if (session.applyInProgress) return;
    if (screen === this.current) return;
    if (screen === "catalog") this.catalogPackage = packageQuery;
    if (TABS.has(screen)) {
      this.stack =
        screen === "dashboard" ? ["dashboard"] : ["dashboard", screen];
    } else {
      this.stack = [...this.stack, screen];
    }
    syncBack(this.stack.length);
  }

  push(screen: Screen): void {
    if (session.applyInProgress) return;
    this.stack = [...this.stack, screen];
    syncBack(this.stack.length);
  }

  replace(screen: Screen): void {
    if (session.applyInProgress) return;
    this.stack = [...this.stack.slice(0, -1), screen];
  }

  /// Pop one level. Returns false when already at the root (nothing to pop).
  back(): boolean {
    if (session.applyInProgress) {
      syncBack(this.stack.length);
      return true;
    }
    if (this.stack.length > 1) {
      this.stack = this.stack.slice(0, -1);
      syncBack(this.stack.length);
      return true;
    }
    return false;
  }
}

export const router = new Router();

/// Set or clear the screen-local Back handler. Replacing one handler with
/// another leaves the native listener or spare history entry alone; set it
/// from the screen body and clear it only when the screen goes away, never as
/// clear-then-set.
export function setLocalBack(handler: (() => void) | null): void {
  if (handler === localBack) return;
  localBack = handler;
  syncBack(syncedDepth);
}

/// Wire the hardware/gesture back button: the native listener in the app, or
/// the spare history entry in a browser, where a press consumes the entry and
/// fires `popstate` and `syncHistory` re-seeds it while there is still
/// somewhere to go.
export function initBackHandler(): () => void {
  const onPop = () => {
    if (native) return;
    if (ignorePops > 0) {
      ignorePops -= 1;
      if (ignorePops === 0) syncHistory();
      return;
    }
    hasSpare = false;
    stepBack();
  };
  window.addEventListener("popstate", onPop);
  started = true;
  syncBack(router.stack.length);
  // An apply swallows Back even where nothing could be popped.
  const stopApplyWatch = $effect.root(() => {
    $effect(() => {
      void session.applyInProgress;
      untrack(() => syncBack(syncedDepth));
    });
  });
  return () => {
    stopApplyWatch();
    window.removeEventListener("popstate", onPop);
    started = false;
    if (native) syncNative();
  };
}
