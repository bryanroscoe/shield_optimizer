import { session } from "./session.svelte";

/// The TV and connection a confirm dialog was opened for. A destructive
/// action (Reboot, Emergency recovery) runs only on that exact connection;
/// `epoch` tells a silent reconnect to the same TV apart from a different TV,
/// so the refusal can say which happened.
export type TvTarget = { serial: string; generation: number; epoch: number; label: string };

export function currentTarget(): TvTarget {
  return {
    serial: session.serial,
    generation: session.generation,
    epoch: session.deviceEpoch,
    label: session.deviceLabel,
  };
}

/// Whether the session is still the same TV (it may have reconnected).
export function sameTv(target: Pick<TvTarget, "serial" | "epoch">): boolean {
  return session.serial === target.serial && session.deviceEpoch === target.epoch;
}

/// `current`: live on the very connection it was opened for. `reconnected`:
/// the same TV, live again after a reconnect, so the person should confirm
/// again. `offline`: the same TV, not answering now. `changed`: another TV
/// or none.
export function targetState(target: TvTarget): "current" | "reconnected" | "offline" | "changed" {
  if (!sameTv(target)) return "changed";
  if (!session.isConnected) return "offline";
  return session.generation === target.generation ? "current" : "reconnected";
}
