//! Safety classification for package operations.
//!
//! Four verdicts, in precedence order:
//!
//! - `NeverDisable` — disabling will brick the device, break ADB, or otherwise
//!   make recovery impossible. The host layer refuses to send `pm disable-user`
//!   / `pm uninstall` for these. v2's hard guardrail.
//! - `Caution` — recoverable but disabling will visibly degrade the device
//!   (remote stops working, accessibility breaks, voice search dies). UI
//!   surfaces a loud confirm with the reason.
//! - `Safe` — the reviewed app list covers this package and rates it safe to
//!   remove on a TV. **Only ever reached from the catalog**, never as a
//!   fallback; that distinction is the whole point of the verdict.
//! - `Unknown` — nothing reviewed covers this package, so we genuinely do not
//!   know what it does.
//!
//! The catalog is a reviewed source, so it is consulted. Ignoring it would
//! report hand-curated apps as "role and effects unknown" while the row beside
//! them displays the role and the effect. It would also make `Unknown`
//! meaningless: when almost everything is unknown, a genuinely unrecognised
//! sideloaded APK no longer stands out, which is exactly what the verdict
//! exists to do.

use serde::Serialize;

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Safety {
    /// Operation refused at the host layer. No `pm disable-user` will be sent.
    NeverDisable {
        reason: String,
        source: SafetySource,
    },
    /// Recoverable, but the UI should surface a loud confirm.
    Caution {
        reason: String,
        source: SafetySource,
    },
    /// Reviewed and rated safe to remove on a TV.
    Safe {
        reason: String,
        source: SafetySource,
    },
    /// Nothing reviewed covers this package.
    Unknown {
        reason: String,
        source: SafetySource,
    },
}

/// Which reviewed source produced a verdict. Surfaced so the UI can say where
/// an answer came from rather than asking the reader to trust a bare word —
/// "we rated this" and "we have never seen this" deserve different treatment.
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SafetySource {
    /// The do-not-disable list in this module.
    ProtectedList,
    /// The caution list in this module.
    CautionList,
    /// The curated app lists under `crates/core/data/app-lists/`.
    ReviewedCatalog,
    /// The custom launchers in `launchers.json`: what the app is (a Home
    /// screen) is known, so the one way removing it goes wrong can be named.
    LauncherList,
    /// Nothing covers it.
    NoRecord,
}

const LAUNCHER_REASON: &str = "A third-party Home screen you installed. Nothing else on the TV depends on it, so removing it only matters if it is your current Home app. ATV Optimizer refuses to disable or uninstall the last enabled launcher; to stop using this one, switch Home in Launcher first.";

const UNKNOWN_REASON: &str = "This app isn't in the catalog, so what it does and what removing it would break are both unknown. Check it yourself before removing it.";

/// Classify a package using only the rule tables in this module.
///
/// Callers that can reach the app lists should prefer
/// [`classify_with_catalog`]; this one is for the never-disable guards, where
/// the catalog is irrelevant because the protected list always wins anyway.
pub fn classify(package: &str) -> Safety {
    classify_with_catalog(package, None)
}

/// A reviewed catalog entry's contribution to a verdict: its risk tier and the
/// sentence that was written about removing it.
///
/// Passed in rather than looked up so this module stays pure — the engine has
/// no I/O and does not own the lists.
#[derive(Debug, Clone, Copy)]
pub struct CatalogVerdict<'a> {
    pub risk: crate::engine::types::RiskTier,
    pub description: &'a str,
}

/// Classify a package, consulting the reviewed catalog when one covers it.
///
/// Precedence is deliberate and the order matters:
///
/// 1. **Protected list** — bricking tier, always wins.
/// 2. **Caution list** — a hand-written reason about this exact package beats a
///    risk tier, which is why the seven packages in both stay Caution.
/// 3. **Reviewed catalog** — a curated entry means somebody assessed it.
/// 4. **Unknown** — nothing covers it, and we say so plainly.
pub fn classify_with_catalog(package: &str, catalog: Option<CatalogVerdict<'_>>) -> Safety {
    classify_with_sources(package, catalog, false)
}

/// [`classify_with_catalog`], also told whether `package` is a custom launcher
/// from `launchers.json`. That ranks after the reviewed catalog and before
/// Unknown: an entry someone wrote about the package still says more.
pub fn classify_with_sources(
    package: &str,
    catalog: Option<CatalogVerdict<'_>>,
    custom_launcher: bool,
) -> Safety {
    if let Some(reason) = never_disable_reason(package) {
        return Safety::NeverDisable {
            reason: reason.to_string(),
            source: SafetySource::ProtectedList,
        };
    }
    if let Some(reason) = caution_reason(package) {
        return Safety::Caution {
            reason: reason.to_string(),
            source: SafetySource::CautionList,
        };
    }
    if let Some(entry) = catalog {
        return from_catalog(entry);
    }
    if custom_launcher {
        return Safety::Caution {
            reason: LAUNCHER_REASON.to_string(),
            source: SafetySource::LauncherList,
        };
    }
    Safety::Unknown {
        reason: UNKNOWN_REASON.to_string(),
        source: SafetySource::NoRecord,
    }
}

/// Map a reviewed risk tier onto a verdict.
///
/// Only `Safe` yields `Safety::Safe`. Everything above it is `Caution`: the
/// tiers above safe exist precisely because removing those has consequences,
/// and the catalog's own sentence explains what they are, so it leads the
/// reason rather than being replaced by a generic one.
fn from_catalog(entry: CatalogVerdict<'_>) -> Safety {
    use crate::engine::types::RiskTier;
    let description = entry.description.trim();
    let detail = if description.is_empty() {
        String::new()
    } else {
        format!(" {description}")
    };
    match entry.risk {
        // The label already says "Safe to remove", so the reason is the
        // catalog's own sentence about the app rather than a restatement.
        RiskTier::Safe => Safety::Safe {
            reason: if description.is_empty() {
                "Reviewed for Android TV.".to_string()
            } else {
                description.to_string()
            },
            source: SafetySource::ReviewedCatalog,
        },
        RiskTier::Medium => Safety::Caution {
            reason: format!(
                "Reviewed and rated medium risk — removable, but you may notice it go.{detail}"
            ),
            source: SafetySource::ReviewedCatalog,
        },
        RiskTier::High => Safety::Caution {
            reason: format!("Reviewed and rated high risk — read this before removing it.{detail}"),
            source: SafetySource::ReviewedCatalog,
        },
        RiskTier::Advanced => Safety::Caution {
            reason: format!(
                "Reviewed and rated advanced — for people who already know what this does.{detail}"
            ),
            source: SafetySource::ReviewedCatalog,
        },
    }
}

/// True if the host layer should refuse to disable/uninstall this package.
pub fn is_never_disable(package: &str) -> bool {
    never_disable_reason(package).is_some()
}

/// Which packages hold the keyboard and WebView roles right now, read from the
/// device just before a disable or uninstall. The read happens in the command
/// layer; this module only decides.
///
/// `None` is unknown: the read failed or the device named nobody. Unknown
/// refuses nothing. The lists below already hold the stock keyboards and
/// WebView packages, and the last-Home guard sets the precedent that an
/// unrelated app is never blocked on a read that failed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ActiveRoles {
    /// The package of the selected keyboard (`default_input_method`).
    pub keyboard: Option<String>,
    /// The package Android uses as WebView now.
    pub webview: Option<String>,
}

impl ActiveRoles {
    /// A later read within the same action. A role it names replaces what
    /// `earlier` found; a role it couldn't read keeps `earlier`'s answer, so
    /// a failed re-read never lifts a refusal a successful one established.
    pub fn read_after(self, earlier: &ActiveRoles) -> ActiveRoles {
        ActiveRoles {
            keyboard: self.keyboard.or_else(|| earlier.keyboard.clone()),
            webview: self.webview.or_else(|| earlier.webview.clone()),
        }
    }
}

const ACTIVE_KEYBOARD_REASON: &str = "The TV reports it as the keyboard in use. Removing it may leave you no way to type. Choose another keyboard in Settings first.";

const ACTIVE_WEBVIEW_REASON: &str = "The TV reports it as the WebView in use. Removing it may break apps that show web pages or sign-in screens. Choose another WebView in Developer options first.";

/// Why `package` must stay because of a role it holds right now, or `None`.
/// Checked after the protected list, so it only ever adds a refusal.
pub fn active_role_reason(package: &str, roles: &ActiveRoles) -> Option<&'static str> {
    if roles.keyboard.as_deref() == Some(package) {
        return Some(ACTIVE_KEYBOARD_REASON);
    }
    if roles.webview.as_deref() == Some(package) {
        return Some(ACTIVE_WEBVIEW_REASON);
    }
    None
}

/// Why `package` must not be disabled or uninstalled: it is on the protected
/// list, or the TV reports it holds the keyboard or WebView role. The one
/// decision every typed removal path asks.
pub fn removal_refusal_reason(package: &str, roles: &ActiveRoles) -> Option<&'static str> {
    never_disable_reason(package).or_else(|| active_role_reason(package, roles))
}

/// Destructive package verbs. Each one takes a package away from the user in a
/// way the never-disable list exists to prevent — `hide` and `suspend` are
/// included because they reach the same end state as `disable` by another
/// name.
const DESTRUCTIVE_VERBS: &[&str] = &[
    "disable",
    "disable-user",
    "disable-until-used",
    "uninstall",
    "uninstall-system-updates",
    "clear",
    "hide",
    "suspend",
];

/// Guard for the free-form shell runner: does this command *obviously* try to
/// take a never-disable package away?
///
/// **This is an anti-footgun, not a security boundary.** Read that literally
/// before relying on it for anything.
///
/// What it catches is the paste-from-a-forum-post case: someone runs
/// `pm disable-user com.android.systemui`, or the same thing chained behind a
/// harmless statement, without knowing it bricks the device. It handles the
/// spellings that show up in real posts — `;` / `&&` chaining, `cmd package`,
/// simple quoting, `$(…)` — because it compares whole tokens rather than
/// modelling shell grammar.
///
/// What it does **not** catch, verified rather than assumed:
///
/// ```text
/// P=com.android.systemui; pm disable-user $P   // variable indirection
/// pm disable-user com.android.system''ui       // split quoting
/// pm disable-user com.android.sys*             // glob
/// ```
///
/// Closing those would mean writing a shell parser, and a shell parser can
/// always be out-argued. The trade is deliberate and it is fine here, because
/// **there is no privilege boundary at this seam**: the user already has
/// `adb shell` on their own machine, pointed at their own device. Someone
/// determined to disable System UI does not need to defeat this function.
///
/// The never-disable list is genuinely *enforced* on the structured paths —
/// `apps::disable_package`, the optimize wizard, apply-snapshot. This only
/// stops the free-form box from becoming an easy way to do the same damage by
/// accident. If you ever need a real boundary here, this is not it.
///
/// It requires *both* a destructive verb and a protected package, so read-only
/// inspection (`dumpsys package com.android.systemui`) stays allowed.
///
/// Returns the reason to show the user, or `None` when the command is clear.
pub fn shell_command_blocked(command: &str) -> Option<(String, &'static str)> {
    // This recognizes literal arguments, not shell expansion or evaluation.
    let tokens: Vec<&str> = command
        .split(|c: char| {
            c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')' | '\'' | '"' | '`')
        })
        .filter(|t| !t.is_empty())
        .collect();

    let has_verb = tokens
        .iter()
        .any(|t| DESTRUCTIVE_VERBS.contains(&t.to_ascii_lowercase().as_str()));
    if !has_verb {
        return None;
    }
    tokens.iter().find_map(|t| {
        let package = t.split('/').next().unwrap_or(t);
        never_disable_reason(package).map(|reason| (package.to_string(), reason))
    })
}

// Bricking-tier: framework, system UI, settings, ADB-adjacent, package +
// permission infrastructure, base Google services.
fn never_disable_reason(package: &str) -> Option<&'static str> {
    reason_in(NEVER_DISABLE, package)
}

fn caution_reason(package: &str) -> Option<&'static str> {
    reason_in(CAUTION, package)
}

fn reason_in(table: &[(&str, &'static str)], package: &str) -> Option<&'static str> {
    table
        .iter()
        .find(|(pkg, _)| *pkg == package)
        .map(|(_, reason)| *reason)
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
        "Settings app. Disabling removes the Settings screen you need for recovery.",
    ),
    (
        "com.android.tv.settings",
        "TV Settings — the emergency Home fallback on Android TV. Never disable.",
    ),
    (
        "com.android.providers.settings",
        "Settings provider. Disabling stops settings from saving across reboots.",
    ),
    // --- ADB / shell — disabling kills our connection to the device ---
    (
        "com.android.shell",
        "Shell — required for ADB. Disabling cuts off this app's connection.",
    ),
    // --- Package install / permission infrastructure ---
    (
        "com.android.packageinstaller",
        "Package Installer. Disabling makes all future installs (including ADB ones) fail.",
    ),
    (
        "com.google.android.packageinstaller",
        "Google Package Installer. Disabling makes installs fail on Google builds.",
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
        "Downloads UI. Works with the Downloads provider — disabling breaks visible downloads.",
    ),
    // --- Connectivity that the remote relies on ---
    (
        "com.android.bluetooth",
        "Bluetooth stack. TV remotes pair over Bluetooth — disabling can leave the device unreachable.",
    ),
    (
        "com.android.bluetoothmidiservice",
        "Bluetooth helper required by remotes on some builds.",
    ),
    (
        "com.android.inputdevices",
        "Input subsystem. Disabling breaks the remote.",
    ),
    // --- Networking: network ADB rides on it, so breaking it strands the
    //     TV from this app as well as from the internet ---
    (
        "com.android.networkstack.inprocess",
        "Network Stack. Runs Wi-Fi and Ethernet connections; disabling can take the TV offline and cut this app's connection.",
    ),
    (
        "com.google.android.networkstack",
        "Network Stack. Runs Wi-Fi and Ethernet connections; disabling can take the TV offline and cut this app's connection.",
    ),
    (
        "com.android.networkstack",
        "Network Stack. Runs Wi-Fi and Ethernet connections; disabling can take the TV offline and cut this app's connection.",
    ),
    (
        "com.android.wifi.resources",
        "Wi-Fi Resources. The Wi-Fi service's configuration; disabling can break Wi-Fi and cut this app's connection.",
    ),
    (
        "com.google.android.wifi.resources",
        "Wi-Fi Resources. The Wi-Fi service's configuration; disabling can break Wi-Fi and cut this app's connection.",
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
        "Google Play Services. Disabling breaks every Google app and most third-party apps.",
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
    // --- WebView provider: apps use it to draw web pages and sign-in
    //     screens, and Android has nothing to fall back to without it ---
    (
        "com.google.android.webview",
        "Android System WebView. Apps use it for web pages and sign-in screens; disabling may break them.",
    ),
    (
        "com.android.webview",
        "Android System WebView. Apps use it for web pages and sign-in screens; disabling may break them.",
    ),
    // --- Persistent system bridges ---
    (
        "com.android.location.fused",
        "Fused Location. Required by location-aware apps.",
    ),
    (
        "com.android.providers.calendar",
        "Calendar provider. Required by apps that schedule reminders.",
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
        "Leanback Keyboard — the Android TV system keyboard. Disabling removes on-screen text input.",
    ),
    (
        "com.google.android.leanback.ime",
        "Leanback Keyboard, an Android TV system keyboard. Disabling may remove on-screen text input.",
    ),
    (
        "com.android.inputmethod.latin",
        "AOSP keyboard. Disabling can remove on-screen text input on builds that use it as the system keyboard.",
    ),
];

const CAUTION: &[(&str, &str)] = &[
    (
        "com.android.providers.tv",
        "Live Channels provider — disabling breaks Watch Next / Continue Watching rows (Netflix, Apple TV, Disney+ and others) and the Live Channels app.",
    ),
    (
        "com.google.android.tts",
        "Text-to-Speech. Disabling breaks every accessibility reader and some video narration.",
    ),
    (
        "com.google.android.katniss",
        "Google app / Assistant — provides the device's voice recognition service. Disabling breaks the remote mic button and in-app voice search (such as SmartTube).",
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
        "Google Play Store. Disabling removes your way to install apps that aren't already on the device.",
    ),
    (
        "com.google.android.feedback",
        "Disabling stops crash reports. Recoverable but it's how Google fixes Android bugs.",
    ),
    (
        "com.android.printspooler",
        "Print Spooler. Unused on TV; shown here so it isn't mistaken for bloat.",
    ),
    // --- Network stack companions: not the stack itself, but nobody has
    //     shown that removing them leaves networking intact ---
    (
        "com.android.networkstack.tethering.inprocess",
        "Tethering, part of Android's network stack. Its effect on the TV's own connection hasn't been verified, so leave it on.",
    ),
    (
        "com.google.android.networkstack.tethering",
        "Tethering, part of Android's network stack. Its effect on the TV's own connection hasn't been verified, so leave it on.",
    ),
    (
        "com.android.networkstack.tethering.nvidia",
        "NVIDIA's tethering settings for the network stack. Its effect on the TV's own connection hasn't been verified, so leave it on.",
    ),
    (
        "com.android.networkstack.permissionconfig",
        "Permission settings for the network stack. Its effect on the TV's own connection hasn't been verified, so leave it on.",
    ),
    // --- Updatable-module plumbing. Each is listed by exact ID: a shared
    //     prefix says nothing about what an unrelated package does ---
    (
        "com.google.android.modulemetadata",
        "Lists Android's updatable system modules for system updates. What depends on it hasn't been verified, so leave it on.",
    ),
    (
        "com.google.android.ext.shared",
        "Shared code library for system apps. What depends on it hasn't been verified, so leave it on.",
    ),
    (
        "com.google.android.overlay.modules.ext.services",
        "Configuration for a core Android module. Its effect hasn't been verified, so leave it on.",
    ),
    (
        "com.google.android.overlay.modules.modulemetadata.forframework",
        "Configuration for a core Android module. Its effect hasn't been verified, so leave it on.",
    ),
    (
        "com.google.android.overlay.modules.permissioncontroller",
        "Configuration for a core Android module. Its effect hasn't been verified, so leave it on.",
    ),
    (
        "com.google.android.overlay.modules.permissioncontroller.forframework",
        "Configuration for a core Android module. Its effect hasn't been verified, so leave it on.",
    ),
];

#[cfg(test)]
mod tests {
    #[test]
    fn a_later_read_replaces_named_roles_and_keeps_unread_ones() {
        use super::ActiveRoles;
        let earlier = ActiveRoles {
            keyboard: Some("com.example.stock".into()),
            webview: Some("com.android.webview".into()),
        };
        let failed = ActiveRoles::default().read_after(&earlier);
        assert_eq!(failed, earlier, "a failed re-read keeps both");
        let moved = ActiveRoles {
            keyboard: Some("com.example.other".into()),
            webview: None,
        }
        .read_after(&earlier);
        assert_eq!(moved.keyboard.as_deref(), Some("com.example.other"));
        assert_eq!(moved.webview.as_deref(), Some("com.android.webview"));
    }

    use super::*;

    #[test]
    fn a_custom_launcher_ranks_after_the_catalog_and_before_unknown() {
        assert!(matches!(
            classify_with_sources("com.spocky.projengmenu", None, true),
            Safety::Caution {
                source: SafetySource::LauncherList,
                ..
            }
        ));
        assert!(matches!(
            classify_with_sources("com.spocky.projengmenu", None, false),
            Safety::Unknown { .. }
        ));
        let reviewed = CatalogVerdict {
            risk: crate::engine::types::RiskTier::Safe,
            description: "Reviewed.",
        };
        assert!(matches!(
            classify_with_sources("com.example.launcher", Some(reviewed), true),
            Safety::Safe {
                source: SafetySource::ReviewedCatalog,
                ..
            }
        ));
        assert!(matches!(
            classify_with_sources("com.android.systemui", None, true),
            Safety::NeverDisable { .. }
        ));
    }

    #[test]
    fn shell_runner_blocks_disabling_a_never_disable_package() {
        let (pkg, reason) =
            shell_command_blocked("pm disable-user --user 0 com.android.systemui").unwrap();
        assert_eq!(pkg, "com.android.systemui");
        assert!(reason.contains("System UI"));
    }

    #[test]
    fn shell_runner_blocks_literal_clear_updates_and_component_targets() {
        for command in [
            "pm clear com.android.systemui",
            "pm uninstall-system-updates com.android.settings",
            "pm disable-user com.android.systemui/.SystemUIService",
            "pm disable-until-used com.android.shell",
        ] {
            assert!(shell_command_blocked(command).is_some(), "{command}");
        }
    }

    #[test]
    fn shell_runner_blocks_every_destructive_spelling() {
        for cmd in [
            "pm disable com.android.systemui",
            "pm disable-user com.android.systemui",
            "pm uninstall --user 0 com.android.systemui",
            "pm hide com.android.systemui",
            "pm suspend com.android.systemui",
            "cmd package disable com.android.systemui",
            "PM DISABLE com.android.systemui",
        ] {
            assert!(shell_command_blocked(cmd).is_some(), "must block: {cmd}");
        }
    }

    #[test]
    fn shell_runner_blocks_chained_and_quoted_attempts() {
        // The hole a naive prefix check would leave: hide the destructive
        // statement behind a harmless first one, or quote the package.
        assert!(shell_command_blocked("echo hi; pm disable-user com.android.systemui").is_some());
        assert!(shell_command_blocked("id && pm uninstall com.android.settings").is_some());
        assert!(shell_command_blocked("pm disable-user 'com.android.systemui'").is_some());
        assert!(shell_command_blocked("pm disable-user \"com.android.systemui\"").is_some());
        assert!(shell_command_blocked("pm disable-user android").is_some());
    }

    #[test]
    fn shell_runner_gate_does_not_pretend_to_stop_deliberate_evasion() {
        // Pinned on purpose. These all reach the device, and that is the
        // documented contract — `shell_command_blocked` is an anti-footgun,
        // not a boundary. Catching them would require a shell parser, which
        // can always be out-argued, and there is nothing to defend anyway:
        // the user already has `adb shell` against their own device.
        //
        // If a future change makes one of these block, that is a behavior
        // change to think about, not a bug fix — and this test failing is the
        // prompt to re-read the docs on the function before "fixing" it.
        for evasion in [
            "P=com.android.systemui; pm disable-user $P", // variable indirection
            "pm disable-user com.android.system''ui",     // split quoting
            "pm disable-user com.android.sys*",           // glob
        ] {
            assert_eq!(
                shell_command_blocked(evasion),
                None,
                "gate is documented as not catching this: {evasion}"
            );
        }
    }

    #[test]
    fn shell_runner_allows_reading_a_protected_package() {
        // Both halves of the rule matter: a protected package with no
        // destructive verb is an inspection, and must go through.
        assert_eq!(
            shell_command_blocked("dumpsys package com.android.systemui"),
            None
        );
        assert_eq!(shell_command_blocked("pm path com.android.systemui"), None);
        assert_eq!(shell_command_blocked("pm list packages"), None);
    }

    #[test]
    fn shell_runner_allows_disabling_an_unprotected_package() {
        // The verb alone must not block — debloating from the shell is the
        // whole point of the tab.
        assert_eq!(
            shell_command_blocked("pm disable-user --user 0 com.facebook.katana"),
            None
        );
        assert_eq!(
            shell_command_blocked("pm uninstall --user 0 com.netflix.ninja"),
            None
        );
    }

    #[test]
    fn shell_runner_gate_agrees_with_the_never_disable_list() {
        // The gate must not drift from the list it enforces.
        for (pkg, _) in NEVER_DISABLE {
            assert!(is_never_disable(pkg));
            assert!(
                shell_command_blocked(&format!("pm disable-user {pkg}")).is_some(),
                "shell gate missed a never-disable package: {pkg}"
            );
        }
    }

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
    fn unmatched_packages_are_unknown_with_the_exact_reason() {
        for package in [
            "com.example.bloat",
            "org.unfamiliar.tv.feature",
            "system",
            "com.example.app:remote",
            "/system/bin/surfaceflinger",
            "",
            "not a package",
        ] {
            assert_eq!(
                classify(package),
                Safety::Unknown {
                    reason: UNKNOWN_REASON.to_string(),
                    source: SafetySource::NoRecord,
                },
                "unexpected verdict for {package:?}"
            );
            assert!(!is_never_disable(package));
        }
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

    #[test]
    fn the_shield_leanback_keyboard_is_never_disable() {
        // The Shield ships the Leanback keyboard as `leanback.ime`, and lists
        // it among the enabled keyboards. The older ID stays protected too.
        assert!(is_never_disable("com.google.android.leanback.ime"));
        assert!(is_never_disable("com.google.android.leanbackkeyboard"));
    }

    #[test]
    fn webview_providers_are_never_disable() {
        assert!(is_never_disable("com.google.android.webview"));
        assert!(is_never_disable("com.android.webview"));
        // Trichrome is a library a provider can depend on, not a provider.
        assert!(!is_never_disable("com.google.android.trichromelibrary"));
    }

    #[test]
    fn the_network_stack_is_never_disable_and_its_companions_are_caution() {
        for package in [
            "com.android.networkstack.inprocess",
            "com.google.android.networkstack",
            "com.android.networkstack",
            "com.android.wifi.resources",
            "com.google.android.wifi.resources",
        ] {
            assert!(is_never_disable(package), "{package}");
        }
        for package in [
            "com.android.networkstack.tethering.inprocess",
            "com.android.networkstack.tethering.nvidia",
            "com.google.android.networkstack.tethering",
            "com.android.networkstack.permissionconfig",
        ] {
            assert!(
                matches!(
                    classify(package),
                    Safety::Caution {
                        source: SafetySource::CautionList,
                        ..
                    }
                ),
                "{package}"
            );
        }
    }

    #[test]
    fn module_plumbing_is_caution_by_exact_id_not_by_prefix() {
        for package in [
            "com.google.android.modulemetadata",
            "com.google.android.ext.shared",
            "com.google.android.overlay.modules.ext.services",
            "com.google.android.overlay.modules.modulemetadata.forframework",
            "com.google.android.overlay.modules.permissioncontroller",
            "com.google.android.overlay.modules.permissioncontroller.forframework",
        ] {
            assert!(
                matches!(classify(package), Safety::Caution { .. }),
                "{package}"
            );
        }
        for package in [
            "com.google.android.overlay.modules.something.new",
            "com.google.android.networkstack.example",
            "com.android.wifi.resources.example",
        ] {
            assert!(
                matches!(classify(package), Safety::Unknown { .. }),
                "{package}"
            );
        }
    }

    #[test]
    fn the_shield_capture_has_no_unguarded_keyboard_webview_or_network_stack() {
        // Every package on the captured Shield that is the keyboard, the
        // WebView or the network stack must reach a hand-written verdict, not
        // Unknown behind a single confirm.
        let list =
            include_str!("../../tests/fixtures/devices/shield-tv-pro/pm-list-packages-s.txt");
        let installed: Vec<&str> = list
            .lines()
            .filter_map(|l| l.trim().strip_prefix("package:"))
            .collect();
        for package in [
            "com.google.android.leanback.ime",
            "com.google.android.webview",
            "com.android.networkstack.inprocess",
            "com.android.wifi.resources",
        ] {
            assert!(installed.contains(&package), "{package} not in capture");
            assert!(is_never_disable(package), "{package}");
        }
        for package in installed.iter().filter(|p| {
            p.contains("networkstack")
                || p.contains("webview")
                || p.contains("wifi.resources")
                || p.contains("leanback.ime")
                || p.starts_with("com.google.android.overlay.modules.")
        }) {
            assert!(
                !matches!(classify(package), Safety::Unknown { .. }),
                "{package} is still Unknown"
            );
        }
    }

    #[test]
    fn an_active_role_refuses_only_its_holder() {
        let roles = ActiveRoles {
            keyboard: Some("com.example.keyboard".into()),
            webview: Some("com.android.chrome".into()),
        };
        assert_eq!(
            active_role_reason("com.example.keyboard", &roles),
            Some(ACTIVE_KEYBOARD_REASON)
        );
        assert_eq!(
            active_role_reason("com.android.chrome", &roles),
            Some(ACTIVE_WEBVIEW_REASON)
        );
        assert_eq!(active_role_reason("com.example.other", &roles), None);
        // The shared decision puts the protected list first.
        assert_eq!(
            removal_refusal_reason("com.example.keyboard", &roles),
            Some(ACTIVE_KEYBOARD_REASON)
        );
        assert!(
            removal_refusal_reason("com.google.android.webview", &ActiveRoles::default())
                .is_some_and(|r| r.starts_with("Android System WebView"))
        );
        assert_eq!(removal_refusal_reason("com.example.other", &roles), None);
        // Unknown roles refuse nothing.
        assert_eq!(
            active_role_reason("com.example.keyboard", &ActiveRoles::default()),
            None
        );
    }

    #[test]
    fn every_protected_rule_keeps_its_reason_and_membership() {
        for &(package, reason) in NEVER_DISABLE {
            assert_eq!(
                classify(package),
                Safety::NeverDisable {
                    reason: reason.to_string(),
                    source: SafetySource::ProtectedList,
                }
            );
            assert!(is_never_disable(package));
        }
    }

    #[test]
    fn caution_rules_keep_their_reasons_unless_protected_precedence_wins() {
        for &(package, caution_reason) in CAUTION {
            let expected = match never_disable_reason(package) {
                Some(reason) => Safety::NeverDisable {
                    reason: reason.to_string(),
                    source: SafetySource::ProtectedList,
                },
                None => Safety::Caution {
                    reason: caution_reason.to_string(),
                    source: SafetySource::CautionList,
                },
            };
            assert_eq!(classify(package), expected);
            assert_eq!(
                is_never_disable(package),
                never_disable_reason(package).is_some()
            );
        }
        assert_eq!(
            classify("com.google.android.feedback"),
            Safety::Caution {
                reason: "Disabling stops crash reports. Recoverable but it's how Google fixes Android bugs."
                    .to_string(),
                source: SafetySource::CautionList,
            }
        );
    }

    #[test]
    fn representative_protected_packages_remain_protected() {
        for package in [
            "android",
            "com.android.shell",
            "com.android.tv.settings",
            "com.android.packageinstaller",
            "com.android.permissioncontroller",
            "com.google.android.gms",
            "com.google.android.inputmethod.latin",
        ] {
            assert!(matches!(classify(package), Safety::NeverDisable { .. }));
        }
    }

    fn catalog(risk: crate::engine::types::RiskTier, description: &str) -> CatalogVerdict<'_> {
        CatalogVerdict { risk, description }
    }

    #[test]
    fn a_reviewed_safe_app_is_safe_not_unknown() {
        use crate::engine::types::RiskTier;
        // A curated app must not report "role and effects unknown" while its
        // own description sits in the next column.
        let verdict = classify_with_catalog(
            "com.android.gallery3d",
            Some(catalog(
                RiskTier::Safe,
                "Photo gallery — rarely used on TV.",
            )),
        );
        match verdict {
            Safety::Safe { reason, source } => {
                assert_eq!(source, SafetySource::ReviewedCatalog);
                // The curated sentence is the whole reason; the label carries
                // "Safe to remove", so repeating it here would say it twice.
                assert_eq!(reason, "Photo gallery — rarely used on TV.");
            }
            other => panic!("expected Safe, got {other:?}"),
        }
    }

    #[test]
    fn a_risky_reviewed_app_is_caution_and_says_why() {
        use crate::engine::types::RiskTier;
        // com.google.android.tvlauncher: risk "high", and the app has a whole
        // guarded wizard for it. Reporting "effects unknown" would be false.
        let verdict = classify_with_catalog(
            "com.google.android.tvlauncher",
            Some(catalog(
                RiskTier::High,
                "Requires custom launcher first! Use Launcher Wizard.",
            )),
        );
        match verdict {
            Safety::Caution { reason, source } => {
                assert_eq!(source, SafetySource::ReviewedCatalog);
                assert!(reason.contains("high risk"), "{reason}");
                assert!(reason.contains("Launcher Wizard"), "{reason}");
            }
            other => panic!("expected Caution, got {other:?}"),
        }
    }

    #[test]
    fn the_protected_list_outranks_any_catalog_entry() {
        use crate::engine::types::RiskTier;
        // Nothing a list says may downgrade a bricking-tier package, however
        // confidently it says it.
        for &(package, _) in NEVER_DISABLE {
            let verdict = classify_with_catalog(
                package,
                Some(catalog(RiskTier::Safe, "definitely fine, promise")),
            );
            assert!(
                matches!(verdict, Safety::NeverDisable { .. }),
                "{package} was downgraded to {verdict:?}"
            );
            assert!(is_never_disable(package));
        }
    }

    #[test]
    fn a_hand_written_caution_outranks_a_generic_risk_tier() {
        use crate::engine::types::RiskTier;
        // Seven packages are in both the caution list and the catalog. A
        // sentence written about that exact package beats a tier.
        let verdict = classify_with_catalog(
            "com.google.android.feedback",
            Some(catalog(RiskTier::Safe, "Crash and feedback telemetry.")),
        );
        match verdict {
            Safety::Caution { reason, source } => {
                assert_eq!(source, SafetySource::CautionList);
                assert!(reason.contains("crash reports"), "{reason}");
            }
            other => panic!("expected the caution-list verdict, got {other:?}"),
        }
    }

    #[test]
    fn an_uncatalogued_package_is_still_unknown() {
        // The verdict has to keep meaning something. A sideloaded APK nobody
        // reviewed is the case it exists for.
        let verdict = classify_with_catalog("com.random.sideload", None);
        match verdict {
            Safety::Unknown { reason, source } => {
                assert_eq!(source, SafetySource::NoRecord);
                assert!(reason.contains("isn't in the catalog"), "{reason}");
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn safe_is_never_reached_without_a_catalog_entry() {
        // Safe must never be a fallback, including through this door.
        for package in [
            "com.example.anything",
            "com.netflix.ninja",
            "",
            "com.google.android.tvlauncher",
        ] {
            assert!(
                !matches!(classify(package), Safety::Safe { .. }),
                "{package} reached Safe with no catalog entry"
            );
        }
    }

    #[test]
    fn a_catalog_entry_with_no_description_still_reads_cleanly() {
        use crate::engine::types::RiskTier;
        let verdict =
            classify_with_catalog("com.example.app", Some(catalog(RiskTier::Safe, "   ")));
        match verdict {
            Safety::Safe { reason, .. } => {
                assert_eq!(reason, "Reviewed for Android TV.");
                assert!(
                    !reason.contains("  "),
                    "no double space from an empty detail"
                );
            }
            other => panic!("expected Safe, got {other:?}"),
        }
    }

    #[test]
    fn all_verdicts_serialize_with_tag_and_reason() {
        assert_eq!(
            serde_json::to_value(Safety::NeverDisable {
                reason: "protected".to_string(),
                source: SafetySource::ProtectedList,
            })
            .unwrap(),
            serde_json::json!({
                "kind": "never_disable",
                "reason": "protected",
                "source": "protected_list"
            })
        );
        assert_eq!(
            serde_json::to_value(Safety::Caution {
                reason: "review".to_string(),
                source: SafetySource::CautionList,
            })
            .unwrap(),
            serde_json::json!({
                "kind": "caution",
                "reason": "review",
                "source": "caution_list"
            })
        );
        assert_eq!(
            serde_json::to_value(Safety::Safe {
                reason: "reviewed".to_string(),
                source: SafetySource::ReviewedCatalog,
            })
            .unwrap(),
            serde_json::json!({
                "kind": "safe",
                "reason": "reviewed",
                "source": "reviewed_catalog"
            })
        );
        assert_eq!(
            serde_json::to_value(classify("com.example.bloat")).unwrap(),
            serde_json::json!({
                "kind": "unknown",
                "reason": UNKNOWN_REASON,
                "source": "no_record"
            })
        );
    }
}
