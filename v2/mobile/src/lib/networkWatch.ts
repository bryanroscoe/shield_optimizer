// The phone's network changed under an open TV connection: its default network
// was lost or replaced, or it lost an address. Emitted by the atv-adb plugin from
// an Android default-network callback (NETWORK_CHANGED_EVENT in
// tauri-plugin-atv-adb/src/lib.rs); the payload is `{ reason }`.

export const NETWORK_CHANGED_EVENT = "network-changed";

export type NetworkChangePayload = { reason?: string };

/// How long to let the network settle before probing the TV. A lost default
/// network is usually replaced within seconds (a Wi-Fi handover, or Wi-Fi giving
/// way to mobile data); probing sooner would spend the session's one silent
/// redial on a phone that has no route to the TV yet.
export function networkProbeDelayMs(reason: unknown): number {
  return reason === "lost" ? 4_000 : 1_000;
}

/// Trailing debounce: every change restarts the wait with its own delay, so a
/// burst (Wi-Fi lost, a new network, its addresses) probes once, after the last.
export function createNetworkProbe(probe: () => void): {
  notify(reason: unknown): void;
  cancel(): void;
} {
  let timer: ReturnType<typeof setTimeout> | undefined;
  return {
    notify(reason) {
      clearTimeout(timer);
      timer = setTimeout(() => {
        timer = undefined;
        probe();
      }, networkProbeDelayMs(reason));
    },
    cancel() {
      clearTimeout(timer);
      timer = undefined;
    },
  };
}
