# Mobile data inventory and privacy-policy preparation draft

Status: internal factual inventory, not approved legal text or a published policy. No owner,
contact, retention promise, payment provider or jurisdiction is invented here. Review against the
final signed artifact, merged manifest and SDK graph before completing store disclosures.

| Data / activity | Code evidence | Current behavior / disclosure topic |
|---|---|---|
| Saved TV endpoints, labels, hardware ID, fingerprint, auto-connect preference | `src/lib/savedDevices.ts`, `identity.ts`, `session.svelte.ts` | Local saved-TV storage; identity distinguishes TVs, never merely an IP. Explain removal and automatic reconnect controls. |
| ADB authorization / pairing material | `src-tauri/src/wireless_adb.rs`, vendored adb_client | Local authentication material enabling access to TVs the user authorizes; disclose safeguarding and how to revoke on the TV. |
| TV installed-app inventory, diagnostics, settings, screenshots and remote input | shared core commands, `src/lib/api.ts` | Sent between phone and selected TV. Legacy ADB and TLS pairing are distinct transports; do not promise all LAN traffic is encrypted. |
| Pulled files, APK bundles and snapshots | `src-tauri/src/file_commands.rs`, shared snapshot/files commands | App storage can contain user/device information. SAF export is not implemented. Verify deletion and OS backup behavior before promising retention/deletion. |
| License key and visible licensee | `src-tauri/src/lib.rs`, `LICENSING.md` | `license.json` in app data; offline signature verification at startup. Licensee may be a name/email embedded by issuer. No purchase/restore backend is implemented. |
| Debug/diagnostic logs | `src/lib/log.ts`, native tracing and `read_debug_log` | Explain logs and redaction limits. Review before sharing; logs are not automatically proof of anonymization. |
| Anonymous device and error reports (telemetry) | `crates/core/src/engine/telemetry.rs` (payload + redaction), `crates/core/src/telemetry.rs` (queue, upload, setting), `shared/telemetry.ts`, `src/lib/api.ts` | **On by default, opt-out** in More ("Share anonymous device and error reports"), with a one-time first-launch notice. Sent only when the build sets `ATVOPT_TELEMETRY_URL`; otherwise a no-op with no network. `POST <base>/v1/events` to an owner-operated Cloudflare Worker; raw events retained 90 days (owner-stated retention, enforced server-side). Fields: random install id (UUID v4, regenerated when the user turns reports back on), app (`mobile`), app version, phone OS release, timestamp, event kind; TV manufacturer, brand, model, device codename, product name, Android release, SDK level, detected family and support tier; action name, success, short error code, and an error message redacted of IPs, host:port, MACs, mDNS names, serial-like tokens, emails, URLs, absolute paths and third-party package names (≤500 chars). Never sent: `ro.serialno` or any hardware id/fingerprint, IP addresses/ports, TV names/labels, installed package lists, file paths, license/licensee data, snapshot contents. Turning it off discards the unsent queue and cancels an upload in flight. Setting stored in app data `telemetry.json`. |
| Device support tiers | `crates/core/data/device-support.json`, `engine/support.rs`, `crates/core/src/telemetry.rs` | Matched locally from the TV's reported manufacturer/brand/model/codename. While reports are on, the app also fetches `GET <base>/v1/support-tiers` from the same server (no install id or device data in the request; the server sees the phone's IP like any HTTP request) and caches it in app data `support-tiers.json`. Off means no request to that server at all. Informational only; never changes safety gates. |
| External links | `api.openUrl`, source-site buttons, report-app flow | User-directed browser navigation leaves the app and follows destination practices. No general promise that the app never opens the internet. |

Manifest declarations include INTERNET, ACCESS_NETWORK_STATE, ACCESS_WIFI_STATE,
CHANGE_WIFI_MULTICAST_STATE and NEARBY_WIFI_DEVICES (`neverForLocation`). No phone microphone
permission is declared here: the Assistant control changes a permission on the selected TV.
No billing SDK or third-party analytics SDK was found in the inspected source. The app's own
anonymous reports (above) are the only automatic upload; they use `reqwest`/`rustls` with
Android's system CA store and are a no-op in builds without `ATVOPT_TELEMETRY_URL`. This is a
scoped source finding, not a guarantee about every transitive dependency or the final merged
artifact.

## Draft policy outline requiring owner completion

- Identify app operator and privacy/support contact: **OWNER DECISION**.
- Explain local TV management, authorization, data categories above and user-initiated external
  navigation/sharing. Validate any no-server/no-analytics statement against the final SDK graph.
- State actual retention, deletion, OS backup/transfer and key revocation behavior after testing:
  **VERIFICATION REQUIRED**, not a promise that uninstall securely erases every copy. Telemetry:
  90-day raw retention is the owner's stated policy for the Worker; the client cannot enforce it.
- Select distribution, payment processing and any trial policy: **OWNER DECISION**. Revisit
  disclosures when billing or verification servers are added. Telemetry now exists (above):
  Play Data safety must declare device/app diagnostics and crash data collected, not shared,
  optional (user can turn off), and processed by the owner.
- Determine audience, jurisdictions, legal bases/rights and effective date: **OWNER/LEGAL REVIEW**.
- Publish an accessible privacy-policy URL and add an in-app entry only after owner approval;
  complete Play Data safety from measured behavior, not the word "local" alone.
