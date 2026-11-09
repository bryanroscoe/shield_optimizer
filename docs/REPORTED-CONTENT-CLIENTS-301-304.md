# Reported content clients 301–304

Source review dated 2026-11-05, based on main
`c3624ecf09a6a8f32f8c7b866bf1dd3fd54ee613`. The four desktop reports describe
installed packages on a Google TV / Android 14 device. Reports are untrusted
observations; their “safe” assertions are not accepted as safety evidence.

## Exact identities and functional assessment

| Issue / exact package | Primary identity and purpose evidence | Consequence and dependency assessment |
| --- | --- | --- |
| [301](https://github.com/bryanroscoe/shield_optimizer/issues/301) `com.abc.abcnews` | [Disney's ABC News listing](https://play.google.com/store/apps/details?id=com.abc.abcnews): live news, programmes and alerts. | Disabling stops those app functions. Keep if anyone uses ABC News, including alerts. This is not the separate ABC entertainment app. |
| [302](https://github.com/bryanroscoe/shield_optimizer/issues/302) `com.amazon.music.tv` | [Amazon's Music listing](https://play.google.com/store/apps/details?id=com.amazon.music.tv): music playback, subscription and purchased-library access. | Disabling stops playback and library access through this client. Subscription billing is separate. The existing descriptive name is moved into the action catalog; it was never a safety assessment. |
| [303](https://github.com/bryanroscoe/shield_optimizer/issues/303) `com.aetnd.svod.historyvault.androidtv` | [A+E's History Vault listing](https://play.google.com/store/apps/details?id=com.aetnd.svod.historyvault.androidtv): subscription documentaries and series. | Disabling stops viewing through this exact client. Subscription billing remains separate; access depends on account and region. Other HISTORY packages are separate identities. |
| [304](https://github.com/bryanroscoe/shield_optimizer/issues/304) `com.amcplus.amcandroidtv` | [AMC's AMC+ listing](https://play.google.com/store/apps/details?id=com.amcplus.amcandroidtv): series, movies and live channels. | Disabling stops viewing through this Android TV client. Subscription billing remains separate. The existing `com.amcplus.amcfiretv` record is preserved byte-for-byte as a distinct app entry. |

All four exact publisher listings were opened during this review. Their content
roles support an explicit disable-if-unused choice, following the existing
optional-content-client precedent. The expected loss of each app's own playback,
viewing or alert functionality is a functional inference, not a measured outcome.
No listing supplies an installed manifest, signature check or complete dependency
graph. No claim is made that every firmware or other app is independent of them.
Shared callers, provider links, account workflows and device-specific integration
remain unverified. Keep any client whose functions or integrations are used.

## Conservative catalog decision

Use `common.json` for the exact installed application identities, with medium
risk (Caution), Disable, review enabled and both Optimize/Restore defaults false.
They start at Keep and are excluded from Select all safe. This does not certify
any TV family or match a source profile to the connected device. Missing packages
are skipped and disabled packages are not disabled again. Explicit Restore may
re-enable an installed disabled app; it is not selected automatically.

[Android's package-manager documentation](https://developer.android.com/tools/adb#pm)
distinguishes disabling and enabling from uninstalling or clearing data.
[PackageManager's disabled-user state](https://developer.android.com/reference/android/content/pm/PackageManager#COMPONENT_ENABLED_STATE_DISABLED_USER)
also documents that a user-disabled app may be re-enabled through system UI.
These support choosing Disable and conditional re-enable guidance, not a promise
that recovery was tested on this reporter's firmware.

[Google's Android TV subscription guidance](https://support.google.com/androidtv/answer/9461288?hl=en)
explicitly says uninstalling an app does not cancel its subscription. Disabling
is likewise not a subscription-cancellation workflow. The three subscription
clients disclose this in their consequences. Re-enabling does not restore sign-in,
subscriptions, deleted downloads or app settings. Exact Play listings set the
existing `play_store` flag: the UI can still offer a separately confirmed Uninstall.
This change recommends Disable and does not promise that Play reinstallation is
available for every device, account or region.

ABC News and Amazon Music also occur in the informational CleanYourTV snapshot.
Those observations and any upstream keep/disable choices are not used as removal
safety evidence here. Its snapshot, provenance, research ledger and notices remain
unchanged. No global Safe verdict, automatic action, performance improvement or
successful device removal/recovery is claimed.

## Validation scope

The reviewed-Optimize fixture includes these four identities so the existing
loader/planner test exercises installed, disabled and missing states in Optimize
and Restore for each device family. A focused loader test checks exact metadata,
evidence links, Caution/Disable/default flags and distinct AMC+ identities. The
Amazon installation-provenance regression now reads the all-installed list;
Amazon Music remains third-party when reported by `pm -3` and is excluded from
the uncatalogued list. The canonical safety engine and action code are unchanged.

Desktop and mobile fixture UI tests check Caution, Keep, explicit Disable choice
and exclusion from automatic choices. Captures use synthetic browser fixtures;
they cannot establish ADB behavior, installed-build behavior, firmware compatibility
or physical disable/re-enable outcomes. Actual commands, tested revision and
outcomes belong in the PR evidence. Hardware validation remains not run.
