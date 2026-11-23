// Expert-shell storage on the phone: custom bookmarks, and the per-TV consent
// that enables Run. Same rules as desktop's `v2/src/lib/prefs.ts`.

import { parseShellBookmarks, type ShellBookmark } from "../../../shared/shell";
import { normalizeHardwareId } from "./identity";

const BOOKMARKS_KEY = "shieldopt.shellBookmarks";
const ACK_KEY = "shieldOptimizer.expertShellAcknowledged";

/// Bookmarks are about Android, not one TV, so they are shared across TVs.
export function getShellBookmarks(): ShellBookmark[] {
  try {
    return parseShellBookmarks(JSON.parse(localStorage.getItem(BOOKMARKS_KEY) ?? "[]"));
  } catch {
    return [];
  }
}

/// Throws when storage refuses the write, so the screen can say the change
/// wasn't saved instead of showing a bookmark that disappears on restart.
export function setShellBookmarks(bookmarks: ShellBookmark[]): void {
  localStorage.setItem(BOOKMARKS_KEY, JSON.stringify(bookmarks));
}

function readAckSet(): Set<string> {
  try {
    const parsed = JSON.parse(localStorage.getItem(ACK_KEY) ?? "[]");
    return Array.isArray(parsed)
      ? new Set(parsed.filter((v): v is string => typeof v === "string"))
      : new Set();
  } catch {
    return new Set();
  }
}

/// Consent is filed under the TV's verified hardware id and nothing else.
/// Consenting on the living-room Shield must not consent for whatever later
/// answers at that address, so a TV with no readable id is never remembered:
/// its tick lives only on the open screen and is asked again next time.
export function getShellAcknowledged(hardwareId: string | null | undefined): boolean {
  const id = normalizeHardwareId(hardwareId);
  return id ? readAckSet().has(id) : false;
}

export function setShellAcknowledged(
  hardwareId: string | null | undefined,
  acknowledged: boolean,
): void {
  const id = normalizeHardwareId(hardwareId);
  if (!id) return;
  const set = readAckSet();
  if (acknowledged) set.add(id);
  else set.delete(id);
  try {
    localStorage.setItem(ACK_KEY, JSON.stringify([...set].sort()));
  } catch {
    /* storage unavailable: the box has to be ticked again next time */
  }
}
