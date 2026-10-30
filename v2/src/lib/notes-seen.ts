/// Whether this launch should open the "Updated to vX" pop-up.
///
/// Pure so the rule can be tested on its own; `+layout.svelte` reads and
/// writes the stored values.
///
/// The rule: show once for each version increase. The stored version is the
/// last one whose notes the user was actually shown, and it is written only
/// once the pop-up has been seen — never merely because a launch happened, or
/// a launch that had nothing to show would use the pop-up up for good.
///
/// `legacyLastSeen` is the superseded per-launch key, set on every launch
/// whether or not anything was shown. Its presence proves this is not a first
/// run; its value can't be trusted to mean "shown", so a user carrying only
/// that key sees the running version's notes once.

export type ArrivalDecision =
  /// Open the pop-up; record `running` once it has been seen.
  | { show: true; record: null }
  /// Say nothing. `record` is the version to store now, or null to store none.
  | { show: false; record: string | null };

export function decideArrival(input: {
  running: string;
  notesSeen: string | null;
  legacyLastSeen: string | null;
  hasNotes: boolean;
  isNewer: (a: string, b: string) => boolean;
}): ArrivalDecision {
  const { running, notesSeen, legacyLastSeen, hasNotes, isNewer } = input;

  if (notesSeen === null) {
    // A first run: greeting a brand-new user with "what's new" is nonsense.
    if (legacyLastSeen === null) return { show: false, record: running };
    return hasNotes ? { show: true, record: null } : { show: false, record: running };
  }

  // Same version, or a downgrade: nothing new arrived. Keep the higher mark so
  // a later re-upgrade does not replay notes already read.
  if (!isNewer(running, notesSeen)) return { show: false, record: null };

  // Newer, but this build carries no notes for itself. Nothing will ever be
  // showable for it, so move the mark on.
  if (!hasNotes) return { show: false, record: running };

  return { show: true, record: null };
}
