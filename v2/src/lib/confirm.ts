import { isTauri } from "@tauri-apps/api/core";
import { confirm as nativeConfirm } from "@tauri-apps/plugin-dialog";

// Never call window.confirm() directly. In the desktop app tauri-plugin-dialog
// replaces it with an async function, so `if (!confirm(...))` sees a Promise
// (truthy) and runs the action; the override also invokes a command
// (`plugin:dialog|confirm`) the plugin no longer has, so no dialog appears.
// The app asks through the plugin's `message` command instead. Demo mode and
// the Playwright tests run in a plain browser and use its own confirm.

/// How long a question may stay unanswered before it counts as Cancel. Long
/// enough that nobody reading a warning hits it; without it, a dialog call
/// that never settles would refuse every later question until a restart.
export const CONFIRM_TIMEOUT_MS = 10 * 60_000;

let asking = false;

/// Ask before a consequential action. Resolves true only on an explicit OK; a
/// dialog that can't be shown, or one left unanswered for CONFIRM_TIMEOUT_MS,
/// counts as Cancel, and an answer arriving after that is ignored. While one
/// question is open, a second one (a double click) resolves false at once
/// instead of queueing a second run of the same action.
export async function confirmAction(message: string): Promise<boolean> {
  if (asking) return false;
  asking = true;
  let timer: ReturnType<typeof setTimeout> | undefined;
  const timedOut = new Promise<false>((resolve) => {
    timer = setTimeout(() => {
      console.error("The confirmation dialog never answered; treating it as Cancel.");
      resolve(false);
    }, CONFIRM_TIMEOUT_MS);
  });
  try {
    const answer = isTauri()
      ? nativeConfirm(message, { title: "ATV Optimizer", kind: "warning" })
      : Promise.resolve(window.confirm(message));
    return (await Promise.race([answer, timedOut])) === true;
  } catch (e) {
    console.error("Couldn't show the confirmation dialog; nothing was changed.", e);
    return false;
  } finally {
    clearTimeout(timer);
    asking = false;
  }
}
