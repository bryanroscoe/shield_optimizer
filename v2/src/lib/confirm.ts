import { isTauri } from "@tauri-apps/api/core";
import { confirm as nativeConfirm } from "@tauri-apps/plugin-dialog";

// Never call window.confirm() directly. In the desktop app tauri-plugin-dialog
// replaces it with an async function, so `if (!confirm(...))` sees a Promise
// (truthy) and runs the action; the override also invokes a command
// (`plugin:dialog|confirm`) the plugin no longer has, so no dialog appears.
// The app asks through the plugin's `message` command instead. Demo mode and
// the Playwright tests run in a plain browser and use its own confirm.

let asking = false;

/// Ask before a consequential action. Resolves true only on an explicit OK; a
/// dialog that can't be shown counts as Cancel. While one question is open, a
/// second one (a double click) resolves false at once instead of queueing a
/// second run of the same action.
export async function confirmAction(message: string): Promise<boolean> {
  if (asking) return false;
  asking = true;
  try {
    if (isTauri()) return await nativeConfirm(message, { title: "ATV Optimizer", kind: "warning" });
    return (await window.confirm(message)) === true;
  } catch (e) {
    console.error("Couldn't show the confirmation dialog; nothing was changed.", e);
    return false;
  } finally {
    asking = false;
  }
}
