// "Stop ADB when quitting" as the native side last confirmed it. It lives here
// rather than in the Devices page so every instance of that page shares one
// view: a reply that lands after its page has closed still counts, and an older
// reply can never replace a newer one, on screen or in the saved choice.
import { api } from "$lib/api";
import { getStopAdbOnQuit, setStopAdbOnQuit } from "$lib/prefs";
import type { StopAdbOnQuit } from "$lib/types";

/// Shown beside the switch wherever it appears (Devices and Settings).
export const QUIT_ADB_TIP =
  "Quitting stops the adb server for everything on this computer, so Android Studio, scrcpy or " +
  "any other tool using adb is disconnected too. It is left running while this app is still " +
  "using it, such as an open Remote, and when the app restarts for an update.";

export const stopOnQuit = $state({
  /// null whenever the setting is unconfirmed: before the first read, while a
  /// change is on its way, and after a read fails. The stop may be on then.
  setting: null as StopAdbOnQuit | null,
  /// Reads and changes still waiting for their reply.
  pending: 0,
  error: "",
});

// Every applied change raises the native generation, so the reply carrying
// the highest one is the newest state.
let newest = 0;
// Changes asked for so far. A reply can only confirm the setting when no change
// was asked for after its own request: until that change answers, the native
// side may already be past what the reply says.
let changes = 0;
// One call at a time, so the native side sees and answers them in the order
// they were made.
let queue: Promise<unknown> = Promise.resolve();

function inTurn<T>(call: () => Promise<T>): Promise<T> {
  const turn = queue.then(call);
  queue = turn.catch(() => {});
  return turn;
}

function adopt(next: StopAdbOnQuit, asked: number) {
  if (asked !== changes || next.generation < newest) return;
  newest = next.generation;
  stopOnQuit.setting = next;
  setStopAdbOnQuit(next.enabled);
}

async function read() {
  stopOnQuit.pending++;
  let asked = changes;
  try {
    let current = await inTurn(() => api.stopAdbOnQuit());
    // The native setting starts off with every launch; hand the saved choice
    // back the first time this run is read.
    if (current.generation === 0 && getStopAdbOnQuit() && asked === changes) {
      asked = ++changes;
      stopOnQuit.setting = null;
      current = await inTurn(() => api.setStopAdbOnQuit(true, 0));
    }
    adopt(current, asked);
  } catch (e) {
    // A change asked for since then answers for itself.
    if (asked === changes) {
      stopOnQuit.setting = null;
      stopOnQuit.error = `Couldn't read Stop ADB when quitting, so it may still be on: ${e}`;
    }
  } finally {
    stopOnQuit.pending--;
  }
}

export function refreshStopOnQuit(): Promise<void> {
  stopOnQuit.error = "";
  return read();
}

export async function toggleStopOnQuit() {
  const from = stopOnQuit.setting;
  if (!from) return;
  const asked = ++changes;
  stopOnQuit.setting = null;
  stopOnQuit.error = "";
  stopOnQuit.pending++;
  try {
    adopt(await inTurn(() => api.setStopAdbOnQuit(!from.enabled, from.generation)), asked);
  } catch (e) {
    stopOnQuit.error = `Couldn't change Stop ADB when quitting: ${e}`;
    // The change may or may not have landed. A new read, queued behind it,
    // says where it stands.
    await read();
  } finally {
    stopOnQuit.pending--;
  }
}
