// Scenario worlds built from the captured profiles. Addresses are in the
// 192.0.2.0/24 documentation range; serials and models are the real ones.

export const SHIELD = {
  serial: "1324619053514",
  key: "192.0.2.1:5555",
  name: "Test SHIELD Android TV",
};

export const PIXEL = {
  serial: "58040DLCH005YV",
  instance: "adb-58040DLCH005YV-jBeCEe",
  key: "adb-58040DLCH005YV-jBeCEe._adb-tls-connect._tcp",
};

/// The captured Shield, reachable over legacy network debugging.
export function shieldScenario(extra = {}) {
  return {
    devices: [{ profile: "shield-tv-pro", ...extra }],
  };
}

/// The Shield with the stock launcher enabled and holding Home, and an
/// enabled stock launcher that overrides every preference — the Android 11
/// Shield behaviour behind the "Disable stock launcher" step.
export function stockHomeShield(extra = {}) {
  return shieldScenario({
    home_policy: "stock_overrides",
    setup: [
      "pm enable com.google.android.tvlauncher",
      "cmd package set-home-activity --user 0 com.google.android.tvlauncher/.MainActivity",
    ],
    ...extra,
  });
}
