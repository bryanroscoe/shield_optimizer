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
| External links | `api.openUrl`, source-site buttons, report-app flow | User-directed browser navigation leaves the app and follows destination practices. No general promise that the app never opens the internet. |

Manifest declarations include INTERNET, ACCESS_NETWORK_STATE, ACCESS_WIFI_STATE,
CHANGE_WIFI_MULTICAST_STATE and NEARBY_WIFI_DEVICES (`neverForLocation`). No phone microphone
permission is declared here: the Assistant control changes a permission on the selected TV.
No billing SDK or automatic analytics upload was found in the inspected source. This is a scoped
source finding, not a guarantee about every transitive dependency or the final merged artifact.

## Draft policy outline requiring owner completion

- Identify app operator and privacy/support contact: **OWNER DECISION**.
- Explain local TV management, authorization, data categories above and user-initiated external
  navigation/sharing. Validate any no-server/no-analytics statement against the final SDK graph.
- State actual retention, deletion, OS backup/transfer and key revocation behavior after testing:
  **VERIFICATION REQUIRED**, not a promise that uninstall securely erases every copy.
- Select distribution, payment processing and any trial policy: **OWNER DECISION**. Revisit
  disclosures when billing, verification servers or telemetry are added.
- Determine audience, jurisdictions, legal bases/rights and effective date: **OWNER/LEGAL REVIEW**.
- Publish an accessible privacy-policy URL and add an in-app entry only after owner approval;
  complete Play Data safety from measured behavior, not the word "local" alone.
