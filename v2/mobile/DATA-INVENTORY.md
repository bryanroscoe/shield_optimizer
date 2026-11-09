# Mobile data inventory and privacy-policy preparation draft

Status: internal factual inventory, not approved legal text or a published policy. No owner,
contact, retention promise, payment provider or jurisdiction is invented here. Review against the
final signed artifact, merged manifest and SDK graph before completing store disclosures.

| Data / activity | Code evidence | Current behavior / disclosure topic |
|---|---|---|
| Saved TV endpoints, labels, hardware ID, fingerprint, auto-connect preference | `src/lib/savedDevices.ts`, `identity.ts`, `session.svelte.ts` | Local saved-TV storage; identity distinguishes TVs, never merely an IP. Explain removal and automatic reconnect controls. |
| Discovered TV display names | `crates/core/src/net.rs`, `engine/cast_name.rs`, `src-tauri/src/wireless_commands.rs` | During a scan the app asks each discovered TV on a private/LAN IPv4 address for its Cast name (`GET http://<tv>:8008/setup/eureka_info`, local network only). Only the `name` field is read; the rest of the reply (it includes MAC, Wi-Fi and location fields) is discarded. The name is shown on the scan list, kept only in memory for that scan, never logged and never sent anywhere; it is not identity. Saved TVs store the name the TV reports after connecting (row above). |
| ADB authorization / pairing material | `src-tauri/src/wireless_adb.rs`, vendored adb_client | Local authentication material (`adb_key` in app data) enabling access to TVs the user authorizes; excluded from Android backup and device transfer (below). Disclose safeguarding and how to revoke on the TV. |
| TV installed-app inventory, diagnostics, settings, screenshots and remote input | shared core commands, `src/lib/api.ts` | Sent between phone and selected TV. Legacy ADB and TLS pairing are distinct transports; do not promise all LAN traffic is encrypted. |
| Pulled files, APK bundles and snapshots | `src-tauri/src/file_commands.rs`, shared snapshot/files commands | App storage can contain user/device information. SAF export is not implemented. Excluded from Android backup and device transfer (below); verify deletion behavior before promising retention/deletion. |
| License key and visible licensee | `src-tauri/src/lib.rs`, `LICENSING.md` | `license.json` in app data; offline signature verification at startup. Licensee may be a name/email embedded by issuer. Sideload/dev builds only; Play builds refuse key entry. |
| Google Play purchase of Pro | `src-tauri/src/billing_commands.rs`, `play_billing.rs`, `tauri-plugin-atv-billing/` | Play Billing Library (Google) handles payment; the app never sees payment details. The app receives the signed purchase record (order id, purchase token, time, package, product) from Play, verifies it on the phone and stores the last verified one in `play_purchase.json` in app data. No purchase data is sent to the developer; there is no verification server. Declare per Play's guidance for apps using Play billing. |
| Debug/diagnostic logs | `src/lib/log.ts`, native tracing and `read_debug_log` | Explain logs and redaction limits. Review before sharing; logs are not automatically proof of anonymization. |
| Anonymous device and error reports (telemetry) | `crates/core/src/engine/telemetry.rs` (payload + redaction), `crates/core/src/telemetry.rs` (queue, upload, setting), `shared/telemetry.ts`, `src/lib/api.ts` | **On by default, opt-out** in More ("Share anonymous device and error reports"), with a one-time first-launch notice. Sent only when the build sets `ATVOPT_TELEMETRY_URL`; otherwise a no-op with no network. `POST <base>/v1/events` to an owner-operated Cloudflare Worker; raw events retained 90 days (owner-stated retention, enforced server-side). Fields: random install id (UUID v4, regenerated when the user turns reports back on), app (`mobile`), app version, phone OS release, timestamp, event kind; TV manufacturer, brand, model, device codename, product name, Android release, SDK level, detected family and support tier; action name, success, short error code, and an error message redacted of IPs, host:port, MACs, mDNS names, serial-like tokens, emails, URLs, absolute paths and third-party package names (≤500 chars). Never sent: `ro.serialno` or any hardware id/fingerprint, IP addresses/ports, TV names/labels, installed package lists, file paths, license/licensee data, snapshot contents. Never stored server-side: the request IP or any location; the Worker does not read Cloudflare's request location data (not even the country, which it stopped storing on 2026-11-01). Turning it off discards the unsent queue and cancels an upload in flight. Setting stored in app data `telemetry.json`. |
| Device support tiers | `crates/core/data/device-support.json`, `engine/support.rs`, `crates/core/src/telemetry.rs` | Matched locally from the TV's reported manufacturer/brand/model/codename. While reports are on, the app also fetches `GET <base>/v1/support-tiers` from the same server (no install id or device data in the request; the server sees the phone's IP like any HTTP request) and caches it in app data `support-tiers.json`. Off means no request to that server at all. Informational only; never changes safety gates. |
| External links | `api.openUrl`, source-site buttons, report-app flow | User-directed browser navigation leaves the app and follows destination practices. No general promise that the app never opens the internet. |

Manifest declarations include INTERNET, ACCESS_NETWORK_STATE, ACCESS_WIFI_STATE,
CHANGE_WIFI_MULTICAST_STATE and, from the Play Billing Library, com.android.vending.BILLING.
NEARBY_WIFI_DEVICES was removed: mDNS discovery through NsdManager does not need it. No phone
microphone permission is declared here: the Assistant control changes a permission on the selected
TV. The Play Billing Library is the only billing SDK, and no third-party analytics SDK was found in
the inspected source. The app's own anonymous reports (above) are the only automatic upload to the
developer; they use `reqwest`/`rustls` with Android's system CA store and are a no-op in builds
without `ATVOPT_TELEMETRY_URL`. Purchase records are never part of those reports: a finished Buy
attempt is reported only as an `other` action with success or a `billing_<outcome>` code (no order
id, purchase token, price, product or license data; cancels and pending payments are not reported). This is a scoped
source finding, not a guarantee about every transitive dependency or the final merged artifact.

Android backup and device transfer are off for all app data. The app manifest sets
`android:allowBackup="false"` and `android:fullBackupContent="false"` (Android 11 and lower) and
`android:dataExtractionRules="@xml/data_extraction_rules"`, which excludes every domain (app files,
databases, shared preferences, WebView storage including saved TVs, external app files and
device-protected storage) from both cloud backup and device-to-device transfer (Android 12 and
higher, where `allowBackup` alone no longer stops a transfer). So the ADB key, `telemetry.json`
(install id and opt-out), `license.json`, `play_purchase.json`, snapshots, pulled files and saved
TVs never leave the phone that way. A new or reset phone starts empty: TVs must be authorized
again, and reports start from their default (on, with a new install id and the first-launch
notice shown again). Play restores a Pro
purchase from the Google account on its own (launch query or Restore purchases); a license key must
be entered again. Source finding; confirm with `adb shell bmgr backupnow` / a transfer on a device
before stating it in the policy.

The 2026-11-03 resolved Gradle audit now records Play Billing's Google DataTransport, Firebase
encoder and Google Play services dependencies in `android-runtime-inventory.json`. The location
library is present transitively; the compiled debug manifest declares no coarse/fine/background
location permission. An encoder dependency is not itself Firebase Analytics, and an included
location library is not proof of collection. Final Data safety must account for actual SDK
behavior in the Play candidate, separately from the no-location guarantee of the reporting Worker.

## Draft policy outline requiring owner completion

The Google Play rollout uses `bryanroscoe+atv@gmail.com` as the owner-approved public support and
privacy contact (2026-11-03). The Worker serves the Android policy at `/privacy` only with a valid
`PRIVACY_CONTACT_EMAIL`; More → Privacy opens that URL even when telemetry cannot load. While
reports are enabled, the same card shows and copies the full current installation Support ID.
Users must preserve it before opting out or resetting app data if requesting deletion of prior
reports. Publication and the final signed artifact still need verification; source completion is
not evidence that the policy URL is live or that the Play form has been submitted.

- App operator: Bryan Roscoe; public privacy/support contact: `bryanroscoe+atv@gmail.com`.
- Explain local TV management, authorization, data categories above and user-initiated external
  navigation/sharing. Validate any no-server/no-analytics statement against the final SDK graph.
- State actual retention, deletion and key revocation behavior after testing:
  **VERIFICATION REQUIRED**, not a promise that uninstall securely erases every copy. OS
  backup/transfer is excluded in the manifest (above); confirm on a device before stating it. Telemetry:
  90-day raw retention is the owner's stated policy for the Worker; the client cannot enforce it.
- Select distribution, payment processing and any trial policy: **OWNER DECISION**. Revisit
  disclosures when billing or verification servers are added. Telemetry now exists (above):
  Play Data safety must declare device/app diagnostics and crash data collected, not shared,
  optional (user can turn off), and processed by the owner.
- Determine audience, jurisdictions, legal bases/rights and effective date: **OWNER/LEGAL REVIEW**.
- Publish an accessible privacy-policy URL and add an in-app entry only after owner approval;
  complete Play Data safety from measured behavior, not the word "local" alone.
