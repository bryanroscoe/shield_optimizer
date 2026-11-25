// After an Optimize run, a TV still on its maker's home screen gets one quiet
// link to the Launcher screen. Both apps use this module so they agree on when
// the link shows and where a dismissal is kept.

/// The parts of a `list_launchers` row this needs.
export interface HomeRow {
  entry: { package: string };
  stock: boolean;
  setup_helper?: boolean;
}

/// Whether the TV's Home app is one of its stock launchers.
///
/// Null when that isn't known: the Home app couldn't be read, it isn't in the
/// list, or it is Setup Wraith. Unknown claims nothing, so it never shows the
/// link. A Home app outside the stock catalog counts as already changed.
export function onStockHome(
  rows: readonly HomeRow[] | null | undefined,
  currentPackage: string | null | undefined,
): boolean | null {
  const pkg = currentPackage?.trim();
  if (!rows || !pkg) return null;
  const row = rows.find((r) => r.entry.package === pkg);
  if (!row || row.setup_helper) return null;
  return row.stock;
}

const STORAGE_KEY = "atvopt.homeScreenOfferDismissed";

/// The key a dismissal is filed under: the TV's verified hardware id, never its
/// address, because whatever answers there later may be another TV. A TV with
/// no readable id has no key and is never remembered.
function idKey(hardwareId: string | null | undefined): string | null {
  const id = hardwareId?.trim();
  return id && id.toLowerCase() !== "unknown" ? id : null;
}

function readDismissed(storage: Storage): Set<string> {
  try {
    const parsed: unknown = JSON.parse(storage.getItem(STORAGE_KEY) ?? "[]");
    return Array.isArray(parsed)
      ? new Set(parsed.filter((v): v is string => typeof v === "string"))
      : new Set();
  } catch {
    return new Set();
  }
}

export function homeOfferDismissed(storage: Storage, hardwareId: string | null | undefined): boolean {
  const key = idKey(hardwareId);
  return key !== null && readDismissed(storage).has(key);
}

/// Remembers that this TV's owner doesn't want the link. Returns false when the
/// TV has no id or storage refused the write, so the caller hides it for the
/// open screen only.
export function dismissHomeOffer(storage: Storage, hardwareId: string | null | undefined): boolean {
  const key = idKey(hardwareId);
  if (!key) return false;
  const set = readDismissed(storage);
  set.add(key);
  try {
    storage.setItem(STORAGE_KEY, JSON.stringify([...set].sort()));
    return true;
  } catch {
    return false;
  }
}
