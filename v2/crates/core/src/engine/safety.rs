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
//! The catalog is a reviewed source. Ignoring it — which this module did until
//! the lists and the rule tables were connected — meant 82 of 89 hand-curated
//! apps reported "role and effects unknown" while the row beside them displayed
//! the role and the effect. It also made `Unknown` meaningless: when almost
//! everything is unknown, a genuinely unrecognised sideloaded APK no longer
//! stands out, which is exactly what the verdict exists to do.

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
    /// Nothing covers it.
    NoRecord,
}

const UNKNOWN_REASON: &str = "No reviewed app list covers this package, so what it does and what removing it would break are both unknown. Check it yourself before removing it.";

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
        RiskTier::Safe => Safety::Safe {
            reason: format!("Reviewed for Android TV and rated safe to remove.{detail}"),
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
        // The regression: 82 of 89 curated apps reported "role and effects
        // unknown" while their own description sat in the next column.
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
                assert!(reason.contains("rated safe to remove"), "{reason}");
                // The curated sentence leads, rather than being replaced.
                assert!(reason.contains("Photo gallery"), "{reason}");
            }
            other => panic!("expected Safe, got {other:?}"),
        }
    }

    #[test]
    fn a_risky_reviewed_app_is_caution_and_says_why() {
        use crate::engine::types::RiskTier;
        // com.google.android.tvlauncher: risk "high", and the app has a whole
        // guarded wizard for it. Reporting "effects unknown" was plainly false.
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
                assert!(reason.contains("No reviewed app list"), "{reason}");
            }
            other => panic!("expected Unknown, got {other:?}"),
        }
    }

    #[test]
    fn safe_is_never_reached_without_a_catalog_entry() {
        // Safe must never be a fallback. That was the original sin the Unknown
        // change set out to fix, and it must not come back through this door.
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
                assert_eq!(reason, "Reviewed for Android TV and rated safe to remove.");
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
