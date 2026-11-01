// One app-level subscription to the backend's `support-tiers-updated` event,
// fanned out to whichever views show support tiers. A view that unmounted
// before the event arrived simply isn't listening; the next one to mount
// reads fresh tiers anyway.

import { listen } from "@tauri-apps/api/event";

const listeners = new Set<() => void>();
let subscribed = false;

function ensureSubscribed(): void {
  // Outside Tauri (tests, the browser demo) there is no event plugin, and
  // unsubscribing would throw.
  if (subscribed || typeof window === "undefined" || !("__TAURI_EVENT_PLUGIN_INTERNALS__" in window)) return;
  subscribed = true;
  listen("support-tiers-updated", () => {
    for (const listener of listeners) listener();
  }).catch(() => {
    subscribed = false;
  });
}

/// Call `listener` whenever fetched support rules replaced the ones in use.
/// Returns the unsubscribe function, for `onMount`'s cleanup.
export function onSupportTiersUpdated(listener: () => void): () => void {
  ensureSubscribed();
  listeners.add(listener);
  return () => listeners.delete(listener);
}
