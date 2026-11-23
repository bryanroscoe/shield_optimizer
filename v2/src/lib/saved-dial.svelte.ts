import type { DialState } from "./saved-devices";

/// Saved-device dial state for the app session. It outlives the Devices
/// screen, so leaving it for a TV's tools and coming back neither redials
/// every saved address nor forgets which ones didn't answer. Launch dials
/// once; after that only Connect does.
export const savedDialSession: { dial: Record<string, DialState>; launched: boolean } = $state({
  dial: {},
  launched: false,
});

/// Not reactive: which dials are running and whose MAC was already asked.
export const savedDialWork = { inFlight: new Set<string>(), macRead: new Set<string>() };
