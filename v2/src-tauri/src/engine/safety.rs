//! Safety classification for package operations.
//!
//! Two tiers:
//!
//! - `NeverDisable` — disabling will brick the device, break ADB, or otherwise
//!   make recovery impossible. The host layer refuses to send `pm disable-user`
//!   / `pm uninstall` for these. v2's hard guardrail.
//! - `Caution` — recoverable but disabling will visibly degrade the device
//!   (remote stops working, accessibility breaks, voice search dies). UI
//!   surfaces a loud confirm with the reason.
//!
//! Everything else is implicitly `Safe`. The user can still disable arbitrary
//! packages they pick from the memory table — the confirm just doesn't shout.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Safety {
    /// Operation refused at the host layer. No `pm disable-user` will be sent.
    NeverDisable {
        reason: &'static str,
    },
    /// Recoverable, but the UI should surface a loud confirm.
    Caution {
        reason: &'static str,
    },
    Safe,
}

/// Classify a package for disable/uninstall safety.
pub fn classify(package: &str) -> Safety {
    if let Some(reason) = never_disable_reason(package) {
        return Safety::NeverDisable { reason };
    }
    if let Some(reason) = caution_reason(package) {
        return Safety::Caution { reason };
    }
    Safety::Safe
}

/// True if the host layer should refuse to disable/uninstall this package.
pub fn is_never_disable(package: &str) -> bool {
    never_disable_reason(package).is_some()
}

fn never_disable_reason(package: &str) -> Option<&'static str> {
    // Bricking-tier: framework, system UI, settings, ADB-adjacent, package +
    // permission infrastructure, base Google services. Order matches what
    // would actually go wrong first if disabled.
    for (pkg, reason) in NEVER_DISABLE {
        if *pkg == package {
            return Some(reason);
        }
    }
    None
}

fn caution_reason(package: &str) -> Option<&'static str> {
    for (pkg, reason) in CAUTION {
        if *pkg == package {
            return Some(reason);
        }
    }
    None
}

const NEVER_DISABLE: &[(&str, &str)] = &[
    // --- The framework itself ---
    (
        "android",
        "The Android framework. Disabling bricks the device.",
    ),
    (
        "com.android.systemui",
        "System UI — the launcher's host process. Disabling makes the device unusable.",
    ),
    // --- Settings + recovery surface ---
    (
        "com.android.settings",
        "Settings app. Disabling removes your recovery surface.",
    ),
    (
        "com.android.tv.settings",
        "TV Settings — the emergency HOME fallback on Android TV. Never disable.",
    ),
    (
        "com.android.providers.settings",
        "Settings provider. Disabling breaks settings persistence across reboot.",
    ),
    // --- ADB / shell — disabling kills our connection to the device ---
    (
        "com.android.shell",
        "Shell — required for ADB. Disabling cuts off this app's connection.",
    ),
    // --- Package install / permission infrastructure ---
    (
        "com.android.packageinstaller",
        "Package Installer. Disabling makes ALL future installs (including ADB ones) fail.",
    ),
    (
        "com.google.android.packageinstaller",
        "Google Package Installer. Disabling makes installs fail on Google-flavor builds.",
    ),
    (
        "com.android.permissioncontroller",
        "Permission Controller. Disabling breaks the runtime permission system.",
    ),
    (
        "com.google.android.permissioncontroller",
        "Google Permission Controller. Disabling breaks permissions on Google builds.",
    ),
    // --- Storage / downloads ---
    (
        "com.android.externalstorage",
        "External Storage provider. Disabling breaks file access for every app.",
    ),
    (
        "com.android.providers.media",
        "Media provider. Disabling breaks media discovery for every player on the device.",
    ),
    (
        "com.android.providers.downloads",
        "Downloads provider. Required for in-app downloads (Play Store, sideload).",
    ),
    (
        "com.android.providers.downloads.ui",
        "Downloads UI. Pair with the provider — disabling breaks visible downloads.",
    ),
    // --- Connectivity that the remote relies on ---
    (
        "com.android.bluetooth",
        "Bluetooth stack. TV remotes pair over BT — disabling can leave the device unreachable.",
    ),
    (
        "com.android.bluetoothmidiservice",
        "Bluetooth helper required by remotes on some builds.",
    ),
    (
        "com.android.inputdevices",
        "Input subsystem. Disabling breaks the remote.",
    ),
    // --- Certs / keychain ---
    (
        "com.android.keychain",
        "System keychain. Disabling breaks app sign-in and DRM playback.",
    ),
    (
        "com.android.certinstaller",
        "Certificate Installer. Disabling breaks corporate / DRM certificate installs.",
    ),
    // --- Google base layer — disabling these on Google-flavor TVs is a brick ---
    (
        "com.google.android.gms",
        "Google Play Services. Disabling breaks every Google app + most third-party apps.",
    ),
    (
        "com.google.android.gsf",
        "Google Services Framework. Without it, Play Services and account sync fail.",
    ),
    (
        "com.google.android.gsf.login",
        "Google login bridge. Disabling logs you out of every Google service.",
    ),
    (
        "com.google.android.ext.services",
        "Android extension services. Disabling breaks notifications and ranking.",
    ),
    // --- Persistent system bridges ---
    (
        "com.android.location.fused",
        "Fused Location. Required by location-aware apps.",
    ),
    (
        "com.android.providers.calendar",
        "Calendar provider. Required by apps that reminder-schedule.",
    ),
    (
        "com.android.providers.contacts",
        "Contacts provider. Required by sign-in and account sync.",
    ),
    // --- On-screen keyboards / IMEs — disabling the active one removes all
    //     text input (no way to type passwords, search, Wi-Fi keys) ---
    (
        "com.google.android.inputmethod.latin",
        "Gboard — the system keyboard on most Android TV / Google TV. Disabling removes on-screen text input.",
    ),
    (
        "com.google.android.leanbackkeyboard",
        "Leanback Keyboard — the Android TV system IME. Disabling removes on-screen text input.",
    ),
    (
        "com.android.inputmethod.latin",
        "AOSP keyboard. Disabling can remove on-screen text input on builds that ship it as the IME.",
    ),
];

const CAUTION: &[(&str, &str)] = &[
    (
        "com.android.providers.tv",
        "Live Channels provider — disabling breaks Watch Next / Continue Watching rows for Netflix, Apple TV, Disney+, etc. and the Live Channels app.",
    ),
    (
        "com.google.android.tts",
        "Text-to-Speech. Disabling breaks every accessibility reader and some video narration.",
    ),
    (
        "com.google.android.katniss",
        "Google app / Assistant — provides the device's voice RecognitionService. Disabling kills the remote mic button AND in-app voice search (SmartTube, etc.).",
    ),
    (
        "com.google.android.speech.pumpkin",
        "Google Speech Services (on-device recognition). Disabling can break voice dictation and in-app mic search.",
    ),
    (
        "com.google.android.apps.mediashell",
        "Chromecast Built-in. Disabling means you can't cast to this device anymore.",
    ),
    (
        "com.android.vending",
        "Google Play Store. Disabling removes your install path for everything not yet on disk.",
    ),
    (
        "com.google.android.feedback",
        "Disabling stops crash reports. Recoverable but it's how Google fixes Android bugs.",
    ),
    (
        "com.android.printspooler",
        "Print Spooler. Unused on TV but listed so users don't think it's hidden bloat.",
    ),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn framework_is_never_disable() {
        assert!(matches!(classify("android"), Safety::NeverDisable { .. }));
        assert!(matches!(
            classify("com.android.systemui"),
            Safety::NeverDisable { .. }
        ));
    }

    #[test]
    fn shell_is_never_disable_for_obvious_reasons() {
        assert!(is_never_disable("com.android.shell"));
    }

    #[test]
    fn safe_fallback_settings_is_never_disable() {
        assert!(is_never_disable("com.android.tv.settings"));
    }

    #[test]
    fn channel_provider_is_caution_not_never() {
        // v1 lets users disable this with a warning; we mirror that.
        assert!(matches!(
            classify("com.android.providers.tv"),
            Safety::Caution { .. }
        ));
    }

    #[test]
    fn unknown_package_is_safe() {
        assert!(matches!(classify("com.example.bloat"), Safety::Safe));
    }

    #[test]
    fn gms_is_never_disable() {
        assert!(is_never_disable("com.google.android.gms"));
    }

    #[test]
    fn system_keyboards_are_never_disable() {
        // Disabling the active IME removes all on-screen text input.
        assert!(is_never_disable("com.google.android.inputmethod.latin"));
        assert!(is_never_disable("com.google.android.leanbackkeyboard"));
        assert!(is_never_disable("com.android.inputmethod.latin"));
    }
}
