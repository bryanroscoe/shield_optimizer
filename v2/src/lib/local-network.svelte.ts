// macOS Local Network privacy, as the native probe last saw it. Shared so the
// Devices page, its error explanations and the app shell read one answer.
//
// The probe runs once at launch, which is also what makes macOS show its alert
// at a moment the person expects. While it says "blocked" it runs again when
// the window comes back to the front (after a trip to System Settings, or
// after answering the alert) and every few seconds, so the notice clears by
// itself once access is granted. A blocked probe sends nothing.
import { api } from "$lib/api";
import { isNoRoute, type LocalNetworkStatus } from "$lib/adb-errors";

export const localNetwork = $state({
  /// null until the first answer, and after a failed first read: unknown.
  status: null as LocalNetworkStatus | null,
  error: "",
});

const BLOCKED_RECHECK_MS = 5000;
let inFlight: Promise<LocalNetworkStatus | null> | null = null;

/// `target` (the TV a failure was about) is also tried from the app itself,
/// which can confirm access; such a check is never shared with another.
export function checkLocalNetwork(target: string | null = null): Promise<LocalNetworkStatus | null> {
  const run = () =>
    api
      .localNetworkAccess(target)
      .then((r) => (localNetwork.status = r.status))
      .catch(() => localNetwork.status);
  if (target) return run();
  inFlight ??= run().finally(() => {
    inFlight = null;
  });
  return inFlight;
}

/// The probe says what it saw now; a no-route failure asks again first (of
/// `target` too, when known) so its explanation is not based on a stale answer.
export async function freshLocalNetworkStatus(
  raw: string,
  target: string | null = null,
): Promise<LocalNetworkStatus | null> {
  if (!isNoRoute(raw)) return localNetwork.status;
  return checkLocalNetwork(target);
}

export async function openLocalNetworkSettings() {
  localNetwork.error = "";
  try {
    await api.openLocalNetworkSettings();
  } catch (e) {
    localNetwork.error = `Couldn't open System Settings (${e}). Open it from the Apple menu, then Privacy & Security → Local Network.`;
  }
}

/// Start at launch. Returns the cleanup.
export function watchLocalNetwork(): () => void {
  void checkLocalNetwork();
  const recheck = () => {
    if (localNetwork.status === "blocked") void checkLocalNetwork();
  };
  const onVisible = () => {
    if (document.visibilityState === "visible") recheck();
  };
  window.addEventListener("focus", recheck);
  document.addEventListener("visibilitychange", onVisible);
  const timer = setInterval(recheck, BLOCKED_RECHECK_MS);
  return () => {
    window.removeEventListener("focus", recheck);
    document.removeEventListener("visibilitychange", onVisible);
    clearInterval(timer);
  };
}
