import type { DialSession } from "./saved-devices";

/// Saved-device dial state for the app session. It outlives the Devices
/// screen, so leaving it for a TV's tools and coming back neither redials
/// every saved address nor loses a dial still in flight. Launch dials once;
/// after that Connect does, and so does a Restart ADB or engine switch
/// (`redialDue`).
export const savedDialSession: DialSession & { launched: boolean; redialDue: boolean } = $state({
  dial: {},
  pending: {},
  landed: 0,
  launched: false,
  redialDue: false,
});

/// Not reactive. `dialledFor`: each address the app dialled for a saved
/// device, and the id it expected there. `macRead`: ids whose MAC was asked
/// for this session.
export const savedDialWork = { dialledFor: new Map<string, string>(), macRead: new Set<string>() };
